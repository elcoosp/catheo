//! The §6.1 v0.3 benchmark suite — the traversal gate, measured.
//!
//! ARCHITECTURE.md §4.3 ships `traverse` in v0.3 and §8 gates it on the
//! same protocol as v0.2: "benchmark gates re-applied". This file is the
//! measurement instrument for the traversal shapes; the deterministic
//! consequences (emission shape, three-way value agreement, allocation
//! profile) live in `tests/v03_gates.rs` and are kept in lockstep with
//! these closures.
//!
//! The §6.1 three-way comparison, over `Vec<i32>` sources with a
//! `Result`-effect traversal (`i32 -> Result<u64, GateError>`):
//!
//! - **`A_naive`** — the naive spelling: the prefix map materializes its
//!   container, the traversal is a hand-rolled pushing loop with an early
//!   exit, and the sum folds over the collected container. The baseline
//!   the macro is meant to beat.
//! - **`B_idiomatic`** — the idiomatic chain: `.map(g).map(f).collect::<
//!   Result<Vec<u64>, _>>()` (std's short-circuiting `FromIterator`
//!   elimination), then the fold. The baseline the macro must match
//!   (the §8 v0.3 gate: **C ≥ B**).
//! - **`C_macro`** — the `pipeline!` macro-flattened output. The target:
//!   the fused lowering chains the prefix into the traversal's iterator
//!   and lowers the suffix fold inside the effect guard, so C is
//!   byte-for-byte B's machine shape.
//!
//! Two universes:
//!
//! - **warm** — every element passes the arrow; the traversal collects
//!   the full container and the guard's `Ok` arm runs the fold;
//! - **cold** — the first element fails, short-circuiting the traversal
//!   before any later element is forced; the guard's `Err` arm forwards
//!   the original error.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench traverse
//! ```

use catheo::{ResultWitness, VecWitness};
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

/// The error type of the traversal effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateError;

/// The traversal arrow shared by A, B, and C (lockstep with
/// `tests/v03_gates.rs`): `i32 -> Result<u64, _>`, rejecting negatives.
fn try_u64(x: i32) -> Result<u64, GateError> {
    u64::try_from(x).map_err(|_| GateError)
}

/// Warm universe: non-negative values, every element survives the arrow.
fn warm_input(size: usize) -> Vec<i32> {
    (0..size)
        .map(|i| i32::try_from(i % 977).unwrap_or(0))
        .collect()
}

/// Cold universe: every element is negative, so the traversal fails at the
/// first element and the guard forwards the error.
fn cold_input(size: usize) -> Vec<i32> {
    warm_input(size).into_iter().map(|x| x - 1_000).collect()
}

// ─── The three-way variants (lockstep with v03_gates.rs) ─────────────────

/// A: the naive spelling — every combinator materializes its container,
/// with a hand-rolled early-exit loop for the traversal. The by-value
/// signature is load-bearing: the body consumes the container with
/// `into_iter`, so the pass-by-value lint correctly stays silent.
fn traverse_naive(input: Vec<i32>) -> Result<u64, GateError> {
    let mapped: Vec<i32> = input.into_iter().map(|x| x.wrapping_mul(3)).collect();
    let mut collected: Vec<u64> = Vec::new();
    for x in mapped {
        collected.push(try_u64(x)?);
    }
    let mut total = 0_u64;
    for x in collected {
        total = total.wrapping_add(x);
    }
    Ok(total)
}

/// B: the idiomatic spelling — one adapter pass, std's short-circuiting
/// collect, then the fold over the collected container.
fn traverse_idiomatic(input: Vec<i32>) -> Result<u64, GateError> {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(try_u64)
        .collect::<Result<Vec<u64>, GateError>>()
        .map(|collected| collected.into_iter().fold(0_u64, u64::wrapping_add))
}

/// C: the macro-flattened output — the exact gated pipeline.
fn traverse_macro(input: Vec<i32>) -> Result<u64, GateError> {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(3),
        traverse: (ResultWitness<GateError>, try_u64),
        fold: (0_u64, u64::wrapping_add),
    }
}

/// The §6.1 three-way harness over the §6.1 sizes.
fn bench_three_way(
    mut group: BenchmarkGroup<'_, criterion::measurement::WallTime>,
    input_for: fn(usize) -> Vec<i32>,
) {
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for &size in &SIZES {
        group.throughput(Throughput::Elements(
            u64::try_from(size).expect("size fits u64"),
        ));
        let input = input_for(size);
        for (name, routine) in [
            (
                "A_naive",
                traverse_naive as fn(Vec<i32>) -> Result<u64, GateError>,
            ),
            ("B_idiomatic", traverse_idiomatic),
            ("C_macro", traverse_macro),
        ] {
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

fn bench_traverse_result_warm(c: &mut Criterion) {
    bench_three_way(c.benchmark_group("traverse/result_warm"), warm_input);
}

fn bench_traverse_result_cold(c: &mut Criterion) {
    bench_three_way(c.benchmark_group("traverse/result_cold"), cold_input);
}

criterion_group!(
    benches,
    bench_traverse_result_warm,
    bench_traverse_result_cold,
);
criterion_main!(benches);
