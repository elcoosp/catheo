//! catheo — category theory at the speed of Rust.
//!
//! A categorical pipeline library whose proc macro compiles functor/monad
//! composition down to flat, fused imperative loops (see
//! `docs/ARCHITECTURE.md`). This crate is the user-facing facade: it
//! re-exports the [`pipeline!`] macro and the runtime layer (witnesses and
//! split-trait kernels) that the macro's generated code calls into. The
//! runnable consumer demos live in `crates/catheo/examples/`: `quickstart`
//! (the README snippet), `fallible_pipeline` (the §4.3 traversal boundary),
//! `monadic_surface` (the monadic surface — `join` and `fold_map`), and
//! `instrumented_pipeline` (the observable seam — `tracing` spans
//! around the run, `miette` help text on the error path; dev-only
//! dependencies that no shipped artifact links).
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
// tests and doctests. Unused at the top level: the expectation documents
// exactly that and errors if the lint ever stops firing.
#[expect(unused_extern_crates)]
extern crate self as catheo;

pub mod categorical;
pub mod hkt;
pub mod witness;

pub use categorical::{
    CategoricalFilter, CategoricalFold, CategoricalFoldMap, CategoricalFunctor,
    CategoricalIntoIter, CategoricalMonad, CategoricalPointed, CategoricalTraverse, Monoid,
    kleisli,
};
pub use catheo_macros::{pipeline, pipeline_unfused};
pub use hkt::HKT;
pub use witness::{OptionWitness, ResultWitness, VecWitness};
