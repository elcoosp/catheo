//! `catheo-parser` — the syntax-directed compiler core of catheo.
//!
//! This crate is deliberately *not* a proc-macro crate: keeping the AST,
//! parser, and codegen here makes them unit-testable and benchmarkable
//! without driving `rustc` (§6.3 expansion budget). The `catheo-macros`
//! crate is a thin wrapper that converts `proc_macro::TokenStream` in and
//! out; all real work happens in this crate.
//!
//! Module map (docs/ARCHITECTURE.md §3):
//! - [`ast`] — the `Pipeline` AST: structure, not semantics.
//! - [`parser`] — grammar: custom keywords, raw-token closures, spanned
//!   errors; barrier annotation happens at parse time (§3.2).
//! - [`observer`] — the lexical structural-observer approximation (§3.2)
//!   and the fold purity check (§3.4).
//! - [`fuse`] — the linear-time deforestation pass (§3.3): Rules 1–3 over
//!   the stage list, plus the shared definition of Rule 4's absorbable
//!   suffix.
//! - [`codegen`] — emission of monomorphic Rust items (§3.4): the fused
//!   SSA lowering with absorbed folds (M2) and the naive sequential
//!   lowering kept as the §6.2 unfused oracle.

pub mod ast;
pub mod codegen;
pub mod fuse;
pub mod observer;
pub mod parser;
#[cfg(test)]
mod parser_tests;

pub use ast::{Closure, Param, Pipeline, Source, Stage, StageKind, Terminal, Witness};
pub use codegen::{Lowering, emit, emit_sequential, emit_with_lowering, token_count};
pub use fuse::{
    FusedStage, absorbable_components, absorbable_suffix, compose, fuse_components, fuse_stages,
};
pub use parser::parse_pipeline;
