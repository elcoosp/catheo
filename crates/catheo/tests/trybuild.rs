//! `trybuild` UI test harness (§6.2).
//!
//! - `pass_*.rs`: valid pipelines must compile (and run their asserts).
//! - `error_*.rs`: malformed or ill-typed pipelines must be rejected with
//!   errors **spanned at the offending user token**, not at macro internals.
//!
//! Snapshots are generated with `TRYBUILD=overwrite` and reviewed by hand;
//! CI pins the toolchain (see `.github/workflows/ci.yml`) so `rustc`
//! diagnostics stay stable.

#[test]
fn ui() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass_*.rs");
    cases.compile_fail("tests/ui/error_*.rs");
}
