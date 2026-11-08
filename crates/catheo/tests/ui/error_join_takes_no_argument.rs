// `join` is the bare keyword: it desugars to `bind(id)` and has no value
// slot to fill (§4.3).
fn main() {
    let nested = vec![vec![1_i32]];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: nested,
        join: |x| x,
    };
}
