//! Layer 1.7 — the strict `Filterable` unification (the §2.4 posture,
//! partial-arrow side).
//!
//! The partial-arrow counterpart of the v0.5 [`Monad`] layer and the
//! [`crate::Foldable`] eliminator layer: `filter_map` and `filter` in
//! **one** trait, shipped additively. Every admitting witness implements
//! [`Filterable`] by delegating to the split kernels of §2.2
//! ([`crate::CategoricalFilterMap`]), which remain the single lowering.
//! No codegen churn: the `pipeline!` grammar and the emitted kernels are
//! untouched, and the §6.1 gates re-apply unchanged.
//!
//! The derivation is the filter-side twin of the v0.5 `fmap == bind ∘
//! pure` pin: [`Filterable::filter`] is *provided*, defined as
//! `filter_map(m, |x| if p(&x) { Some(x) } else { None })` — the
//! predicate spelled through the partial arrow it is (§4.3.3: `filter`'s
//! keep-or-drop semantics *are* a partial arrow whose kept arm is the
//! identity). The derivation is pinned against the split
//! [`crate::CategoricalFilter::run_filter`] kernel by
//! `tests/filterable_gates.rs`, per witness over the xorshift sweep,
//! alongside the trivial-predicate laws (identity, empty), idempotence,
//! and composition.
//!
//! # Who admits a partial arrow
//!
//! The trait is implemented for [`VecWitness`] and [`OptionWitness`]
//! only, mirroring the split kernels: `ResultWitness<E>` has **no**
//! impl — a dropped `Ok` has no error-free denotation, and a partial
//! arrow over `Result` would have to fabricate an `Err` value the user
//! never produced (the §4.3.3 posture, stated on
//! [`crate::CategoricalFilterMap`] and enforced by the `pipeline!`
//! grammar's `E0277` at the witness token). The absence is pinned by the
//! compile-fail doctest on this trait.
//!
//! The delegation monomorphizes into the kernels (§2.3), so the
//! abstraction is free: the allocation gates pin strict ==
//! split-kernel counts exactly — the same posture as
//! `tests/v05_gates.rs` (monad side) and `tests/foldable_gates.rs`
//! (fold side). This layer is a consumer seam — one trait to bound on in
//! a generic keep-or-drop program — and touches neither the parser nor
//! the fusion pass.

use crate::categorical::CategoricalFilterMap;
use crate::hkt::HKT;
use crate::witness::{OptionWitness, VecWitness};

/// The strict partial arrow: `filter_map` and `filter` in one trait
/// (the §2.4 posture, partial-arrow side).
///
/// A consumer program bounds on a single [`Filterable`] parameter and
/// keeps, monomorphized, across both admitting witnesses — the generic
/// body `fn kept<F: Filterable>(xs: F::Type<i32>) -> F::Type<i64>`
/// instantiates for `VecWitness` and `OptionWitness` without naming the
/// split trait. The required-kernel signature is the split layer's,
/// verbatim: a consumer that today bounds on
/// [`crate::CategoricalFilterMap`] moves to the single bound without
/// rewriting a call site (the §5.1 non-breaking-migration posture).
///
/// Not implemented for [`crate::ResultWitness`]: dropping an `Ok` has no
/// error-free denotation (§4.3.3). The compile-fail doctest below pins
/// the absence.
///
/// ```compile_fail
/// use catheo::{Filterable, ResultWitness};
///
/// fn assert_admits<W: Filterable>() {}
///
/// // A `Result` has no error-free partial arrow: a dropped `Ok` would
/// // need a fabricated `Err` value the user never produced, so the
/// // witness does not admit the trait (§4.3.3).
/// assert_admits::<ResultWitness<String>>();
/// ```
pub trait Filterable: HKT {
    /// Keep-or-drop elimination: apply the partial arrow `f` to every
    /// element and drop the `None`s (§4.3.3). The required kernel;
    /// delegates to [`CategoricalFilterMap::run_filter_map`].
    fn filter_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: FnMut(T) -> Option<U>;

    /// Predicate elimination, derived: `filter(m, p)` is
    /// `filter_map(m, |x| if p(&x) { Some(x) } else { None })`.
    ///
    /// The derivation is pinned against the split
    /// [`crate::CategoricalFilter::run_filter`] kernel by
    /// `tests/filterable_gates.rs` (`filter == run_filter`): keep-or-drop
    /// *is* the partial arrow whose kept arm is the identity (§4.3.3). A
    /// provided kernel by design, mirroring [`crate::Monad::fmap`] and
    /// [`crate::Foldable::fold_map`]: a single default body shared by
    /// every admitting witness is the specification, not a convenience,
    /// and it monomorphizes per `(witness, T, P)` exactly like a required
    /// kernel (§2.3).
    fn filter<T, P>(container: Self::Type<T>, p: P) -> Self::Type<T>
    where
        P: Fn(&T) -> bool,
    {
        Self::filter_map(container, |x| if p(&x) { Some(x) } else { None })
    }
}

impl Filterable for VecWitness {
    fn filter_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: FnMut(T) -> Option<U>,
    {
        <Self as CategoricalFilterMap>::run_filter_map(container, f)
    }
}

impl Filterable for OptionWitness {
    fn filter_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: FnMut(T) -> Option<U>,
    {
        <Self as CategoricalFilterMap>::run_filter_map(container, f)
    }
}

// The delegation sites carry no `#[inline(always)]` of their own: §2.2
// mandates the attribute on the *kernels* (every split-impl site), and the
// delegation monomorphizes into those kernels — the abstraction is erased
// by construction, which is exactly what the allocation gate pins
// (`tests/filterable_gates.rs`: strict == split-kernel counts). The
// `ResultWitness` non-impl is the §4.3.3 posture: see the trait docs.
