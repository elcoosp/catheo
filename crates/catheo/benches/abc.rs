//! The §6.1 three-way comparison suite (M3) — the decision-gate instrument.
//!
//! For every representative `Vec` pipeline shape (map-heavy, fold-heavy,
//! filter chain, collect), on `Vec<i32>` and `Vec<String>`, at 10k / 100k /
//! 1M elements, this suite measures:
//!
//! - **`A_naive`** — naive nested closures: every stage materializes its
//!   own container. The baseline the macro is meant to beat.
//! - **`B_idiomatic`** — one idiomatic iterator chain. The baseline the
//!   macro must match.
//! - **`C_macro`** — the `pipeline!` macro-flattened output. The target.
//!
//! The §6.1 verdict — *C ≥ B on all pipelines; C > A on at least the
//! map-heavy and fold-heavy pipelines* — is read from this suite's
//! criterion output and recorded in docs/ARCHITECTURE.md §6.1. The
//! deterministic, CI-enforceable form of the same claim lives in
//! `tests/abc_gates.rs` (value agreement + allocation gates); the two are
//! kept in lockstep: identical shapes, identical closures.
//!
//! The published outcome on this toolchain (see §6.1 for the measured
//! records): C ≈ B on every barrier-free shape, C < A on
//! map-heavy, fold-heavy and collect (allocation-free vs materializing),
//! and on the filter chain the barrier's documented cost is *temporal* —
//! extra passes over the in-place-materialized kernel state — with
//! allocation parity against B intact (pinned by `tests/abc_gates.rs`).
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench abc
//! ```

use catheo::VecWitness;
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

// ─── Deterministic inputs ───────────────────────────────────────────────

/// Deterministic xorshift64\* (the repo's dependency-free input idiom).
struct XorShift(u64);

impl XorShift {
    const SEED: u64 = 0x5EED_1A7E_5EED_1A7E;

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Mixed-sign workload values, bounded well inside `i32` so the
/// `wrapping_*` ops never depend on two's-complement wraparound for
/// correctness.
fn i32_input(size: usize) -> Vec<i32> {
    let mut rng = XorShift(XorShift::SEED);
    (0..size)
        .map(|_| i32::try_from(rng.next_u64() % 2_000).unwrap_or(0) - 1_000)
        .collect()
}

/// ASCII words of length 5..=9 (mixed parity, so the filter shape keeps a
/// realistic fraction), deterministic per index.
fn string_input(size: usize) -> Vec<String> {
    (0..size)
        .map(|i| format!("{}{}", "w".repeat(4 + i % 5), i % 10))
        .collect()
}

// ─── Universe 1: Vec<i32> — variants in lockstep with abc_gates.rs ──────

/// A: every stage materializes its own container. Stages borrow (`iter`)
/// and collect fresh containers — `into_iter` chains are *not* naive on
/// rustc 1.99, std reuses the owned source buffer in place.
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn map_heavy_i32_naive(input: Vec<i32>) -> i64 {
    let step_1: Vec<i32> = input.iter().map(|x| x.wrapping_mul(3)).collect();
    let step_2: Vec<i32> = step_1.iter().map(|x| x.wrapping_add(7)).collect();
    let step_3: Vec<i32> = step_2.iter().map(|x| x.wrapping_mul(5)).collect();
    step_3.into_iter().fold(0_i64, |acc, x| acc + i64::from(x))
}

/// B: one iterator pass, no intermediate containers.
fn map_heavy_i32_idiomatic(input: Vec<i32>) -> i64 {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(|x| x.wrapping_add(7))
        .map(|x| x.wrapping_mul(5))
        .fold(0_i64, |acc, x| acc + i64::from(x))
}

/// C: the macro-flattened output.
fn map_heavy_i32_macro(input: Vec<i32>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(3),
        map: |x| x.wrapping_add(7),
        map: |x| x.wrapping_mul(5),
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    }
}

