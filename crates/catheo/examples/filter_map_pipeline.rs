//! The partial arrow — `filter_map` — as a consumer pipeline, runnable.
//!
//! ```text
//! cargo run -p catheo --example filter_map_pipeline
//! ```
//!
//! The stage is the §4.3 v0.6 partial arrow: `T -> Option<U>` on the
//! pipeline surface, std's `filter_map` on `Vec` and `and_then` on
//! `Option`. The demo crosses it together with the traversal boundary —
//! `filter_map` prunes the junk records in the fused prefix (one pass, no
//! intermediate container — the §4.2 `filter`+`map` spelling pays a
//! barrier and a pass the arrow does not), and `traverse` validates every
//! kept record against a bound.
//!
//! The observability lives around the run, on the consumer seam: `tracing`
//! spans bracket the batch and each pipeline invocation, and the fallible
//! bound check renders as a `miette` diagnostic with help text instead of
//! a bare message. Both dependencies are dev-only — the shipped crate
//! stays dependency-free.
//!
//! Set `RUST_LOG=debug` to widen the span/event filter.
use catheo::{ResultWitness, VecWitness, pipeline};
use miette::Diagnostic;
use thiserror::Error;
use tracing::instrument;

/// The fallible per-record bound the traversal arrow lands in.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic)]
#[error("`{record}` exceeds the u16 bound")]
#[diagnostic(
    code(catheo::out_of_range),
    help(
        "kept records must fit u16; the first out-of-range record \
          short-circuits the traversal (§4.3 v0.3) and every later element \
          is left unforced — prune the junk first with `filter_map`"
    )
)]
struct OutOfRange {
    /// The offending record, carried for the diagnostic rendering.
    record: i32,
}

/// Validate one kept record — the traversal arrow `A -> G<B>` over the
/// effect.
fn check_u16(record: i32) -> Result<i32, OutOfRange> {
    if (0..=i32::from(u16::MAX)).contains(&record) {
        Ok(record)
    } else {
        Err(OutOfRange { record })
    }
}

/// One instrumented batch: a `tracing` span brackets the run; the fused
/// prefix parses the records partially (junk dropped, no container), and
/// the traversal collects one effect around the kept, validated records.
#[instrument(skip(records), fields(elements = records.len()))]
fn run_batch(records: Vec<String>) -> Result<Vec<i32>, OutOfRange> {
    let span = tracing::info_span!("pipeline.run", elements = records.len());
    let _guard = span.enter();
    let kept: Result<Vec<i32>, OutOfRange> = pipeline! {
        witness: VecWitness,
        source: records,
        map: |s: String| s.trim().to_owned(),
        filter_map: |s: String| s.parse::<i32>().ok(),
        traverse: (ResultWitness::<OutOfRange>, check_u16),
        collect,
    };
    tracing::info!(?kept, "pipeline complete");
    kept
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let warm = run_batch(vec![
        "41".to_owned(),
        "  junk  ".to_owned(),
        "13".to_owned(),
        "not a number".to_owned(),
        "7".to_owned(),
    ]);
    println!("warm batch: {warm:?}");

    // The pointwise spelling: `filter_map` over `Option` is `and_then`.
    let pointwise: Option<i64> = pipeline! {
        witness: catheo::OptionWitness,
        source: Some(21_i32),
        filter_map: |x: i32| if x > 0 { Some(i64::from(x) * 2) } else { None },
    };
    println!("pointwise filter_map (and_then): {pointwise:?}");

    // The cold batch keeps a record the bound rejects; the miette report
    // renders the error with its code and help text — the consumer-facing
    // diagnostic, not a bare message.
    let cold = run_batch(vec!["41".to_owned(), "70000".to_owned()]);
    match cold {
        Ok(kept) => println!("cold batch: {kept:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("cold batch failed:\n{report:?}");
        }
    }
}
