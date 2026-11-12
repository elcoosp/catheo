//! The §6.1 strict `Foldable` benchmark suite — the eliminator-side
//! unification, measured.
//!
//! The §2.4 posture extended to the fold side, gated on the benchmark
//! protocol re-applied (the v0.5 shape). This file is the measurement
//! instrument for the strict layer's universes; the deterministic
//! consequences (value agreement, allocation profile, split-kernel
//! parity) live in `tests/foldable_gates.rs` and are kept in lockstep
//! with these programs.
//!
//! The §6.1 three-way comparison, over two shapes:
//!
//! - **`vec_fold_warm`** — the container eliminator: `Vec<i32>` →
//!   `fold` into an `i64`, 10k/100k/1M elements. `A_naive` is the
//!   two-pass spelling — map into a materialized intermediate, then fold
//!   over it. `B_idiomatic` is the lazy one-pass iterator chain. `C_strict`
//!   is the generic program over the one `Foldable` bound — expected ≈ B
//!   after monomorphization (the bound erases into the kernel, which *is*
//!   B's elimination).
//! - **`vec_fold_map_warm`** — the monoid eliminator: `Vec<String>` →
//!   `fold_map` into a `usize`. Same A/B/C protocol; the derived
//!   `fold_map` is pinned value-identical by the gates, and the wall-clock
//!   question is the same bound-erasure question.
//! - **`pointwise_option_warm` / `pointwise_option_cold`** and the
//!   `Result` pair — the zero-or-one-element containers, per call. On a
//!   pointwise witness the naive match spelling *is* the idiomatic
//!   elimination (the §6.1 M4 posture), so all three routines are
//!   expected in the same discriminant-test band.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench foldable
//! ```
use catheo::{Foldable, OptionWitness, ResultWitness, VecWitness};
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

// ─── The lockstep programs (mirrored verbatim in tests/foldable_gates.rs) ──

/// C, the strict spelling: the eliminator over ONE `Foldable` bound.
fn total<W: Foldable>(xs: W::Type<i32>) -> i64 {
    W::fold(xs, 0_i64, |acc, x| acc + i64::from(x))
}

/// A: the two-pass spelling — a materialized intermediate, then the fold.
fn total_naive(xs: Vec<i32>) -> i64 {
    let widened: Vec<i64> = xs.into_iter().map(i64::from).collect();
    widened.into_iter().sum()
}

/// B: one lazy iterator pass, no intermediate container.
fn total_idiomatic(xs: Vec<i32>) -> i64 {
    xs.into_iter().map(i64::from).sum()
}

/// C, the strict `fold_map`: the monoid eliminator over one bound.
fn letters<W: Foldable>(xs: W::Type<String>) -> usize {
    W::fold_map(xs, |word: String| word.len())
}

/// A: the two-pass `fold_map` — map into a materialized `Vec`, then sum.
fn letters_naive(xs: Vec<String>) -> usize {
    let lengths: Vec<usize> = xs.into_iter().map(|word: String| word.len()).collect();
    lengths.into_iter().sum()
}

/// B: the idiomatic one-pass chain.
fn letters_idiomatic(xs: Vec<String>) -> usize {
    xs.into_iter().map(|word: String| word.len()).sum()
}

// ─── The container universes (§6.1 sizes) ─────────────────────────────────

/// The element shape of the fold universe: a deterministic spread over
/// `i32`.
fn scalar_input(size: usize) -> Vec<i32> {
    (0..size)
        .map(|i| i32::try_from(i % 977).unwrap_or(0))
        .collect()
}

/// The element shape of the `fold_map` universe: deterministic words of
/// varying length, so the `usize` metric is non-trivial.
fn words_input(size: usize) -> Vec<String> {
    (0..size)
        .map(|i| {
            let n = i % 977;
            format!("word{n:04}")
        })
        .collect()
}

