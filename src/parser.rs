// ToDo: Avoid reading all of the file into memory at once and use io::Read to minimise the memory
// footprint (maybe even merging the parse_to_tokens and analyse steps together)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    IncCell,
    DecCell,
    IncIdx,
    DecIdx,
    Put,
    Input,
    LoopEntry,
    LoopEnd,
}

pub fn parse_to_tokens(input: Vec<char>) -> Vec<Token> {
    let mut res = Vec::new();
    for c in &input {
        res.push(match *c {
            '+' => Token::IncCell,
            '-' => Token::DecCell,
            '>' => Token::IncIdx,
            '<' => Token::DecIdx,
            '.' => Token::Put,
            ',' => Token::Input,
            '[' => Token::LoopEntry,
            ']' => Token::LoopEnd,

            _ => continue,
        });
    }
    res
}
