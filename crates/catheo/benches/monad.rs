//! The §6.1 v0.5 benchmark suite — the strict `Monad` layer, measured.
//!
//! ARCHITECTURE.md §4.4 and the §8 v0.5 row gate the one-trait surface on
//! the benchmark protocol re-applied. This file is the measurement
//! instrument for the strict layer's pointwise universes; the deterministic
//! consequences (value agreement, allocation profile, split-kernel parity)
//! live in `tests/v05_gates.rs` and are kept in lockstep with these
//! closures.
//!
//! The §6.1 three-way comparison, over the pointwise witnesses — the
//! universe where the strict layer *is* the idiomatic elimination (one
//! discriminant test, no container to build):
//!
//! - **`pointwise_option_warm` / `pointwise_option_cold`** —
//!   `checked_doubling` over `Option<i32>` (`Some(21)` / `None`); `A_naive`
//!   spells every step as its own match, `B_idiomatic` is the lazy `.map`
//!   chain, `C_strict` is the generic program over the one `Monad` bound.
//! - **`pointwise_result_warm` / `pointwise_result_cold`** — the same
//!   program over `Result<i32, String>` with a fallible arrow (warm: every
//!   element passes; cold: the arrow rejects the value and the
//!   short-circuit is the timed region).
//!
//! The eager `Vec` shape is deliberately not a wall-clock group here: each
//! runtime `bind` collects its container and each `pure` allocates its
//! single-element one (§2.3) — that eager floor is pinned deterministically
//! by the allocation gate in `tests/v05_gates.rs` (strict == split kernels,
//! exactly), and the fused B-parity story it defers to is `benches/abc.rs`'s
//! territory. The pointwise groups carry the §6.1 verdict: after
//! monomorphization the generic bound is erased, so C is B's spelling.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench monad
//! ```
use catheo::{Monad, OptionWitness, ResultWitness};
use criterion::{BenchmarkGroup, Criterion};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// The strict program (lockstep with `tests/v05_gates.rs`).
fn pointwise_strict<M: Monad>(input: M::Type<i32>) -> M::Type<i64> {
    M::bind(input, |x| M::pure(i64::from(x) * 2))
}

/// The fallible arrow of the `Result` universes (lockstep with the gates).
fn checked_arrow(x: i32) -> Result<i64, String> {
    if x < 0 {
        Err("negative record".to_owned())
    } else {
        Ok(i64::from(x) * 2)
    }
}

// ─── Option universes ─────────────────────────────────────────────────────

/// A: every step is its own match, materializing its own intermediate.
#[expect(
    clippy::manual_map,
    reason = "A spells every step as its own match with a materialized \
              intermediate — that shape is the cost the naive baseline \
              represents, so the idiomatic `.map` spelling would not \
              measure the naive universe."
)]
fn option_naive(input: Option<i32>) -> Option<i64> {
    match input {
        Some(x) => match Some(i64::from(x)) {
            Some(y) => Some(y * 2),
            None => None,
        },
        None => None,
    }
}

/// B: the idiomatic spelling — one lazy `.map` chain.
fn option_idiomatic(input: Option<i32>) -> Option<i64> {
    input.map(i64::from).map(|x| x * 2)
}

/// C: the generic program over the one `Monad` bound.
fn option_strict(input: Option<i32>) -> Option<i64> {
    pointwise_strict::<OptionWitness>(input)
}

// ─── Result universes ─────────────────────────────────────────────────────

/// A: nested matches over the fallible arrow.
fn result_naive(input: Result<i32, String>) -> Result<i64, String> {
    match input {
        Ok(x) => match checked_arrow(x) {
            Ok(y) => Ok(y),
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    }
}

/// B: the idiomatic spelling — `and_then` short-circuits identically.
fn result_idiomatic(input: Result<i32, String>) -> Result<i64, String> {
    input.and_then(checked_arrow)
}

/// C: the generic program over the one `Monad` bound.
fn result_strict(input: Result<i32, String>) -> Result<i64, String> {
    <ResultWitness<String> as Monad>::bind(input, checked_arrow)
}

// ─── The harness ──────────────────────────────────────────────────────────

/// One bench routine: run once, yield the `i64` observable. The warm
/// universes unwrap the computed value; the cold universes unwrap the
/// fallback — the observable is constant, the timed region is the
/// discriminant test the universe exists to measure.
type BenchRoutine = fn() -> i64;

fn option_warm_naive() -> i64 {
    option_naive(Some(21_i32)).unwrap_or(-1)
}

fn option_warm_idiomatic() -> i64 {
    option_idiomatic(Some(21_i32)).unwrap_or(-1)
}

fn option_warm_strict() -> i64 {
    option_strict(Some(21_i32)).unwrap_or(-1)
}

fn option_cold_naive() -> i64 {
    option_naive(None).unwrap_or(-1)
}

fn option_cold_idiomatic() -> i64 {
    option_idiomatic(None).unwrap_or(-1)
}

fn option_cold_strict() -> i64 {
    option_strict(None).unwrap_or(-1)
}

fn result_warm_naive() -> i64 {
    result_naive(Ok(21_i32)).unwrap_or(-1)
}

fn result_warm_idiomatic() -> i64 {
    result_idiomatic(Ok(21_i32)).unwrap_or(-1)
}

fn result_warm_strict() -> i64 {
    result_strict(Ok(21_i32)).unwrap_or(-1)
}

/// The arrow rejects: `checked_arrow(-21)` is the timed short-circuit.
fn result_cold_naive() -> i64 {
    result_naive(Ok(-21_i32)).unwrap_or(-1)
}

fn result_cold_idiomatic() -> i64 {
    result_idiomatic(Ok(-21_i32)).unwrap_or(-1)
}

fn result_cold_strict() -> i64 {
    result_strict(Ok(-21_i32)).unwrap_or(-1)
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

fn bench_option_warm(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("monad/pointwise_option_warm"),
        [
            ("A_naive", option_warm_naive),
            ("B_idiomatic", option_warm_idiomatic),
            ("C_strict", option_warm_strict),
        ],
    );
}

fn bench_option_cold(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("monad/pointwise_option_cold"),
        [
            ("A_naive", option_cold_naive),
            ("B_idiomatic", option_cold_idiomatic),
            ("C_strict", option_cold_strict),
        ],
    );
}

fn bench_result_warm(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("monad/pointwise_result_warm"),
        [
            ("A_naive", result_warm_naive),
            ("B_idiomatic", result_warm_idiomatic),
            ("C_strict", result_warm_strict),
        ],
    );
}

fn bench_result_cold(c: &mut Criterion) {
    bench_pointwise_three_way(
        c.benchmark_group("monad/pointwise_result_cold"),
        [
            ("A_naive", result_cold_naive),
            ("B_idiomatic", result_cold_idiomatic),
            ("C_strict", result_cold_strict),
        ],
    );
}

criterion_group!(
    benches,
    bench_option_warm,
    bench_option_cold,
    bench_result_warm,
    bench_result_cold,
);
criterion_main!(benches);
