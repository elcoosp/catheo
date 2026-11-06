//! Unknown combinators must be rejected, naming the offender.
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: catheo::VecWitness,
        source: xs,
        fmap: |x| x * 2,
    };
}
