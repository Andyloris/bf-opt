use crate::bytecode::Instruction;

// Sample bf interpreter
pub struct Interpreter {
    cells: Vec<u8>,
    idx: usize,
}

impl Interpreter {
    pub fn new(num_cells: usize) -> Self {
        Self {
            cells: vec![0; num_cells],
            idx: 0,
        }
    }

    pub fn execute(&mut self, insts: &[Instruction]) {
        let mut inst_idx = 0;
        loop {
            if inst_idx >= insts.len() {
                break;
            }

            let i = insts[inst_idx];
            match i {
                Instruction::IncCell(off) => {
                    self.cells[self.idx] =
                        ((self.cells[self.idx] as usize).wrapping_add_signed(off) % 256) as u8
                }
                Instruction::IncIdx(off) => {
                    self.idx = self.idx.wrapping_add_signed(off);
                }
                Instruction::Put => print!("{}", self.cells[self.idx] as char),
                Instruction::LoopEntry(_, end_idx) => {
                    if self.cells[self.idx] == 0 {
                        inst_idx = end_idx;
                    }
                }

                Instruction::LoopEnd(start_idx, _) => {
                    if self.cells[self.idx] != 0 {
                        inst_idx = start_idx;
                    }
                }

                _ => todo!(),
            }

            inst_idx += 1;
        }
    }
}
