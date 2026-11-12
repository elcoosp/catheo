//! Layer 1.6 — the strict `Foldable` unification (the §2.4 posture,
//! eliminator side).
//!
//! The fold-side counterpart of the v0.5 [`Monad`] layer: `fold` and
//! `fold_map` in **one** trait, shipped additively. Every witness
//! implements [`Foldable`] by delegating to the split kernels of §2.2
//! ([`crate::CategoricalFold`]), which remain the single lowering. No
//! codegen churn: the `pipeline!` grammar and the emitted kernels are
//! untouched, and the §6.1 gates re-apply unchanged.
//!
//! The derivation is the eliminator-side twin of the v0.5 `fmap == bind ∘
//! pure` pin: [`Foldable::fold_map`] is *provided*, defined as
//! `fold(m, M::identity(), |acc, x| M::combine(acc, f(x)))` — sound for
//! every [`Monoid`] because the identity and associativity laws are
//! exactly what make the two lowerings interchangeable (§4.3 v0.2). The
//! derivation is pinned against the split
//! [`crate::CategoricalFoldMap::run_fold_map`] kernel by
//! `tests/foldable_gates.rs`, per witness over the xorshift sweep.
//!
//! The delegation monomorphizes into the kernels (§2.3), so the
//! abstraction is free: the allocation gates pin strict == split-kernel
//! counts exactly, the same posture as `tests/v05_gates.rs` for the
//! monad side. The v0.2-era caveat stands for the *macro surface*: this
//! layer is a consumer seam — one trait to bound on in a generic
//! eliminator program — and touches neither the parser nor the fusion
//! pass.

use crate::categorical::{CategoricalFold, Monoid};
use crate::hkt::HKT;
use crate::witness::{OptionWitness, ResultWitness, VecWitness};

/// The strict eliminator: `fold` and `fold_map` in one trait (the §2.4
/// posture, fold side).
///
/// A consumer program bounds on a single [`Foldable`] parameter and
/// eliminates, monomorphized, across every witness — the generic body
/// `fn total<F: Foldable>(xs: F::Type<i32>) -> i64` instantiates for
/// `VecWitness`, `OptionWitness`, and `ResultWitness<E>` without naming
/// the split trait. The required-kernel signature is the split layer's,
/// verbatim: a consumer that today bounds on
/// [`crate::CategoricalFold`] moves to the single bound without
/// rewriting a call site (the §5.1 non-breaking-migration posture).
pub trait Foldable: HKT {
    /// Universal elimination: left-fold the elements into the
    /// accumulator `init` (§4.3 `fold`). The required kernel; delegates
    /// to [`CategoricalFold::run_fold`].
    fn fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A;

    /// Monoid-valued elimination, derived: `fold_map(m, f)` is
    /// `fold(m, M::identity(), |acc, x| M::combine(acc, f(x)))`.
    ///
    /// The derivation is pinned against the split
    /// [`crate::CategoricalFoldMap::run_fold_map`] kernel by
    /// `tests/foldable_gates.rs` (`fold_map == run_fold_map`): the
    /// [`Monoid`] identity and associativity laws are exactly what makes
    /// the two lowerings interchangeable for any element order (§4.3
    /// v0.2). A provided kernel by design, mirroring [`Monad::fmap`]: a
    /// single default body shared by every witness is the specification,
    /// not a convenience, and it monomorphizes per `(witness, T, M)`
    /// exactly like a required kernel (§2.3).
    fn fold_map<T, M, F>(container: Self::Type<T>, f: F) -> M
    where
        M: Monoid,
        F: Fn(T) -> M,
    {
        Self::fold(container, <M as Monoid>::identity(), |acc, x| {
            <M as Monoid>::combine(acc, f(x))
        })
    }
}

impl Foldable for VecWitness {
    fn fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A,
    {
        <Self as CategoricalFold>::run_fold(container, init, f)
    }
}

impl Foldable for OptionWitness {
    fn fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A,
    {
        <Self as CategoricalFold>::run_fold(container, init, f)
    }
}

impl<E> Foldable for ResultWitness<E> {
    fn fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A,
    {
        <Self as CategoricalFold>::run_fold(container, init, f)
    }
}

// The delegation sites carry no `#[inline(always)]` of their own: §2.2
// mandates the attribute on the *kernels* (every split-impl site), and the
// delegation monomorphizes into those kernels — the abstraction is erased
// by construction, which is exactly what the allocation gate pins
// (`tests/foldable_gates.rs`: strict == split-kernel counts).
