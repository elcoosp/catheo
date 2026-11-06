//! `filter` over `ResultWitness` is a compile error: dropping an `Ok`
//! has no error-free denotation (see `ResultWitness` docs).
fn main() {
    let raw = Ok::<i32, String>(1);
    let _ = catheo::pipeline! {
        witness: catheo::ResultWitness<String>,
        source: raw,
        filter: |x| *x > 0,
        collect,
    };
}
