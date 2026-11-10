//! `filter_map` over `ResultWitness` is a compile error: a dropped `Ok`
//! has no error-free denotation (see `ResultWitness` and
//! `CategoricalFilterMap` docs) — the same posture as `filter`.
fn main() {
    let raw = Ok::<i32, String>(1);
    let _ = catheo::pipeline! {
        witness: catheo::ResultWitness<String>,
        source: raw,
        filter_map: |x| if x > 0 { Some(x * 2) } else { None },
        collect,
    };
}
