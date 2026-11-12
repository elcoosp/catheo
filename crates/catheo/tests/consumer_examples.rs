//! The consumer-surface gate — the `examples/` demonstration is part of the
//! publication contract, kept deterministic in the house gate style of
//! `release_gates.rs`.
//!
//! A proc-macro library sells a *demonstration* as much as an API: the
//! crate-level rustdoc points consumers at runnable examples, so this gate
//! keeps the pointer honest. Twelve demos are pinned:
//!
//! - `quickstart.rs` — the README quickstart, runnable;
//! - `fallible_pipeline.rs` — the traversal boundary over a `Result` effect;
//! - `instrumented_pipeline.rs` — the observable consumer seam: `tracing`
//!   spans around the run and `miette` help text on the error path;
//! - `monadic_surface.rs` — the monadic multiplication and the monoid
//!   eliminator: `join` (fused and pointwise) and `fold_map` over real
//!   `Monoid`s, the newest combinator surface with the smallest doc cost;
//! - `strict_monad.rs` — the §2.4/§4.4 one-trait surface: a program
//!   composed over a single `Monad` bound (`pure` + `bind`, provided
//!   `join` and derived `fmap`) run across every witness, with the house
//!   observable seam around it;
//! - `escape_hatches.rs` — the §4.2 escape hatches (`bind!: force`,
//!   `bind!: materialize`), the `pipeline_unfused!` oracle that isolates
//!   any mis-rewrite to the fusion pass, the witness-free source
//!   ascription, and the Option bind chain, in the same observable
//!   posture;
//! - `filter_map_pipeline.rs` — the §4.3 v0.6 partial arrow: `filter_map`
//!   pruning the junk records in the fused prefix, the traversal boundary
//!   validating what it keeps, in the observable seam posture.
//! - `hatch_matrix.rs` — the §4.2 hatch matrix: all six accepted spellings
//!   (`map!`, `bind!`, `filter_map!`, each × `force`/`materialize`) run
//!   side by side on the same arrows, the values asserted equal across the
//!   matrix, with the `pipeline_unfused!` oracle pairing the forced shapes
//!   and the v0.6 `filter_map!` rows finally demonstrated.
//! - `kernel_surface.rs` — the runtime layer *without* the macro: the §2.2
//!   split kernels (`run_map`, `run_filter_map`, `run_join`,
//!   `run_traverse`), the standalone `kleisli` composition, and a
//!   user-defined `Monoid`, in the observable seam posture.
//! - `ascribed_witnesses.rs` — the §4.2 source-ascription surface: every
//!   built-in witness derived from the `as` spelling (`as Vec<i32>`,
//!   `as Option<String>`, `as Result<Vec<String>, BadLine>`, and the fully
//!   qualified `std::vec::Vec<…>` spelling), with no `witness:` entry and
//!   no witness identifier anywhere in the file, in the observable seam
//!   posture.
//! - `foldable_surface.rs` — the strict `Foldable` layer: one generic
//!   program over one `Foldable` bound (`fold` required, `fold_map`
//!   derived) run across every witness, eliminating into a user-defined
//!   `Monoid`, with the `Err`-contributes-identity denotation
//!   demonstrated and the house observable seam around it.
//! - `filterable_surface.rs` — the strict `Filterable` layer: one generic
//!   program over one `Filterable` bound (`filter_map` required,
//!   `filter` derived) run across both admitting witnesses, the trivial
//!   -predicate laws demonstrated, and the fallible boundary validated
//!   through the traversal kernel with the house observable seam.
//!
//! plus the README and the crate rustdoc must both link the directory.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // `crates/catheo` → workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root resolves")
}

fn read(relative: &str) -> String {
    let path = workspace_root().join(relative);
    let display = path.display();
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{relative} must be readable at {display}: {err}"))
}

fn example(name: &str) -> String {
    read(&format!("crates/catheo/examples/{name}"))
}

#[test]
fn the_quickstart_demo_exists_and_compiles_the_readme_shape() {
    let quickstart = example("quickstart.rs");
    assert!(
        quickstart.contains("pipeline!"),
        "the quickstart demo drives the `pipeline!` macro"
    );
    assert!(
        quickstart.contains("VecWitness"),
        "the quickstart demo uses the head-anchored `VecWitness` (§4.1)"
    );
    assert!(
        quickstart.contains("fold:"),
        "the quickstart demo eliminates through a `fold` terminal"
    );
    assert!(
        quickstart.contains("--example quickstart"),
        "the demo documents its own run line: cargo run -p catheo --example quickstart"
    );
}

