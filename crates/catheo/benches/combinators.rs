//! The §6.1 v0.2 benchmark suite — the measurement instrument for the
//! roadmap's v0.2 gate ("Kleisli, `fold_map`, escape hatches — benchmark
//! gates re-applied").
//!
//! Three §6.1 three-way comparisons over the v0.2 combinators:
//!
//! - **`v02/kleisli_entry`** — the §4.3 Kleisli chain written with the new
//!   `kleisli: (f, g)` grammar entry over `OptionWitness`. A/B are the M4
//!   Kleisli suite's naive ladder and idiomatic `and_then` chain (kept in
//!   lockstep); C is the new entry's output, which the deterministic gate
//!   (`tests/v02_gates.rs`) pins byte-identical to the two-`bind`
//!   spelling — so this also re-measures the M4 chain through the new
//!   grammar.
//! - **`v02/kleisli_vec_prefix`** — a `map` feeding a two-morphism
//!   `kleisli:` chain over `VecWitness` with a `fold` terminal: the
//!   "fusion pass extended" claim (§4.3). B is the idiomatic
//!   `.map().flat_map().flat_map()` chain; C's fused lowering must absorb
//!   the map and both morphisms into one pass (Rule 2 across the desugared
//!   chain boundary, then Rule 4).
//! - **`v02/fold_map`** — the §4.3 monoid eliminator over `Vec<i32>`.
//!   A materializes the mapped intermediate (the naive spelling the
//!   eliminator exists to replace); B is the idiomatic
//!   `.map(f).fold(identity, combine)`; C is `pipeline!` with
//!   `fold_map:`, whose fused lowering emits exactly B's shape into one
//!   pass.
//!
//! The deterministic, CI-enforceable form of the same claims lives in
//! `tests/v02_gates.rs` (three-way agreement, allocation profiles, and
//! emission lockstep); the closures here are kept in lockstep with it.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench combinators
//! ```

use catheo::{Monoid, OptionWitness, VecWitness};
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M elements.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

// ─── Suite 1: the kleisli grammar entry over OptionWitness ───────────────

/// First Kleisli morphism: keep the tripler's even outputs (the M4
/// Kleisli suite's `f`, in lockstep).
fn kleisli_f(value: i64) -> Option<i64> {
    let tripled = value.checked_mul(3)?;
    (tripled % 2 == 0).then_some(tripled)
}

/// Second Kleisli morphism: keep the shifted value when it does not
/// divide by three (the M4 suite's `g`).
fn kleisli_g(value: i64) -> Option<i64> {
    let shifted = value.checked_add(7)?;
    (shifted % 3 != 0).then_some(shifted)
}

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

fn kleisli_idiomatic(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + element
                .and_then(kleisli_f)
                .and_then(kleisli_g)
                .unwrap_or(0_i64)
    })
}

/// C: the new §4.3 grammar entry. The desugar is pinned byte-identical to
/// the two-bind spelling by `tests/v02_gates.rs`, so this *is* the M4
/// Kleisli chain re-measured through the v0.2 grammar.
fn kleisli_entry_macro(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + catheo::pipeline! {
                witness: OptionWitness,
                source: element,
                kleisli: (kleisli_f, kleisli_g),
                fold: (0_i64, i64::wrapping_add),
            }
    })
}

/// Deterministic Kleisli universe: a `None` every third element, `Some`
/// values in `[-200, 200]` (the M4 universe).
fn kleisli_input(size: usize) -> Vec<Option<i64>> {
    (0..size)
        .map(|i| match i % 3 {
            0 => None,
            _ => Some(i64::try_from(i % 401).unwrap_or(0) - 200),
        })
        .collect()
}

// ─── Suite 2: map feeding a kleisli chain over VecWitness ────────────────

/// First morphism: fan out. Second: filter-shaped flatten.
fn vec_expand(value: i64) -> Vec<i64> {
    if let Some(doubled) = value.checked_mul(2) {
        vec![value, doubled]
    } else {
        Vec::new()
    }
}

