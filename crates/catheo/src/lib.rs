//! catheo — category theory at the speed of Rust.
//!
//! A categorical pipeline library whose proc macro compiles functor/monad
//! composition down to flat, fused imperative loops (see
//! `docs/ARCHITECTURE.md`). This crate is the user-facing facade: it
//! re-exports the [`pipeline!`] macro, the runtime layer (witnesses,
//! split-trait kernels, the v0.6 partial arrow [`CategoricalFilterMap`],
//! and the v0.5 strict [`Monad`] unification that delegates to those
//! kernels) that the macro's generated code calls into. The runnable
//! consumer demos live in `crates/catheo/examples/`: `quickstart` (the
//! README snippet), `fallible_pipeline` (the §4.3 traversal boundary),
//! `monadic_surface` (the monadic surface — `join` and `fold_map`),
//! `strict_monad` (the §4.4 one-trait surface — one `Monad` bound across
//! every witness), `instrumented_pipeline` (the observable seam —
//! `tracing` spans around the run, `miette` help text on the error path;
//! dev-only dependencies that no shipped artifact links),
//! `filter_map_pipeline` (the §4.3 v0.6 partial arrow behind the same
//! observable seam), `escape_hatches` (the §4.2 escape hatches and
//! the §6.2 unfused oracle, in the same observable posture), and
//! `kernel_surface` (this runtime layer driven directly — the §2.2
//! split kernels, the standalone [`kleisli`] composition, and a
//! user-defined [`Monoid`], no macro in sight).
//!
//! ```
//! use catheo::{pipeline, VecWitness};
//!
//! let input = vec![1_i32, 2, 3];
//! let total: i64 = pipeline! {
//!     witness: VecWitness,
//!     source: input,
//!     map: |x| x * 2,
//!     fold: (0_i64, |acc, x| acc + i64::from(x)),
//! };
//! assert_eq!(total, 12);
//! ```

// The generated code refers to the runtime through the absolute path
// `::catheo::…`; this alias makes that work inside this crate's own unit
// tests and doctests. Unused at the top level by construction — the
// `unused_extern_crates` allowance lives in `[workspace.lints.rust]` with
// its rationale (and the `tests/v05_gates.rs` gate that keeps this the
// only declared `extern crate` in the tree).
extern crate self as catheo;

pub mod categorical;
pub mod hkt;
pub mod monad;
pub mod witness;

pub use categorical::{
    CategoricalFilter, CategoricalFilterMap, CategoricalFold, CategoricalFoldMap,
    CategoricalFunctor, CategoricalIntoIter, CategoricalMonad, CategoricalPointed,
    CategoricalTraverse, Monoid, kleisli,
};
pub use catheo_macros::{pipeline, pipeline_unfused};
pub use hkt::HKT;
pub use monad::Monad;
pub use witness::{OptionWitness, ResultWitness, VecWitness};
