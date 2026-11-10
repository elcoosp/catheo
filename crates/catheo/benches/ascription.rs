//! The §6.1 head-anchor benchmark suite — the §4.2 ascription, measured.
//!
//! The ascription surface is published and demonstrated
//! (`examples/ascribed_witnesses.rs`,
//! `tests/consumer_examples.rs::the_ascribed_witness_demo_derives_every_builtin_witness`);
//! this file is its measurement instrument, per the §6.1 three-way
//! protocol. The question it answers is the one §4.2 raises at the head
//! anchor: does the derived witness cost anything at runtime?
//!
//! One container universe over the flagship fused shape — `Vec<i32>`
//! doubled, the multiples of four kept (the partial arrow), folded into an
//! `i64`, 10k/100k/1M elements, warm:
//!
//! - **`A_naive`** — the per-stage materializing spelling the macro is
//!   meant to beat: one intermediate `Vec` per stage.
//! - **`B_idiomatic`** — the std chain the macro must match.
//! - **`C_macro_witness`** — the pipeline with the head anchored by the
//!   `witness:` entry (§4.1).
//! - **`C_macro_ascribed`** — the same pipeline with the head derived from
//!   a `source: input as Vec<i32>` ascription (§4.2).
//!
//! Expected verdict: **`C_macro_witness` == `C_macro_ascribed`** — the
//! derivation is compile-time only, so the two expansions are
//! token-identical modulo the head binding's `: Type` annotation (the
//! user's spelled type re-anchors the expression, §4.2) — and both ≈ B
//! (the kernels *are* the std spellings; the §6.1 filter_map record),
//! both > A. The deterministic consequences (allocation profiles,
//! fused-emission shape) stay with the gate suites (`tests/v02_gates.rs`,
//! §6.1 posture: wall-clock is never the CI gate).
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench ascription
//! ```
use catheo::VecWitness;
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

/// The element shape: a deterministic spread over `i32`.
fn scalar_input(size: usize) -> Vec<i32> {
    (0..size)
        .map(|i| i32::try_from(i % 977).unwrap_or(0))
        .collect()
}

// ─── The four routines ────────────────────────────────────────────────────

/// A: the naive spelling — one materialized container per stage.
fn naive(input: Vec<i32>) -> i64 {
    let doubled: Vec<i64> = input.iter().map(|x| i64::from(*x) * 2).collect();
    let kept: Vec<i64> = doubled.into_iter().filter(|x| x % 4 == 0).collect();
    kept.into_iter().fold(0_i64, |acc, x| acc + x)
}

/// B: the idiomatic spelling — the std chain.
fn idiomatic(input: Vec<i32>) -> i64 {
    input
        .into_iter()
        .map(|x| i64::from(x) * 2)
        .filter(|x| x % 4 == 0)
        .sum::<i64>()
}

/// C, witness-spelled: the head anchored by the `witness:` entry (§4.1).
fn macro_witness(input: Vec<i32>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| i64::from(x) * 2,
        filter_map: |x| if x % 4 == 0 { Some(x) } else { None },
        fold: (0_i64, |acc, x| acc + x),
    }
}

/// C, ascribed: the head derived from the §4.2 ascription — the only
/// written difference is the `as Vec<i32>` suffix and the absent
/// `witness:` entry.
fn macro_ascribed(input: Vec<i32>) -> i64 {
    catheo::pipeline! {
        source: input as Vec<i32>,
        map: |x| i64::from(x) * 2,
        filter_map: |x| if x % 4 == 0 { Some(x) } else { None },
        fold: (0_i64, |acc, x| acc + x),
    }
}

// ─── The harness (§6.1 protocol) ──────────────────────────────────────────

/// One bench routine: run once over the scalar input, yield the `i64`.
type BenchRoutine = fn(Vec<i32>) -> i64;

/// The §6.1 four-way harness over the §6.1 sizes.
fn bench_head_anchor(
    mut group: BenchmarkGroup<'_, criterion::measurement::WallTime>,
    routines: [(&str, BenchRoutine); 4],
) {
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for &size in &SIZES {
        group.throughput(Throughput::Elements(
            u64::try_from(size).expect("size fits u64"),
        ));
        let input = scalar_input(size);
        for (name, routine) in routines {
            group.bench_with_input(BenchmarkId::new(name, size), &input, |b, input| {
                b.iter_batched(
                    || input.clone(),
                    |input| black_box(routine(input)),
                    BatchSize::PerIteration,
                );
            });
        }
    }
}

fn bench_ascription_vec_warm(c: &mut Criterion) {
    bench_head_anchor(
        c.benchmark_group("ascription/vec_warm"),
        [
            ("A_naive", naive),
            ("B_idiomatic", idiomatic),
            ("C_macro_witness", macro_witness),
            ("C_macro_ascribed", macro_ascribed),
        ],
    );
}

criterion_group!(benches, bench_ascription_vec_warm);
criterion_main!(benches);
