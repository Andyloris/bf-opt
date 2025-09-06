mod bytecode;
mod codegen;
mod interpreter;
mod parser;

use clap::{Parser, Subcommand, arg};
use std::fs;

use crate::codegen::{OptimizationLevel, OutputFileType};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    file: String,
    #[arg(short)]
    output: Option<String>,
    #[arg(long = "emit-llvm")]
    emit_llvm: bool,
    #[arg(long = "max-cells", default_value_t = 30000)]
    max_cells: usize,
    #[arg(short = 'S', conflicts_with = "emit_llvm")]
    emit_asm: bool,
    #[command(subcommand)]
    optimize: Option<ArgsOptimizationLevel>,
}

#[derive(Subcommand, Debug)]
enum ArgsOptimizationLevel {
    #[clap(short_flag = 'O')]
    O {
        #[arg(short = '0', conflicts_with_all = ["level_one", "level_two", "level_three", "level_size", "level_zize"])]
        level_zero: bool,
        #[arg(short = '1', conflicts_with_all = ["level_zero", "level_two", "level_three", "level_size", "level_zize"])]
        level_one: bool,
        #[arg(short = '2', conflicts_with_all = ["level_zero", "level_one", "level_three", "level_size", "level_zize"])]
        level_two: bool,
        #[arg(short = '3', conflicts_with_all = ["level_zero", "level_one", "level_two", "level_size", "level_zize"])]
        level_three: bool,
        #[arg(short = 's', conflicts_with_all = ["level_zero", "level_one", "level_two", "level_three", "level_zize"])]
        level_size: bool,
        #[arg(short = 'z', conflicts_with_all = ["level_zero", "level_one", "level_two", "level_three", "level_size"])]
        level_zize: bool,
    },
}

fn main() {
    let args = Args::parse();
    let raw_bf_code = fs::read(&args.file).unwrap();
    let bf_code_string = String::from_utf8(raw_bf_code).unwrap();
    let tokens = parser::parse_to_tokens(bf_code_string.chars().collect());
    let insts = bytecode::analyse(tokens.as_slice()).unwrap();
    let mem_usage = bytecode::analyse_mem_usage(&insts).unwrap_or(args.max_cells);

    let mut outfile_type = OutputFileType::ObjectFile;
    if args.emit_llvm {
        outfile_type = OutputFileType::IR;
    }

    if args.emit_asm {
        outfile_type = OutputFileType::Assembly;
    }

    // Uber spaghetti code
    let opt_level = match args.optimize {
        Some(ArgsOptimizationLevel::O {
            level_zero: true,
            level_one: _,
            level_two: _,
            level_three: _,
            level_size: _,
            level_zize: _,
        }) => OptimizationLevel::O0,
        Some(ArgsOptimizationLevel::O {
            level_zero: _,
            level_one: true,
            level_two: _,
            level_three: _,
            level_size: _,
            level_zize: _,
        }) => OptimizationLevel::O1,
        Some(ArgsOptimizationLevel::O {
            level_zero: _,
            level_one: _,
            level_two: true,
            level_three: _,
            level_size: _,
            level_zize: _,
        }) => OptimizationLevel::O2,
        Some(ArgsOptimizationLevel::O {
            level_zero: _,
            level_one: _,
            level_two: _,
            level_three: true,
            level_size: _,
            level_zize: _,
        }) => OptimizationLevel::O3,
        Some(ArgsOptimizationLevel::O {
            level_zero: _,
            level_one: _,
            level_two: _,
            level_three: _,
            level_size: true,
            level_zize: _,
        }) => OptimizationLevel::Os,
        Some(ArgsOptimizationLevel::O {
            level_zero: _,
            level_one: _,
            level_two: _,
            level_three: _,
            level_size: _,
            level_zize: true,
        }) => OptimizationLevel::Oz,
        _ => OptimizationLevel::O0,
    };

    codegen::gen_ir(
        insts.clone(),
        mem_usage,
        c"x86_64-pc-linux-gnu",
        c"generic",
        c"",
        args.output.unwrap(),
        outfile_type,
        opt_level,
    );
}
