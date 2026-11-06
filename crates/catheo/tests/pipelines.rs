//! End-to-end `pipeline!` behavior tests (M0 naive lowering).
//!
//! These pin the *semantics* the M2 deforestation pass must preserve
//! (§6.2 "fused == unfused"): whatever the optimizer rewrites, the result
//! values of these pipelines may never change.
//
// Stage closures below deliberately exercise specific syntactic shapes
// (`.iter()` to trip the structural-observer barrier, closure form instead
// of method references to keep the closure-header path covered), so the
// clippy suggestions would defeat the tests' purpose.
#![allow(
    clippy::iter_cloned_collect,
    clippy::redundant_closure_for_method_calls
)]

use catheo::{OptionWitness, ResultWitness, VecWitness, pipeline};

#[derive(Debug, PartialEq)]
struct ParseError;

#[test]
fn the_architecture_headline_pipeline() {
    // §4.1: map → map → fold with type inference flowing through the chain.
    let input_vector = vec![1_i32, 2, 3, 4];
    let total: usize = pipeline! {
        witness: VecWitness,
        source: input_vector,
        map: |x| x * 2,
        map: |x| x.to_string(),
        fold: (0_usize, |acc, s| acc + s.len()),
    };
    assert_eq!(total, 4, "digits: 2,4,6,8 are each one character");
}

#[test]
fn bind_flattens_and_fold_elimiminates() {
    let input = vec![1_i32, 2, 3];
    let sum: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        bind: |x| vec![x, -x],
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    };
    assert_eq!(sum, 0, "every element annihilates its negation");
}

#[test]
fn filter_is_an_absolute_barrier_but_semantically_transparent() {
    let input = vec![1_i32, 2, 3, 4, 5, 6];
    let sum: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 10,
        filter: |x| x % 20 == 0,
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    };
    assert_eq!(sum, 120, "20 + 40 + 60");
}

#[test]
fn observing_bind_blocks_fusion_but_keeps_semantics() {
    let rows = vec![vec![1_i32, 2], vec![3], vec![], vec![4, 5]];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: rows,
        bind: |row| row.iter().copied().collect::<Vec<i32>>(),
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    };
    assert_eq!(total, 15);
}

#[test]
fn collect_terminal_hands_back_the_container() {
    let input = vec![1_i32, 2, 3];
    let out: Vec<i32> = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * 2,
        filter: |x| *x != 4,
        collect,
    };
    assert_eq!(out, vec![2, 6]);
}

#[test]
fn identity_pipeline_is_its_source() {
    let input = vec![7_i64, 8];
    let out: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: input,
        collect,
    };
    assert_eq!(out, vec![7, 8]);
}

#[test]
fn option_witness_bind_chain_short_circuits() {
    let maybe = Some(2_i32);
    let out: i32 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 1,
        bind: |x| if x > 2 { Some(x * 10) } else { None },
        fold: (0_i32, |acc, x| acc + x),
    };
    assert_eq!(out, 30);

    let maybe: Option<i32> = None;
    let out: i32 = pipeline! {
        witness: OptionWitness,
        source: maybe,
        map: |x: i32| x + 1,
        bind: |x| if x > 2 { Some(x * 10) } else { None },
        fold: (0_i32, |acc, x| acc + x),
    };
    assert_eq!(out, 0, "None leaves the accumulator at its initial value");
}

#[test]
fn result_witness_error_chain_accumulates_ok_values() {
    let raw = Ok::<&str, ParseError>("42");
    let out: usize = pipeline! {
        witness: ResultWitness<ParseError>,
        source: raw,
        map: |s| s.len(),
        fold: (0_usize, |acc, n| acc + n),
    };
    assert_eq!(out, 2);

    let raw: Result<&str, ParseError> = Err(ParseError);
    let out: usize = pipeline! {
        witness: ResultWitness<ParseError>,
        source: raw,
        map: |s| s.len(),
        fold: (0_usize, |acc, n| acc + n),
    };
    assert_eq!(out, 0, "Err leaves the accumulator at its initial value");
}

#[test]
fn result_witness_bind_runs_the_parser_pipeline() {
    let parse = |s: String| -> Result<Vec<u8>, ParseError> {
        s.bytes()
            .map(|b| {
                if b.is_ascii_digit() {
                    Ok(b - b'0')
                } else {
                    Err(ParseError)
                }
            })
            .collect()
    };
    let raw = Ok::<String, ParseError>("42".to_string());
    let digits: usize = pipeline! {
        witness: ResultWitness<ParseError>,
        source: raw,
        bind: parse,
        fold: (0_usize, |acc, digits: Vec<u8>| acc + digits.len()),
    };
    assert_eq!(digits, 2, "two ASCII digits parsed");

    let raw = Ok::<String, ParseError>("4x2".to_string());
    let digits: usize = pipeline! {
        witness: ResultWitness<ParseError>,
        source: raw,
        bind: parse,
        fold: (0_usize, |acc, digits: Vec<u8>| acc + digits.len()),
    };
    assert_eq!(digits, 0, "non-digit must poison the chain");
}

#[test]
fn closures_capture_the_call_site_environment() {
    let factor = 3_i64;
    let input = vec![1_i64, 2];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x * factor,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(total, 9);
}
