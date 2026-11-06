//! `catheo-macros` — thin proc-macro front end for catheo.
//!
//! All parsing and emission lives in [`catheo-parser`] so it can be tested
//! and benchmarked without driving `rustc` (§6.3). This wrapper only
//! converts token streams across the proc-macro boundary and maps any parse
//! error to a spanned `compile_error!` at the offending user token.

use proc_macro::TokenStream;

/// Compile a categorical pipeline into flat imperative Rust.
///
/// ```ignore
/// let total: usize = catheo::pipeline! {
///     witness: VecWitness,
///     source: input,
///     map: |x| x * 2,
///     fold: (0_usize, |acc, x| acc + x),
/// };
/// ```
#[proc_macro]
pub fn pipeline(input: TokenStream) -> TokenStream {
    let tokens: proc_macro2::TokenStream = input.into();
    let expanded = match catheo_parser::parse_pipeline(tokens) {
        Ok(parsed) => catheo_parser::emit(&parsed),
        Err(err) => Err(err),
    };
    match expanded {
        Ok(emitted) => emitted.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
