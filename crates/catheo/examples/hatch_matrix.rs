//! The §4.2 hatch matrix — all six accepted spellings, runnable.
//!
//! ```text
//! cargo run -p catheo --example hatch_matrix
//! ```
//!
//! The grammar accepts the escape hatch on three stages (`map!`, `bind!`,
//! `filter_map!`) in two directions (`force` fuses, `materialize` pins the
//! intermediate) — six spellings, all published by the README grammar table
//! and pinned by `tests/release_gates.rs`. `escape_hatches` ran three of
//! the six; this demo runs the whole matrix, one row per stage keyword,
//! each row on the *same* arrow so the values are asserted equal across the
//! row: the hatch changes the lowering's shape, never the value.
//!
//! The `force` direction rides the fused path (the scan it silences is a
//! compile-time flag, not a runtime cost), so a forced stage is value- and
//! cost-free against its plain spelling; the `materialize` direction is the
//! documented hard barrier — the stage keeps its kernel and an intermediate
//! container, the shape `benches/hatches.rs` measures as the hatch's cost.
//! The §6.2 oracle closes the demo: the forced `bind` shape expands through
//! `pipeline!` and `pipeline_unfused!` and the example asserts the two
//! lowerings agree value-for-value.
//!
//! The observable seam is the house posture: `tracing` spans bracket each
//! matrix row and the fallible path renders as a `miette` diagnostic with
//! help text instead of a bare message. Both dependencies are dev-only —
//! the shipped crate stays dependency-free. Set `RUST_LOG=debug` to widen
//! the span/event filter.
use catheo::{ResultWitness, VecWitness, pipeline, pipeline_unfused};
use miette::Diagnostic;
use thiserror::Error;

/// The fallible per-record bound the demo's traversal lands in.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic)]
#[error("`{record}` exceeds the u16 bound")]
#[diagnostic(
    code(catheo::out_of_range),
    help(
        "kept records must fit u16; `filter_map!: force` keeps the fused \
          prefix at one pass — prune the junk first, then let the traversal \
          short-circuit on the first out-of-range record (§4.3 v0.3)"
    )
)]
struct OutOfRange {
    /// The offending record, carried for the diagnostic rendering.
    record: i32,
}

/// Validate one record — the traversal arrow `A -> G<B>` over the effect.
fn check_u16(record: i32) -> Result<i32, OutOfRange> {
    if (0..=i32::from(u16::MAX)).contains(&record) {
        Ok(record)
    } else {
        Err(OutOfRange { record })
    }
}

/// The matrix arrow for the `filter_map` row: keep the evens, double into
/// `i64` — the same arrow both spellings run, so the row's values must
/// agree.
fn keep_even_double(x: i32) -> Option<i64> {
    if x % 2 == 0 {
        Some(i64::from(x) * 2)
    } else {
        None
    }
}

/// The matrix arrow for the `map` row: double each element.
///
/// One arrow per row across both spellings is the point of the matrix: the
/// two spellings must agree value-for-value or the hatch is broken.
fn doubled(x: i32) -> i32 {
    x * 2
}

/// The `map` row — `map!: force` (accepted, inert: maps are unconditional
/// fusion targets) beside `map!: materialize` (the hard barrier: the map
/// keeps its kernel and an intermediate container is materialized).
fn row_map() {
    let span = tracing::info_span!("hatch.matrix", row = "map");
    let _guard = span.enter();
    let forced: i64 = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4],
        map!: force doubled,
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    let materialized: i64 = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4],
        map!: materialize doubled,
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(forced, 20, "map!: force folds the doubled values");
    assert_eq!(
        forced, materialized,
        "the hatch changes the shape, never the value"
    );
    println!("map!: force {forced} == map!: materialize {materialized}");
}

/// The `bind` row — the README flagship shape (`Vec<Vec<i32>>` row sums)
/// through both directions. The closure body mentions `.iter`, so the
/// lexical scan (§3.2) flags the plain spelling as an observer: `force`
/// silences that flag and the fold absorbs the bind; `materialize` keeps
/// the `run_bind` kernel and the intermediate.
fn row_bind() {
    let span = tracing::info_span!("hatch.matrix", row = "bind");
    let _guard = span.enter();
    let rows = vec![vec![1_i32, 2], vec![3, 4]];
    let forced: i64 = pipeline! {
        witness: VecWitness,
        source: rows.clone(),
        bind!: force |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    let materialized: i64 = pipeline! {
        witness: VecWitness,
        source: rows,
        bind!: materialize |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(forced, 10, "one scalar per row: 3 + 7");
    assert_eq!(forced, materialized, "fused == kernel-path value");
    println!("bind!: force {forced} == bind!: materialize {materialized}");
}

/// The `filter_map` row — the v0.6 hatches, demonstrated for the first
/// time: keep the evens and double into `i64`, through `force` (the
/// constructors are the arrow's effect, so this spelling is inert but
/// accepted — the uniform-hatch posture) and through `materialize`.
fn row_filter_map() {
    let span = tracing::info_span!("hatch.matrix", row = "filter_map");
    let _guard = span.enter();
    let forced: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4],
        filter_map!: force keep_even_double,
        collect,
    };
    let materialized: Vec<i64> = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4],
        filter_map!: materialize keep_even_double,
        collect,
    };
    assert_eq!(forced, [4, 8], "the evens kept, doubled");
    assert_eq!(forced, materialized, "fused == kernel-path value");
    println!("filter_map!: force {forced:?} == filter_map!: materialize {materialized:?}");
}

/// The §6.2 oracle row: the forced `bind` shape through both macros —
/// expanding a suspect pipeline through `pipeline!` and
/// `pipeline_unfused!` isolates any mis-rewrite to the fusion pass.
fn row_oracle() {
    let span = tracing::info_span!("hatch.oracle", rows = 2);
    let _guard = span.enter();
    let rows = vec![vec![9_i32, 9], vec![7, 7]];
    let fused: i64 = pipeline! {
        witness: VecWitness,
        source: rows.clone(),
        bind!: force |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    let oracle: i64 = pipeline_unfused! {
        witness: VecWitness,
        source: rows,
        bind!: force |row: Vec<i32>| vec![row.iter().sum::<i32>()],
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(fused, oracle, "fused == unfused (§6.2)");
    assert_eq!(fused, 32, "18 + 14");
    println!("oracle agreement on the forced bind: {fused}");
}

/// The fallible row: the forced partial arrow prunes nothing here, the
/// bound rejects a record, and the miette report renders the code and the
/// help text — the observable seam, not a bare message.
fn row_fallible() {
    let span = tracing::info_span!("hatch.fallible", records = 2);
    let _guard = span.enter();
    let cold: Result<Vec<i32>, OutOfRange> = pipeline! {
        witness: VecWitness,
        source: vec![41_i32, 70_000],
        filter_map!: force |x: i32| if x > 0 { Some(x) } else { None },
        traverse: (ResultWitness::<OutOfRange>, check_u16),
        collect,
    };
    match cold {
        Ok(kept) => println!("fallible row kept: {kept:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("fallible row forwarded:\n{report:?}");
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    row_map();
    row_bind();
    row_filter_map();
    row_oracle();
    row_fallible();
}
