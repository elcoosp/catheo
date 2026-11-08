// The terminal consumes the container, so nothing may follow it (§6.2
// ordering rules) — `join` included.
fn main() {
    let xs = vec![vec![1_i32]];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: xs,
        collect,
        join,
    };
}
