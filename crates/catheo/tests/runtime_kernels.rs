//! Layer 0/1 runtime kernel tests (TDD: written first).
//!
//! Drives the HKT witness pattern (§2.1) and the split-trait dual-witness
//! kernels (§2.2) for the `Vec`, `Option`, and `Result` type constructors.
//! All kernels must run on stable Rust (M1 gate).

use catheo::{
    CategoricalFilter, CategoricalFold, CategoricalFunctor, CategoricalIntoIter, CategoricalMonad,
    HKT, OptionWitness, ResultWitness, VecWitness,
};

#[derive(Debug, PartialEq)]
struct ParseError {
    at: usize,
}

fn parse_tokens(s: &str) -> Result<Vec<i32>, ParseError> {
    if s.is_empty() {
        return Err(ParseError { at: 0 });
    }
    let digits: Vec<i32> = s
        .chars()
        .map(|c| c.to_digit(10).map_or(0, |d| i32::try_from(d).unwrap_or(0)))
        .collect();
    Ok(digits)
}

#[test]
fn witnesses_are_zero_sized() {
    // Layer 0 contract: witnesses proxy for type constructors only; they
    // must never cost a byte or a cycle at runtime.
    assert_eq!(size_of::<VecWitness>(), 0);
    assert_eq!(size_of::<OptionWitness>(), 0);
    assert_eq!(size_of::<ResultWitness<ParseError>>(), 0);
}

fn assert_projection<W, T>()
where
    W: HKT<Type<T> = Vec<T>>,
{
}
fn assert_option_projection<W, T>()
where
    W: HKT<Type<T> = Option<T>>,
{
}
fn assert_result_projection<E, T>()
where
    ResultWitness<E>: HKT<Type<T> = Result<T, E>>,
{
}

#[test]
fn hkt_projection_resolves_the_type_constructor() {
    assert_projection::<VecWitness, i32>();
    assert_projection::<VecWitness, String>();
    assert_option_projection::<OptionWitness, i32>();
    assert_result_projection::<ParseError, i32>();
}

// ─── Vec kernels ────────────────────────────────────────────────────────────

#[test]
fn vec_run_map_preserves_order() {
    let out = <VecWitness as CategoricalFunctor>::run_map(vec![1, 2, 3], |x| x * 2);
    assert_eq!(out, vec![2, 4, 6]);
}

#[test]
fn vec_run_bind_flattens_in_order() {
    let out = <VecWitness as CategoricalMonad>::run_bind(vec![1, 2], |x| vec![x, x * 10]);
    assert_eq!(out, vec![1, 10, 2, 20]);
}

#[test]
fn vec_run_filter_keeps_only_matching() {
    let out = <VecWitness as CategoricalFilter>::run_filter(vec![1, 2, 3, 4], |x| x % 2 == 0);
    assert_eq!(out, vec![2, 4]);
}

#[test]
fn vec_run_fold_is_a_left_fold() {
    let out = <VecWitness as CategoricalFold>::run_fold(vec![1, 2, 3], 0, |acc, x| acc + x);
    assert_eq!(out, 6);
    let order: Vec<u32> =
        <VecWitness as CategoricalFold>::run_fold(vec![1, 2, 3], Vec::new(), |mut acc, x| {
            acc.push(x);
            acc
        });
    assert_eq!(order, vec![1, 2, 3], "fold must visit items head to tail");
}

#[test]
fn vec_into_iter_yields_all_items() {
    let items: Vec<i32> = <VecWitness as CategoricalIntoIter>::into_iter(vec![7, 8]).collect();
    assert_eq!(items, vec![7, 8]);
}

// ─── Option kernels ─────────────────────────────────────────────────────────

#[test]
fn option_run_map_transforms_some() {
    let out = <OptionWitness as CategoricalFunctor>::run_map(Some(2), |x| x + 1);
    assert_eq!(out, Some(3));
    let none: Option<i32> = None;
    let out = <OptionWitness as CategoricalFunctor>::run_map(none, |x: i32| x + 1);
    assert_eq!(out, None);
}

#[test]
fn option_run_bind_short_circuits_on_none() {
    let out = <OptionWitness as CategoricalMonad>::run_bind(Some(3), |x| Some(x * 2));
    assert_eq!(out, Some(6));
    let none: Option<i32> = None;
    let out = <OptionWitness as CategoricalMonad>::run_bind(none, |x: i32| Some(x * 2));
    assert_eq!(out, None);
}

#[test]
fn option_run_filter_discards_non_matching_some() {
    let out = <OptionWitness as CategoricalFilter>::run_filter(Some(3), |x| *x > 5);
    assert_eq!(out, None);
    let out = <OptionWitness as CategoricalFilter>::run_filter(Some(9), |x| *x > 5);
    assert_eq!(out, Some(9));
}

#[test]
fn option_run_fold_defaults_to_init_on_none() {
    let out = <OptionWitness as CategoricalFold>::run_fold(Some(5), 100, |acc, x| acc + x);
    assert_eq!(out, 105);
    let none: Option<i32> = None;
    let out = <OptionWitness as CategoricalFold>::run_fold(none, 100, |acc, x: i32| acc + x);
    assert_eq!(out, 100);
}

// ─── Result kernels ─────────────────────────────────────────────────────────

#[test]
fn result_run_map_transforms_ok() {
    let out: Result<i32, ParseError> =
        <ResultWitness<ParseError> as CategoricalFunctor>::run_map(Ok::<i32, ParseError>(2), |x| {
            x + 1
        });
    assert_eq!(out, Ok(3));
}

#[test]
fn result_run_bind_short_circuits_on_err() {
    let witness = <ResultWitness<ParseError> as CategoricalMonad>::run_bind;
    let out: Result<Vec<i32>, ParseError> = witness(Ok::<&str, ParseError>("42"), parse_tokens);
    assert_eq!(out, Ok(vec![4, 2]));
    let err = ParseError { at: 7 };
    let out: Result<Vec<i32>, ParseError> = witness(Err(err), parse_tokens);
    assert_eq!(out, Err(ParseError { at: 7 }));
}

#[test]
fn result_run_fold_skips_err() {
    let out: usize = <ResultWitness<ParseError> as CategoricalFold>::run_fold(
        Ok::<&str, ParseError>("42"),
        0usize,
        |acc, s: &str| acc + s.len(),
    );
    assert_eq!(out, 2);
    let out: usize = <ResultWitness<ParseError> as CategoricalFold>::run_fold(
        Err(ParseError { at: 1 }),
        0usize,
        |acc, s: &str| acc + s.len(),
    );
    assert_eq!(out, 0, "Err must leave the accumulator untouched");
}

#[test]
fn result_into_iter_yields_the_ok_value() {
    let items: Vec<i32> =
        <ResultWitness<ParseError> as CategoricalIntoIter>::into_iter(Ok::<i32, ParseError>(1))
            .collect();
    assert_eq!(items, vec![1]);
}
