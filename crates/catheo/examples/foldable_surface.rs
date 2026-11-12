//! The strict `Foldable` surface — the eliminator-side unification,
//! runnable.
//!
//! ```text
//! cargo run -p catheo --example foldable_surface
//! ```
//!
//! The §2.4 posture extended to the fold side: one trait — `fold`
//! required, `fold_map` provided (derived from `fold` through the
//! [`Monoid`] identity and associativity laws) — over every witness. A
//! consumer program bounds on the single [`Foldable`] parameter and
//! eliminates, monomorphized, across `Vec`, `Option`, and `Result` — the
//! fold-side twin of the `strict_monad` demo's one-`Monad`-bound program.
//! The derived `fold_map` is pinned value-identical to the split kernel
//! and the allocation profile strict == split-kernel by
//! `tests/foldable_gates.rs`; `benches/foldable.rs` re-applies the §6.1
//! protocol (C ≈ B: the bound erases).
//!
//! The observable seam is the house posture: `tracing` spans bracket each
//! section and `miette` help text renders the error path — both
//! dev-only, so the shipped crate stays dependency-free.

use catheo::{Foldable, Monoid, OptionWitness, ResultWitness, VecWitness};
use miette::Diagnostic;
use thiserror::Error;

/// The fallible producer feeding the `Result` universe: every record must
/// be a lowercase word; the first offender becomes the carried `Err`.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic, PartialEq, Eq)]
#[error("`{record}` is not a lowercase word")]
#[diagnostic(
    code(catheo::bad_record),
    help(
        "the eliminator never drops a record: an `Err` contributes the \
          monoid identity by the documented denotation, and a rejected \
          producer is fixed before the fold — this help names the fix"
    )
)]
struct BadRecord {
    /// The offending record, carried for the diagnostic rendering.
    record: String,
}

/// A user-defined `Monoid`: two metrics in one accumulator, combined
/// elementwise — exactly the algebra the derived `fold_map` eliminates
/// into (§4.3 v0.2). The identity is the zero record; `combine` is
/// associative because both components are.
#[derive(Debug, PartialEq, Eq)]
struct WordStats {
    /// How many records contributed.
    words: usize,
    /// The total letter count.
    letters: usize,
}

impl Monoid for WordStats {
    fn identity() -> Self {
        Self {
            words: <usize as Monoid>::identity(),
            letters: <usize as Monoid>::identity(),
        }
    }

    fn combine(self, other: Self) -> Self {
        Self {
            words: self.words.combine(other.words),
            letters: self.letters.combine(other.letters),
        }
    }
}

/// The consumer seam the layer exists for: ONE generic program over ONE
/// `Foldable` bound, eliminating into a user `Monoid`. It instantiates
/// for every witness at the call sites below — no split-trait pair is
/// named anywhere in this file's generic body.
fn stats<W: Foldable>(records: W::Type<String>) -> WordStats {
    W::fold_map(records, |record: String| WordStats {
        words: 1,
        letters: record.len(),
    })
}

/// The fallible producer of the `Result` universe.
fn parse_records(records: Vec<String>) -> Result<Vec<String>, BadRecord> {
    let first_bad = records
        .iter()
        .find(|record| record.chars().any(|c: char| !c.is_ascii_lowercase()));
    match first_bad {
        Some(record) => Err(BadRecord {
            record: record.clone(),
        }),
        None => Ok(records),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // ── The container universe: `fold`, the universal eliminator ───────────
    let span = tracing::info_span!("foldable.fold", elements = 4);
    let _guard = span.enter();
    let total =
        <VecWitness as Foldable>::fold(vec![1_i32, 2, 3, 4], 0_i64, |acc, x| acc + i64::from(x));
    assert_eq!(total, 10);
    println!("fold over Vec (the universal eliminator): {total}");

    // ── The derived `fold_map` into a user `Monoid`, warm ──────────────────
    let span = tracing::info_span!("foldable.fold_map", records = 3);
    let _guard = span.enter();
    let warm = parse_records(vec![
        "alpha".to_owned(),
        "be".to_owned(),
        "gamma".to_owned(),
    ]);
    match warm {
        Ok(records) => {
            let summary = stats::<VecWitness>(records);
            assert_eq!(
                summary,
                WordStats {
                    words: 3,
                    letters: 12
                }
            );
            println!("fold_map into a user Monoid (warm): {summary:?}");
        }
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("parse_records (warm) unexpectedly rejected:\n{report:?}");
        }
    }

    // ── The one-bound promise: the same program, pointwise witnesses ───────
    let span = tracing::info_span!("foldable.one_bound");
    let _guard = span.enter();
    let single = stats::<OptionWitness>(Some("headline".to_owned()));
    assert_eq!(
        single,
        WordStats {
            words: 1,
            letters: 8
        }
    );
    let absent = stats::<OptionWitness>(None::<String>);
    assert_eq!(
        absent,
        WordStats::identity(),
        "None contributes the identity"
    );
    println!("the same bound over Option: {single:?} (None -> identity: {absent:?})");

    // The `Err` denotation, demonstrated: an `Err` contributes the
    // monoid identity — the eliminator never fabricates a dropped value
    // (the `filter` posture's eliminator-side twin).
    let cold_fold: Result<i32, BadRecord> = Err(BadRecord {
        record: "Not A Word".to_owned(),
    });
    let identity =
        <ResultWitness<BadRecord> as Foldable>::fold(cold_fold, 0_i64, |acc, x| acc + i64::from(x));
    assert_eq!(identity, 0);
    println!("fold over an Err contributes the identity: {identity}");

    // ── The cold path: the producer's rejection, rendered ──────────────────
    let span = tracing::info_span!("foldable.cold");
    let _guard = span.enter();
    let cold = parse_records(vec!["alpha".to_owned(), "Not A Word".to_owned()]);
    match cold {
        Ok(records) => println!("parse_records (cold): {records:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("parse_records (cold) rejected:\n{report:?}");
        }
    }
}
