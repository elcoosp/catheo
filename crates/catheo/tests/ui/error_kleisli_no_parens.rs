use catheo::{OptionWitness, pipeline};

fn main() {
    let o = Some(2_i32);
    let _ = pipeline! {
        witness: OptionWitness,
        source: o,
        kleisli: halve,
    };
}
