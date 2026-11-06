//! Criterion benchmarks for the Layer-2 execution kernels (§2.3).
//!
//! M1 scope: pin down the *kernel floor* — the standalone cost of each
//! `run_*` kernel per witness at representative sizes. The full three-way
//! A/B/C comparison (naive closures vs iterator chains vs macro output,
//! §6.1) lands with M3 and reads these numbers as its reference baseline:
//! a pipeline can never be cheaper than the kernels it lowers into.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench kernels
//! ```

use catheo::{
    CategoricalFold, CategoricalFunctor, CategoricalIntoIter, CategoricalMonad, VecWitness,
};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

const SIZES: [usize; 2] = [10_000, 100_000];

/// A deterministic sum-preserving map workload: multiply by 3, add 1.
fn workload(x: i64) -> i64 {
    x.wrapping_mul(3).wrapping_add(1)
}

fn bench_map(c: &mut Criterion) {
    let mut group = c.benchmark_group("kernels/vec_run_map");
    for &size in &SIZES {
        let len = i64::try_from(size).expect("size fits i64");
        let input: Vec<i64> = (0..len).collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| {
                <VecWitness as CategoricalFunctor>::run_map(black_box(input.clone()), workload)
            });
        });
    }
    group.finish();
}

fn bench_bind(c: &mut Criterion) {
    let mut group = c.benchmark_group("kernels/vec_run_bind");
    for &size in &SIZES {
        let len = i64::try_from((size / 8).max(1)).expect("size fits i64");
        let input: Vec<i64> = (0..len).collect();
        // Each element expands to an 8-element chunk: exercises the flatten
        // path with real allocation per element.
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| {
                <VecWitness as CategoricalMonad>::run_bind(black_box(input.clone()), |x| {
                    vec![x, x + 1, x + 2, x + 3, x + 4, x + 5, x + 6, x + 7]
                });
            });
        });
    }
    group.finish();
}

fn bench_fold(c: &mut Criterion) {
    let mut group = c.benchmark_group("kernels/vec_run_fold");
    for &size in &SIZES {
        let len = i64::try_from(size).expect("size fits i64");
        let input: Vec<i64> = (0..len).collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| {
                <VecWitness as CategoricalFold>::run_fold(
                    black_box(input.clone()),
                    0_i64,
                    |acc, x| acc.wrapping_add(workload(x)),
                )
            });
        });
    }
    group.finish();
}

fn bench_into_iter(c: &mut Criterion) {
    let mut group = c.benchmark_group("kernels/vec_into_iter");
    for &size in &SIZES {
        let len = i64::try_from(size).expect("size fits i64");
        let input: Vec<i64> = (0..len).collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| {
                <VecWitness as CategoricalIntoIter>::into_iter(black_box(input.clone()))
                    .fold(0_i64, |acc, x| acc.wrapping_add(workload(x)))
            });
        });
    }
    group.finish();
}

// The kernel-linearity gate lives in `tests/kernel_gates.rs`: a
// `harness = false` bench target never executes `#[test]`s, so the gate
// would silently stop running. As a plain integration test it is enforced
// by every plain `cargo test --workspace`.

criterion_group!(benches, bench_map, bench_bind, bench_fold, bench_into_iter);
criterion_main!(benches);
