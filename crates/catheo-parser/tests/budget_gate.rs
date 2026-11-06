//! The §6.3 expansion-budget gate, as a real integration test.
//!
//! This gate originally lived as a `#[test]` inside the `harness = false`
//! bench target — which never executes `#[test]`s, so the gate silently
//! stopped running. It lives here instead, where every plain
//! `cargo test --workspace` (and therefore the CI `test` job) enforces it.
//!
//! The real budget is 5 ms per expansion (§6.3); we fail at 50 ms to stay
//! far from scheduler noise while still catching order-of-magnitude blowups.

use catheo_parser::{emit, parse_pipeline};
use quote::quote;
use std::time::{Duration, Instant};

/// Build a synthetic pipeline with `stages` map stages plus a fold terminal
/// (kept in lockstep with `benches/parse.rs`).
fn synthetic_pipeline(stages: usize) -> proc_macro2::TokenStream {
    let mut entries = quote! {
        witness: VecWitness,
        source: input_vector,
    };
    for _ in 0..stages {
        entries.extend(quote! { map: |x| x + 1, });
    }
    entries.extend(quote! { fold: (0_i64, |acc, x| acc + x) });
    entries
}

#[test]
fn expansion_budget_gate() {
    for stages in [4, 16, 64, 256, 1024] {
        let tokens = synthetic_pipeline(stages);
        let start = Instant::now();
        let pipeline = parse_pipeline(tokens).expect("parses");
        let _expanded = emit(&pipeline).expect("emits");
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(50),
            "expansion of a {stages}-stage pipeline took {elapsed:?}, \
             blowing past the §6.3 budget sanity ceiling (50 ms)"
        );
    }
}
