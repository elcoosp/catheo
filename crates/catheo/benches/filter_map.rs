//! The §6.1 v0.6 benchmark suite — the partial arrow, measured.
//!
//! ARCHITECTURE.md §4.3.3 and the §8 v0.6 row gate `filter_map` on the
//! benchmark protocol re-applied. This file is the measurement instrument
//! for the partial-arrow shapes; the deterministic consequences (emission
//! shape, three-way value agreement, allocation profile) live in
//! `tests/v06_gates.rs` and are kept in lockstep with these closures.
//!
//! The §6.1 three-way comparison, over two shapes:
//!
//! - **`vec_warm`** — the container shape: `Vec<i32>` → `filter_map:`
//!   `partial_double` → pure `fold`. `A_naive` is the pre-v0.6 riddle the
//!   stage retires: the partial arrow hand-rolled through the `bind`
//!   kernel, re-collecting the pointed effect into a `Vec` at every kept
//!   element. `B_idiomatic` is std's `.filter_map(f).fold(...)` chain —
//!   the baseline the macro must match. `C_macro` is the `pipeline!`
//!   output, which lowers to exactly that chain (the kernel *is* the B
//!   spelling, so C == B by construction; the group measures the proof).
//! - **`option_warm` / `option_cold`** — the pointwise shape: `filter_map`
//!   over `OptionWitness` *is* `and_then` (one discriminant, no heap).
//!   Warm keeps the value; cold drops it — the timed region is the
//!   discriminant test either way.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench filter_map
//! ```
use catheo::{OptionWitness, VecWitness};
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

/// The partial arrow of every universe (lockstep with `tests/v06_gates.rs`):
/// keep the evens, widen and double.
fn partial_double(x: i32) -> Option<i64> {
    if x % 2 == 0 {
        Some(i64::from(x) * 2)
    } else {
        None
    }
}

/// Flat source: a deterministic spread of odds and evens.
fn warm_input(size: usize) -> Vec<i32> {
    (0..size)
        .map(|i| i32::try_from(i % 977).unwrap_or(0))
        .collect()
}

// ─── The vec shape (lockstep with v06_gates.rs) ───────────────────────────

/// A: the pre-v0.6 riddle — the partial arrow through the `bind` kernel,
/// re-collecting the pointed effect into the witness container per element.
fn bind_riddle_naive(input: Vec<i32>) -> i64 {
    let bound = <VecWitness as catheo::CategoricalMonad>::run_bind(input, |x| {
        partial_double(x).into_iter().collect::<Vec<i64>>()
    });
    bound.into_iter().sum()
}

/// B: the idiomatic spelling — std's `filter_map` elimination (`sum` is
/// the idiomatic spelling of this fold).
fn filter_map_idiomatic(input: Vec<i32>) -> i64 {
    input.into_iter().filter_map(partial_double).sum::<i64>()
}

/// C: the macro-flattened output — the exact gated pipeline (the
/// callable-form fold: the grammar's eliminator, lint-silent in the
/// expanded code).
fn filter_map_macro(input: Vec<i32>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        filter_map: partial_double,
        fold: (0_i64, i64::wrapping_add),
    }
}

// ─── The pointwise shape ──────────────────────────────────────────────────

/// A: the naive spelling — every step is its own match.
#[expect(
    clippy::manual_map,
    reason = "A spells every step as its own match — that shape is the cost \
              the naive baseline represents, so the idiomatic `and_then` \
              spelling would not measure the naive universe."
)]
fn option_naive(input: Option<i32>) -> Option<i64> {
    match input {
        Some(x) => match partial_double(x) {
            Some(y) => Some(y),
            None => None,
        },
        None => None,
    }
}

/// B: the idiomatic spelling — `and_then` short-circuits identically.
fn option_idiomatic(input: Option<i32>) -> Option<i64> {
    input.and_then(partial_double)
}

/// C: the macro-flattened output — the partial arrow over the witness.
fn option_macro(input: Option<i32>) -> Option<i64> {
    catheo::pipeline! {
        witness: OptionWitness,
        source: input,
        filter_map: partial_double,
    }
}

// ─── The harness ──────────────────────────────────────────────────────────

/// One bench routine: run once, yield the `i64` observable.
type BenchRoutine = fn() -> i64;

/// The §6.1 three-way harness over the §6.1 sizes — the vec shape.
fn bench_vec_three_way(
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
            ("A_naive", bind_riddle_naive as fn(Vec<i32>) -> i64),
            ("B_idiomatic", filter_map_idiomatic),
            ("C_macro", filter_map_macro),
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

/// The pointwise universe has no size dimension: the container is a
/// zero-or-one-element value, so the timed unit is one program run (the
/// §6.1 element sizes would just repeat the same discriminant test).
fn bench_pointwise_three_way(
    mut group: BenchmarkGroup<'_, criterion::measurement::WallTime>,
    routines: [(&str, BenchRoutine); 3],
) {
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for (name, routine) in routines {
        group.bench_function(name, |b| {
            b.iter(|| black_box(routine()));
        });
    }
}

/// Warm keeps: `partial_double(20)` is `Some(40)`.
fn option_warm_naive() -> i64 {
    option_naive(Some(20_i32)).unwrap_or(-1)
}

fn option_warm_idiomatic() -> i64 {
    option_idiomatic(Some(20_i32)).unwrap_or(-1)
}

fn option_warm_macro() -> i64 {
    option_macro(Some(20_i32)).unwrap_or(-1)
}

/// The arrow drops: `partial_double(21)` is `None` — the timed region is
/// the discard.
fn option_cold_naive() -> i64 {
    option_naive(Some(21_i32)).unwrap_or(-1)
}

fn option_cold_idiomatic() -> i64 {
    option_idiomatic(Some(21_i32)).unwrap_or(-1)
}

fn option_cold_macro() -> i64 {
    option_macro(Some(21_i32)).unwrap_or(-1)
}

fn bench_filter_map_vec_warm(c: &mut Criterion) {
    bench_vec_three_way(c.benchmark_group("filter_map/vec_warm"), warm_input);
}

fn bench_filter_map_option_warm(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("filter_map/pointwise_option_warm"),
        [
            ("A_naive", option_warm_naive),
            ("B_idiomatic", option_warm_idiomatic),
            ("C_macro", option_warm_macro),
        ],
    );
}

fn bench_filter_map_option_cold(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("filter_map/pointwise_option_cold"),
        [
            ("A_naive", option_cold_naive),
            ("B_idiomatic", option_cold_idiomatic),
            ("C_macro", option_cold_macro),
        ],
    );
}

criterion_group!(
    benches,
    bench_filter_map_vec_warm,
    bench_filter_map_option_warm,
    bench_filter_map_option_cold,
);
criterion_main!(benches);
