//! The §6.1 v0.4 benchmark suite — the join gate, measured.
//!
//! ARCHITECTURE.md §4.3 lists `join` among the combinators, and the §8
//! roadmap gates every milestone on "benchmark gates re-applied". This file
//! is the measurement instrument for the join shapes; the deterministic
//! consequences (emission shape, three-way value agreement, allocation
//! profile) live in `tests/v04_gates.rs` and are kept in lockstep with
//! these closures.
//!
//! The §6.1 three-way comparison. The identity never rejects, so the join
//! has one universe (warm) — unlike `traverse`, there is no cold
//! short-circuit to measure. Two shapes:
//!
//! - **`flatten`** — the map-prefix shape: `Vec<i32>` →
//!   `map: |x| vec![x, x]` → `join` → fold. `A_naive` materializes the
//!   nested container and the flattened one; `B_idiomatic` is the
//!   `.map(f).flat_map(id).fold(...)` chain (the baseline the macro must
//!   match); `C_macro` is the `pipeline!` output.
//! - **`nested_source`** — the pure shape: `Vec<Vec<i32>>` source, `join`
//!   as the only stage, `collect` terminal. B is
//!   `.into_iter().flat_map(id).collect()`.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench join
//! ```

use catheo::VecWitness;
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

/// Flat source shared by both shapes.
fn warm_input(size: usize) -> Vec<i32> {
    (0..size)
        .map(|i| i32::try_from(i % 977).unwrap_or(0))
        .collect()
}

/// Nested source: pairs of elements, the canonical `W<W<T>>` input.
fn nested_input(size: usize) -> Vec<Vec<i32>> {
    warm_input(size).chunks(2).map(<[i32]>::to_vec).collect()
}

// ─── The flatten shape (lockstep with v04_gates.rs) ──────────────────────

/// A: the naive spelling — every combinator materializes its container.
/// The by-value signature is load-bearing: the body consumes the container
/// with `into_iter`, so the pass-by-value lint correctly stays silent.
fn flatten_naive(input: Vec<i32>) -> i64 {
    let mapped: Vec<Vec<i32>> = input.into_iter().map(|x| vec![x, x]).collect();
    let mut flattened: Vec<i32> = Vec::new();
    for inner in mapped {
        flattened.extend(inner);
    }
    let mut total = 0_i64;
    for x in flattened {
        total += i64::from(x);
    }
    total
}

/// B: the idiomatic spelling — one adapter pass, no intermediate container.
fn flatten_idiomatic(input: Vec<i32>) -> i64 {
    input
        .into_iter()
        .flat_map(|x| vec![x, x])
        .fold(0_i64, |acc, x| acc + i64::from(x))
}

/// C: the macro-flattened output — the exact gated pipeline.
fn flatten_macro(input: Vec<i32>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| vec![x, x],
        join,
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    }
}

// ─── The nested-source shape ──────────────────────────────────────────────

/// A: the naive spelling — a hand-rolled pushing loop over the nesting.
fn nested_naive(input: Vec<Vec<i32>>) -> Vec<i32> {
    let mut flattened: Vec<i32> = Vec::new();
    for inner in input {
        for x in inner {
            flattened.push(x);
        }
    }
    flattened
}

/// B: the idiomatic spelling — std's `flat_map` elimination.
fn nested_idiomatic(input: Vec<Vec<i32>>) -> Vec<i32> {
    input.into_iter().flatten().collect()
}

/// C: the macro-flattened output — the join is the whole chain.
fn nested_macro(input: Vec<Vec<i32>>) -> Vec<i32> {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        join,
        collect,
    }
}

/// The §6.1 three-way harness over the §6.1 sizes — the flatten shape.
fn bench_flatten_three_way(
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
            ("A_naive", flatten_naive as fn(Vec<i32>) -> i64),
            ("B_idiomatic", flatten_idiomatic),
            ("C_macro", flatten_macro),
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

/// The §6.1 three-way harness over the §6.1 sizes — the nested shape.
fn bench_nested_three_way(
    mut group: BenchmarkGroup<'_, criterion::measurement::WallTime>,
    input_for: fn(usize) -> Vec<Vec<i32>>,
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
            ("A_naive", nested_naive as fn(Vec<Vec<i32>>) -> Vec<i32>),
            ("B_idiomatic", nested_idiomatic),
            ("C_macro", nested_macro),
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

fn bench_join_flatten_warm(c: &mut Criterion) {
    bench_flatten_three_way(c.benchmark_group("join/flatten_warm"), warm_input);
}

fn bench_join_nested_source(c: &mut Criterion) {
    bench_nested_three_way(c.benchmark_group("join/nested_source"), nested_input);
}

criterion_group!(benches, bench_join_flatten_warm, bench_join_nested_source,);
criterion_main!(benches);
