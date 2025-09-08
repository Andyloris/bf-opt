use crate::parser::Token;

#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    IncCell(isize),
    IncIdx(isize),
    Put,
    Input,
    LoopEntry(usize, usize),
    LoopEnd(usize, usize),
}

impl Instruction {
    pub fn is_changing_mem(self) -> bool {
        matches!(self, Self::IncCell(_) | Self::Input)
    }
}

pub fn analyse_mem_usage(instructions: &Vec<Instruction>) -> Option<usize> {
    let mut res = 0;
    // Stores the data pointer index change at the end of a loop iteration and the maximum reached mem usage
    let mut loop_info: Vec<Option<(usize, usize)>> = Vec::new();
    let mut cur_nesting_level = 0;
    for &inst in instructions {
        match inst {
            Instruction::LoopEntry(_, _) => {
                cur_nesting_level += 1;
                if loop_info.len() <= cur_nesting_level {
                    loop_info.push(None)
                }
                loop_info[cur_nesting_level - 1] = Some((0, 0))
            }

            Instruction::LoopEnd(_, _) => {
                match loop_info[cur_nesting_level - 1] {
                    Some((change, max_idx)) => {
                        if change != 0 {
                            return None;
                        } else {
                            res += max_idx;
                        }
                    }
                    None => unreachable!(),
                }
                loop_info[cur_nesting_level - 1] = None;
                cur_nesting_level -= 1;
            }

            Instruction::IncIdx(inc) => {
                if let Some(Some(cur_loop_info)) =
                    loop_info.get_mut(cur_nesting_level.saturating_sub(1))
                {
                    cur_loop_info.0 = cur_loop_info.0.wrapping_add_signed(inc);
                    cur_loop_info.1 += inc.max(0) as usize;
                    continue;
                }
                res += inc.max(0) as usize;
            }

            _ => {}
        };
    }

    Some(res + 1)
}

fn delete_extraneous_instructions(instructions: Vec<Instruction>) -> (Vec<Instruction>, bool) {
    let mut res: Vec<Instruction> = Vec::new();
    let mut org_indices_to_new = Vec::new();
    let mut is_changed = false;
    let mut last_inst = None;
    for inst in &instructions {
        org_indices_to_new.push(res.len());
        match (last_inst, inst) {
            (_, Instruction::IncIdx(0)) => is_changed = true,
            (_, Instruction::IncCell(0)) => is_changed = true,
            _ => {
                last_inst = Some(*inst);
                res.push(*inst)
            }
        };
    }

    if is_changed {
        res = res
            .iter()
            .map(|i| match &i {
                Instruction::LoopEntry(entry_idx, end_idx) => Instruction::LoopEntry(
                    org_indices_to_new[*entry_idx],
                    org_indices_to_new[*end_idx],
                ),
                Instruction::LoopEnd(entry_idx, end_idx) => Instruction::LoopEnd(
                    org_indices_to_new[*entry_idx],
                    org_indices_to_new[*end_idx],
                ),
                a => **a,
            })
            .collect::<Vec<Instruction>>();
    }

    (res, is_changed)
}

fn remove_extra_loops(instructions: Vec<Instruction>) -> (Vec<Instruction>, bool) {
    let mut res = Vec::new();
    let mut org_indices_to_new = Vec::new();
    let mut is_changed = false;
    let mut stop_flag = false;
    let mut cur_omitted_loop_bounds = None;

    for (idx, &inst) in instructions.iter().enumerate() {
        org_indices_to_new.push(res.len());
        if stop_flag {
            res.push(inst);
            continue;
        }

        if inst.is_changing_mem() && cur_omitted_loop_bounds.is_none() {
            stop_flag = true;
            res.push(inst);
            continue;
        }

        match inst {
            Instruction::LoopEntry(start, end) => {
                if cur_omitted_loop_bounds.is_none() {
                    cur_omitted_loop_bounds = Some((start, end));
                    is_changed = true;
                }
            }
            _ => {
                if let Some((_, end)) = cur_omitted_loop_bounds {
                    if idx == end {
                        cur_omitted_loop_bounds = None;
                    }
                } else {
                    res.push(inst);
                }
            }
        };
    }

    if is_changed {
        res = res
            .iter()
            .map(|i| match &i {
                Instruction::LoopEntry(entry_idx, end_idx) => Instruction::LoopEntry(
                    org_indices_to_new[*entry_idx],
                    org_indices_to_new[*end_idx],
                ),
                Instruction::LoopEnd(entry_idx, end_idx) => Instruction::LoopEnd(
                    org_indices_to_new[*entry_idx],
                    org_indices_to_new[*end_idx],
                ),
                a => **a,
            })
            .collect::<Vec<Instruction>>();
    }

    (res, is_changed)
}

