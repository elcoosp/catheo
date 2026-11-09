//! The consumer-surface gate — the `examples/` demonstration is part of the
//! publication contract, kept deterministic in the house gate style of
//! `release_gates.rs`.
//!
//! A proc-macro library sells a *demonstration* as much as an API: the
//! crate-level rustdoc points consumers at runnable examples, so this gate
//! keeps the pointer honest. Three demos are pinned:
//!
//! - `quickstart.rs` — the README quickstart, runnable;
//! - `fallible_pipeline.rs` — the traversal boundary over a `Result` effect;
//! - `instrumented_pipeline.rs` — the observable consumer seam: `tracing`
//!   spans around the run and `miette` help text on the error path.
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
