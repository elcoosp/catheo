// A `Vec` effect is the cartesian traversal (each element expands to a
// container and the results combine combinatorially) — not a pointwise
// effect, and not supported. Flattening stages are spelled `bind`.
fn main() {
    let xs = vec![1_i32, 2, 3];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: xs,
        traverse: (VecWitness, |x| vec![x]),
        collect,
    };
}
