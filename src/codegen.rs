use crate::bytecode::Instruction;

use llvm::prelude::*;
use llvm_sys::{
    self as llvm, target::LLVMTargetDataRef, target_machine::LLVMTargetMachineRef,
    transforms::pass_builder::LLVMPassBuilderOptionsRef,
};
use std::{
    ffi::{CStr, CString, c_char, c_int},
    mem::ManuallyDrop,
    ptr,
    str::FromStr,
    sync::Mutex,
};

static OPTLEVEL_STRINGS: [&CStr; 6] = [
    c"default<O0>",
    c"default<O1>",
    c"default<O2>",
    c"default<O3>",
    c"default<Os>",
    c"default<Oz>",
];

#[derive(Clone, Copy)]
pub enum OptimizationLevel {
    O0,
    O1,
    O2,
    O3,
    Os,
    Oz,
}

impl OptimizationLevel {
    pub fn as_index(self) -> usize {
        match self {
            Self::O0 => 0,
            Self::O1 => 1,
            Self::O2 => 2,
            Self::O3 => 3,
            Self::Os => 4,
            Self::Oz => 5,
        }
    }
}

pub enum OutputFileType {
    IR,
    Assembly,
    ObjectFile,
}

pub struct TargetInfo<'a> {
    pub target_triple: &'a str,
    pub cpu: &'a str,
    pub features: &'a str,
}

pub struct OutInfo {
    pub out_file: String,
    pub out_type: OutputFileType,
}

pub struct CodeGenData {
    module: LLVMModuleRef,
    context: LLVMContextRef,
    builder: LLVMBuilderRef,
    target_machine: LLVMTargetMachineRef,
    pass_builder_opts: LLVMPassBuilderOptionsRef,
    data_layout: LLVMTargetDataRef,
}

static LLVM_IS_INIT: Mutex<bool> = Mutex::new(false);
unsafe fn llvm_init() {
    let lock = LLVM_IS_INIT.lock().unwrap();
    if !*lock {
        llvm::target::LLVM_InitializeAllTargetInfos();
        llvm::target::LLVM_InitializeAllTargets();
        llvm::target::LLVM_InitializeAllTargetMCs();
        llvm::target::LLVM_InitializeAllAsmParsers();
        llvm::target::LLVM_InitializeAllAsmPrinters();
    }
}

impl CodeGenData {
    unsafe fn llvm_err_assert(result: c_int, err_msg: *mut c_char) -> bool {
        let result = result != 0;
        if result {
            let s = ManuallyDrop::new(CStr::from_ptr(err_msg.cast_const()));
            eprintln!("LLVM Error: {}", s.to_str().unwrap());
            llvm::error::LLVMDisposeErrorMessage(err_msg);
        }
        result
    }

    // LLVM Seems to copy the strings that it gets, so be careful when using this
    unsafe fn gen_cstring_from_str(str: &str) -> CString {
        CString::from_str(str).unwrap()
    }

    unsafe fn new_impl(module_name: &str, target_info: TargetInfo) -> Option<Self> {
        let module_name = Self::gen_cstring_from_str(module_name);
        let module = llvm::core::LLVMModuleCreateWithName(module_name.as_ptr() as *const _);

        let context = llvm::core::LLVMContextCreate();
        let builder = llvm::core::LLVMCreateBuilderInContext(context);

        llvm_init();
        let target_triple = Self::gen_cstring_from_str(target_info.target_triple);
        let cpu = Self::gen_cstring_from_str(target_info.cpu);
        let features = Self::gen_cstring_from_str(target_info.features);

        let mut target: llvm_sys::target_machine::LLVMTargetRef = ptr::null_mut();
        let mut err_msg: *mut c_char = ptr::null_mut();

        if Self::llvm_err_assert(
            llvm::target_machine::LLVMGetTargetFromTriple(
                target_triple.as_ptr(),
                &mut target as *mut llvm::target_machine::LLVMTargetRef,
                &mut err_msg as *mut *mut c_char,
            ),
            err_msg,
        ) {
            return None;
        }

        let target_machine = llvm::target_machine::LLVMCreateTargetMachine(
            target,
            target_triple.as_ptr(),
            cpu.as_ptr(),
            features.as_ptr(),
            llvm::target_machine::LLVMCodeGenOptLevel::LLVMCodeGenLevelDefault,
            llvm::target_machine::LLVMRelocMode::LLVMRelocPIC,
            llvm::target_machine::LLVMCodeModel::LLVMCodeModelDefault,
        );

        let pass_builder_opts = llvm::transforms::pass_builder::LLVMCreatePassBuilderOptions();
        let data_layout = llvm::target_machine::LLVMCreateTargetDataLayout(target_machine);

        Some(Self {
            module,
            context,
            builder,
            target_machine,
            pass_builder_opts,
            data_layout,
        })
    }

    pub fn new(module_name: &str, target_info: TargetInfo) -> Option<Self> {
        unsafe { Self::new_impl(module_name, target_info) }
    }

