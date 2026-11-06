//! catheo — category theory at the speed of Rust.
//!
//! A categorical pipeline library whose proc macro compiles functor/monad
//! composition down to flat, fused imperative loops (see
//! `docs/ARCHITECTURE.md`). This crate is the user-facing facade: it
//! re-exports the [`pipeline!`] macro and the runtime layer (witnesses and
//! split-trait kernels) that the macro's generated code calls into.
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
// tests and doctests. Unused at the top level, hence the allow.
#[allow(unused_extern_crates)]
extern crate self as catheo;

pub mod categorical;
pub mod hkt;
pub mod witness;

pub use categorical::{
    CategoricalFilter, CategoricalFold, CategoricalFunctor, CategoricalIntoIter, CategoricalMonad,
};
pub use catheo_macros::pipeline;
pub use hkt::HKT;
pub use witness::{OptionWitness, ResultWitness, VecWitness};
