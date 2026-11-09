//! Layer 1.5 — the strict `Monad` unification (ARCHITECTURE.md §2.4, v0.5).
//!
//! The classical definition — `pure` and `bind` in **one** trait — shipped
//! additively: every witness implements [`Monad`] by delegating to the
//! split kernels of §2.2, which remain the single lowering. No codegen
//! churn: the `pipeline!` grammar and the emitted kernels are untouched,
//! and the §6.1 gates re-apply unchanged.
//!
//! The v0.2-era caveat stands for the *surface migration*: folding the
//! split traits into GAT supertraits with default bodies over a generic
//! `Self` still fails on stable Rust with `E0276`/`E0277` (the
//! shape-generic wall, §5.1), so the macro surface keeps the dual-witness
//! layer. What v0.5 delivers is the *consumer-facing* unification: one
//! trait to bound on, laws-tested per witness (`tests/v05_gates.rs`),
//! with [`Monad::join`] provided and [`Monad::fmap`] derived from
//! `bind ∘ pure` — the derivation is pinned against the split kernels
//! (`fmap == run_map`) by the same gates.
//!
//! Full surface migration stays tracked against `-Znext-solver`
//! stabilization (§7 limitation 3, §8 roadmap).

use crate::categorical::{CategoricalMonad, CategoricalPointed};
use crate::hkt::HKT;
use crate::witness::{OptionWitness, ResultWitness, VecWitness};

/// The strict monad: `pure` and `bind` in one trait (ARCHITECTURE.md
/// §4.4, v0.5).
///
/// Every monad is a pointed functor — [`Monad::fmap`] derives `map` as
/// `bind(m, |x| pure(f(x)))` and [`Monad::join`] stays `bind(m, identity)`
/// (§4.3.2) — but the trait carries exactly the two required kernels;
/// everything else is provided and law-checked (`tests/v05_gates.rs`).
///
/// The required-kernel signatures are the split layer's, verbatim: a
/// consumer that today bounds on `CategoricalMonad` +
/// `CategoricalPointed` moves to the single bound without rewriting a
/// call site (§5.1's non-breaking-migration posture, delivered at the
/// consumer seam).
pub trait Monad: HKT {
    /// Lift `value` into the witness's minimal container (§4.3 `pure`).
    fn pure<T>(value: T) -> Self::Type<T>;

    /// Monadic sequence: element-to-container transformation, flattened.
    fn bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>;

    /// Squash a doubled container: `join(m)` is `bind(m, identity)`
    /// (§4.3.2). Provided, mirroring [`CategoricalMonad::run_join`]: a
    /// single default body shared by every witness is the specification
    /// (§2.3), and it monomorphizes per `(witness, T)` exactly like a
    /// required kernel.
    fn join<T>(container: Self::Type<Self::Type<T>>) -> Self::Type<T> {
        Self::bind(container, |inner| inner)
    }

    /// The functor operation, derived: `fmap(m, f)` is
    /// `bind(m, |x| pure(f(x)))`. The derivation is pinned against the
    /// split functor kernel by `tests/v05_gates.rs` (`fmap == run_map`).
    fn fmap<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U,
    {
        Self::bind(container, |x| Self::pure(f(x)))
    }
}

impl Monad for VecWitness {
    fn pure<T>(value: T) -> Self::Type<T> {
        <Self as CategoricalPointed>::run_pure(value)
    }

    fn bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        <Self as CategoricalMonad>::run_bind(container, f)
    }
}

impl Monad for OptionWitness {
    fn pure<T>(value: T) -> Self::Type<T> {
        <Self as CategoricalPointed>::run_pure(value)
    }

    fn bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        <Self as CategoricalMonad>::run_bind(container, f)
    }
}

impl<E> Monad for ResultWitness<E> {
    fn pure<T>(value: T) -> Self::Type<T> {
        <Self as CategoricalPointed>::run_pure(value)
    }

    fn bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        <Self as CategoricalMonad>::run_bind(container, f)
    }
}

// The delegation sites carry no `#[inline(always)]` of their own: §2.2
// mandates the attribute on the *kernels* (every split-impl site), and the
// delegation monomorphizes into those kernels — the abstraction is erased
// by construction, which is exactly what the v0.5 allocation gate pins
// (`tests/v05_gates.rs`: strict == split-kernel counts).