    pub unsafe fn gen_ir_impl(&self, num_cells: usize, insts: Vec<Instruction>) {
        // Types
        let pointer_size_bits = llvm::target::LLVMPointerSize(self.data_layout) * 8;
        let size_t_type = llvm::core::LLVMIntTypeInContext(self.context, pointer_size_bits);
        let mut int_type = llvm::core::LLVMInt32TypeInContext(self.context);
        let cell_ptr_type = llvm::core::LLVMPointerTypeInContext(self.context, 0);
        let cell_type = llvm::core::LLVMInt8TypeInContext(self.context);

        let getchar_type = llvm::core::LLVMFunctionType(int_type, ptr::null_mut(), 0, 0);
        let putchar_type = llvm::core::LLVMFunctionType(int_type, &mut int_type as *mut _, 1, 0);
        let mainfn_type = llvm::core::LLVMFunctionType(int_type, ptr::null_mut(), 0, 0);

        // Constants
        let empty_name = c"".as_ptr() as *const _;
        let cell_zero = llvm::core::LLVMConstInt(cell_type, 0, 1);
        let alloc_size = llvm::core::LLVMConstInt(size_t_type, num_cells as u64, 1);
        let int_zero = llvm::core::LLVMConstInt(int_type, 0, 1);

        // Functions
        let getchar_fn =
            llvm::core::LLVMAddFunction(self.module, c"getchar".as_ptr() as *const _, getchar_type);
        let putchar_fn =
            llvm::core::LLVMAddFunction(self.module, c"putchar".as_ptr() as *const _, putchar_type);
        llvm::core::LLVMSetLinkage(putchar_fn, llvm::LLVMLinkage::LLVMExternalLinkage);
        llvm::core::LLVMSetLinkage(getchar_fn, llvm::LLVMLinkage::LLVMExternalLinkage);
        let main_fn =
            llvm::core::LLVMAddFunction(self.module, c"main".as_ptr() as *const _, mainfn_type);

        // Beginning of the actual codegen
        let entry_bb = llvm::core::LLVMAppendBasicBlockInContext(
            self.context,
            main_fn,
            c"entry".as_ptr() as *const _,
        );
        llvm::core::LLVMPositionBuilderAtEnd(self.builder, entry_bb);

        // Declare the cell array
        let bfarray =
            llvm::core::LLVMBuildArrayMalloc(self.builder, cell_type, alloc_size, empty_name);
        // Zero the cell array
        llvm::core::LLVMBuildMemSet(self.builder, bfarray, cell_zero, alloc_size, 1);
        let elem_ptr = llvm::core::LLVMBuildAlloca(self.builder, cell_ptr_type, empty_name);
        llvm::core::LLVMBuildStore(self.builder, bfarray, elem_ptr);

        // Declare the data pointer
        //let data_ptr_val = llvm::core::LLVMBuildAlloca(self.builder, int_type, empty_name);
        // Set the data pointer to the start of the cell array
        //llvm::core::LLVMBuildStore(self.builder, int_zero, data_ptr_val);

        let mut loop_labels: Vec<Option<(LLVMBasicBlockRef, LLVMBasicBlockRef)>> = Vec::new();
        let mut nesting_level = 0;

        for inst in insts {
            match inst {
                Instruction::IncCell(val) => {
                    let tmp = llvm::core::LLVMBuildLoad2(
                        self.builder,
                        cell_ptr_type,
                        elem_ptr,
                        empty_name,
                    );
                    let elem_val =
                        llvm::core::LLVMBuildLoad2(self.builder, cell_type, tmp, empty_name);
                    let val = llvm::core::LLVMConstInt(cell_type, (val % 256) as u64, 1);
                    let add = llvm::core::LLVMBuildAdd(self.builder, elem_val, val, empty_name);
                    let _ = llvm::core::LLVMBuildStore(self.builder, add, tmp);
                }

                Instruction::IncIdx(val) => {
                    let mut off = llvm::core::LLVMConstInt(int_type, val as u64, 1);
                    let tmp_ptr = llvm::core::LLVMBuildLoad2(
                        self.builder,
                        cell_ptr_type,
                        elem_ptr,
                        empty_name,
                    );
                    let tmp_new_ptr = llvm::core::LLVMBuildInBoundsGEP2(
                        self.builder,
                        cell_type,
                        tmp_ptr,
                        &mut off as *mut _,
                        1,
                        empty_name,
                    );
                    llvm::core::LLVMBuildStore(self.builder, tmp_new_ptr, elem_ptr);
                }

                Instruction::LoopEntry(_, _) => {
                    let loop_test = llvm::core::LLVMAppendBasicBlockInContext(
                        self.context,
                        main_fn,
                        empty_name,
                    );

                    let loop_code = llvm::core::LLVMAppendBasicBlockInContext(
                        self.context,
                        main_fn,
                        empty_name,
                    );

                    let loop_end = llvm::core::LLVMAppendBasicBlockInContext(
                        self.context,
                        main_fn,
                        empty_name,
                    );

                    llvm::core::LLVMBuildBr(self.builder, loop_test);
                    llvm::core::LLVMPositionBuilderAtEnd(self.builder, loop_test);

                    let tmp = llvm::core::LLVMBuildLoad2(
                        self.builder,
                        cell_ptr_type,
                        elem_ptr,
                        empty_name,
                    );
                    let elem_val =
                        llvm::core::LLVMBuildLoad2(self.builder, cell_type, tmp, empty_name);
                    let cond = llvm::core::LLVMBuildICmp(
                        self.builder,
                        llvm::LLVMIntPredicate::LLVMIntEQ,
                        cell_zero,
                        elem_val,
                        empty_name,
                    );
                    llvm::core::LLVMBuildCondBr(self.builder, cond, loop_end, loop_code);
                    llvm::core::LLVMPositionBuilderAtEnd(self.builder, loop_code);
                    nesting_level += 1;
                    if loop_labels.len() < nesting_level {
                        loop_labels.push(None);
                    }

                    loop_labels[nesting_level - 1] = Some((loop_test, loop_end));
                }

                Instruction::LoopEnd(_, _) => {
                    let labels = loop_labels[nesting_level - 1].unwrap();
                    nesting_level -= 1;
                    llvm::core::LLVMBuildBr(self.builder, labels.0);
                    llvm::core::LLVMPositionBuilderAtEnd(self.builder, labels.1);
                }

                Instruction::Put => {
                    let tmp = llvm::core::LLVMBuildLoad2(
                        self.builder,
                        cell_ptr_type,
                        elem_ptr,
                        empty_name,
                    );
                    let mut elem_val =
                        llvm::core::LLVMBuildLoad2(self.builder, cell_type, tmp, empty_name);
                    llvm::core::LLVMBuildCall2(
                        self.builder,
                        putchar_type,
                        putchar_fn,
                        &mut elem_val as *mut _,
                        1,
                        empty_name,
                    );
                }

                Instruction::Input => {
                    let char = llvm::core::LLVMBuildCall2(
                        self.builder,
                        getchar_type,
                        getchar_fn,
                        ptr::null_mut(),
                        0,
                        empty_name,
                    );

                    let tmp = llvm::core::LLVMBuildLoad2(
                        self.builder,
                        cell_ptr_type,
                        elem_ptr,
                        empty_name,
                    );
                    let _ = llvm::core::LLVMBuildStore(self.builder, char, tmp);
                }
            };
        }

        llvm::core::LLVMBuildRet(self.builder, int_zero);
    }

