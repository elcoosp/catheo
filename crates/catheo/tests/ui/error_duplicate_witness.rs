//! Exactly one witness per pipeline.
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: catheo::VecWitness,
        witness: catheo::OptionWitness,
        source: xs,
    };
}