#[test]
fn the_fallible_demo_exists_and_runs_the_traversal_boundary() {
    let fallible = example("fallible_pipeline.rs");
    assert!(
        fallible.contains("traverse:"),
        "the fallible demo crosses the §4.3 v0.3 traversal boundary"
    );
    assert!(
        fallible.contains("ResultWitness"),
        "the fallible demo collects one effect around a `Vec`"
    );
    assert!(
        fallible.contains("--example fallible_pipeline"),
        "the demo documents its own run line"
    );
}

#[test]
fn the_instrumented_demo_carries_the_observable_seam() {
    let instrumented = example("instrumented_pipeline.rs");
    assert!(
        instrumented.contains("tracing::"),
        "the instrumented demo spans the pipeline run with `tracing`"
    );
    assert!(
        instrumented.contains("miette::Diagnostic"),
        "the instrumented demo derives `miette::Diagnostic` on its error"
    );
    assert!(
        instrumented.contains("help"),
        "the diagnostic carries miette help text, not a bare message"
    );
    assert!(
        instrumented.contains("--example instrumented_pipeline"),
        "the demo documents its own run line"
    );
}

#[test]
fn the_monadic_demo_runs_join_and_fold_map_on_the_surface() {
    let monadic = example("monadic_surface.rs");
    assert!(
        monadic.contains("join"),
        "the monadic demo stages the §4.3 monadic multiplication"
    );
    assert!(
        monadic.contains("fold_map:"),
        "the monadic demo eliminates through the §4.3 v0.2 monoid eliminator"
    );
    assert!(
        monadic.contains("OptionWitness") && monadic.contains("ResultWitness"),
        "the monadic demo covers the pointwise witnesses, not only `Vec`"
    );
    assert!(
        monadic.contains("tracing::"),
        "the new consumer seam carries `tracing` spans like the instrumented demo"
    );
    assert!(
        monadic.contains("help"),
        "the fallible surface carries miette help text, not a bare message"
    );
    assert!(
        monadic.contains("--example monadic_surface"),
        "the demo documents its own run line"
    );
}

#[test]
fn the_strict_monad_demo_carries_the_one_trait_surface() {
    let strict = example("strict_monad.rs");
    assert!(
        strict.contains("Monad"),
        "the strict-monad demo bounds on the §4.4 one-trait surface"
    );
    assert!(
        strict.contains("M::pure") && strict.contains("M::bind"),
        "the strict-monad demo drives `pure` and `bind` through the unified trait"
    );
    assert!(
        strict.contains("VecWitness")
            && strict.contains("OptionWitness")
            && strict.contains("ResultWitness"),
        "the one-trait surface runs across every witness, not only `Vec`"
    );
    assert!(
        strict.contains("tracing::"),
        "the strict-monad demo spans its sections like the instrumented demo"
    );
    assert!(
        strict.contains("help"),
        "the fallible path carries miette help text, not a bare message"
    );
    assert!(
        strict.contains("--example strict_monad"),
        "the demo documents its own run line"
    );
}

#[test]
fn the_escape_hatch_demo_runs_the_hatches_and_the_oracle() {
    let hatches = example("escape_hatches.rs");
    assert!(
        hatches.contains("bind!: force"),
        "the demo forces fusion past a false-positive observer (§4.2)"
    );
    assert!(
        hatches.contains("bind!: materialize"),
        "the demo materializes an intermediate, the opposite direction (§4.2)"
    );
    assert!(
        hatches.contains("pipeline_unfused!"),
        "the demo pairs `pipeline!` with the §6.2 unfused oracle — expanding \
         a suspect pipeline through both macros isolates any mis-rewrite \
         to the fusion pass"
    );
    assert!(
        hatches.contains("as Vec"),
        "the demo derives the witness from a §4.2 source ascription"
    );
    assert!(
        hatches.contains("OptionWitness"),
        "the demo runs the Option bind-chain surface (and_then on Option)"
    );
    assert!(
        hatches.contains("tracing::"),
        "the consumer seam carries `tracing` spans like the instrumented demo"
    );
    assert!(
        hatches.contains("help"),
        "the fallible path carries miette help text, not a bare message"
    );
    assert!(
        hatches.contains("--example escape_hatches"),
        "the demo documents its own run line"
    );
}

