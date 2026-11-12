//! The strict `Filterable` surface — the partial-arrow-side unification,
//! runnable.
//!
//! ```text
//! cargo run -p catheo --example filterable_surface
//! ```
//!
//! The §2.4 posture extended to the partial-arrow side: one trait —
//! `filter_map` required, `filter` provided (the predicate spelled
//! through the partial arrow it is) — over the witnesses that admit a
//! partial arrow. A consumer program bounds on the single
//! [`Filterable`] parameter and keeps, monomorphized, across `Vec` and
//! `Option` — the keep-or-drop twin of the `strict_monad` demo's
//! one-`Monad`-bound program. `ResultWitness<E>` admits no partial arrow
//! (a dropped `Ok` has no error-free denotation, §4.3.3); this demo's
//! fallible boundary is the traversal kernel that validates what the
//! arrow kept, rendered with `miette` on the cold path.
//!
//! The derived `filter` is pinned value-identical to the split kernel and
//! the allocation profile strict == split-kernel by
//! `tests/filterable_gates.rs`; `benches/filterable.rs` re-applies the
//! §6.1 protocol (C ≈ B: the bound erases).
//!
//! The observable seam is the house posture: `tracing` spans bracket each
//! section and `miette` help text renders the error path — both
//! dev-only, so the shipped crate stays dependency-free.

use catheo::{CategoricalTraverse, Filterable, OptionWitness, ResultWitness, VecWitness};
use miette::Diagnostic;
use thiserror::Error;

/// The fallible bound the traversal validates against: a kept record must
/// fit `u16` after halving.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic, PartialEq, Eq)]
#[error("`{input}` does not fit the u16 bound")]
#[diagnostic(
    code(catheo::out_of_bound),
    help(
        "the partial arrow pruned the odds; the traversal forwards the \
          first out-of-bound record unchanged and never forces later \
          elements (§4.3 v0.3) — prune harder upstream with `filter_map`"
    )
)]
struct OutOfBound {
    /// The offending input, carried for the diagnostic rendering.
    input: i64,
}

/// The traversal arrow: halve into the `u16` bound, or reject.
fn check_bound(x: i64) -> Result<i64, OutOfBound> {
    if u16::try_from(x).is_ok() {
        Ok(x)
    } else {
        Err(OutOfBound { input: x })
    }
}

/// The consumer seam the layer exists for: ONE generic program over ONE
/// `Filterable` bound — the partial arrow, then the derived `filter`. It
/// instantiates for both admitting witnesses at the call sites below; no
/// split-trait pair is named in this file's generic body.
fn kept_and_tagged<W: Filterable>(xs: W::Type<i32>) -> W::Type<i64> {
    let halved = W::filter_map(xs, |x: i32| {
        if x % 2 == 0 {
            Some(i64::from(x) / 2)
        } else {
            None
        }
    });
    // The derived predicate kernel rides the same bound: keep the
    // in-bound halves for the traversal to validate.
    W::filter(halved, |x: &i64| *x >= 0)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // ── The container universe: the partial arrow + the derived filter ─────
    let span = tracing::info_span!("filterable.vec", elements = 6);
    let _guard = span.enter();
    let kept = kept_and_tagged::<VecWitness>(vec![4_i32, 7, 16, 23, 8, 5]);
    assert_eq!(kept, vec![2_i64, 8, 4]);
    println!("filter_map + filter over Vec (one bound): {kept:?}");

    // The derived `filter` is the predicate spelled through the arrow:
    // the trivial predicates are the identity and the empty container.
    let everything = <VecWitness as Filterable>::filter(kept.clone(), |_| true);
    assert_eq!(everything, kept, "filter(always-true) is the identity");
    let nothing = <VecWitness as Filterable>::filter(kept.clone(), |_| false);
    assert!(nothing.is_empty(), "filter(always-false) is empty");
    println!("the trivial predicates: {everything:?} (identity) / {nothing:?} (empty)");

    // ── The pointwise universe: `and_then` on the kept arm ─────────────────
    let span = tracing::info_span!("filterable.option");
    let _guard = span.enter();
    let some = kept_and_tagged::<OptionWitness>(Some(16_i32));
    assert_eq!(some, Some(8_i64));
    let none = kept_and_tagged::<OptionWitness>(None::<i32>);
    assert_eq!(none, None, "None forwards unchanged");
    println!("the same bound over Option: {some:?} (None -> {none:?})");

    // ── The fallible boundary: validate what the arrow kept ────────────────
    let span = tracing::info_span!("filterable.traverse", kept = kept.len());
    let _guard = span.enter();
    let warm = <ResultWitness<OutOfBound> as CategoricalTraverse>::run_traverse(
        kept.clone().into_iter(),
        check_bound,
    );
    assert_eq!(warm, Ok(kept.clone()));
    println!("traverse over the kept records (warm): {warm:?}");

    // The cold path: an in-bound partial arrow cannot prune an out-of-bound
    // record it never saw as odd — the first offender forwards unchanged,
    // rendered with its help text.
    let polluted = vec![2_i64, 400_000, 8];
    let cold = <ResultWitness<OutOfBound> as CategoricalTraverse>::run_traverse(
        polluted.into_iter(),
        check_bound,
    );
    match cold {
        Ok(kept) => println!("traverse (cold): {kept:?}"),
        Err(err) => {
            assert_eq!(err, OutOfBound { input: 400_000 });
            let report = miette::Report::new(err);
            eprintln!("traverse (cold) short-circuited:\n{report:?}");
        }
    }
}
