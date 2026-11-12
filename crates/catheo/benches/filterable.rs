//! The §6.1 strict `Filterable` benchmark suite — the partial-arrow-side
//! unification, measured.
//!
//! The §2.4 posture extended to the partial-arrow side, gated on the
//! benchmark protocol re-applied (the v0.5/v0.6 shape). This file is the
//! measurement instrument for the strict layer's universes; the
//! deterministic consequences (value agreement, allocation profile,
//! split-kernel parity) live in `tests/filterable_gates.rs` and are kept
//! in lockstep with these programs.
//!
//! The §6.1 three-way comparison, over two shapes:
//!
//! - **`vec_filter_map_warm`** — the container shape: `Vec<i32>` →
//!   partial arrow (`keep_half`, lockstep with the gates) → fold into an
//!   `i64`, 10k/100k/1M elements. `A_naive` is the two-barrier spelling —
//!   filter into one materialized container, map into a second. `B_idiomatic`
//!   is the lazy `filter_map` chain. `C_strict` is the generic program over
//!   the one `Filterable` bound — expected ≈ B after monomorphization (the
//!   bound erases into the kernel, which *is* B's elimination). The fused
//!   zero-intermediate story is the macro pass's (§4.3.3, `benches/filter_map.rs`);
//!   this universe measures the runtime layer's own question.
//! - **`pointwise_option_warm` / `pointwise_option_cold`** — the
//!   zero-or-one-element container, per call: on a pointwise witness the
//!   naive match spelling *is* the idiomatic `and_then` (the §6.1 M4
//!   posture), so all three routines are expected in the same
//!   discriminant-test band.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench filterable
//! ```
use catheo::{CategoricalIntoIter, Filterable, OptionWitness, VecWitness};
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

// ─── The lockstep programs (mirrored verbatim in tests/filterable_gates.rs) ─

/// The partial arrow of the universe: keep the evens, halve them.
fn keep_half(x: i32) -> Option<i64> {
    if x % 2 == 0 {
        Some(i64::from(x) / 2)
    } else {
        None
    }
}

/// C, the strict spelling: the partial arrow over ONE `Filterable` bound,
/// eliminated through the split into-iter kernel.
fn kept_total<W: Filterable + CategoricalIntoIter>(xs: W::Type<i32>) -> i64 {
    W::into_iter(W::filter_map(xs, keep_half)).sum()
}

/// A: the two-barrier spelling — filter into one materialized container,
/// map into a second.
fn kept_total_naive(xs: Vec<i32>) -> i64 {
    let evens: Vec<i32> = xs.into_iter().filter(|x| x % 2 == 0).collect();
    let halves: Vec<i64> = evens.into_iter().map(i64::from).map(|x| x / 2).collect();
    halves.into_iter().sum()
}

/// B: one lazy iterator pass, no intermediate container.
fn kept_total_idiomatic(xs: Vec<i32>) -> i64 {
    xs.into_iter().filter_map(keep_half).sum()
}

// ─── The container universe (§6.1 sizes) ──────────────────────────────────

/// The element shape: a deterministic spread over `i32` (the evens/odds
/// mix drives the partial arrow).
fn scalar_input(size: usize) -> Vec<i32> {
    (0..size)
        .map(|i| i32::try_from(i % 977).unwrap_or(0))
        .collect()
}

/// One bench routine over the container universe: run once, yield the
/// `i64`. The fn-pointer type is the coercion site that lets distinct fn
/// items share the routines array (fn items have unique types).
type VecRoutine = fn(Vec<i32>) -> i64;

fn bench_vec_filter_map_warm(c: &mut Criterion) {
    let mut group = c.benchmark_group("filterable/vec_filter_map_warm");
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for &size in &SIZES {
        group.throughput(Throughput::Elements(
            u64::try_from(size).expect("size fits u64"),
        ));
        let input = scalar_input(size);
        let routines: [(&str, VecRoutine); 3] = [
            ("A_naive", kept_total_naive),
            ("B_idiomatic", kept_total_idiomatic),
            ("C_strict", kept_total::<VecWitness>),
        ];
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
    group.finish();
}

// ─── The pointwise universe (per call) ────────────────────────────────────

/// A: the match-spelled partial arrow — on a zero-or-one-element
/// container this *is* the idiomatic elimination (the §6.1 M4 posture).
#[expect(
    clippy::manual_map,
    reason = "A spells the keep-or-drop as its own nested match — that \
              shape is the naive baseline the universe exists to measure, \
              so the idiomatic `and_then` spelling would not measure it. \
              The suppression is verifiable: the moment the match stops \
              matching the pattern, the expect errors."
)]
fn option_naive(input: Option<i32>) -> Option<i64> {
    match input {
        Some(x) => match keep_half(x) {
            Some(y) => Some(y),
            None => None,
        },
        None => None,
    }
}

/// B: the idiomatic spelling — one lazy `and_then`.
fn option_idiomatic(input: Option<i32>) -> Option<i64> {
    input.and_then(keep_half)
}

/// C: the generic program over the one `Filterable` bound.
fn option_strict(input: Option<i32>) -> Option<i64> {
    <OptionWitness as Filterable>::filter_map(input, keep_half)
}

/// One bench routine: run once, yield the `i64` observable (the monad
/// bench's per-call posture).
type BenchRoutine = fn() -> i64;

fn bench_pointwise(
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

fn bench_option_warm(c: &mut Criterion) {
    bench_pointwise(
        c.benchmark_group("filterable/pointwise_option_warm"),
        [
            ("A_naive", || option_naive(Some(84_i32)).unwrap_or(-1)),
            ("B_idiomatic", || {
                option_idiomatic(Some(84_i32)).unwrap_or(-1)
            }),
            ("C_strict", || option_strict(Some(84_i32)).unwrap_or(-1)),
        ],
    );
}

fn bench_option_cold(c: &mut Criterion) {
    bench_pointwise(
        c.benchmark_group("filterable/pointwise_option_cold"),
        [
            ("A_naive", || option_naive(None).unwrap_or(-1)),
            ("B_idiomatic", || option_idiomatic(None).unwrap_or(-1)),
            ("C_strict", || option_strict(None).unwrap_or(-1)),
        ],
    );
}

criterion_group!(
    benches,
    bench_vec_filter_map_warm,
    bench_option_warm,
    bench_option_cold,
);
criterion_main!(benches);