fn merge_instructions(instructions: Vec<Instruction>) -> (Vec<Instruction>, bool) {
    let mut res = Vec::new();
    let mut org_indices_to_new = Vec::new();
    let mut is_changed = false;
    let mut last_inst = None;
    for inst in instructions {
        org_indices_to_new.push(res.len());
        match (last_inst, inst) {
            (Some(Instruction::IncCell(val1)), Instruction::IncCell(val2)) => {
                res.pop();
                let merged = Instruction::IncCell(val1 + val2);
                last_inst = Some(merged);
                res.push(merged);
                is_changed = true;
            }

            (Some(Instruction::IncIdx(val1)), Instruction::IncIdx(val2)) => {
                res.pop();
                let merged = Instruction::IncIdx(val1 + val2);
                last_inst = Some(merged);
                res.push(merged);
                is_changed = true;
            }

            _ => {
                last_inst = Some(inst);
                res.push(inst)
            }
        };
    }

    if is_changed {
        // Loops now point to wrong instructions. This needs to be fixed
        res = res
            .iter()
            .map(|i| match &i {
                Instruction::LoopEntry(entry_idx, end_idx) => Instruction::LoopEntry(
                    org_indices_to_new[*entry_idx],
                    org_indices_to_new[*end_idx],
                ),
                Instruction::LoopEnd(entry_idx, end_idx) => Instruction::LoopEnd(
                    org_indices_to_new[*entry_idx],
                    org_indices_to_new[*end_idx],
                ),
                a => **a,
            })
            .collect::<Vec<Instruction>>();
    }
    (res, is_changed)
}

fn construct_loop_trees(tokens: &[Token]) -> Result<Vec<(usize, usize)>, &'static str> {
    let mut cur_nesting_level = 0;
    let mut res = Vec::new();
    let mut loop_entries = Vec::new();
    let mut loop_ends = Vec::new();
    for (i, tok) in tokens.iter().enumerate() {
        match tok {
            Token::LoopEntry => {
                cur_nesting_level += 1;
                loop_entries.push((i, cur_nesting_level));
            }

            Token::LoopEnd => {
                if cur_nesting_level == 0 {
                    return Err("] without opening bracket");
                }

                loop_ends.push((i, cur_nesting_level));
                cur_nesting_level -= 1;
            }

            _ => continue,
        }
    }

    if cur_nesting_level != 0 {
        return Err("Unmatched [");
    }

    for start in loop_entries {
        let mut end_index = 0;
        for (i, possible_end) in loop_ends.iter().enumerate() {
            if start.1 != possible_end.1 {
                continue;
            }

            res.push((start.0, possible_end.0));
            end_index = i;
            break;
        }

        loop_ends.remove(end_index);
    }

    Ok(res)
}

pub fn analyse(tokens: &[Token]) -> Result<Vec<Instruction>, &'static str> {
    let mut res = Vec::new();
    let mut cur_tok = 0;
    let loop_tree = construct_loop_trees(tokens)?;
    loop {
        if cur_tok >= tokens.len() {
            break;
        }

        let tok = tokens[cur_tok];
        let inst = match tok {
            Token::IncCell => Instruction::IncCell(1),
            Token::DecCell => Instruction::IncCell(-1),
            Token::IncIdx => Instruction::IncIdx(1),
            Token::DecIdx => Instruction::IncIdx(-1),
            Token::Put => Instruction::Put,
            Token::Input => Instruction::Input,
            Token::LoopEntry => {
                let entry = loop_tree.iter().find(|p| p.0 == cur_tok).unwrap();
                Instruction::LoopEntry(entry.0, entry.1)
            }

            Token::LoopEnd => {
                let entry = loop_tree.iter().find(|p| p.1 == cur_tok).unwrap();
                Instruction::LoopEnd(entry.0, entry.1)
            }
        };
        res.push(inst);

        cur_tok += 1;
    }

    // Run the optimisation passes
    loop {
        let mut is_changed = false;
        for pass in [
            merge_instructions,
            delete_extraneous_instructions,
            remove_extra_loops,
        ] {
            let (new_res, new_is_changed) = pass(res);
            res = new_res;
            is_changed |= new_is_changed;
        }

        if !is_changed {
            break;
        }
    }
    Ok(res)
}