/// The multi-accumulator fold state: (sum, min, max, positive count).
type I32Stats = (i64, i32, i32, u64);

/// The per-element fold step, shared by all three fold-heavy variants.
fn fold_heavy_i32_step(acc: I32Stats, x: i32) -> I32Stats {
    (
        acc.0.wrapping_add(i64::from(x)),
        acc.1.min(x),
        acc.2.max(x),
        acc.3.wrapping_add(u64::from(x.is_positive())),
    )
}

/// A: the map stage materializes, then the fold consumes the copy.
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn fold_heavy_i32_naive(input: Vec<i32>) -> I32Stats {
    let mapped: Vec<i32> = input.iter().map(|x| x.wrapping_mul(2)).collect();
    mapped
        .into_iter()
        .fold((0_i64, i32::MAX, i32::MIN, 0_u64), fold_heavy_i32_step)
}

/// B: one iterator pass.
fn fold_heavy_i32_idiomatic(input: Vec<i32>) -> I32Stats {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(2))
        .fold((0_i64, i32::MAX, i32::MIN, 0_u64), fold_heavy_i32_step)
}

/// C: the macro-flattened output.
fn fold_heavy_i32_macro(input: Vec<i32>) -> I32Stats {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(2),
        fold: (
            (0_i64, i32::MAX, i32::MIN, 0_u64),
            |acc: I32Stats, x| fold_heavy_i32_step(acc, x)
        ),
    }
}

/// A: every stage materializes — including both sides of the barrier.
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn filter_chain_i32_naive(input: Vec<i32>) -> i64 {
    let mapped: Vec<i32> = input.iter().map(|x| x.wrapping_mul(3)).collect();
    let filtered: Vec<i32> = mapped.iter().copied().filter(|x| x % 2 == 0).collect();
    let shifted: Vec<i32> = filtered.iter().map(|x| x.wrapping_add(1)).collect();
    shifted.into_iter().fold(0_i64, |acc, x| acc + i64::from(x))
}

/// B: one iterator pass.
fn filter_chain_i32_idiomatic(input: Vec<i32>) -> i64 {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .filter(|x| x % 2 == 0)
        .map(|x| x.wrapping_add(1))
        .fold(0_i64, |acc, x| acc + i64::from(x))
}

/// C: the macro-flattened output — the filter stays a kernel barrier.
fn filter_chain_i32_macro(input: Vec<i32>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(3),
        filter: |x| x % 2 == 0,
        map: |x| x.wrapping_add(1),
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    }
}

/// A: materialize each map, return the last.
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn collect_i32_naive(input: Vec<i32>) -> Vec<i32> {
    let mapped: Vec<i32> = input.iter().map(|x| x.wrapping_mul(2)).collect();
    let shifted: Vec<i32> = mapped.iter().map(|x| x.wrapping_add(3)).collect();
    shifted
}

/// B: one adapter chain into `collect`.
fn collect_i32_idiomatic(input: Vec<i32>) -> Vec<i32> {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(2))
        .map(|x| x.wrapping_add(3))
        .collect()
}

/// C: the macro-flattened output.
fn collect_i32_macro(input: Vec<i32>) -> Vec<i32> {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(2),
        map: |x| x.wrapping_add(3),
        collect,
    }
}

// ─── Universe 2: Vec<String> — variants in lockstep with abc_gates.rs ───

/// A: per-stage materialization (borrowing stages, fresh containers).
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn map_heavy_str_naive(input: Vec<String>) -> usize {
    let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
    let marked: Vec<String> = shouted.iter().map(|s| format!("{s}!")).collect();
    marked.into_iter().fold(0_usize, |acc, s| acc + s.len())
}

/// B: one iterator pass.
fn map_heavy_str_idiomatic(input: Vec<String>) -> usize {
    input
        .into_iter()
        .map(|s| s.to_ascii_uppercase())
        .map(|s| format!("{s}!"))
        .fold(0_usize, |acc, s| acc + s.len())
}

