//! A stage below the terminal cannot keep its written position.
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: catheo::VecWitness,
        source: xs,
        fold: (0_usize, |acc, x| acc + x),
        map: |x| x * 2,
    };
}
