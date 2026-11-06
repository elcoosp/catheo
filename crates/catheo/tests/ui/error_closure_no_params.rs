//! Zero-parameter closures violate the stage arity (§7 limitation 1).
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        witness: catheo::VecWitness,
        source: xs,
        map: || 42,
    };
}
