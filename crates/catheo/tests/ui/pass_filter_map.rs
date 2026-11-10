//! `filter_map` — the §4.3 v0.6 partial arrow — compiles, fuses, and runs.
//!
//! The inline `Some`/`None` spelling is the point: those constructors are
//! the arrow's effect, not container-shape observation (§3.2), so the
//! stage fuses into the adapter chain instead of materializing.
use catheo::{OptionWitness, VecWitness, pipeline};

fn partial_double(x: i32) -> Option<i64> {
    if x % 2 == 0 {
        Some(i64::from(x) * 2)
    } else {
        None
    }
}

fn main() {
    // The bare partial arrow with a `collect` terminal: std's `filter_map`
    // spelling — keeps the evens, doubles them.
    let kept: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4],
        filter_map: partial_double,
        collect,
    };
    assert_eq!(kept, vec![4_i64, 8]);

    // The inline constructor spelling — `Some`/`None` in the body — fuses
    // under the §3.2 refinement and absorbs into a pure `fold` (Rule 4).
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4, 5, 6],
        filter_map: |x: i32| if x % 2 == 0 {
            Some(i64::from(x) * 10)
        } else {
            None
        },
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 20 + 40 + 60);

    // A `map` prefix fuses into the partial arrow (Rule 2): one pass.
    let doubled_kept: i64 = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4],
        map: |x: i32| x + 1,
        filter_map: partial_double,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(doubled_kept, 4 + 8); // +1 makes [2,3,4,5]; the evens 2,4 keep

    // The pointwise container: `filter_map` over `Option` is `and_then`.
    let pointwise: Option<i64> = pipeline! {
        witness: OptionWitness,
        source: Some(21_i32),
        filter_map: |x: i32| if x > 0 { Some(i64::from(x) * 2) } else { None },
    };
    assert_eq!(pointwise, Some(42_i64));

    // The §4.2 hatch spells on the partial arrow too: `materialize` pins
    // the intermediate container, `force` overrides the methods scan.
    let materialized: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: vec![2_i32, 4],
        filter_map!: materialize partial_double,
        collect,
    };
    assert_eq!(materialized, vec![4_i64, 8]);
}
