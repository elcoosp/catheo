//! The consumer-surface gate — the `examples/` demonstration is part of the
//! publication contract, kept deterministic in the house gate style of
//! `release_gates.rs`.
//!
//! A proc-macro library sells a *demonstration* as much as an API: the
//! crate-level rustdoc points consumers at runnable examples, so this gate
//! keeps the pointer honest. Six demos are pinned:
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
//!   posture.
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
