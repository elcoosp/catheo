//! M1 kernel specs for the §4.3 v0.1 combinators `pure` and `join` (TDD:
//! written first).
//!
//! `pure` is the pointed operation (the monad's `return`): it injects a bare
//! value into the witness's minimal container. `join` is monadic squash:
//! `join(m) = bind(m, identity)` (§4.3). Both must exist as runtime kernels
//! for every built-in witness so the M2 codegen can desugar against them.

use catheo::{
    CategoricalFold, CategoricalMonad, CategoricalPointed, OptionWitness, ResultWitness, VecWitness,
};

#[derive(Debug, PartialEq)]
struct ParseError {
    at: usize,
}

// ─── pure (CategoricalPointed::run_pure) ────────────────────────────────────

#[test]
fn vec_run_pure_injects_a_single_element() {
    let out = <VecWitness as CategoricalPointed>::run_pure(7_i32);
    assert_eq!(out, vec![7]);
}

#[test]
fn option_run_pure_is_some() {
    let out = <OptionWitness as CategoricalPointed>::run_pure("catheo");
    assert_eq!(out, Some("catheo"));
}

#[test]
fn result_run_pure_is_ok() {
    let out = <ResultWitness<ParseError> as CategoricalPointed>::run_pure::<i32>(7);
    assert_eq!(out, Ok::<i32, ParseError>(7));
}

#[test]
fn pure_is_the_compile_time_identity_kernel() {
    // §4.3: `pure` is documented as a compile-time identity: it adds exactly
    // one minimal container layer and nothing else. Folding it away must
    // reproduce the injected value verbatim.
    let folded = <VecWitness as CategoricalFold>::run_fold(
        <VecWitness as CategoricalPointed>::run_pure(42_i32),
        0_i32,
        |acc, x| acc + x,
    );
    assert_eq!(folded, 42);
}

// ─── join (CategoricalMonad::run_join) ──────────────────────────────────────

#[test]
fn vec_run_join_flattens_one_level_in_order() {
    let nested = vec![vec![1, 2], vec![], vec![3]];
    let out = <VecWitness as CategoricalMonad>::run_join(nested);
    assert_eq!(out, vec![1, 2, 3]);
}

#[test]
fn option_run_join_squashes_some_some() {
    let out = <OptionWitness as CategoricalMonad>::run_join(Some(Some(7)));
    assert_eq!(out, Some(7));
    // `Some(None)` collapses to `None`: the inner container wins.
    let out = <OptionWitness as CategoricalMonad>::run_join(Some(None::<i32>));
    assert_eq!(out, None);
    let none: Option<Option<i32>> = None;
    let out = <OptionWitness as CategoricalMonad>::run_join(none);
    assert_eq!(out, None);
}

#[test]
fn result_run_join_squashes_ok_ok() {
    let ok_ok: Result<Result<i32, ParseError>, ParseError> = Ok(Ok(7));
    let out = <ResultWitness<ParseError> as CategoricalMonad>::run_join(ok_ok);
    assert_eq!(out, Ok::<i32, ParseError>(7));

    // An inner `Err` propagates: the inner error type is authoritative.
    let ok_err: Result<Result<i32, ParseError>, ParseError> = Ok(Err(ParseError { at: 3 }));
    let out = <ResultWitness<ParseError> as CategoricalMonad>::run_join(ok_err);
    assert_eq!(out, Err(ParseError { at: 3 }));

    let err: Result<Result<i32, ParseError>, ParseError> = Err(ParseError { at: 9 });
    let out = <ResultWitness<ParseError> as CategoricalMonad>::run_join(err);
    assert_eq!(out, Err(ParseError { at: 9 }));
}

#[test]
fn join_is_bind_of_the_identity_kernel() {
    // §4.3: `join` desugars to `bind(id)`; the kernel must not do anything
    // else. Compare against an explicit `run_bind` call with `Some`/`Ok`/
    // collection identity.
    let nested = vec![vec![1, 2], vec![3]];
    let by_join = <VecWitness as CategoricalMonad>::run_join(nested.clone());
    let by_bind = <VecWitness as CategoricalMonad>::run_bind(nested, |inner| inner);
    assert_eq!(by_join, by_bind);

    let nested = Some(Some(4));
    let by_join = <OptionWitness as CategoricalMonad>::run_join(nested);
    let by_bind = <OptionWitness as CategoricalMonad>::run_bind(nested, |inner| inner);
    assert_eq!(by_join, by_bind);

    let nested: Result<Result<u8, ParseError>, ParseError> = Ok(Ok(4));
    let by_join = <ResultWitness<ParseError> as CategoricalMonad>::run_join(nested);
    let nested: Result<Result<u8, ParseError>, ParseError> = Ok(Ok(4));
    let by_bind = <ResultWitness<ParseError> as CategoricalMonad>::run_bind(nested, |inner| inner);
    assert_eq!(by_join, by_bind);
}
