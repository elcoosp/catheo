// Escape hatches modulate the fusion analysis of `map`/`bind` stages; the
// traversal is a boundary stage, so no §4.2 hatch applies to it.
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: xs,
        traverse!: force (OptionWitness, |x| Some(x)),
    };
}
