// Only a `Vec` container witness admits a traversal: `Option`/`Result`
// containers are pointwise (zero-or-one-element projections, the §6.1 M4
// posture) and have no multi-element traversal.
fn main() {
    let element = Some(3_i32);
    let _ = catheo::pipeline! {
        witness: OptionWitness,
        source: element,
        traverse: (OptionWitness, |x| Some(x)),
    };
}
