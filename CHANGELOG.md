# Changelog

All notable changes to catheo are documented here. The format is based on
[Conventional Commits](https://www.conventionalcommits.org), and releases
are cut by pushing a `v*.*.*` tag (see `.github/workflows/release.yml`).

<!-- git-cliff: end of header -->
## [0.0.1] - 2026-11-06

### 🚀 Features

- *(parser)* Implement syntax-directed pipeline parser (TDD green)
- *(runtime)* Layer 0 HKT witnesses and Layer 1 split-trait kernels
- *(macro)* Pipeline! proc-macro with naive M0 lowering and trybuild UI tests

### 🎨 Styling

- Satisfy clippy pedantic and rustfmt across the workspace

### 🧪 Testing

- *(parser)* Add pipeline grammar unit tests (TDD red)
- *(runtime)* Add Layer 0/1 kernel specs for Vec/Option/Result (TDD red)

### ⚙️ Miscellaneous Tasks

- Add arch
