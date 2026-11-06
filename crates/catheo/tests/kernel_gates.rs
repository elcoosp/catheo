//! Kernel performance gates, as real integration tests.
//!
//! These gates originally lived as `#[test]`s inside the `harness = false`
//! bench target — which never executes `#[test]`s, so the gate silently
//! stopped running. It lives here instead, where every plain
//! `cargo test --workspace` (and therefore the CI `test` job) enforces it.
//!
//! The gate is linearity, not a latency record: a 1M-element map+fold round
//! trip must finish far below one second. That catches accidental quadratic
//! kernels (e.g. a regressed `run_bind` that reallocates per element)
//! deterministically, on any CI runner, without a dashboard.

use catheo::{CategoricalFold, CategoricalFunctor, VecWitness};
use std::time::{Duration, Instant};

/// The same deterministic workload the `kernels` bench uses (kept in
/// lockstep): multiply by 3, add 1.
fn workload(x: i64) -> i64 {
    x.wrapping_mul(3).wrapping_add(1)
}

#[test]
fn kernels_stay_linear_at_one_million_elements() {
    let input: Vec<i64> = (0..1_000_000_i64).collect();
    let start = Instant::now();
    let mapped = <VecWitness as CategoricalFunctor>::run_map(input, workload);
    let total = <VecWitness as CategoricalFold>::run_fold(mapped, 0_i64, i64::wrapping_add);
    let elapsed = start.elapsed();
    // Sum of 0..1_000_000 mapped by (3x + 1): 3 * n(n-1)/2 + n.
    let n = 1_000_000_i64;
    assert_eq!(total, 3 * n * (n - 1) / 2 + n);
    assert!(
        elapsed < Duration::from_secs(1),
        "1M-element map+fold took {elapsed:?}: a kernel went super-linear"
    );
}
