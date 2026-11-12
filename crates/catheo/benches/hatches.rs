//! The §6.1 escape-hatch benchmark suite — the §4.2 matrix, measured.
//!
//! The hatch matrix is published and value-checked (`examples/hatch_matrix.rs`,
//! `tests/consumer_examples.rs::the_hatch_matrix_demo_runs_every_accepted_spelling`);
//! this file is its measurement instrument, per the §6.1 three-way protocol.
//! The deterministic consequences (allocation profiles, fused-emission shape)
//! stay with the gate suites (`tests/v02_gates.rs`, `tests/fusion_properties.rs`).
//!
//! Two container universes over the README flagship shape — `Vec<Vec<i32>>`
//! row sums into a pure `fold`, 10k/100k/1M rows. Every pipeline spells the
//! §4.2 arrow inline (`|row| vec![row.iter().sum()]`), the body the §3.2
//! lexical scan flags as a structural observer — the false positive the
//! hatches exist for, exactly as `examples/escape_hatches.rs` and
//! `tests/v02_gates.rs` pin it:
//!
//! - **`bind_force_vec_warm`** — C spells the stage `bind!: force` (the
//!   fusion direction: the silenced scan is a compile-time flag, not a
//!   runtime cost). `A_naive` is the same grammar through the §6.2 oracle
//!   `pipeline_unfused!` — sequential kernels, one materialized container
//!   per stage, the shape `force` avoids. `B_idiomatic` is the std chain.
//!   Expected verdict: C ≈ B (the hatch is free), C > A.
//! - **`bind_materialize_vec_warm`** — C spells the stage
//!   `bind!: materialize` (the documented hard barrier). `A_naive` is the
//!   same sequential-kernel shape written plainly, `B_idiomatic` the same
//!   std chain. Expected verdict: C ≈ A — this universe *measures the
//!   hatch's documented cost* (the per-stage intermediate) against B, so a
//!   consumer choosing `materialize` knows what it pays; it is not a
//!   parity claim, and the §6.1 re-examination duty is "the cost matches
//!   the documented barrier", not "C ≥ B".
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench hatches
//! ```
use catheo::VecWitness;
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput};
use criterion::{criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

/// §6.1 sizes: 10k, 100k, 1M rows.
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];

/// The row shape: two elements per row, a deterministic spread.
fn rows_input(size: usize) -> Vec<Vec<i32>> {
    (0..size)
        .map(|i| {
            let base = i32::try_from(i % 977).unwrap_or(0);
            vec![base, base]
        })
        .collect()
}

/// The fold every routine shares (the callable form: the grammar's
/// eliminator, lint-silent in the expanded code).
fn sum_fold(acc: i64, x: i32) -> i64 {
    acc + i64::from(x)
}

// ─── The force universe (fusion direction) ────────────────────────────────

/// A: the §6.2 oracle spelling — sequential kernels, one materialized
/// container per stage.
fn force_unfused(input: Vec<Vec<i32>>) -> i64 {
    catheo::pipeline_unfused! {
        witness: VecWitness,
        source: input,
        bind!: force |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, sum_fold),
    }
}

/// B: the idiomatic spelling — the std chain.
fn force_idiomatic(input: Vec<Vec<i32>>) -> i64 {
    input
        .into_iter()
        .map(|row| i64::from(row.iter().sum::<i32>()))
        .sum::<i64>()
}

/// C: the macro output — `bind!: force` rides the fused path.
fn force_macro(input: Vec<Vec<i32>>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        bind!: force |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, sum_fold),
    }
}

// ─── The materialize universe (the documented cost) ───────────────────────

/// A: the sequential-kernel shape, written plainly — the same lowering the
/// materialize hatch pins.
fn materialize_unfused(input: Vec<Vec<i32>>) -> i64 {
    catheo::pipeline_unfused! {
        witness: VecWitness,
        source: input,
        bind: |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, sum_fold),
    }
}

/// B: the idiomatic spelling — identical to the force universe's B.
fn materialize_idiomatic(input: Vec<Vec<i32>>) -> i64 {
    force_idiomatic(input)
}

/// C: the macro output — `bind!: materialize` pins the hard barrier, so C
/// materializes one intermediate per stage: the hatch's documented cost.
fn materialize_macro(input: Vec<Vec<i32>>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        bind!: materialize |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, sum_fold),
    }
}

// ─── The harness (§6.1 protocol) ──────────────────────────────────────────

/// One bench routine: run once over the row input, yield the `i64`.
type BenchRoutine = fn(Vec<Vec<i32>>) -> i64;

/// The §6.1 three-way harness over the §6.1 sizes.
fn bench_three_way(
    mut group: BenchmarkGroup<'_, criterion::measurement::WallTime>,
    routines: [(&str, BenchRoutine); 3],
) {
    group.sample_size(30);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for &size in &SIZES {
        group.throughput(Throughput::Elements(
            u64::try_from(size).expect("size fits u64"),
        ));
        let input = rows_input(size);
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

fn bench_hatches_bind_force(c: &mut Criterion) {
    bench_three_way(
        c.benchmark_group("hatches/bind_force_vec_warm"),
        [
            ("A_naive", force_unfused),
            ("B_idiomatic", force_idiomatic),
            ("C_macro", force_macro),
        ],
    );
}

fn bench_hatches_bind_materialize(c: &mut Criterion) {
    bench_three_way(
        c.benchmark_group("hatches/bind_materialize_vec_warm"),
        [
            ("A_naive", materialize_unfused),
            ("B_idiomatic", materialize_idiomatic),
            ("C_macro", materialize_macro),
        ],
    );
}

criterion_group!(
    benches,
    bench_hatches_bind_force,
    bench_hatches_bind_materialize,
);
criterion_main!(benches);
