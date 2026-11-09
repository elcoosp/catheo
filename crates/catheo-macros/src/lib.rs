//! `catheo-macros` — thin proc-macro front end for catheo.
//!
//! All parsing and emission lives in [`catheo-parser`] so it can be tested
//! and benchmarked without driving `rustc` (§6.3). This wrapper only
//! converts token streams across the proc-macro boundary and maps any parse
//! error to a spanned `compile_error!` at the offending user token.

use proc_macro::TokenStream;

/// Compile a categorical pipeline into flat imperative Rust.
///
/// The stage list first runs the deforestation pass (§3.3): adjacent maps
/// fuse, maps fuse into non-observing binds, and a pure `fold` absorbs its
/// trailing map/bind chain into imperative loops (§3.3 Rule 4), so no
/// intermediate container is allocated on the fast path.
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
    expand_with(input, catheo_parser::Lowering::Fused)
}

/// The unfused reference lowering of [`pipeline`] — same grammar, same
/// semantics, no deforestation: one monomorphic kernel call per stage, the
/// M0 sequential shape.
///
/// This is the oracle of the §6.2 correctness gate (`fused == unfused`
/// property tests) and a debugging aid: expanding a suspect pipeline
/// through both macros isolates any mis-rewrite to the fusion pass.
#[proc_macro]
pub fn pipeline_unfused(input: TokenStream) -> TokenStream {
    expand_with(input, catheo_parser::Lowering::Sequential)
}

fn expand_with(input: TokenStream, lowering: catheo_parser::Lowering) -> TokenStream {
    let tokens: proc_macro2::TokenStream = input.into();
    // Parse, then emit — the parse error's spanned `compile_error!` is the
    // error path for both stages (the parser validates everything the
    // emitter relies on), so the pipeline is one `and_then`, not a
    // match with an identity error arm.
    let expanded = catheo_parser::parse_pipeline(tokens)
        .and_then(|parsed| catheo_parser::emit_with_lowering(&parsed, lowering));
    match expanded {
        Ok(emitted) => emitted.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
