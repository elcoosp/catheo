//! The §6.1 M4 benchmark suite — the last measurement gate before v0.1.
//!
//! ARCHITECTURE.md §6.1 ("Additional benchmarks") defines the two M4
//! instruments and §8 gates M4 on them:
//!
//! - **`Result`-chain error handling** — a 10-stage `bind`-heavy pipeline
//!   over `ResultWitness<ChainError>`, run *cold* (every element fails at
//!   stage 3; stages 4–10 are short-circuited) and *warm* (every element
//!   survives all ten stages).
//! - **Kleisli composition** — a two-morphism Kleisli chain
//!   (`f >=> g == bind(f) then bind(g)`) over `OptionWitness`.
//!
//! For each, the §6.1 three-way comparison:
//!
//! - **`A_naive`** — the naive spelling: a hand-rolled nested-match
//!   ladder, one lexical continuation per stage. The baseline the macro
//!   is meant to beat.
//! - **`B_idiomatic`** — the idiomatic chain: `and_then` end to end,
//!   eliminated with `unwrap_or`. The baseline the macro must match
//!   (the §8 M4 gate: **C ≥ B on both**).
//! - **`C_macro`** — the `pipeline!` macro-flattened output. The target.
//!
//! These chains run over pointwise witnesses (zero-or-one-element
//! projections): the fused lowering keeps them at the kernel shape — one
//! `run_bind` kernel per stage, each kernel *is* the idiomatic
//! `and_then`, into one `run_fold` eliminator — so C is byte-for-byte
//! the idiomatic machine shape. (The §6.1 protocol caught the first
//! attempt: adapter-chain absorption over `option::IntoIter`/
//! `result::IntoIter` measured 1.2–2.7× slower than B; the responsible
//! lowering was re-examined and revised — see `catheo-parser`'s codegen
//! module and the README's "M4 notes".)
//!
//! Each pipeline processes one `Result`/`Option` element, so the suite
//! streams a §6.1-sized universe of elements through the chain per
//! iteration and folds the outputs — same input→clone→fold scaffolding
//! for all three variants, so the comparison isolates the chain shape.
//! The deterministic, CI-enforceable form of the same claim lives in
//! `tests/error_chain_gates.rs` (three-way agreement + zero-allocation +
//! emission lockstep); the closures here are kept in lockstep with it.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench error_chains
//! ```

use catheo::{OptionWitness, ResultWitness};
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

// ─── The 10-stage Result chain (cold and warm) ──────────────────────────

/// The chain's error type: one overflow, tagged with the failing stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChainError {
    /// The 1-based stage index that rejected the value.
    stage: u8,
}

/// Stage `n` of the chain: a Kleisli morphism `i64 -> Result<i64, _>`,
/// a bare fn item (the parser's callable stage form).
macro_rules! chain_stage {
    ($name:ident, $n:literal) => {
        fn $name(value: i64) -> Result<i64, ChainError> {
            value.checked_add($n).ok_or(ChainError { stage: $n })
        }
    };
}

chain_stage!(stage_01, 1);
chain_stage!(stage_02, 2);
chain_stage!(stage_03, 3);
chain_stage!(stage_04, 4);
chain_stage!(stage_05, 5);
chain_stage!(stage_06, 6);
chain_stage!(stage_07, 7);
chain_stage!(stage_08, 8);
chain_stage!(stage_09, 9);
chain_stage!(stage_10, 10);

/// Warm universe: every element survives all ten stages, so the pipeline
/// value is `seed + 55` and the full chain executes per element.
fn warm_seed(index: usize) -> i64 {
    i64::try_from(index % 1_000).unwrap_or(0) + 1
}

/// Cold universe: every element fails at stage 3 (`i64::MAX - 3 + 3`
/// overflows), leaving stages 4–10 short-circuited — the error path.
fn cold_seed(_index: usize) -> i64 {
    i64::MAX - 3
}

/// The warm §6.1 universe: `size` all-pass elements.
fn warm_input(size: usize) -> Vec<Result<i64, ChainError>> {
    (0..size).map(|index| Ok(warm_seed(index))).collect()
}

/// The cold §6.1 universe: `size` stage-3-failing elements.
fn cold_input(size: usize) -> Vec<Result<i64, ChainError>> {
    (0..size).map(|index| Ok(cold_seed(index))).collect()
}

/// A: the naive spelling — the ten-deep nested-match ladder, no
/// combinators.
fn result_chain_naive(input: Vec<Result<i64, ChainError>>) -> i64 {
    input
        .into_iter()
        .fold(0_i64, |total, element| total + ladder(element))
}

