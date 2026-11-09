# Changelog

All notable changes to catheo are documented here. The format is based on
[Conventional Commits](https://www.conventionalcommits.org), and releases
are cut by pushing a `v*.*.*` tag (see `.github/workflows/release.yml`).

<!-- git-cliff: end of header -->
## [0.4.1] - 2026-11-09

### 📚 Documentation

- *(examples)* The quickstart consumer demo
- *(examples)* The fallible traversal consumer demo
- *(examples)* The instrumented consumer seam
- *(readme)* The examples section and the UI-suite truth sweep
- *(architecture)* The 3.1 sketch and the v0.4 keyword set
- *(rustdoc)* Point the crate facade at the examples

### 🧪 Testing

- *(examples)* The consumer-surface deterministic gate
- *(release)* Move the publication contract to v0.4.1

### 🔨 Build

- *(deps)* The tracing and miette dev-only seam

### ⚙️ Continuous Integration

- The bench-smoke comment counts the seven targets

### ⚙️ Miscellaneous Tasks

- *(release)* Bump the workspace to 0.4.1
- Gitignore

## [0.4.0] - 2026-11-08

### 🚀 Features

- *(parser)* The §4.3 `join` stage — the bare-keyword monadic multiplication, desugared at parse time to `bind(id)` with spanned rejections for `join:` arguments, `join!` hatches, and post-terminal positions
- *(codegen)* The join identity lowers to std's `.flatten()` in adapter chains (`.flat_map(f)` for a fused prefix), keeping the fused output at the lint-clean B-parity spelling

### 📚 Documentation

- *(readme)* Publish the v0.4 join milestone: feature bullet, grammar row, usage example, milestone table
- *(architecture)* §4.3.2 the join stage; §4.3 table row delivered as v0.4; §8 roadmap updated

### 🧪 Testing

- *(v0.4)* Deterministic gates (`tests/v04_gates.rs`): emission shape, three-way agreement, allocation profile, oracle agreement over every witness
- *(ui)* The join cases: compile-pass coverage and the three spanned rejections; combinator-list snapshot refreshed
- *(parser)* Pin the join grammar and its fusion behavior (TDD)

### ⚙️ Continuous Integration

- Smoke the join bench as the seventh target

### 📦 Release

- Bump the workspace to 0.4.0

## [0.2.0] - 2026-11-06

### 🚀 Features

- *(runtime)* §4.3 v0.2 kernels — Monoid, run_fold_map, kleisli
- *(parser)* V0.2 grammar — kleisli, fold_map, §4.2 hatches, derived witnesses
- *(bench)* §6.1 combinators suite — the v0.2 gate re-applied

### 📚 Documentation

- *(readme)* Record the v0.2 milestone status and the measured gates

### 🧪 Testing

- *(v0.2)* Deterministic gates, property shapes, UI cases, kernel laws

### ⚙️ Continuous Integration

- Combinators bench smoke (fifth target)

### ⚙️ Miscellaneous Tasks

- *(release)* Git-cliff changelog for 0.2.0
## [0.1.0] - 2026-11-06

### 🐛 Bug Fixes

- *(parser)* Reject stages below the terminal and duplicate terminals

### 📚 Documentation

- *(gates)* Align the Result-chain C doc with the pointwise dispatch
- *(readme)* Record milestone M5 status and the release audit

### 🧪 Testing

- *(release)* Deterministic M5 release gates

### ⚙️ Miscellaneous Tasks

- *(release)* Git-cliff changelog for 0.1.0
## [0.0.5] - 2026-11-06

### 🚀 Features

- *(bench)* §6.1 Result-chain and Kleisli criterion suites (M4)
- *(parser)* Pointwise witnesses keep the kernel chain (§6.1 M4 gate)

### 📚 Documentation

- *(readme)* Record milestone M4 status and the measured error-chain gate
- *(bench)* Align the Result-chain C doc with the pointwise emission

### 🧪 Testing

- *(gates)* Deterministic §6.1 error-chain and Kleisli gates (M4)

### ⚙️ Continuous Integration

- Error_chains bench smoke (fourth target)

### ⚙️ Miscellaneous Tasks

- *(release)* Git-cliff changelog for 0.0.5
## [0.0.4] - 2026-11-06

### 🚀 Features

- *(test-support)* Counting allocator for deterministic §6.1 gates
- *(bench)* §6.1 three-way A/B/C criterion suite (M3)

### 📚 Documentation

- *(readme)* Record milestone M3 status and the measured decision gate

### 🧪 Testing

- *(gates)* Deterministic §6.1 decision gates (M3)
- *(macro)* 50-pipeline compile-time fixture (§6.1)

### ⚙️ Continuous Integration

- Abc bench smoke, macro-stats fixture note, fix push branch filter

### ⚙️ Miscellaneous Tasks

- *(release)* Git-cliff changelog for 0.0.4
## [0.0.3] - 2026-11-06

### 🚀 Features

- *(parser)* Linear-time deforestation pass (§3.3 Rules 1-3)
- *(parser)* SSA codegen with absorbed-fold lowering and unfused oracle

### 📚 Documentation

- *(readme)* Record milestone M2 status, fusion semantics, solver walls

### 🎨 Styling

- Remove the last allow block; clippy pedantic stays exemption-free

### 🧪 Testing

- *(runtime)* Fused-vs-unfused property harness over randomized inputs (§6.2)

### ⚙️ Miscellaneous Tasks

- *(release)* Git-cliff changelog for 0.0.3
## [0.0.2] - 2026-11-06

### 🚀 Features

- *(runtime)* Pointed and join kernels plus categorical law proofs

### 📈 Benchmarks

- *(runtime)* Criterion kernel-floor benchmark with enforced gates

### 📚 Documentation

- *(readme)* Record milestone M1 status, kernel coverage, lint posture

### 🎨 Styling

- Clippy pedantic at deny with zero allow-list exemptions
## [0.0.1] - 2026-11-06

### 🚀 Features

- *(parser)* Implement syntax-directed pipeline parser (TDD green)
- *(runtime)* Layer 0 HKT witnesses and Layer 1 split-trait kernels
- *(macro)* Pipeline! proc-macro with naive M0 lowering and trybuild UI tests

### 🐛 Bug Fixes

- *(ci)* Scope the bench smoke job to the criterion target

### 🎨 Styling

- Satisfy clippy pedantic and rustfmt across the workspace

### 🧪 Testing

- *(parser)* Add pipeline grammar unit tests (TDD red)
- *(runtime)* Add Layer 0/1 kernel specs for Vec/Option/Result (TDD red)

### ⚙️ Continuous Integration

- Add CI/CD, git-cliff release tooling, changelog, and README

### ⚙️ Miscellaneous Tasks

- Add arch
