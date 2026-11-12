//! The §4.2 source-ascription surface, runnable — a whole pipeline file
//! with no `witness:` entry and no witness identifier spelled anywhere.
//!
//! ```text
//! cargo run -p catheo --example ascribed_witnesses
//! ```
//!
//! `source: expr as Type` derives the head witness from the ascription
//! (§4.2): `Vec<T>` derives the vector witness, `Option<T>` the pointwise
//! option witness, and `Result<T, E>` the parameterized result witness.
//! Only the *last* path segment decides, so the fully qualified
//! `std::vec::Vec<i32>` spelling derives too. The derivation is
//! compile-time only: the expansion re-anchors the head expression with
//! the user's written `: Type` annotation, so the spelled type still
//! participates in inference — the runtime shape is the witness-spelled
//! pipeline's, byte for byte (`benches/ascription.rs` measures that
//! parity; §6.1).
//!
//! The observable seam is the house posture: `tracing` spans bracket each
//! universe, and the fallible path renders as a `miette` diagnostic with
//! help text instead of a bare message. Both dependencies are dev-only —
//! the shipped crate stays dependency-free.

use catheo::pipeline;
use miette::Diagnostic;
use thiserror::Error;
use tracing::instrument;

/// The fallible per-line check the ascribed result head carries.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic, PartialEq, Eq)]
#[error("`{line}` is not a lowercase word")]
#[diagnostic(
    code(catheo::bad_line),
    help(
        "the ascribed `as Result<T, E>` spelling derives the parameterized \
          witness from the error type alone; the first bad line forwards \
          unchanged out of the pipeline, ready for this diagnostic"
    )
)]
struct BadLine {
    /// The offending line, carried for the diagnostic rendering.
    line: String,
}

/// The fallible producer feeding universe 3: every line must be a
/// lowercase word; the first offender becomes the carried `Err`.
fn parse_lines(lines: Vec<String>) -> Result<Vec<String>, BadLine> {
    let first_bad = lines
        .iter()
        .find(|line| line.chars().any(|c: char| !c.is_ascii_lowercase()));
    match first_bad {
        Some(line) => Err(BadLine { line: line.clone() }),
        None => Ok(lines),
    }
}

/// The eliminator's callable form (the grammar's `fold` entry): a named
/// two-parameter fold, lint-silent in the expanded code — the inline
/// `|acc, x| acc + x` spelling trips `clippy::unnecessary_fold` through
/// the macro expansion.
fn sum_i64(acc: i64, x: i64) -> i64 {
    acc + x
}

/// Universe 1 — a `Vec<i32>` ascription derives the vector witness: the
/// fused map → partial-arrow → fold pass, no `witness:` entry in sight.
#[instrument(skip(raw), fields(elements = raw.len()))]
fn even_doubles(raw: Vec<i32>) -> i64 {
    let span = tracing::info_span!("pipeline.run", kind = "vec ascription");
    let _guard = span.enter();
    let total: i64 = pipeline! {
        source: raw as Vec<i32>,
        map: |x| x * 2,
        filter_map: |x| if x % 4 == 0 { Some(i64::from(x) * 2) } else { None },
        fold: (0_i64, sum_i64),
    };
    tracing::info!(total, "vec universe complete");
    total
}

/// Universe 2 — an `Option<String>` ascription derives the pointwise
/// witness: the monoid eliminator runs per element, `None` contributing
/// the identity.
#[instrument(skip(maybe))]
fn headline_length(maybe: Option<String>) -> usize {
    let span = tracing::info_span!("pipeline.run", kind = "option ascription");
    let _guard = span.enter();
    let letters: usize = pipeline! {
        source: maybe as Option<String>,
        fold_map: |line: String| line.len(),
    };
    tracing::info!(letters, "option universe complete");
    letters
}

/// Universe 3 — a `Result<Vec<String>, BadLine>` ascription derives the
/// parameterized witness: the map rides the `Ok` arm and the first `Err`
/// forwards out of the pipeline unchanged, ready for the miette render.
#[instrument(skip(attempt))]
fn word_counts(attempt: Result<Vec<String>, BadLine>) -> Result<Vec<usize>, BadLine> {
    let span = tracing::info_span!("pipeline.run", kind = "result ascription");
    let _guard = span.enter();
    let counted: Result<Vec<usize>, BadLine> = pipeline! {
        source: attempt as Result<Vec<String>, BadLine>,
        map: |lines: Vec<String>| lines.iter().map(String::len).collect::<Vec<usize>>(),
    };
    tracing::info!(?counted, "result universe complete");
    counted
}

/// Universe 4 — the fully qualified spelling: only the last path segment
/// decides (§4.2), so `std::vec::Vec<Vec<i32>>` derives the same vector
/// witness, and the `join` desugar flattens the rows.
#[instrument(skip(rows), fields(rows = rows.len()))]
fn flatten_rows(rows: Vec<Vec<i32>>) -> Vec<i32> {
    let span = tracing::info_span!("pipeline.run", kind = "qualified vec ascription");
    let _guard = span.enter();
    let flat: Vec<i32> = pipeline! {
        source: rows as std::vec::Vec<Vec<i32>>,
        join,
        collect,
    };
    tracing::info!(elements = flat.len(), "qualified universe complete");
    flat
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let total = even_doubles(vec![1_i32, 2, 3, 4]);
    assert_eq!(total, 24); // kept arms: 8 + 16
    println!("vec ascription: {total}");

    let letters = headline_length(Some("headline".to_owned()));
    assert_eq!(letters, 8);
    let none = headline_length(None);
    assert_eq!(none, 0);
    println!("option ascription: {letters} (None contributes the identity: {none})");

    let warm = word_counts(parse_lines(vec!["alpha".to_owned(), "be".to_owned()]));
    assert_eq!(warm, Ok(vec![5, 2]));
    println!("result ascription (warm): {warm:?}");

    let flat = flatten_rows(vec![vec![1_i32, 2], vec![3, 4]]);
    assert_eq!(flat, vec![1, 2, 3, 4]);
    println!("qualified ascription: {flat:?}");

    // The cold path fails on the first bad line; the miette report renders
    // the error with its code and help text — the consumer-facing
    // diagnostic, not a bare message.
    let cold = word_counts(parse_lines(vec![
        "alpha".to_owned(),
        "Not A Word".to_owned(),
    ]));
    match cold {
        Ok(counted) => println!("result ascription (cold): {counted:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("result ascription failed:\n{report:?}");
        }
    }
}
