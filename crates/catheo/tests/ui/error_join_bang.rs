// Escape hatches tune the *fusion analysis* of `map`/`bind` closures; the
// desugared identity has no closure, so no hatch applies (§4.2).
fn main() {
    let nested = vec![vec![1_i32]];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: nested,
        join!: force,
    };
}
