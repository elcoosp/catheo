//! The monadic surface — `join` and `fold_map`, runnable.
//!
//! ```text
//! cargo run -p catheo --example monadic_surface
//! ```
//!
//! The v0.4 `join` stage is the §4.3 monadic multiplication — `bind(id)`,
//! written as the bare keyword. On `Vec` it flattens one nesting level and
//! fuses with a map prefix into a single `flat_map` pass (§4.3.2); on the
//! pointwise witnesses (`OptionWitness`, `ResultWitness<E>`) it lowers to
//! the `run_join` kernel, whose forwarded error is exactly std's
//! `and_then(identity)`. The v0.2 `fold_map:` terminal eliminates into any
//! `Monoid` — including tuples, so two metrics leave in one pass.
//!
//! The runs are bracketed in `tracing` spans and the fallible surface
//! renders through `miette` with help text — the same observable posture as
//! `instrumented_pipeline`, which also shows the subscriber setup.

use catheo::{OptionWitness, ResultWitness, VecWitness, pipeline};
use miette::Diagnostic;
use thiserror::Error;

/// The error of the fallible inner check the `Result` join flattens.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic, PartialEq, Eq)]
#[error("`{record}` failed the inner check")]
#[diagnostic(
    code(catheo::bad_inner_record),
    help(
        "join flattens one `W<W<T>>` level (§4.3): an inner `Err` is \
         forwarded unchanged as the outer error — no error is fabricated, \
         merged or dropped"
    )
)]
struct BadInnerRecord {
    /// The offending record, carried for the diagnostic rendering.
    record: String,
}

fn main() {
    // ── join on `Vec`: the fused flat_map pass ─────────────────────────────
    let span = tracing::info_span!("monadic.vec_join", elements = 3);
    let _guard = span.enter();
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3],
        map: |x| vec![x, x],
        join,
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(total, 12); // 1,1,2,2,3,3
    tracing::info!(total, "vec join complete");
    println!("vec join (map → join fused): {total}");

    // ── join on `Option`: the pointwise kernel ─────────────────────────────
    let span = tracing::info_span!("monadic.option_join");
    let _guard = span.enter();
    let nested: Option<Option<i64>> = Some(Some(21_i64));
    let flat: Option<i64> = pipeline! {
        witness: OptionWitness,
        source: nested,
        join,
    };
    assert_eq!(flat, Some(21));
    println!("option join: {flat:?}");

    // ── fold_map: one pass, two monoid metrics ─────────────────────────────
    let span = tracing::info_span!("monadic.fold_map", elements = 6);
    let _guard = span.enter();
    let letters: usize = pipeline! {
        witness: VecWitness,
        source: vec![
            "alpha".to_owned(),
            "be".to_owned(),
            "gamma".to_owned(),
        ],
        fold_map: |word: String| word.len(),
    };
    assert_eq!(letters, 12); // 5 + 2 + 5
    println!("fold_map usize monoid: {letters} letters");

    let numbers = vec![1_i64, 2, 3, 4, 5, 6];
    let (count, sum): (usize, i64) = pipeline! {
        witness: VecWitness,
        source: numbers,
        fold_map: |x: i64| (1_usize, x),
    };
    assert_eq!((count, sum), (6, 21));
    println!("fold_map tuple monoid: {count} elements summing to {sum}");

    // ── join on `Result`: the inner error forwards unchanged ───────────────
    let span = tracing::info_span!("monadic.result_join");
    let _guard = span.enter();
    let warm: Result<Result<i32, BadInnerRecord>, BadInnerRecord> = Ok(Ok(41));
    let flat: Result<i32, BadInnerRecord> = pipeline! {
        witness: ResultWitness::<BadInnerRecord>,
        source: warm,
        join,
    };
    assert_eq!(flat, Ok(41));
    println!("result join (warm): {flat:?}");

    let cold: Result<Result<i32, BadInnerRecord>, BadInnerRecord> = Ok(Err(BadInnerRecord {
        record: "7b".to_owned(),
    }));
    let flat: Result<i32, BadInnerRecord> = pipeline! {
        witness: ResultWitness::<BadInnerRecord>,
        source: cold,
        join,
    };
    match flat {
        Ok(value) => println!("result join (cold): {value:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("result join (cold) forwarded the inner error:\n{report:?}");
        }
    }
}
