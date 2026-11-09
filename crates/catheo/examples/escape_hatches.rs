//! The §4.2 escape hatches and the §6.2 unfused oracle, runnable.
//!
//! ```text
//! cargo run -p catheo --example escape_hatches
//! ```
//!
//! The fusion analysis is conservative by design (§3.2): a `bind` closure
//! whose token text mentions container-shape markers (`.iter`, `.len`,
//! `Some`, `None`, `Ok`, `Err`) is treated as a structural observer and
//! keeps its kernel. False positives are acceptable — and the §4.2 escape
//! hatches are how a consumer overrides one in either direction:
//!
//! - `bind!: force` silences the observer flag, so the stage fuses and the
//!   fold absorbs it (documented as unsound if the closure really observes
//!   the container's shape);
//! - `bind!: materialize` is the opposite direction: the stage is a hard
//!   barrier and lowers through its kernel, materializing the intermediate
//!   container;
//! - `map!: force` is accepted but inert — maps are unconditional fusion
//!   targets.
//!
//! Alongside them live the two §4.2 conveniences: the witness-free spelling
//! (`source: rows as Vec<Vec<i32>>`, the witness derives from the
//! ascription) and the `Option` bind chain, which is exactly std's
//! `and_then` composed on the pipeline surface. The §6.2 oracle closes the
//! demo: `pipeline_unfused!` expands the same grammar without fusion, and
//! the example asserts the two lowerings agree value-for-value — expanding
//! a suspect pipeline through both macros isolates any mis-rewrite to the
//! fusion pass.
//!
//! The observable seam is the house posture: `tracing` spans bracket each
//! section and the fallible path renders as a `miette` diagnostic with
//! help text. Both dependencies are dev-only — no shipped artifact links
//! them. The fused-emission *shape* each hatch produces is pinned by the
//! deterministic gates (`tests/v02_gates.rs`), not by this run.

use catheo::{OptionWitness, ResultWitness, VecWitness, pipeline, pipeline_unfused};
use miette::Diagnostic;
use thiserror::Error;

/// The fallible per-record check the oracle's traversal lands in.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic, PartialEq, Eq)]
#[error("`{record}` is not a number")]
#[diagnostic(
    code(catheo::bad_record),
    help(
        "records must parse as i32; both lowerings (`pipeline!` and \
         `pipeline_unfused!`) forward the first `Err` unchanged, so the \
         oracle compares values, not error shapes — a disagreement belongs \
         to the fusion pass, never to the grammar"
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

fn main() {
    // ── Witness-free source + `bind!: force` past a false positive ────────
    let span = tracing::info_span!("hatches.force", rows = 2);
    let _guard = span.enter();
    // The witness derives from the `as Vec<Vec<i32>>` ascription (§4.2): no
    // `witness:` entry is written. The bind's body mentions `.iter`, so the
    // lexical scan (§3.2) flags it as a structural observer — a plain
    // `bind:` would keep its kernel and materialize the sums. The sum feeds
    // the element value only (one scalar per row), which is exactly the
    // `force` contract, so the mapless bind fuses and the fold absorbs it.
    let rows = vec![vec![1_i32, 2], vec![3, 4]];
    let total: i64 = pipeline! {
        source: rows as Vec<Vec<i32>>,
        bind!: force |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(total, 10); // 3 + 7
    println!("bind!: force past the observer, witness-free source: {total}");

    // ── `map!: force` (accepted, inert) + `bind!: materialize` ────────────
    let span = tracing::info_span!("hatches.materialize", elements = 3);
    let _guard = span.enter();
    // `map!: force` changes nothing on a map — maps are unconditional fusion
    // targets — but the spelling is accepted so a consumer can keep a
    // uniform hatch on every stage while debugging. `bind!: materialize` is
    // the real lever: the bind is a hard barrier, so the fused output keeps
    // its `run_bind` kernel and an intermediate container is materialized
    // no matter what the deforestation pass would otherwise do.
    let doubled: i64 = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3],
        map!: force |x: i32| x * 2,
        bind!: materialize |x: i32| vec![x, x],
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(doubled, 24); // 2,2,4,4,6,6
    println!("map!: force (inert) + bind!: materialize: {doubled}");

    // ── The Option bind chain: std's `and_then`, on the surface ───────────
    let span = tracing::info_span!("hatches.option_chain");
    let _guard = span.enter();
    // Each `bind:` on `OptionWitness` is an `and_then`: `None` short-circuits
    // the chain, `Some` feeds the next stage. The first pipeline terminates
    // with `filter` + `fold`; the second shows the bare `None` hand-off.
    let kept: i64 = pipeline! {
        witness: OptionWitness,
        source: Some(21_i32),
        bind: |x: i32| if x > 0 { Some(x * 2) } else { None },
        filter: |x: &i32| *x % 7 == 0,
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(kept, 42);
    println!("Option bind chain (and_then), filter, fold: {kept}");

    let short_circuit: Option<i64> = pipeline! {
        witness: OptionWitness,
        source: Some(-5_i32),
        bind: |x: i32| if x > 0 { Some(x * 2) } else { None },
        bind: |x: i32| Some(i64::from(x) + 1),
    };
    assert_eq!(short_circuit, None, "the first None stops the chain");
    println!("Option bind chain, short-circuited: {short_circuit:?}");

    // ── The §6.2 oracle: fused == unfused, value-for-value ────────────────
    let span = tracing::info_span!("hatches.oracle", records = 4);
    let _guard = span.enter();
    // A "suspect" pipeline — an observer-ish body, a filter barrier, a fold.
    // The same grammar runs through both macros; any value disagreement
    // would isolate the mis-rewrite to the fusion pass (§6.2), and the
    // property suite (`tests/fusion_properties.rs`) sweeps this agreement
    // over randomized inputs.
    let words = vec![
        "alpha".to_owned(),
        "be".to_owned(),
        "gamma".to_owned(),
        "x".to_owned(),
    ];
    let fused_letters: usize = pipeline! {
        witness: VecWitness,
        source: words.clone(),
        map: |word: String| word.len(),
        filter: |n: &usize| *n > 2,
        fold_map: |n: usize| n,
    };
    let oracle_letters: usize = pipeline_unfused! {
        witness: VecWitness,
        source: words,
        map: |word: String| word.len(),
        filter: |n: &usize| *n > 2,
        fold_map: |n: usize| n,
    };
    assert_eq!(fused_letters, oracle_letters, "fused == unfused (§6.2)");
    println!("oracle agreement (fold_map over letters): {fused_letters}");

    // ── The fallible oracle path: the first Err forwards, both lowerings ──
    let span = tracing::info_span!("hatches.fallible_oracle", records = 3);
    let _guard = span.enter();
    let records = vec!["12".to_owned(), "oops".to_owned(), "56".to_owned()];
    let cold: Result<i64, BadRecord> = pipeline! {
        witness: VecWitness,
        source: records.clone(),
        traverse: (ResultWitness::<BadRecord>, parse_record),
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    let cold_oracle: Result<i64, BadRecord> = pipeline_unfused! {
        witness: VecWitness,
        source: records,
        traverse: (ResultWitness::<BadRecord>, parse_record),
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(cold, cold_oracle, "both lowerings forward the same Err");
    match cold {
        Ok(parsed) => println!("fallible oracle: {parsed:?}"),
        Err(err) => {
            // The miette report renders the code and the help text — the
            // consumer-facing diagnostic, not a bare message.
            let report = miette::Report::new(err);
            eprintln!("fallible oracle forwarded:\n{report:?}");
        }
    }
}
