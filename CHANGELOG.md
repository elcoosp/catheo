# Changelog

All notable changes to catheo are documented here. The format is based on
[Conventional Commits](https://www.conventionalcommits.org), and releases
are cut by pushing a `v*.*.*` tag (see `.github/workflows/release.yml`).

<!-- git-cliff: end of header -->
## [0.6.0] - 2026-11-10

### 🚀 Features

- *(runtime)* The §4.3.3 partial-arrow kernel (`CategoricalFilterMap`) — std's `filter_map` on `Vec`, `and_then` on `Option`; no impl for `ResultWitness<E>` (a dropped `Ok` has no error-free denotation), and the facade exports the trait
- *(parser)* The `filter_map` stage grammar — `StageKind::FilterMap` joins the grammar, the `filter_map:` entry with the §4.2 hatches, the methods-only §3.2 scan (the arrow's `Some`/`None` are its effect constructors, not an observation), and the kind-aware stage flags
- *(fuse)* Rule 2 pairs a map into the partial arrow; the composed-body rescan applies the refined scan
- *(codegen)* The partial arrow lowers at its std position — `.filter_map`/`.and_then` in adapter chains, the B-parity spelling by construction

### 🧪 Testing

- *(v0.6)* The partial-arrow deterministic gates: the kernel pinned against the std eliminations, three-way value agreement over the xorshift sweep, the allocation profile (C == B exactly — the bind riddle retired), the scan refinement, and fused == unfused oracle agreement
- *(ui)* The partial-arrow cases: compile-pass coverage and the spanned rejections (`ResultWitness` is rejected at the witness token), the rejection snapshot pinned
- *(examples)* Pin the partial-arrow consumer gate
- *(release)* The publication contract covers 0.5.1

### 📈 Benchmarks

- *(filter_map)* The §6.1 v0.6 three-way suite — container and pointwise universes; C within ±1.5% of B, the pre-v0.6 bind riddle 15.9–23.4× slower (the verdict recorded in ARCHITECTURE.md §6.1)

### 📚 Documentation

- *(architecture)* §4.3.3 the partial arrow; the §6.1 v0.6 verdict and the delivered roadmap row
- *(readme)* The v0.6.0 milestone row and the partial-arrow feature bullet
- *(examples)* The partial-arrow consumer demo

### ⚙️ Continuous Integration

- Smoke the filter_map bench as the ninth target

## [0.5.1] - 2026-11-09

### 🐛 Bug Fixes

- *(parser)* Unterminated turbofish reports the opening angle
- *(release)* Pins track the version; the gate enforces it

### 🚜 Refactor

- *(ast)* Drop the test-only stage_param accessor

### 📚 Documentation

- *(architecture)* The shipped truth for the budget fallback
- *(readme)* The compiling hatch snippet and demo pointers
- *(examples)* The escape-hatch and oracle consumer demo

## [0.5.0] - 2026-11-10

### 🚀 Features

- *(runtime)* The §2.4/§4.4 strict `Monad` layer — one trait carrying `pure` + `bind`, with provided `join` (`bind(id)`) and derived `fmap` (`bind ∘ pure`), implemented per witness by delegating to the split kernels, the single lowering

### 🧪 Testing

- *(v0.5)* The strict-layer gates — monad laws per witness over the xorshift sweep, the derived/provided kernels pinned against the split layer (`fmap == run_map`, `join == run_join == bind(id)`), three-way value agreement, the allocation profile (strict == split-kernel exactly; the pointwise strict chain == the idiomatic chain), and the extern-crate migration contract
- *(examples)* Pin the strict-monad consumer gate
- *(release)* Move the publication contract to v0.5

### 📈 Benchmarks

- *(monad)* The §6.1 v0.5 pointwise three-way suite — `Option`/`Result` warm and cold universes; C ≈ B within noise on every universe (the bound erases under monomorphization)

### 📚 Documentation

- *(architecture)* The §2.4/§4.4 strict `Monad` layer, the §6.1 counting-alloc record and the v0.5 verdict, the observer module pointer, the delivered roadmap row
- *(readme)* The v0.5 milestone row and the one-trait feature bullet
- *(rustdoc)* The facade points at the strict-monad demo and the `Monad` re-export
- *(changelog)* Record the 0.4.2 entry

### 🚜 Refactor

- *(macros)* Collapse the parse/emit pipeline to one `and_then` (drop the identity error arm)
- *(lints)* Migrate the facade's `#[expect(unused_extern_crates)]` to `[workspace.lints.rust]` with its rationale, gated by the one-`extern crate` contract

### ⚙️ Continuous Integration

- Smoke the monad bench as the eighth target

## [0.4.2] - 2026-11-09

### 📚 Documentation

- *(examples)* The monadic consumer demo (join, fold_map)
- *(architecture)* The pure row names the runtime kernel
- *(readme)* The 40-case truth, the de-drifted badge and status
- *(rustdoc)* Point the facade at the monadic demo
- *(changelog)* Record the 0.4.1 entry

### 🧪 Testing

- *(examples)* Pin the monadic-surface consumer gate
- *(release)* Derive the publication contract from the manifest

### 🚜 Refactor

- *(parser)* Drop the unused KeywordSpan trait

### ⚙️ Miscellaneous Tasks

- *(release)* Bump the workspace to v0.4.2
- Gitignore

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
