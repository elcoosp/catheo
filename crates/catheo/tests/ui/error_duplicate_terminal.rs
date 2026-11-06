//! Exactly one terminal per pipeline.
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: catheo::VecWitness,
        source: xs,
        fold: (0_usize, |acc, x| acc + x),
        collect,
    };
}
