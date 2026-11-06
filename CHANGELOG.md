# Changelog

All notable changes to catheo are documented here. The format is based on
[Conventional Commits](https://www.conventionalcommits.org), and releases
are cut by pushing a `v*.*.*` tag (see `.github/workflows/release.yml`).

<!-- git-cliff: end of header -->
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
