//! Valid pipeline: `Option` bind chain with a collect terminal.
use catheo::{OptionWitness, pipeline};

fn main() {
    let maybe = Some(5_i32);
    let out: Vec<i32> = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x * 2,
        bind: |x| if x > 5 { Some(x) } else { None },
        collect,
    };
    assert_eq!(out, vec![10]);
}
