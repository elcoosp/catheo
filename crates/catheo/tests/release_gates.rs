//! The M5 release gates (ARCHITECTURE.md §8: "Public v0.1 release — M0–M4
//! gates met; README limitations published"), maintained at each release
//! so the publication contract stays current.
//!
//! The M0–M4 technical gates are enforced by the rest of this suite
//! (kernel/fusion/error-chain gates, expansion budget, trybuild UI, the
//! A/B/C lockstep). What M5 adds is the *publication* half of the gate,
//! made deterministic here, in the house gate style:
//!
//! - the published README carries the complete §7 limitation set under the
//!   limitations heading (§7: "Known Limitations (Published in README)");
//! - the workspace version is the current public release version
//!   (`0.4.0`);
//! - the changelog documents the release history;
//! - the README status table records the milestone history.

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

/// The §7 limitation set, as published fragments. The README is the
/// *published form* of §7, so each item must appear under the limitations
/// heading. The list moves with §7 itself: v0.3 replaced the "No
/// `traverse`" limitation with the constrained-shape statement (§4.3.1).
const LIMITATION_FRAGMENTS: [&str; 5] = [
    "Closure shape is constrained",
    "Structural observers are detected lexically",
    "No strict `Monad` on stable",
    "`traverse` arrived in v0.3, and its shape is constrained",
    "LLVM is not controlled",
];

#[test]
fn readme_publishes_every_section_7_limitation_under_the_limitations_heading() {
    let readme = read("README.md");
    assert!(
        readme.contains("## Known limitations"),
        "the limitations section must carry the `Known limitations` heading: \
         the §7 gate is `README limitations published`"
    );
    for fragment in LIMITATION_FRAGMENTS {
        assert!(
            readme.contains(fragment),
            "the README must publish §7's limitation `{fragment}`"
        );
    }
}

#[test]
fn workspace_version_is_the_public_release_version() {
    let manifest = read("Cargo.toml");
    let version = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("version = "))
        .expect("the workspace manifest declares a version");
    assert_eq!(
        version, "\"0.4.1\"",
        "the current release is v0.4.0 (join shipped); bump the workspace version"
    );
}

#[test]
fn changelog_documents_the_releases() {
    let changelog = read("CHANGELOG.md");
    assert!(
        changelog.contains("## [0.4.0]"),
        "the git-cliff changelog must document the 0.4.0 release: {changelog}"
    );
    assert!(
        changelog.contains("## [0.2.0]"),
        "the git-cliff changelog must document the 0.2.0 release: {changelog}"
    );
    assert!(
        changelog.contains("## [0.1.0]"),
        "the changelog keeps the v0.1.0 release history: {changelog}"
    );
}

#[test]
fn readme_status_records_the_release_history() {
    let readme = read("README.md");
    assert!(
        readme.contains("v0.4.0") && readme.contains("join"),
        "the README status must record the v0.4 join milestone"
    );
    assert!(
        readme.contains("v0.3.0") && readme.contains("traverse"),
        "the README status must record the v0.3 traverse milestone"
    );
    assert!(
        readme.contains("v0.2.0") && readme.contains("escape hatches"),
        "the README status keeps the v0.2 milestone history"
    );
    assert!(
        readme.contains("v0.1.0") && readme.contains("M0–M5"),
        "the README status keeps the M5 release history"
    );
}
