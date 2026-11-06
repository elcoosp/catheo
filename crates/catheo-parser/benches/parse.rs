//! Criterion benchmarks for the macro-compiler front end (§6.1, §6.3).
//!
//! The expansion budget of §6.3 (≤ 5 ms per pipeline, excluding the macro
//! crate's own compilation) is what M0's gate measures: `parse` + `emit`
//! are the entire macro-attributable cost of an expansion in M0 — there is
//! no deforestation yet (M2), so these numbers are the *floor* the later
//! passes must stay within.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench parse
//! ```

use catheo_parser::{emit, parse_pipeline};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use quote::quote;
use std::hint::black_box;
use std::time::Duration;

/// Build a synthetic pipeline with `stages` map stages plus a fold terminal.
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

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    for stages in [4, 16, 64, 256] {
        let tokens = synthetic_pipeline(stages);
        group.bench_with_input(BenchmarkId::new("stages", stages), &tokens, |b, tokens| {
            b.iter(|| parse_pipeline(black_box(tokens.clone())).expect("parses"));
        });
    }
    group.finish();
}

fn bench_expansion_parse_plus_emit(c: &mut Criterion) {
    let mut group = c.benchmark_group("expansion");
    group.measurement_time(Duration::from_secs(3));
    for stages in [4, 16, 64, 256] {
        let tokens = synthetic_pipeline(stages);
        group.bench_with_input(BenchmarkId::new("stages", stages), &tokens, |b, tokens| {
            b.iter(|| {
                let pipeline = parse_pipeline(black_box(tokens.clone())).expect("parses");
                emit(black_box(&pipeline)).expect("emits")
            });
        });
    }
    group.finish();
}

/// §6.3 budget gate, asserted as a normal test so a regression fails CI
/// deterministically rather than hiding in a dashboard. The real budget is
/// 5 ms; we fail at 50 ms to stay far from scheduler noise while still
/// catching order-of-magnitude blowups.
#[test]
fn expansion_budget_gate() {
    for stages in [4, 16, 64, 256, 1024] {
        let tokens = synthetic_pipeline(stages);
        let start = std::time::Instant::now();
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

criterion_group!(benches, bench_parse, bench_expansion_parse_plus_emit);
criterion_main!(benches);
