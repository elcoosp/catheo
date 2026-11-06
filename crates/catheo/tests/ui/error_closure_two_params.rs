//! Two-parameter closures violate the stage arity; `fold` is the eliminator.
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: catheo::VecWitness,
        source: xs,
        map: |x, y| x + y,
    };
}