#[test]
fn the_partial_arrow_demo_carries_the_observable_seam() {
    let partial = example("filter_map_pipeline.rs");
    assert!(
        partial.contains("filter_map:"),
        "the partial-arrow demo stages the §4.3 v0.6 arrow"
    );
    assert!(
        partial.contains("traverse:"),
        "the partial-arrow demo crosses the §4.3 v0.3 traversal boundary \
         after the fused prefix"
    );
    assert!(
        partial.contains("OptionWitness"),
        "the partial-arrow demo covers the pointwise spelling (`and_then`)"
    );
    assert!(
        partial.contains("tracing::"),
        "the partial-arrow demo spans the pipeline run with `tracing`"
    );
    assert!(
        partial.contains("help"),
        "the fallible path carries miette help text, not a bare message"
    );
    assert!(
        partial.contains("--example filter_map_pipeline"),
        "the demo documents its own run line"
    );
}

/// The §4.2 hatch matrix has six accepted spellings and the release gate
/// (`readme_publishes_every_accepted_hatch_spelling`) publishes all of
/// them — but the v0.6 train demonstrated only three: the `filter_map!`
/// hatches shipped with the partial arrow and never ran in any demo, and
/// `map!: materialize` shipped with the v0.2 hatch grammar equally
/// unrun. This gate pins the runnable truth: one demo carries the whole
/// matrix, value-checked across the rows, oracle-paired on the forced
/// shapes, in the house observable seam posture.
#[test]
fn the_hatch_matrix_demo_runs_every_accepted_spelling() {
    let matrix = example("hatch_matrix.rs");
    for spelling in [
        "map!: force",
        "map!: materialize",
        "bind!: force",
        "bind!: materialize",
        "filter_map!: force",
        "filter_map!: materialize",
    ] {
        assert!(
            matrix.contains(spelling),
            "the hatch-matrix demo must run the accepted spelling \
             `{spelling}` (§4.2): the release gate publishes the \
             six-spelling matrix, so a consumer can run what is published"
        );
    }
    assert!(
        matrix.contains("pipeline_unfused!"),
        "the hatch-matrix demo pairs the forced shapes with the §6.2 \
         unfused oracle — expanding a suspect pipeline through both macros \
         isolates any mis-rewrite to the fusion pass"
    );
    assert!(
        matrix.contains("tracing::"),
        "the hatch-matrix demo spans its sections like the instrumented demo"
    );
    assert!(
        matrix.contains("help"),
        "the fallible path carries miette help text, not a bare message"
    );
    assert!(
        matrix.contains("--example hatch_matrix"),
        "the demo documents its own run line"
    );
}

#[test]
fn the_kernel_surface_demo_carries_the_runtime_layer() {
    let kernels = example("kernel_surface.rs");
    assert!(
        kernels.contains("run_map") && kernels.contains("run_filter_map"),
        "the demo drives the §2.2 split kernels directly — the functor \
         arrow and the v0.6 partial arrow, no macro in sight"
    );
    assert!(
        kernels.contains("run_join") && kernels.contains("run_traverse"),
        "the demo covers the monadic multiplication and the traversal \
         boundary as plain trait calls"
    );
    assert!(
        kernels.contains("kleisli("),
        "the demo composes with the standalone `catheo::kleisli` the \
         README advertises but no demo ever ran"
    );
    assert!(
        kernels.contains("impl Monoid"),
        "the demo implements `Monoid` for a user type — the \
         \"eliminates into any Monoid\" promise, substantiated"
    );
    assert!(
        kernels.contains("tracing::"),
        "the kernel demo spans its sections like the instrumented demo"
    );
    assert!(
        kernels.contains("help"),
        "the fallible kernel path carries miette help text, not a bare message"
    );
    assert!(
        kernels.contains("--example kernel_surface"),
        "the demo documents its own run line"
    );
}

/// The §4.2 ascription surface is published (`ast.rs::witness_from_type`
/// carries the derivation and its spanned rejections, the release gate
/// publishes the witness-free posture in `README.md`'s ascription bullet)
/// and the ui suite runs it (`tests/ui/pass_source_ascription.rs`) — but
/// no runnable demo ever derived a pointwise witness from an ascription:
/// the demos spelled every witness at the head except the ones the
/// `as` spelling can produce. This gate pins the runnable truth: one demo
/// derives every built-in witness from the ascription alone — no
/// `witness:` entry, and no witness identifier anywhere in the file.
#[test]
fn the_ascribed_witness_demo_derives_every_builtin_witness() {
    let demo = example("ascribed_witnesses.rs");
    assert!(
        demo.contains("pipeline!"),
        "the ascribed demo drives the `pipeline!` macro"
    );
    for ascription in ["as Vec<", "as Option<", "as Result<"] {
        assert!(
            demo.contains(ascription),
            "the demo must derive a witness from the §4.2 `{ascription}` \
             ascription: every built-in type constructor is derivable, so a \
             consumer can run what the grammar accepts"
        );
    }
    assert!(
        demo.contains("std::vec::Vec<"),
        "the demo spells the fully qualified path: only the last path \
         segment decides (§4.2), so `std::vec::Vec<T>` derives too"
    );
    assert!(
        !demo.contains("VecWitness")
            && !demo.contains("OptionWitness")
            && !demo.contains("ResultWitness"),
        "the demo names no witness identifier: the ascriptions derive every \
         head, which is the §4.2 posture the demo exists to demonstrate"
    );
    assert!(
        demo.contains("tracing::"),
        "the ascribed demo spans its universes like the instrumented demo"
    );
    assert!(
        demo.contains("help"),
        "the fallible ascribed path carries miette help text, not a bare message"
    );
    assert!(
        demo.contains("--example ascribed_witnesses"),
        "the demo documents its own run line"
    );
}