/// The ladder, naive per element (ten lexical continuations deep).
fn ladder(element: Result<i64, ChainError>) -> i64 {
    match element {
        Ok(seed) => match stage_01(seed) {
            Ok(x) => match stage_02(x) {
                Ok(x) => match stage_03(x) {
                    Ok(x) => match stage_04(x) {
                        Ok(x) => match stage_05(x) {
                            Ok(x) => match stage_06(x) {
                                Ok(x) => match stage_07(x) {
                                    Ok(x) => match stage_08(x) {
                                        Ok(x) => match stage_09(x) {
                                            Ok(x) => stage_10(x).unwrap_or_default(),
                                            Err(_) => 0,
                                        },
                                        Err(_) => 0,
                                    },
                                    Err(_) => 0,
                                },
                                Err(_) => 0,
                            },
                            Err(_) => 0,
                        },
                        Err(_) => 0,
                    },
                    Err(_) => 0,
                },
                Err(_) => 0,
            },
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

/// B: the idiomatic spelling — one short-circuiting `and_then` chain.
fn result_chain_idiomatic(input: Vec<Result<i64, ChainError>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + element
                .and_then(stage_01)
                .and_then(stage_02)
                .and_then(stage_03)
                .and_then(stage_04)
                .and_then(stage_05)
                .and_then(stage_06)
                .and_then(stage_07)
                .and_then(stage_08)
                .and_then(stage_09)
                .and_then(stage_10)
                .unwrap_or(0_i64)
    })
}

/// C: the macro-flattened output — on this pointwise witness the fused
/// lowering emits one `run_bind` kernel per stage (each kernel *is*
/// `Result::and_then`) and a `run_fold` eliminator: byte-for-byte the
/// idiomatic machine shape, pinned by `tests/error_chain_gates.rs`'s
/// emission-lockstep gate.
fn result_chain_macro(input: Vec<Result<i64, ChainError>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + catheo::pipeline! {
                witness: ResultWitness<ChainError>,
                source: element,
                bind: stage_01,
                bind: stage_02,
                bind: stage_03,
                bind: stage_04,
                bind: stage_05,
                bind: stage_06,
                bind: stage_07,
                bind: stage_08,
                bind: stage_09,
                bind: stage_10,
                fold: (0_i64, i64::wrapping_add),
            }
    })
}

fn bench_result_chain(
    c: &mut Criterion,
    name: &str,
    input_for: fn(usize) -> Vec<Result<i64, ChainError>>,
) {
    let mut group = c.benchmark_group(name);
    bench_three_way(
        &mut group,
        input_for,
        [
            ("A_naive", result_chain_naive),
            ("B_idiomatic", result_chain_idiomatic),
            ("C_macro", result_chain_macro),
        ],
    );
    group.finish();
}

// ─── The two-morphism Kleisli chain over OptionWitness ──────────────────

/// First Kleisli morphism: keep the tripler's even outputs — a
/// deterministic `Some`/`None` mix over the sweep.
fn kleisli_f(value: i64) -> Option<i64> {
    let tripled = value.checked_mul(3)?;
    (tripled % 2 == 0).then_some(tripled)
}

/// Second Kleisli morphism: keep the shifted value when it does not
/// divide by three — the complement mix.
fn kleisli_g(value: i64) -> Option<i64> {
    let shifted = value.checked_add(7)?;
    (shifted % 3 != 0).then_some(shifted)
}

/// A: the naive spelling — the composition unrolled as a nested ladder.
fn kleisli_naive(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + match element {
                Some(x) => match kleisli_f(x) {
                    Some(y) => kleisli_g(y).unwrap_or_default(),
                    None => 0,
                },
                None => 0,
            }
    })
}

/// B: the idiomatic spelling — `(f >=> g)(x)` as an `and_then` chain.
fn kleisli_idiomatic(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + element
                .and_then(kleisli_f)
                .and_then(kleisli_g)
                .unwrap_or(0_i64)
    })
}

/// C: the macro-flattened output — on the `OptionWitness` spelling of the
/// pointwise dispatch (see [`result_chain_macro`]).
fn kleisli_macro(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + catheo::pipeline! {
                witness: OptionWitness,
                source: element,
                bind: kleisli_f,
                bind: kleisli_g,
                fold: (0_i64, i64::wrapping_add),
            }
    })
}

/// Deterministic Kleisli universe: a `None` every third element, `Some`
/// values in `[-200, 200]` so both morphisms reject a realistic fraction.
fn kleisli_input(size: usize) -> Vec<Option<i64>> {
    (0..size)
        .map(|i| match i % 3 {
            0 => None,
            _ => Some(i64::try_from(i % 401).unwrap_or(0) - 200),
        })
        .collect()
}

// ─── The harness: one group per chain, three variants each ──────────────

/// The three variants of one chain: `("A_naive", …)`,
/// `("B_idiomatic", …)`, `("C_macro", …)`.
type Variants<T, R> = [(&'static str, fn(T) -> R); 3];

/// Run the A/B/C trio for one chain across the §6.1 sizes.
///
/// `iter_batched` keeps the input clone in the *setup* phase, so the
/// measured routine starts from an owned container exactly like a real
/// call site would hand one over.
fn bench_three_way<T, R>(
    group: &mut BenchmarkGroup<'_, criterion::measurement::WallTime>,
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

fn bench_result_chain_warm(c: &mut Criterion) {
    bench_result_chain(c, "m4/result_chain_warm", warm_input);
}

fn bench_result_chain_cold(c: &mut Criterion) {
    bench_result_chain(c, "m4/result_chain_cold", cold_input);
}

fn bench_kleisli_option(c: &mut Criterion) {
    let mut group = c.benchmark_group("m4/kleisli_option");
    bench_three_way(
        &mut group,
        kleisli_input,
        [
            ("A_naive", kleisli_naive),
            ("B_idiomatic", kleisli_idiomatic),
            ("C_macro", kleisli_macro),
        ],
    );
    group.finish();
}

criterion_group!(
    benches,
    bench_result_chain_warm,
    bench_result_chain_cold,
    bench_kleisli_option,
);
criterion_main!(benches);