/// One bench routine over the container universes.
type ThreeWay<T> = [(&'static str, fn(Vec<T>) -> i64); 3];

fn bench_three_way<T: Clone + 'static>(
    mut group: BenchmarkGroup<'_, criterion::measurement::WallTime>,
    input_of: fn(usize) -> Vec<T>,
    routines: ThreeWay<T>,
) {
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for &size in &SIZES {
        group.throughput(Throughput::Elements(
            u64::try_from(size).expect("size fits u64"),
        ));
        let input = input_of(size);
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

fn bench_vec_fold_warm(c: &mut Criterion) {
    bench_three_way(
        c.benchmark_group("foldable/vec_fold_warm"),
        scalar_input,
        [
            ("A_naive", total_naive),
            ("B_idiomatic", total_idiomatic),
            ("C_strict", total::<VecWitness>),
        ],
    );
}

fn bench_vec_fold_map_warm(c: &mut Criterion) {
    // The `fold_map` universe yields a `usize`; the harness normalizes to
    // `i64` for the shared runner (the values stay well under `i64::MAX`).
    fn a_naive(xs: Vec<String>) -> i64 {
        i64::try_from(letters_naive(xs)).unwrap_or(i64::MAX)
    }
    fn b_idiomatic(xs: Vec<String>) -> i64 {
        i64::try_from(letters_idiomatic(xs)).unwrap_or(i64::MAX)
    }
    fn c_strict(xs: Vec<String>) -> i64 {
        i64::try_from(letters::<VecWitness>(xs)).unwrap_or(i64::MAX)
    }
    bench_three_way(
        c.benchmark_group("foldable/vec_fold_map_warm"),
        words_input,
        [
            ("A_naive", a_naive),
            ("B_idiomatic", b_idiomatic),
            ("C_strict", c_strict),
        ],
    );
}

// ─── The pointwise universes (per call) ───────────────────────────────────

/// A: the match-spelled eliminator — on a zero-or-one-element container
/// this *is* the idiomatic elimination (the §6.1 M4 posture).
fn option_naive(input: Option<i32>) -> i64 {
    match input {
        Some(x) => i64::from(x),
        None => 0,
    }
}

/// B: the idiomatic spelling — one lazy `map_or`.
fn option_idiomatic(input: Option<i32>) -> i64 {
    input.map_or(0, i64::from)
}

/// C: the generic program over the one `Foldable` bound.
fn option_strict(input: Option<i32>) -> i64 {
    <OptionWitness as Foldable>::fold(input, 0_i64, |acc, x| acc + i64::from(x))
}

/// A, over `Result`: the match-spelled eliminator; `Err` contributes the
/// identity.
fn result_naive(input: &Result<i32, String>) -> i64 {
    match *input {
        Ok(x) => i64::from(x),
        Err(_) => 0,
    }
}

/// B: the idiomatic spelling — one lazy `map_or_else`.
fn result_idiomatic(input: Result<i32, String>) -> i64 {
    input.map_or_else(|_| 0, i64::from)
}

/// C: the generic program over the one `Foldable` bound.
fn result_strict(input: Result<i32, String>) -> i64 {
    <ResultWitness<String> as Foldable>::fold(input, 0_i64, |acc, x| acc + i64::from(x))
}

/// One bench routine: run once, yield the `i64` observable (the monad
/// bench's per-call posture — the pointwise container has no size
/// dimension).
type BenchRoutine = fn() -> i64;

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

fn bench_option_warm(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("foldable/pointwise_option_warm"),
        [
            ("A_naive", || option_naive(Some(21_i32))),
            ("B_idiomatic", || option_idiomatic(Some(21_i32))),
            ("C_strict", || option_strict(Some(21_i32))),
        ],
    );
}

fn bench_option_cold(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("foldable/pointwise_option_cold"),
        [
            ("A_naive", || option_naive(None)),
            ("B_idiomatic", || option_idiomatic(None)),
            ("C_strict", || option_strict(None)),
        ],
    );
}

fn bench_result_warm(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("foldable/pointwise_result_warm"),
        [
            ("A_naive", || result_naive(&Ok(21_i32))),
            ("B_idiomatic", || result_idiomatic(Ok(21_i32))),
            ("C_strict", || result_strict(Ok(21_i32))),
        ],
    );
}

fn bench_result_cold(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("foldable/pointwise_result_cold"),
        [
            ("A_naive", || result_naive(&Err("cold".to_owned()))),
            ("B_idiomatic", || result_idiomatic(Err("cold".to_owned()))),
            ("C_strict", || result_strict(Err("cold".to_owned()))),
        ],
    );
}

criterion_group!(
    benches,
    bench_vec_fold_warm,
    bench_vec_fold_map_warm,
    bench_option_warm,
    bench_option_cold,
    bench_result_warm,
    bench_result_cold,
);
criterion_main!(benches);