/// The §2.4 posture, fold side: the strict `Foldable` layer is published
/// (`foldable.rs` carries the trait and its derivation rationale, the
/// facade exports it) — but no runnable demo ever eliminated through the
/// one-trait bound. This gate pins the runnable truth: one demo drives
/// `fold` and the derived `fold_map` through a single generic program
/// across every witness, into a user `Monoid`, with the identity
/// denotation demonstrated.
#[test]
fn the_foldable_demo_carries_the_one_bound_eliminator() {
    let demo = example("foldable_surface.rs");
    assert!(
        demo.contains("Foldable"),
        "the foldable demo bounds on the strict one-trait eliminator surface"
    );
    assert!(
        demo.contains("W::fold_map") && demo.contains("as Foldable>::fold"),
        "the foldable demo drives the required `fold` and the derived \
         `fold_map` through the unified trait"
    );
    assert!(
        demo.contains("VecWitness")
            && demo.contains("OptionWitness")
            && demo.contains("ResultWitness"),
        "the one-bound eliminator runs across every witness, not only `Vec`"
    );
    assert!(
        demo.contains("impl Monoid"),
        "the demo eliminates into a user `Monoid` — the derivation's \
         any-Monoid promise, substantiated"
    );
    assert!(
        demo.contains("identity"),
        "the demo demonstrates the `Err`-contributes-identity denotation"
    );
    assert!(
        demo.contains("tracing::"),
        "the foldable demo spans its sections like the instrumented demo"
    );
    assert!(
        demo.contains("help"),
        "the fallible fold path carries miette help text, not a bare message"
    );
    assert!(
        demo.contains("--example foldable_surface"),
        "the demo documents its own run line"
    );
}

/// The §2.4 posture, partial-arrow side: the strict `Filterable` layer is
/// published (`filterable.rs` carries the trait, its derivation, and the
/// §4.3.3 exclusion doctest, the facade exports it) — but no runnable
/// demo ever kept through the one-trait bound. This gate pins the
/// runnable truth: one demo drives `filter_map` and the derived `filter`
/// through a single generic program across both admitting witnesses,
/// with the fallible boundary on the traversal kernel.
#[test]
fn the_filterable_demo_carries_the_one_bound_partial_arrow() {
    let demo = example("filterable_surface.rs");
    assert!(
        demo.contains("Filterable"),
        "the filterable demo bounds on the strict one-trait partial-arrow \
         surface"
    );
    assert!(
        demo.contains("W::filter_map") && demo.contains("W::filter"),
        "the filterable demo drives the required `filter_map` and the \
         derived `filter` through the unified trait"
    );
    assert!(
        demo.contains("VecWitness") && demo.contains("OptionWitness"),
        "the one-bound arrow runs across both admitting witnesses"
    );
    assert!(
        demo.contains("run_traverse"),
        "the fallible boundary rides the §4.3 v0.3 traversal kernel — the \
         layer composes with the split kernels it delegates to"
    );
    assert!(
        demo.contains("tracing::"),
        "the filterable demo spans its sections like the instrumented demo"
    );
    assert!(
        demo.contains("help"),
        "the fallible traverse path carries miette help text, not a bare \
         message"
    );
    assert!(
        demo.contains("--example filterable_surface"),
        "the demo documents its own run line"
    );
}

#[test]
fn the_readme_links_the_examples_directory() {
    let readme = read("README.md");
    assert!(
        readme.contains("crates/catheo/examples"),
        "the README must point consumers at the runnable examples"
    );
}

#[test]
fn the_crate_rustdoc_points_at_the_examples() {
    let lib = read("crates/catheo/src/lib.rs");
    assert!(
        lib.contains("examples/"),
        "the crate-level rustdoc must point consumers at the runnable examples"
    );
}