/// C: the macro-flattened output.
fn map_heavy_str_macro(input: Vec<String>) -> usize {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |s| s.to_ascii_uppercase(),
        map: |s| format!("{s}!"),
        fold: (0_usize, |acc, s| acc + s.len()),
    }
}

/// The multi-accumulator fold state: (total length, longest, count > 3).
type StrStats = (usize, usize, u64);

/// The per-element fold step, shared by all three variants.
fn fold_heavy_str_step(acc: StrStats, s: &str) -> StrStats {
    (
        acc.0.wrapping_add(s.len()),
        acc.1.max(s.len()),
        acc.2.wrapping_add(u64::from(s.len() > 3)),
    )
}

/// A: the map stage materializes.
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn fold_heavy_str_naive(input: Vec<String>) -> StrStats {
    let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
    shouted
        .into_iter()
        .fold((0, 0, 0), |acc, s| fold_heavy_str_step(acc, &s))
}

/// B: one iterator pass.
fn fold_heavy_str_idiomatic(input: Vec<String>) -> StrStats {
    input
        .into_iter()
        .map(|s| s.to_ascii_uppercase())
        .fold((0, 0, 0), |acc, s| fold_heavy_str_step(acc, &s))
}

/// C: the macro-flattened output.
fn fold_heavy_str_macro(input: Vec<String>) -> StrStats {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |s| s.to_ascii_uppercase(),
        fold: ((0_usize, 0_usize, 0_u64), |acc: StrStats, s| {
            fold_heavy_str_step(acc, &s)
        }),
    }
}

/// A: both sides of the barrier materialize (borrowing map stages).
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn filter_chain_str_naive(input: Vec<String>) -> usize {
    let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
    let filtered: Vec<String> = shouted.into_iter().filter(|s| s.len() % 2 == 0).collect();
    let marked: Vec<String> = filtered.iter().map(|s| format!("{s}?")).collect();
    marked.into_iter().fold(0_usize, |acc, s| acc + s.len())
}

/// B: one iterator pass.
fn filter_chain_str_idiomatic(input: Vec<String>) -> usize {
    input
        .into_iter()
        .map(|s| s.to_ascii_uppercase())
        .filter(|s| s.len() % 2 == 0)
        .map(|s| format!("{s}?"))
        .fold(0_usize, |acc, s| acc + s.len())
}

/// C: the macro-flattened output.
fn filter_chain_str_macro(input: Vec<String>) -> usize {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |s| s.to_ascii_uppercase(),
        filter: |s| s.len() % 2 == 0,
        map: |s| format!("{s}?"),
        fold: (0_usize, |acc, s| acc + s.len()),
    }
}

/// A: materialize each map, return the last.
#[expect(
    clippy::needless_pass_by_value,
    reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
)]
fn collect_str_naive(input: Vec<String>) -> Vec<String> {
    let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
    let marked: Vec<String> = shouted.iter().map(|s| format!("{s}?")).collect();
    marked
}

/// B: one adapter chain into `collect`.
fn collect_str_idiomatic(input: Vec<String>) -> Vec<String> {
    input
        .into_iter()
        .map(|s| s.to_ascii_uppercase())
        .map(|s| format!("{s}?"))
        .collect()
}

/// C: the macro-flattened output.
fn collect_str_macro(input: Vec<String>) -> Vec<String> {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |s| s.to_ascii_uppercase(),
        map: |s| format!("{s}?"),
        collect,
    }
}

// ─── The harness: one group per (shape, universe), three variants each ──

