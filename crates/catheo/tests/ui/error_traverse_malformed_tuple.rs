// The tuple form is mandatory: `(EffectWitness, f)`. A one-segment tuple
// has no traversal arrow.
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: xs,
        traverse: (OptionWitness),
        collect,
    };
}
