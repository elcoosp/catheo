// One traversal boundary per pipeline: a second `traverse` would re-wrap
// the already-guarded suffix in another effect, which the one-witness
// grammar cannot name.
fn main() {
    let xs = vec![1_i32, 2];
    let _: Option<Vec<i32>> = catheo::pipeline! {
        witness: VecWitness,
        source: xs,
        traverse: (OptionWitness, |x| Some(x)),
        traverse: (OptionWitness, |x| Some(x)),
    };
}
