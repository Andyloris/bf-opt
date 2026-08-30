# bf-opt

An Optimizing Brainfuck Compiler built on LLVM

## Features

- Optimization passes:
  - Run-length encoding for arithmetic and cell pointer instructions
  - Loop elimination
  - Static memory usage analysis for optimized memory usage

## Building

0. Install [Rust](https://rust-lang.org/tools/install/) and [LLVM 20.1.x](https://github.com/llvm/llvm-project/releases?page=3#release-llvmorg-20.1.8)
1. Clone this repository: `git clone https://github.com/Andyloris/bf-opt`
2. Enter the source directory: `cd bf-opt`
3. Build: `cargo build --release`
4. The compiler will be at `target/release/bf-opt`

## Usage

For usage information, run `bf-opt --help`

## License

bf-opt is distributed under the [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.en.html)

## ToDo

- Speculative execution
- Buffered reading using std::io::Read
- Merging parsing and analysis steps
- Perhaps single pass optimization
