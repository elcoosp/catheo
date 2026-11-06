//! `fold` must be a two-tuple of init and a two-parameter closure.
fn main() {
    let xs = vec![1_i32];
    let _: i32 = catheo::pipeline! {
        witness: catheo::VecWitness,
        source: xs,
        fold: |acc, x| acc + x,
    };
}
