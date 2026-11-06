//! Missing `source:` must be rejected.
fn main() {
    let _ = catheo::pipeline! {
        witness: catheo::VecWitness,
        map: |x| x,
    };
}
