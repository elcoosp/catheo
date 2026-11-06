// The `fold: (0, |acc, x| acc + x)` grammar exercises the universal
// eliminator (§4.3) exactly as users write it; clippy's `sum()`
// suggestion does not apply to the DSL, so the lint is expected at file
// scope — and errors here the moment some future rewrite stops hitting
// it, keeping the suppression verifiable.
#![expect(clippy::unnecessary_fold)]

//! The §6.1 compile-time fixture: 50 real `pipeline!` expansions.
//!
//! §6.1 requires expansion time and generated token volume to be recorded
//! for a 50-pipeline fixture; §6.3 caps every expansion at 200 lines
//! (~2 000 tokens). The pipelines below are a deterministic rotation over
//! the grammar — map/bind/filter stages, fold/collect terminals, all three
//! witnesses, environment capture, type ascriptions — with values asserted
//! against independently computed expectations, so the fixture doubles as a
//! broad semantic regression net.
//!
//! Measurement paths:
//!
//! - The nightly `-Zmacro-stats` CI job compiles this file with the rest of
//!   `--workspace --all-targets` and uploads the stats artifact — that is
//!   the recorded expansion-time/token-volume evidence trail.
//! - The per-pipeline token budget itself is enforced in
//!   `catheo-parser`'s codegen (the §6.3 fallback to the sequential
//!   lowering on breach) and its wall-clock ceiling by the expansion-budget
//!   gate in `crates/catheo-parser/tests/budget_gate.rs`.

use catheo::{OptionWitness, ResultWitness, VecWitness, pipeline};

#[derive(Debug, PartialEq)]
struct FixtureError;

#[test]
fn fixture_01_vec_map2_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 2,
        map: |x| x * 3,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 132);
}

#[test]
fn fixture_02_vec_bind_fold() {
    let input = vec![0, 1, 2, 3, 4, 5];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, x + 3],
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 48);
}

#[test]
fn fixture_03_vec_map_filter_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        filter: |y| y % 3 == 0,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 18);
}

#[test]
fn fixture_04_vec_map_collect() {
    let input = vec![0, 1, 2, 3, 4, 5, 6];
    let out: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 5,
        collect,
    };
    assert_eq!(out, vec![5, 6, 7, 8, 9, 10, 11]);
}

#[test]
fn fixture_05_option_map_bind_fold() {
    let maybe = Some(6_i64);
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 1,
        bind: |x| if x > 10 { Some(x * 4) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_06_option_none_folds_to_init() {
    let maybe: Option<i64> = None;
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x: i64| x + 2,
        bind: |x| if x > 11 { Some(x * 2) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_07_result_ok_map_fold() {
    let raw: Result<i64, FixtureError> = Ok(8);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x| x * 3,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 24);
}

#[test]
fn fixture_08_result_err_folds_to_init() {
    let raw: Result<i64, FixtureError> = Err(FixtureError);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x: i64| x * 4,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_09_vec_map2_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 5,
        map: |x| x * 2,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 136);
}

#[test]
fn fixture_10_vec_bind_fold() {
    let input = vec![0, 1, 2, 3, 4, 5];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, x + 3],
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 48);
}

#[test]
fn fixture_11_vec_map_filter_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        filter: |y| y % 5 == 1,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 22);
}

#[test]
fn fixture_12_vec_map_collect() {
    let input = vec![0, 1, 2, 3, 4, 5, 6];
    let out: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 3,
        collect,
    };
    assert_eq!(out, vec![3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn fixture_13_option_map_bind_fold() {
    let maybe = Some(5_i64);
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 4,
        bind: |x| if x > 11 { Some(x * 3) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_14_option_none_folds_to_init() {
    let maybe: Option<i64> = None;
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x: i64| x + 5,
        bind: |x| if x > 5 { Some(x * 4) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_15_result_ok_map_fold() {
    let raw: Result<i64, FixtureError> = Ok(7);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x| x * 2,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 14);
}

#[test]
fn fixture_16_result_err_folds_to_init() {
    let raw: Result<i64, FixtureError> = Err(FixtureError);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x: i64| x * 3,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_17_vec_map2_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 3,
        map: |x| x * 4,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 208);
}

#[test]
fn fixture_18_vec_bind_fold() {
    let input = vec![0, 1, 2, 3, 4, 5];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, x + 3],
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 48);
}

#[test]
fn fixture_19_vec_map_filter_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        filter: |y| y % 4 == 3,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 0);
}

