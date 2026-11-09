//! The README quickstart, runnable.
//!
//! ```text
//! cargo run -p catheo --example quickstart
//! ```
//!
//! The two `map` stages and the `fold` fuse into a single pass — no
//! intermediate `Vec` is built between source and eliminator (the
//! deterministic allocation gates in `tests/v02_gates.rs` pin exactly
//! that). The second pipeline adds a `filter` — an absolute fusion barrier
//! by design (§3.3 Rule 3) — and collects the surviving elements back out.

use catheo::{VecWitness, pipeline};

fn main() {
    let input = vec![1_i32, 2, 3, 4];
    let total: usize = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        map: |x| x.to_string(),
        fold: (0_usize, |acc, s| acc + s.len()),
    };
    assert_eq!(total, 4);
    println!("fused map → map → fold total: {total}");

    // The maps before the barrier lower to one fused pass, the filter is
    // its own kernel, and the `collect` terminal evaluates to the
    // surviving container.
    let survivors: Vec<i32> = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4, 5, 6],
        map: |x| x * 10,
        filter: |x: &i32| *x % 20 == 0,
        collect,
    };
    assert_eq!(survivors, vec![20, 40, 60]);
    println!("filtered survivors: {survivors:?}");
}
