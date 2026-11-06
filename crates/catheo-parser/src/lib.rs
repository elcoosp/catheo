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
//! - [`codegen`] — emission of monomorphic Rust items (§3.4); M0 emits the
//!   naive sequential lowering, deforestation lands in M2.

pub mod ast;
pub mod codegen;
pub mod observer;
pub mod parser;
#[cfg(test)]
mod parser_tests;

pub use ast::{Closure, Param, Pipeline, Source, Stage, StageKind, Terminal, Witness};
pub use codegen::emit;
pub use parser::parse_pipeline;
