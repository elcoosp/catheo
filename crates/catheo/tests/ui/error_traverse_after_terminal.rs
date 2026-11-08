// The traversal is a stage, not a terminal: a terminal consumes the
// container, so nothing may follow it (§6.2 ordering rules).
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: xs,
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
        traverse: (OptionWitness, |x| Some(x)),
    };
}
