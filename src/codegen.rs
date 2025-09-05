use crate::bytecode::Instruction;

use llvm::prelude::*;
use llvm_sys as llvm;
use std::{
    ffi::{CStr, c_char},
    ptr,
};

pub fn gen_ir(
    insts: Vec<Instruction>,
    num_cells: usize,
    target_triple: &CStr,
    cpu: &CStr,
    features: &CStr,
) {
    unsafe {
        let module = llvm::core::LLVMModuleCreateWithName(c"OverengineeredBF".as_ptr() as *const _);
        let context = llvm::core::LLVMContextCreate();
        let builder = llvm::core::LLVMCreateBuilderInContext(context);

        llvm::target::LLVM_InitializeAllTargetInfos();
        llvm::target::LLVM_InitializeAllTargets();
        llvm::target::LLVM_InitializeAllTargetMCs();
        llvm::target::LLVM_InitializeAllAsmParsers();
        llvm::target::LLVM_InitializeAllAsmPrinters();

        let mut target: llvm_sys::target_machine::LLVMTargetRef = ptr::null_mut();
        let mut err_msg: *mut c_char = ptr::null_mut();
        if llvm::target_machine::LLVMGetTargetFromTriple(
            target_triple.as_ptr(),
            &mut target as *mut llvm::target_machine::LLVMTargetRef,
            &mut err_msg as *mut *mut c_char,
        ) != 0
        {
            let s = CStr::from_ptr(err_msg.cast_const());
            panic!("{}", s.to_str().unwrap());
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

        let dl = llvm::target_machine::LLVMCreateTargetDataLayout(target_machine);
        llvm::target::LLVMSetModuleDataLayout(module, dl);

        let pass_builder_opts = llvm::transforms::pass_builder::LLVMCreatePassBuilderOptions();

        let pointer_size_bits = llvm::target::LLVMPointerSize(dl) * 8;
        let size_t_type = llvm::core::LLVMIntTypeInContext(context, pointer_size_bits);

        let mut int = llvm::core::LLVMInt32TypeInContext(context);
        let mainfn_type = llvm::core::LLVMFunctionType(int, ptr::null_mut(), 0, 0);
        let mainfn = llvm::core::LLVMAddFunction(module, c"main".as_ptr() as *const _, mainfn_type);

        let bb = llvm::core::LLVMAppendBasicBlockInContext(
            context,
            mainfn,
            c"entry".as_ptr() as *const _,
        );
        llvm::core::LLVMPositionBuilderAtEnd(builder, bb);
        // Right now, we only depend on the C standard library (for malloc and putchar)
        /*let ptr = llvm::core::LLVMPointerTypeInContext(context, 0);
        let mut args = [size_t_type, size_t_type];
        let calloc_type = llvm::core::LLVMFunctionType(ptr, &mut args as *mut *mut _, 1, 0);
        let calloc_fn =
            llvm::core::LLVMAddFunction(module, c"bfcalloc".as_ptr() as *const _, calloc_type);
        llvm::core::LLVMSetLinkage(calloc_fn, llvm::LLVMLinkage::LLVMExternalLinkage);*/
        // Define extern putchar function
        let putchar_type = llvm::core::LLVMFunctionType(int, &mut int as *mut _, 1, 0);
        let putchar_fn =
            llvm::core::LLVMAddFunction(module, c"putchar".as_ptr() as *const _, putchar_type);
        llvm::core::LLVMSetLinkage(putchar_fn, llvm::LLVMLinkage::LLVMExternalLinkage);

        // Declare the cell array
        let cell_type = llvm::core::LLVMInt8TypeInContext(context);
        let cell_zero = llvm::core::LLVMConstInt(cell_type, 0, 1);
        let alloc_size = llvm::core::LLVMConstInt(size_t_type, num_cells as u64, 1);
        let bfarray = llvm::core::LLVMBuildArrayMalloc(
            builder,
            cell_type,
            alloc_size,
            c"bfarray".as_ptr() as *const _,
        );
        llvm::core::LLVMBuildMemSet(builder, bfarray, cell_zero, alloc_size, 1);

        // Declare the data pointer
        let data_ptr_val =
            llvm::core::LLVMBuildAlloca(builder, int, c"bfdataidx".as_ptr() as *const _);
        llvm::core::LLVMBuildStore(builder, llvm::core::LLVMConstInt(int, 0, 1), data_ptr_val);
        let mut loop_labels: Vec<Option<(LLVMBasicBlockRef, LLVMBasicBlockRef)>> = Vec::new();
        let mut nesting_level = 0;
        for inst in insts {
            match inst {
                Instruction::IncCell(val) => {
                    let mut tmp = llvm::core::LLVMBuildLoad2(
                        builder,
                        int,
                        data_ptr_val,
                        c"bfdataptrval".as_ptr() as *const _,
                    );
                    let elem_ptr = llvm::core::LLVMBuildInBoundsGEP2(
                        builder,
                        cell_type,
                        bfarray,
                        &mut tmp as *mut _,
                        1,
                        c"bfelemptr".as_ptr() as *const _,
                    );
                    let elem_val = llvm::core::LLVMBuildLoad2(
                        builder,
                        cell_type,
                        elem_ptr,
                        c"bfcellval".as_ptr() as *const _,
                    );
                    let val = llvm::core::LLVMConstInt(cell_type, (val % 256) as u64, 1);
                    let add = llvm::core::LLVMBuildAdd(
                        builder,
                        elem_val,
                        val,
                        c"bfcelladdress".as_ptr() as *const _,
                    );
                    let _ = llvm::core::LLVMBuildStore(builder, add, elem_ptr);
                }

                Instruction::IncIdx(val) => {
                    let off = llvm::core::LLVMConstInt(int, val as u64, 1);
                    let tmp = llvm::core::LLVMBuildLoad2(
                        builder,
                        int,
                        data_ptr_val,
                        c"bftmpdataidx".as_ptr() as *const _,
                    );
                    let add = llvm::core::LLVMBuildAdd(
                        builder,
                        tmp,
                        off,
                        c"bftmp2dataidx".as_ptr() as *const _,
                    );
                    llvm::core::LLVMBuildStore(builder, add, data_ptr_val);
                }

                Instruction::LoopEntry(_, _) => {
                    let loop_test = llvm::core::LLVMAppendBasicBlockInContext(
                        context,
                        mainfn,
                        c"".as_ptr() as *const _,
                    );

                    let loop_code = llvm::core::LLVMAppendBasicBlockInContext(
                        context,
                        mainfn,
                        c"loop_code".as_ptr() as *const _,
                    );

                    let loop_end = llvm::core::LLVMAppendBasicBlockInContext(
                        context,
                        mainfn,
                        c"loop_end".as_ptr() as *const _,
                    );

                    llvm::core::LLVMBuildBr(builder, loop_test);
                    llvm::core::LLVMPositionBuilderAtEnd(builder, loop_test);
                    let mut tmp = llvm::core::LLVMBuildLoad2(
                        builder,
                        int,
                        data_ptr_val,
                        c"bfdataptrval".as_ptr() as *const _,
                    );
                    let elem_ptr = llvm::core::LLVMBuildInBoundsGEP2(
                        builder,
                        cell_type,
                        bfarray,
                        &mut tmp as *mut _,
                        1,
                        c"bfelemptr".as_ptr() as *const _,
                    );
                    let elem_val = llvm::core::LLVMBuildLoad2(
                        builder,
                        cell_type,
                        elem_ptr,
                        c"bfcellval".as_ptr() as *const _,
                    );
                    let cond = llvm::core::LLVMBuildICmp(
                        builder,
                        llvm::LLVMIntPredicate::LLVMIntEQ,
                        cell_zero,
                        elem_val,
                        c"loop_entry_cmp".as_ptr() as *const _,
                    );
                    llvm::core::LLVMBuildCondBr(builder, cond, loop_end, loop_code);
                    llvm::core::LLVMPositionBuilderAtEnd(builder, loop_code);
                    nesting_level += 1;
                    if loop_labels.len() < nesting_level {
                        loop_labels.push(None);
                    }

                    loop_labels[nesting_level - 1] = Some((loop_test, loop_end));
                }

                Instruction::LoopEnd(_, _) => {
                    let labels = loop_labels[nesting_level - 1].unwrap();
                    nesting_level -= 1;
                    llvm::core::LLVMBuildBr(builder, labels.0);
                    llvm::core::LLVMPositionBuilderAtEnd(builder, labels.1);
                }

                Instruction::Put => {
                    let mut tmp = llvm::core::LLVMBuildLoad2(
                        builder,
                        int,
                        data_ptr_val,
                        c"bfdataptrval".as_ptr() as *const _,
                    );
                    let elem_ptr = llvm::core::LLVMBuildInBoundsGEP2(
                        builder,
                        cell_type,
                        bfarray,
                        &mut tmp as *mut _,
                        1,
                        c"bfelemptr".as_ptr() as *const _,
                    );
                    let mut elem_val = llvm::core::LLVMBuildLoad2(
                        builder,
                        cell_type,
                        elem_ptr,
                        c"bfcellval".as_ptr() as *const _,
                    );
                    llvm::core::LLVMBuildCall2(
                        builder,
                        putchar_type,
                        putchar_fn,
                        &mut elem_val as *mut _,
                        1,
                        c"".as_ptr() as *const _,
                    );
                }

                _ => {}
            };
        }
        let zero = llvm::core::LLVMConstInt(int, 0, 1);
        llvm::core::LLVMBuildRet(builder, zero);

        llvm::transforms::pass_builder::LLVMRunPasses(
            module,
            c"default<O3>".as_ptr() as *const _,
            target_machine,
            pass_builder_opts,
        );

        llvm::target_machine::LLVMTargetMachineEmitToFile(
            target_machine,
            module,
            c"out.o".as_ptr() as *const _,
            llvm::target_machine::LLVMCodeGenFileType::LLVMObjectFile,
            &mut err_msg as *mut _,
        );
        llvm::core::LLVMPrintModuleToFile(
            module,
            c"out.ll".as_ptr() as *const _,
            &mut err_msg as *mut _,
        );
        //let s = CStr::from_ptr(err_msg.cast_const());
        //println!("{}", s.to_str().unwrap());
        llvm::core::LLVMDisposeBuilder(builder);
        llvm::core::LLVMDisposeModule(module);
        llvm::core::LLVMContextDispose(context);
    }
}
