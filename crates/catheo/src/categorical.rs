//! Layer 1 — the split-trait dual-witness kernels (ARCHITECTURE.md §2.2).
//!
//! Strict `Monad` implementations currently fail on stable Rust with
//! `E0276`/`E0277` GAT normalization errors, so the categorical operations
//! are decoupled into shallow traits that `rustc` normalizes linearly
//! (the *transitional architecture*). When `-Znext-solver` stabilizes,
//! `CategoricalMonad` is superseded by a strict `Monad` supertrait without
//! breaking user code: the `run_bind` signature is compatible.
//!
//! Every kernel is an associated function taking the container by value and
//! monomorphizes per call site (§2.3). Implementations are `#[inline(always)]`
//! per §2.2 so LLVM sees through the witness indirection entirely.

use crate::hkt::HKT;
use crate::witness::{OptionWitness, ResultWitness, VecWitness};

/// Functor operation: structure-preserving element transformation.
pub trait CategoricalFunctor: HKT {
    /// Apply `f` to every element, preserving the container's shape.
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U;
}

/// Monadic sequence: element-to-container transformation, flattened.
pub trait CategoricalMonad: CategoricalFunctor {
    /// Bind each element through `f` and flatten one level, in order.
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>;
}

/// Predicate-based element removal.
///
/// Implemented for `VecWitness` and `OptionWitness` only: a `Result` has no
/// error-free denotation for a dropped `Ok` (see [`crate::ResultWitness`]).
pub trait CategoricalFilter: CategoricalFunctor {
    /// Keep only the elements for which `p` holds.
    fn run_filter<T, P>(container: Self::Type<T>, p: P) -> Self::Type<T>
    where
        P: Fn(&T) -> bool;
}

/// Universal eliminator: consume the container into a single value.
pub trait CategoricalFold: HKT {
    /// Left-fold the elements into the accumulator `init`.
    fn run_fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A;
}

/// Container-to-iterator lowering, used by `collect` and `fold` terminals.
pub trait CategoricalIntoIter: HKT {
    /// The owning iterator type for this container.
    type Item<T>: Iterator<Item = T>;
    /// Consume the container into its owning iterator.
    fn into_iter<T>(container: Self::Type<T>) -> Self::Item<T>;
}

// ─── VecWitness kernels ─────────────────────────────────────────────────────

impl CategoricalFunctor for VecWitness {
    #[inline(always)]
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U,
    {
        container.into_iter().map(f).collect()
    }
}

impl CategoricalMonad for VecWitness {
    #[inline(always)]
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        container.into_iter().flat_map(f).collect()
    }
}

impl CategoricalFilter for VecWitness {
    #[inline(always)]
    fn run_filter<T, P>(container: Self::Type<T>, p: P) -> Self::Type<T>
    where
        P: Fn(&T) -> bool,
    {
        container.into_iter().filter(p).collect()
    }
}

impl CategoricalFold for VecWitness {
    #[inline(always)]
    fn run_fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A,
    {
        container.into_iter().fold(init, f)
    }
}

impl CategoricalIntoIter for VecWitness {
    type Item<T> = std::vec::IntoIter<T>;

    #[inline(always)]
    fn into_iter<T>(container: Self::Type<T>) -> Self::Item<T> {
        container.into_iter()
    }
}

// ─── OptionWitness kernels ──────────────────────────────────────────────────

impl CategoricalFunctor for OptionWitness {
    #[inline(always)]
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U,
    {
        container.map(f)
    }
}

impl CategoricalMonad for OptionWitness {
    #[inline(always)]
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        container.and_then(f)
    }
}

impl CategoricalFilter for OptionWitness {
    #[inline(always)]
    fn run_filter<T, P>(container: Self::Type<T>, p: P) -> Self::Type<T>
    where
        P: Fn(&T) -> bool,
    {
        container.filter(p)
    }
}

impl CategoricalFold for OptionWitness {
    #[inline(always)]
    fn run_fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A,
    {
        match container {
            Some(item) => f(init, item),
            None => init,
        }
    }
}

impl CategoricalIntoIter for OptionWitness {
    type Item<T> = std::option::IntoIter<T>;

    #[inline(always)]
    fn into_iter<T>(container: Self::Type<T>) -> Self::Item<T> {
        container.into_iter()
    }
}

// ─── ResultWitness kernels ──────────────────────────────────────────────────
//
// No `CategoricalFilter` impl by design: dropping an `Ok` would require
// fabricating an `Err` value that the user never produced.

impl<E> CategoricalFunctor for ResultWitness<E> {
    #[inline(always)]
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U,
    {
        container.map(f)
    }
}

impl<E> CategoricalMonad for ResultWitness<E> {
    #[inline(always)]
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        container.and_then(f)
    }
}

impl<E> CategoricalFold for ResultWitness<E> {
    #[inline(always)]
    fn run_fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A,
    {
        match container {
            Ok(item) => f(init, item),
            Err(_) => init,
        }
    }
}

impl<E> CategoricalIntoIter for ResultWitness<E> {
    type Item<T> = std::result::IntoIter<T>;

    #[inline(always)]
    fn into_iter<T>(container: Self::Type<T>) -> Self::Item<T> {
        container.into_iter()
    }
}