    pub fn gen_ir(&self, num_cells: usize, insts: Vec<Instruction>) {
        unsafe {
            self.gen_ir_impl(num_cells, insts);
        }
    }

    pub fn run_passes(&self, opt_level: OptimizationLevel) {
        unsafe {
            llvm::transforms::pass_builder::LLVMRunPasses(
                self.module,
                OPTLEVEL_STRINGS[opt_level.as_index()].as_ptr() as *const _,
                self.target_machine,
                self.pass_builder_opts,
            );
        }
    }

    pub fn output_code(&self, out_info: OutInfo) -> bool {
        unsafe {
            let mut err_msg: *mut c_char = ptr::null_mut();
            let file_name = Self::gen_cstring_from_str(&out_info.out_file);
            match out_info.out_type {
                OutputFileType::IR => Self::llvm_err_assert(
                    llvm::core::LLVMPrintModuleToFile(
                        self.module,
                        file_name.as_ptr() as *const _,
                        &mut err_msg as *mut _,
                    ),
                    err_msg,
                ),

                OutputFileType::Assembly => Self::llvm_err_assert(
                    llvm::target_machine::LLVMTargetMachineEmitToFile(
                        self.target_machine,
                        self.module,
                        file_name.as_ptr() as *const _,
                        llvm::target_machine::LLVMCodeGenFileType::LLVMAssemblyFile,
                        &mut err_msg as *mut _,
                    ),
                    err_msg,
                ),

                OutputFileType::ObjectFile => Self::llvm_err_assert(
                    llvm::target_machine::LLVMTargetMachineEmitToFile(
                        self.target_machine,
                        self.module,
                        file_name.as_ptr() as *const _,
                        llvm::target_machine::LLVMCodeGenFileType::LLVMObjectFile,
                        &mut err_msg as *mut _,
                    ),
                    err_msg,
                ),
            }
        }
    }
}

impl Drop for CodeGenData {
    fn drop(&mut self) {
        unsafe {
            let Self {
                module,
                context,
                builder,
                target_machine,
                pass_builder_opts,
                data_layout,
            } = *self;

            llvm::core::LLVMDisposeBuilder(builder);
            llvm::core::LLVMDisposeModule(module);
            llvm::core::LLVMContextDispose(context);
            llvm::target_machine::LLVMDisposeTargetMachine(target_machine);
            llvm::transforms::pass_builder::LLVMDisposePassBuilderOptions(pass_builder_opts);
            llvm::target::LLVMDisposeTargetData(data_layout);
        }
    }
}
