//! The strict one-trait surface — `Monad` across every witness, runnable.
//!
//! ```text
//! cargo run -p catheo --example strict_monad
//! ```
//!
//! The §2.4/§4.4 unification (v0.5): `pure` and `bind` live in **one**
//! trait, so a composed program bounds on a single `Monad` parameter and
//! runs, monomorphized, across `VecWitness`, `OptionWitness`, and
//! `ResultWitness` — no split-trait pair to name, no macro in the path.
//! Each required kernel delegates to the split layer of §2.2, so the
//! allocation profile is pinned equal to the split-kernel spelling
//! (`tests/v05_gates.rs`); the provided `join` and the derived `fmap`
//! (`bind ∘ pure`) are law-checked against the same kernels.
//!
//! The observability lives around the program, on the consumer seam:
//! `tracing` spans bracket each section, and the fallible step renders as
//! a `miette` diagnostic with help text. Both dependencies are dev-only —
//! the shipped crate stays dependency-free.
use catheo::{CategoricalTraverse, Monad, OptionWitness, ResultWitness, VecWitness};
use miette::Diagnostic;
use thiserror::Error;
use tracing::instrument;

/// One program, one bound. `M::pure` lifts, `M::bind` sequences; the same
/// function monomorphizes per witness at every call site below.
fn checked_doubling<M: Monad>(program: M::Type<i32>) -> M::Type<i64> {
    M::bind(program, |x| M::pure(i64::from(x) * 2))
}

/// The derived functor operation, from the same bound: `fmap` is
/// `bind(m, |x| pure(f(x)))` — the law gate pins it against the split
/// `run_map` kernel.
fn derived_labels<M: Monad>(program: M::Type<i32>) -> M::Type<String> {
    M::fmap(program, |x| format!("x={x}"))
}

/// The fallible per-record check the `Result` witness runs through the
/// unified trait — the miette attributes are the consumer-facing
/// diagnostic: a code for machine handling and help text that names the
/// fix (§6.2 posture — a rejection is never a riddle).
#[derive(Debug, Error, Diagnostic, PartialEq)]
#[error("`{record}` is not a number")]
#[diagnostic(
    code(catheo::bad_record),
    help(
        "records must parse as i32; the strict bind forwards the first Err \
        unchanged — no error is fabricated, merged or dropped"
    )
)]
struct BadRecord {
    /// The offending record, carried for the diagnostic rendering.
    record: String,
}

/// Lift the batch into the `Result` effect with `pure`, sequence one
/// `bind`, and inside the guard cross the §4.3 v0.3 traversal boundary —
/// the per-record effect-collection the split kernel exists for, with the
/// same generic strict program as the arrow.
#[instrument(skip(records), fields(elements = records.len()))]
fn run_batch(records: Vec<String>) -> Result<Vec<i64>, BadRecord> {
    let span = tracing::info_span!("monad.run_batch", elements = records.len());
    let _guard = span.enter();
    let doubled: Result<Vec<i64>, BadRecord> = <ResultWitness<BadRecord> as Monad>::bind(
        <ResultWitness<BadRecord> as Monad>::pure(records),
        |batch: Vec<String>| {
            <ResultWitness<BadRecord> as CategoricalTraverse>::run_traverse(
                batch.into_iter(),
                |record: String| {
                    checked_doubling::<ResultWitness<BadRecord>>(
                        record.parse::<i32>().map_err(|_| BadRecord { record }),
                    )
                },
            )
        },
    );
    tracing::info!(?doubled, "strict batch complete");
    doubled
}

fn main() {
    // ── One bound, three containers ───────────────────────────────────────
    let span = tracing::info_span!("monad.one_bound", program = "checked_doubling");
    let _guard = span.enter();
    let vecs: Vec<i64> = checked_doubling::<VecWitness>(vec![1_i32, 2, 3]);
    assert_eq!(vecs, vec![2, 4, 6]);
    let opt: Option<i64> = checked_doubling::<OptionWitness>(Some(21_i32));
    assert_eq!(opt, Some(42));
    let res: Result<i64, String> = checked_doubling::<ResultWitness<String>>(Ok(9_i32));
    assert_eq!(res, Ok(18));
    println!("one `Monad` bound across every witness: {vecs:?} / {opt:?} / {res:?}");

    // ── The derived `fmap` (bind ∘ pure), pinned against `run_map` ────────
    let span = tracing::info_span!("monad.derived_fmap");
    let _guard = span.enter();
    let labels: Vec<String> = derived_labels::<VecWitness>(vec![1_i32, 2, 3]);
    assert_eq!(labels, vec!["x=1", "x=2", "x=3"]);
    let label: Option<String> = derived_labels::<OptionWitness>(Some(7_i32));
    assert_eq!(label, Some("x=7".to_owned()));
    println!("derived `fmap` over the same bound: {labels:?} / {label:?}");

    // ── The provided `join` (bind(id)), concrete per witness ──────────────
    let span = tracing::info_span!("monad.provided_join");
    let _guard = span.enter();
    let flat: Vec<i32> = <VecWitness as Monad>::join(vec![vec![1_i32, 2], vec![3, 4]]);
    assert_eq!(flat, vec![1, 2, 3, 4]);
    let flat: Option<i32> = <OptionWitness as Monad>::join(Some(Some(21_i32)));
    assert_eq!(flat, Some(21));
    let flat: Result<i32, BadRecord> = <ResultWitness<BadRecord> as Monad>::join(Ok(Ok(41)));
    assert_eq!(flat, Ok(41));
    println!("provided `join` == bind(id): {flat:?}");

    // ── The fallible path: the miette diagnostic on the error ────────────
    let warm = run_batch(vec!["12".to_owned(), "34".to_owned(), "56".to_owned()]);
    assert_eq!(warm, Ok(vec![24, 68, 112]));
    println!("warm batch: {warm:?}");

    let cold = run_batch(vec!["12".to_owned(), "oops".to_owned(), "56".to_owned()]);
    match cold {
        Ok(parsed) => println!("cold batch: {parsed:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("cold batch failed:\n{report:?}");
        }
    }
}