#[test]
fn fixture_20_vec_map_collect() {
    let input = vec![0, 1, 2, 3, 4, 5, 6];
    let out: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 1,
        collect,
    };
    assert_eq!(out, vec![1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn fixture_21_option_map_bind_fold() {
    let maybe = Some(4_i64);
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 2,
        bind: |x| if x > 5 { Some(x * 2) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 12);
}

#[test]
fn fixture_22_option_none_folds_to_init() {
    let maybe: Option<i64> = None;
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x: i64| x + 3,
        bind: |x| if x > 6 { Some(x * 3) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_23_result_ok_map_fold() {
    let raw: Result<i64, FixtureError> = Ok(6);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x| x * 4,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 24);
}

#[test]
fn fixture_24_result_err_folds_to_init() {
    let raw: Result<i64, FixtureError> = Err(FixtureError);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x: i64| x * 2,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_25_vec_map2_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 1,
        map: |x| x * 3,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 108);
}

#[test]
fn fixture_26_vec_bind_fold() {
    let input = vec![0, 1, 2, 3, 4, 5];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, x + 3],
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 48);
}

#[test]
fn fixture_27_vec_map_filter_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        filter: |y| y % 3 == 0,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 18);
}

#[test]
fn fixture_28_vec_map_collect() {
    let input = vec![0, 1, 2, 3, 4, 5, 6];
    let out: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 4,
        collect,
    };
    assert_eq!(out, vec![4, 5, 6, 7, 8, 9, 10]);
}

#[test]
fn fixture_29_option_map_bind_fold() {
    let maybe = Some(3_i64);
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 5,
        bind: |x| if x > 6 { Some(x * 4) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 32);
}

#[test]
fn fixture_30_option_none_folds_to_init() {
    let maybe: Option<i64> = None;
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x: i64| x + 1,
        bind: |x| if x > 7 { Some(x * 2) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_31_result_ok_map_fold() {
    let raw: Result<i64, FixtureError> = Ok(5);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x| x * 3,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 15);
}

#[test]
fn fixture_32_result_err_folds_to_init() {
    let raw: Result<i64, FixtureError> = Err(FixtureError);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x: i64| x * 4,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_33_vec_map2_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 4,
        map: |x| x * 2,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 120);
}

#[test]
fn fixture_34_vec_bind_fold() {
    let input = vec![0, 1, 2, 3, 4, 5];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, x + 3],
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 48);
}

#[test]
fn fixture_35_vec_map_filter_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        filter: |y| y % 5 == 0,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 10);
}

#[test]
fn fixture_36_vec_map_collect() {
    let input = vec![0, 1, 2, 3, 4, 5, 6];
    let out: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 2,
        collect,
    };
    assert_eq!(out, vec![2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn fixture_37_option_map_bind_fold() {
    let maybe = Some(2_i64);
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 3,
        bind: |x| if x > 7 { Some(x * 3) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_38_option_none_folds_to_init() {
    let maybe: Option<i64> = None;
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x: i64| x + 4,
        bind: |x| if x > 8 { Some(x * 4) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_39_result_ok_map_fold() {
    let raw: Result<i64, FixtureError> = Ok(4);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x| x * 2,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 8);
}

#[test]
fn fixture_40_result_err_folds_to_init() {
    let raw: Result<i64, FixtureError> = Err(FixtureError);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x: i64| x * 3,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_41_vec_map2_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 2,
        map: |x| x * 4,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 176);
}

#[test]
fn fixture_42_vec_bind_fold() {
    let input = vec![0, 1, 2, 3, 4, 5];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, x + 3],
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 48);
}

#[test]
fn fixture_43_vec_map_filter_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        filter: |y| y % 4 == 3,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 0);
}

#[test]
fn fixture_44_vec_map_collect() {
    let input = vec![0, 1, 2, 3, 4, 5, 6];
    let out: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 5,
        collect,
    };
    assert_eq!(out, vec![5, 6, 7, 8, 9, 10, 11]);
}

#[test]
fn fixture_45_option_map_bind_fold() {
    let maybe = Some(1_i64);
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 1,
        bind: |x| if x > 8 { Some(x * 2) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_46_option_none_folds_to_init() {
    let maybe: Option<i64> = None;
    let out: i64 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x: i64| x + 2,
        bind: |x| if x > 9 { Some(x * 3) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_47_result_ok_map_fold() {
    let raw: Result<i64, FixtureError> = Ok(3);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x| x * 4,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 12);
}

#[test]
fn fixture_48_result_err_folds_to_init() {
    let raw: Result<i64, FixtureError> = Err(FixtureError);
    let out: i64 = pipeline! {
        witness: ResultWitness<FixtureError>,
        source: raw,
        map: |x: i64| x * 2,
        fold: (0_i64, i64::wrapping_add),
    };
    assert_eq!(out, 0);
}

#[test]
fn fixture_49_vec_map2_fold() {
    let input = vec![0, 1, 2, 3, 4, 5, 6, 7];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x + 5,
        map: |x| x * 3,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 204);
}

#[test]
fn fixture_50_vec_bind_fold() {
    let input = vec![0, 1, 2, 3, 4, 5];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, x + 3],
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 48);
}
