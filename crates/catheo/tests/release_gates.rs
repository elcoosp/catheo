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
//! - the workspace version is the public release version, verified by
//!   *derivation* instead of a pinned literal: the facade crate must
//!   inherit `[workspace.package]` verbatim and the version must be bare
//!   semver — the tag the release workflow cuts (`v*.*.*`) is checked
//!   against the manifest by the release driver, so a bump stays one edit;
//! - the changelog documents the release history;
//! - the README status table records the milestone history;
//! - the README trybuild-case counts track the `tests/ui/` suite on disk —
//!   the suite is globbed (`trybuild.rs`), so a case added without a README
//!   bump is exactly the drift the v0.6 train shipped (40 published, 42
//!   on disk) and this gate keeps it impossible.

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
    // The facade crate declares `version.workspace = true`, so
    // `CARGO_PKG_VERSION` — resolved from this test binary's own package —
    // is the version the release driver publishes and tags. Deriving the
    // expectation keeps the contract green across bumps: a stray
    // `[package]` override in `crates/catheo` (the split-version failure
    // mode) or a malformed manifest version fails here instead of at
    // publish time.
    let manifest = read("Cargo.toml");
    let declared = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("version = "))
        .map(|value| value.trim().trim_matches('"'))
        .expect("the workspace manifest declares a version");
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        declared,
        "the facade crate must inherit the `[workspace.package]` version \
         verbatim; a `[package]` override would publish a split version"
    );
    let parts: Vec<&str> = declared.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "the published version must be bare `major.minor.patch` \
         semver, found `{declared}`"
    );
    for part in &parts {
        assert!(
            !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()),
            "the published version must be bare numeric \
             `major.minor.patch` semver, found `{declared}`"
        );
    }
}

/// The internal workspace-dependency pins, as shipped in the root
/// `Cargo.toml`'s `[workspace.dependencies]` table. The release convention
/// (recorded in the v0.4.0 bump message) is that *the workspace version and
/// the internal dependency pins move together*: the pins exist so the
/// member crates can be published as a set with matching minimum
/// requirements. The v0.5.0 bump missed them (`^0.4.1` against 0.5.0
/// members), which broke workspace resolution outright — this gate keeps
/// the drift impossible.
const INTERNAL_PINS: [&str; 3] = ["catheo-parser", "catheo-macros", "catheo-counting-alloc"];

#[test]
fn internal_dependency_pins_track_the_workspace_version() {
    let manifest = read("Cargo.toml");
    let declared_version = manifest
        .lines()
        .find_map(|line| line.trim().strip_prefix("version = "))
        .map(|value| value.trim().trim_matches('"'))
        .expect("the workspace manifest declares a version");

    // Scope the scan to the `[workspace.dependencies]` table so a
    // third-party `version = "…"` line can never satisfy the gate.
    let mut table = false;
    let mut found: Vec<&str> = Vec::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed == "[workspace.dependencies]" {
            table = true;
            continue;
        }
        if table && trimmed.starts_with('[') {
            break;
        }
        if !table {
            continue;
        }
        for pin in INTERNAL_PINS {
            if trimmed.starts_with(&format!("{pin} =")) && trimmed.contains("version = \"") {
                let req = trimmed
                    .rsplit("version = \"")
                    .next()
                    .and_then(|tail| tail.split('"').next())
                    .expect("the pin line carries a quoted version requirement");
                assert_eq!(
                    req, declared_version,
                    "the internal pin `{pin}` must track the workspace \
                     version: the release convention is that the workspace \
                     version and the internal dependency pins move together \
                     (the v0.5.0 bump missed them and the tree stopped \
                     resolving)"
                );
                found.push(pin);
            }
        }
    }
    for pin in INTERNAL_PINS {
        assert!(
            found.contains(&pin),
            "the `[workspace.dependencies]` table must pin `{pin}` with an \
             explicit `version = \"…\"` requirement"
        );
    }
}

#[test]
fn changelog_documents_the_releases() {
    let changelog = read("CHANGELOG.md");
    assert!(
        changelog.contains("## [0.6.0]"),
        "the git-cliff changelog must document the 0.6.0 partial-arrow \
         release: the v0.6 train merged but the publication entry never \
         landed, so the release history stops at 0.5.1: {changelog}"
    );
    assert!(
        changelog.contains("## [0.5.1]"),
        "the git-cliff changelog must document the 0.5.1 maintenance release: {changelog}"
    );
    assert!(
        changelog.contains("## [0.5.0]"),
        "the git-cliff changelog must document the 0.5.0 release: {changelog}"
    );
    assert!(
        changelog.contains("## [0.4.2]"),
        "the changelog must document the 0.4.2 maintenance release: {changelog}"
    );
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
        readme.contains("v0.6.0") && readme.contains("filter_map"),
        "the README status must record the v0.6 partial-arrow milestone"
    );
    assert!(
        readme.contains("v0.5.0") && readme.contains("`Monad`"),
        "the README status must record the v0.5 strict-Monad milestone"
    );
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

/// The README's published trybuild counts must equal the suite the harness
/// actually globs. `trybuild.rs` runs `pass_*.rs` and `error_*.rs` by
/// filename glob, so the on-disk counts are the truth; the README restates
/// them in prose ("a 40-case trybuild suite (9 compile-pass, 31
/// compile-fail)"), and the v0.6 train shipped that prose two cases behind
/// the suite. The fragments are derived from disk, not pinned, so the next
/// added case cannot drift the way the hatch-matrix rows did.
#[test]
fn readme_trybuild_counts_track_the_ui_suite() {
    let ui_dir = workspace_root().join("crates/catheo/tests/ui");
    let display = ui_dir.display();
    let mut pass = 0usize;
    let mut error = 0usize;
    let entries = fs::read_dir(&ui_dir)
        .unwrap_or_else(|err| panic!("the ui suite must be readable at {display}: {err}"));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|err| panic!("the ui suite must be readable at {display}: {err}"))
            .path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !std::path::Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        {
            continue;
        }
        if name.starts_with("pass_") {
            pass += 1;
        }
        if name.starts_with("error_") {
            error += 1;
        }
    }
    assert!(
        pass > 0 && error > 0,
        "the ui suite must be discovered: found {pass} compile-pass and \
         {error} compile-fail cases under {display}"
    );
    let readme = read("README.md");
    for fragment in [
        format!("{}-case trybuild suite", pass + error),
        format!("{pass} compile-pass"),
        format!("{error} compile-fail"),
    ] {
        assert!(
            readme.contains(&fragment),
            "the README must publish the on-disk ui truth `{fragment}`: the \
             suite is globbed by filename, so a case added without a README \
             bump publishes a stale count (the v0.6 train shipped 40 against \
             the 42 on disk)"
        );
    }
}

/// The README grammar table is the published §4.2 hatch matrix: it must
/// name every spelling `parse_bang_stage` accepts. The v0.6 train left the
/// row at four of the six accepted spellings — `map!: materialize` and
/// `filter_map!: materialize` parse but were never published, so the
/// README under-documented the surface it advertises.
#[test]
fn readme_publishes_every_accepted_hatch_spelling() {
    let readme = read("README.md");
    for spelling in [
        "`map!: force`",
        "`map!: materialize`",
        "`bind!: force`",
        "`bind!: materialize`",
        "`filter_map!: force`",
        "`filter_map!: materialize`",
    ] {
        assert!(
            readme.contains(spelling),
            "the README grammar table must publish the accepted hatch \
             spelling `{spelling}` (§4.2: the hatches apply to `map`, \
             `bind`, and `filter_map` alike)"
        );
    }
}
