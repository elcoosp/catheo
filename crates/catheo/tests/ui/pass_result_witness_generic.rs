//! Valid pipeline: parameterized `ResultWitness<E>` with a bare callable stage.
use catheo::{ResultWitness, pipeline};

#[derive(Debug, PartialEq)]
enum CmdError {
    Empty,
}

fn tokenize(s: String) -> Result<Vec<String>, CmdError> {
    let toks: Vec<String> = s.split_whitespace().map(str::to_string).collect();
    if toks.is_empty() { Err(CmdError::Empty) } else { Ok(toks) }
}

fn main() {
    let raw = Ok::<String, CmdError>("bind me".to_string());
    let count: usize = pipeline! {
        witness: ResultWitness<CmdError>,
        source: raw,
        bind: tokenize,
        fold: (0_usize, |acc, toks: Vec<String>| acc + toks.len()),
    };
    assert_eq!(count, 2);
}
