//! The M5 release gates (ARCHITECTURE.md §8: "Public v0.1 release — M0–M4
//! gates met; README limitations published").
//!
//! The M0–M4 technical gates are enforced by the rest of this suite
//! (kernel/fusion/error-chain gates, expansion budget, trybuild UI, the
//! A/B/C lockstep). What M5 adds is the *publication* half of the gate,
//! made deterministic here, in the house gate style:
//!
//! - the published README carries the complete §7 limitation set under the
//!   v0.1 heading (§7: "Known Limitations (Published in README)");
//! - the workspace version is the public v0.1 version (`0.1.0`);
//! - the changelog documents the release;
//! - the README status table records the milestone.

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

/// The §7 limitation set, as (heading fragment, §7 item) pairs. The README
/// is the *published form* of §7, so each item must appear under the v0.1
/// limitations heading.
const LIMITATION_FRAGMENTS: [&str; 5] = [
    "Closure shape is constrained",
    "Structural observers are detected lexically",
    "No strict `Monad` on stable",
    "No `traverse`",
    "LLVM is not controlled",
];

#[test]
fn readme_publishes_every_section_7_limitation_under_the_v01_heading() {
    let readme = read("README.md");
    assert!(
        readme.contains("## Known limitations (v0.1"),
        "the limitations section must carry the v0.1 heading: the §7 gate \
         is `README limitations published`"
    );
    for fragment in LIMITATION_FRAGMENTS {
        assert!(
            readme.contains(fragment),
            "the README must publish §7's limitation `{fragment}`"
        );
    }
}

#[test]
fn workspace_version_is_the_public_v0_1() {
    let manifest = read("Cargo.toml");
    let version = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("version = "))
        .expect("the workspace manifest declares a version");
    assert_eq!(
        version, "\"0.1.0\"",
        "M5 is the public v0.1 release; bump the workspace version"
    );
}

#[test]
fn changelog_documents_the_0_1_0_release() {
    let changelog = read("CHANGELOG.md");
    assert!(
        changelog.contains("## [0.1.0]"),
        "the git-cliff changelog must document the 0.1.0 release: {changelog}"
    );
}

#[test]
fn readme_status_records_the_m5_release() {
    let readme = read("README.md");
    assert!(
        readme.contains("milestone M5") && readme.contains("v0.1.0"),
        "the README status must record the M5 release"
    );
}
