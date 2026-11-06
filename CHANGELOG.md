# Changelog

All notable changes to catheo are documented here. The format is based on
[Conventional Commits](https://www.conventionalcommits.org), and releases
are cut by pushing a `v*.*.*` tag (see `.github/workflows/release.yml`).

<!-- git-cliff: end of header -->
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