fn vec_keep(value: i64) -> Vec<i64> {
    if value % 3 == 0 {
        vec![value]
    } else {
        vec![value, value]
    }
}

fn kleisli_vec_naive(input: Vec<i64>) -> i64 {
    input.into_iter().fold(0_i64, |total, x| {
        let mapped = x.wrapping_mul(3);
        let mut subtotal = 0_i64;
        for first in vec_expand(mapped) {
            for kept in vec_keep(first) {
                subtotal += kept;
            }
        }
        total + subtotal
    })
}

fn kleisli_vec_idiomatic(input: Vec<i64>) -> i64 {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .flat_map(vec_expand)
        .flat_map(vec_keep)
        .sum()
}

/// C: `map` + `kleisli:` + `fold`. The fused lowering absorbs the map into
/// the first morphism (Rule 2 across the desugared chain boundary) and the
/// chain into the fold eliminator (Rule 4) — one pass, no intermediate
/// containers (pinned by `tests/v02_gates.rs`).
fn kleisli_vec_macro(input: Vec<i64>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(3),
        kleisli: (vec_expand, vec_keep),
        fold: (0_i64, i64::wrapping_add),
    }
}

fn kleisli_vec_input(size: usize) -> Vec<i64> {
    (0..size)
        .map(|i| i64::try_from(i % 199).unwrap_or(0) - 99)
        .collect()
}

// ─── Suite 3: the fold_map monoid eliminator over Vec<i32> ───────────────

fn fold_map_naive(input: Vec<i32>) -> u64 {
    let mapped: Vec<u64> = input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(|x| u64::try_from(x).unwrap_or(0))
        .collect();
    let mut total = 0_u64;
    for x in mapped {
        total += x;
    }
    total
}

fn fold_map_idiomatic(input: Vec<i32>) -> u64 {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(|x| u64::try_from(x).unwrap_or(0))
        .fold(<u64 as Monoid>::identity(), <u64 as Monoid>::combine)
}

/// C: the §4.3 `fold_map` terminal — the fused lowering chains the metric
/// map into `.fold(Monoid::identity(), Monoid::combine)` in one pass.
fn fold_map_macro(input: Vec<i32>) -> u64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(3),
        fold_map: |x: i32| u64::try_from(x).unwrap_or(0),
    }
}

fn fold_map_input(size: usize) -> Vec<i32> {
    (0..size)
        .map(|i| i32::try_from(i % 1_999).unwrap_or(0) - 999)
        .collect()
}

// ─── The harness (the M4 bench's `bench_three_way`, verbatim shape) ──────

type Variants<T, R> = [(&'static str, fn(T) -> R); 3];

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

fn bench_kleisli_entry(c: &mut Criterion) {
    let mut group = c.benchmark_group("v02/kleisli_entry");
    bench_three_way(
        &mut group,
        kleisli_input,
        [
            ("A_naive", kleisli_naive),
            ("B_idiomatic", kleisli_idiomatic),
            ("C_macro", kleisli_entry_macro),
        ],
    );
    group.finish();
}

fn bench_kleisli_vec_prefix(c: &mut Criterion) {
    let mut group = c.benchmark_group("v02/kleisli_vec_prefix");
    bench_three_way(
        &mut group,
        kleisli_vec_input,
        [
            ("A_naive", kleisli_vec_naive),
            ("B_idiomatic", kleisli_vec_idiomatic),
            ("C_macro", kleisli_vec_macro),
        ],
    );
    group.finish();
}

fn bench_fold_map(c: &mut Criterion) {
    let mut group = c.benchmark_group("v02/fold_map");
    bench_three_way(
        &mut group,
        fold_map_input,
        [
            ("A_naive", fold_map_naive),
            ("B_idiomatic", fold_map_idiomatic),
            ("C_macro", fold_map_macro),
        ],
    );
    group.finish();
}

criterion_group!(
    benches,
    bench_kleisli_entry,
    bench_kleisli_vec_prefix,
    bench_fold_map,
);
criterion_main!(benches);
