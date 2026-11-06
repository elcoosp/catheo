//! Semantic errors inside a stage must be spanned at the stage keyword
//! (§3.4: `quote_spanned!` ties generated tokens to the user's `map:`).
fn main() {
    let xs = vec![1_i32, 2];
    let _: Vec<String> = catheo::pipeline! {
        witness: catheo::VecWitness,
        source: xs,
        map: |x| x + "not a number",
        collect,
    };
}
