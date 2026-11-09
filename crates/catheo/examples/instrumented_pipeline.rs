//! The observable consumer seam, runnable.
//!
//! ```text
//! cargo run -p catheo --example instrumented_pipeline
//! ```
//!
//! The pipeline itself is the fused, allocation-free pass the §6.1 gates
//! pin, so the observability lives *around* it, on the consumer seam:
//! `tracing` spans bracket the batch and each pipeline invocation, and the
//! fallible record check renders as a `miette` diagnostic with help text
//! instead of a bare message. Both dependencies are dev-only — the shipped
//! crate stays dependency-free.
//!
//! Set `RUST_LOG=debug` to widen the span/event filter.

use catheo::{ResultWitness, VecWitness, pipeline};
use miette::Diagnostic;
use thiserror::Error;
use tracing::instrument;

/// The fallible per-record check the traversal arrow lands in.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic)]
#[error("`{record}` is not a number")]
#[diagnostic(
    code(catheo::bad_record),
    help(
        "records must parse as i32; the first bad record short-circuits the \
          traversal (§4.3 v0.3) and every later element is left unforced"
    )
)]
struct BadRecord {
    /// The offending record, carried for the diagnostic rendering.
    record: String,
}

/// Parse one record — the traversal arrow `A -> G<B>` over the effect.
fn parse_record(record: String) -> Result<i32, BadRecord> {
    record.parse::<i32>().map_err(|_| BadRecord { record })
}

/// One instrumented batch: a `tracing` span brackets the run, the pipeline
/// is the fused traversal, and the outcome is logged as a structured event.
#[instrument(skip(records), fields(elements = records.len()))]
fn run_batch(records: Vec<String>) -> Result<Vec<i32>, BadRecord> {
    let span = tracing::info_span!("pipeline.run", elements = records.len());
    let _guard = span.enter();
    let parsed: Result<Vec<i32>, BadRecord> = pipeline! {
        witness: VecWitness,
        source: records,
        traverse: (ResultWitness::<BadRecord>, parse_record),
        collect,
    };
    tracing::info!(?parsed, "pipeline complete");
    parsed
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let warm = run_batch(vec!["12".to_owned(), "34".to_owned(), "56".to_owned()]);
    println!("warm batch: {warm:?}");

    // The cold batch fails on the second record; the miette report renders
    // the error with its code and help text — the consumer-facing
    // diagnostic, not a bare message.
    let cold = run_batch(vec!["12".to_owned(), "oops".to_owned(), "56".to_owned()]);
    match cold {
        Ok(parsed) => println!("cold batch: {parsed:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("cold batch failed:\n{report:?}");
        }
    }
}