/// The three variants of one shape: `("A_naive", …)`, `("B_idiomatic", …)`,
/// `("C_macro", …)`.
type Variants<T, R> = [(&'static str, fn(T) -> R); 3];

/// Run the A/B/C trio for one shape/universe across the §6.1 sizes.
///
/// `iter_batched` keeps the input clone in the *setup* phase, so the
/// measured routine starts from an owned container exactly like a real
/// call site would hand one over.
fn bench_three_way<T, R>(
    mut group: BenchmarkGroup<'_, criterion::measurement::WallTime>,
    input_for: fn(usize) -> T,
    variants: Variants<T, R>,
) where
    T: Clone + 'static,
{
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for &size in &SIZES {
        group.throughput(Throughput::Elements(
            u64::try_from(size).expect("size fits u64"),
        ));
        let input = input_for(size);
        for (name, routine) in variants {
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

fn bench_map_heavy_i32(c: &mut Criterion) {
    let group = c.benchmark_group("abc/map_heavy_i32");
    bench_three_way(
        group,
        i32_input,
        [
            ("A_naive", map_heavy_i32_naive),
            ("B_idiomatic", map_heavy_i32_idiomatic),
            ("C_macro", map_heavy_i32_macro),
        ],
    );
}

fn bench_fold_heavy_i32(c: &mut Criterion) {
    let group = c.benchmark_group("abc/fold_heavy_i32");
    bench_three_way(
        group,
        i32_input,
        [
            ("A_naive", fold_heavy_i32_naive),
            ("B_idiomatic", fold_heavy_i32_idiomatic),
            ("C_macro", fold_heavy_i32_macro),
        ],
    );
}

fn bench_filter_chain_i32(c: &mut Criterion) {
    let group = c.benchmark_group("abc/filter_chain_i32");
    bench_three_way(
        group,
        i32_input,
        [
            ("A_naive", filter_chain_i32_naive),
            ("B_idiomatic", filter_chain_i32_idiomatic),
            ("C_macro", filter_chain_i32_macro),
        ],
    );
}

fn bench_collect_i32(c: &mut Criterion) {
    let group = c.benchmark_group("abc/collect_i32");
    bench_three_way(
        group,
        i32_input,
        [
            ("A_naive", collect_i32_naive),
            ("B_idiomatic", collect_i32_idiomatic),
            ("C_macro", collect_i32_macro),
        ],
    );
}

fn bench_map_heavy_str(c: &mut Criterion) {
    let group = c.benchmark_group("abc/map_heavy_str");
    bench_three_way(
        group,
        string_input,
        [
            ("A_naive", map_heavy_str_naive),
            ("B_idiomatic", map_heavy_str_idiomatic),
            ("C_macro", map_heavy_str_macro),
        ],
    );
}

fn bench_fold_heavy_str(c: &mut Criterion) {
    let group = c.benchmark_group("abc/fold_heavy_str");
    bench_three_way(
        group,
        string_input,
        [
            ("A_naive", fold_heavy_str_naive),
            ("B_idiomatic", fold_heavy_str_idiomatic),
            ("C_macro", fold_heavy_str_macro),
        ],
    );
}

fn bench_filter_chain_str(c: &mut Criterion) {
    let group = c.benchmark_group("abc/filter_chain_str");
    bench_three_way(
        group,
        string_input,
        [
            ("A_naive", filter_chain_str_naive),
            ("B_idiomatic", filter_chain_str_idiomatic),
            ("C_macro", filter_chain_str_macro),
        ],
    );
}

fn bench_collect_str(c: &mut Criterion) {
    let group = c.benchmark_group("abc/collect_str");
    bench_three_way(
        group,
        string_input,
        [
            ("A_naive", collect_str_naive),
            ("B_idiomatic", collect_str_idiomatic),
            ("C_macro", collect_str_macro),
        ],
    );
}

criterion_group!(
    benches,
    bench_map_heavy_i32,
    bench_fold_heavy_i32,
    bench_filter_chain_i32,
    bench_collect_i32,
    bench_map_heavy_str,
    bench_fold_heavy_str,
    bench_filter_chain_str,
    bench_collect_str,
);
criterion_main!(benches);
