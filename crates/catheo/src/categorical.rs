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
//!
//! # On `#[expect(clippy::inline_always)]`
//!
//! The workspace runs clippy pedantic at deny level with **no allow-list**
//! and no `#[allow]` anywhere. §2.2 nevertheless mandates `#[inline(always)]`
//! on kernels, which pedantic flags. The reconciliation is `#[expect]` at
//! each mandated site: a *verifiable* suppression that becomes a compile
//! error the moment the lint stops firing, so no exemption can silently rot.

use crate::hkt::HKT;
use crate::witness::{OptionWitness, ResultWitness, VecWitness};

/// Functor operation: structure-preserving element transformation.
pub trait CategoricalFunctor: HKT {
    /// Apply `f` to every element, preserving the container's shape.
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U;
}

/// Pointed functor: the minimal injection `pure` (§4.3 `pure` / `return`).
///
/// Kept separate from [`CategoricalMonad`] so kernel constraints stay shallow
/// (§2.2): generated code that only needs `pure` does not force `rustc` to
/// normalize the full monad hierarchy. Every monad is pointed; the converse
/// does not hold, which is exactly why this is its own trait.
pub trait CategoricalPointed: HKT {
    /// Lift `value` into the witness's minimal container (§4.3: the
    /// compile-time identity kernel).
    fn run_pure<T>(value: T) -> Self::Type<T>;
}

/// Monadic sequence: element-to-container transformation, flattened.
pub trait CategoricalMonad: CategoricalFunctor {
    /// Bind each element through `f` and flatten one level, in order.
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>;

    /// Squash a doubled container: `join(m)` is `bind(m, identity)` (§4.3).
    ///
    /// A provided kernel by design: §4.3 defines `join` as a *desugar*, so a
    /// single default body shared by every witness is the specification, not
    /// a convenience. It still monomorphizes per `(witness, T)` exactly like
    /// a required kernel (§2.3).
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_join<T>(container: Self::Type<Self::Type<T>>) -> Self::Type<T> {
        Self::run_bind(container, |inner| inner)
    }
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

/// A type with an associative binary operation and an identity element —
/// the algebra `fold_map` eliminates into (§4.3 v0.2).
///
/// The laws (`e ∘ a == a == a ∘ e`, `(a ∘ b) ∘ c == a ∘ (b ∘ c)`) are
/// exactly what makes `fold_map`'s two lowerings interchangeable: the
/// fused `.map(f).fold(identity, combine)` chain and the kernel's
/// element-wise elimination produce the same value for any element order.
/// Numeric impls pick *sum* (documented choice — Rust has no distinct
/// `Sum`/`Product` traits at the type level to disambiguate).
pub trait Monoid {
    /// The identity element: combining it with any value is that value.
    #[must_use]
    fn identity() -> Self;

    /// The associative binary operation.
    #[must_use]
    fn combine(self, other: Self) -> Self;
}

macro_rules! impl_sum_monoid {
    ($($t:ty),* $(,)?) => {$(
        impl Monoid for $t {
            fn identity() -> Self {
                0
            }

            fn combine(self, other: Self) -> Self {
                self + other
            }
        }
    )*};
}

impl_sum_monoid!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

macro_rules! impl_sum_monoid_float {
    ($($t:ty),* $(,)?) => {$(
        impl Monoid for $t {
            fn identity() -> Self {
                0.0
            }

            fn combine(self, other: Self) -> Self {
                self + other
            }
        }
    )*};
}

impl_sum_monoid_float!(f32, f64);

impl Monoid for () {
    fn identity() -> Self {}

    fn combine(self, other: Self) -> Self {
        let (): () = other;
    }
}

impl Monoid for String {
    fn identity() -> Self {
        String::new()
    }

    fn combine(mut self, other: Self) -> Self {
        self.push_str(&other);
        self
    }
}

impl<T> Monoid for Vec<T> {
    fn identity() -> Self {
        Vec::new()
    }

    fn combine(mut self, mut other: Self) -> Self {
        self.append(&mut other);
        self
    }
}

impl<T: Monoid> Monoid for Option<T> {
    fn identity() -> Self {
        None
    }

    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Some(inner), Some(other)) => Some(inner.combine(other)),
            (Some(inner), None) | (None, Some(inner)) => Some(inner),
            (None, None) => None,
        }
    }
}

impl<A: Monoid, B: Monoid> Monoid for (A, B) {
    fn identity() -> Self {
        (<A as Monoid>::identity(), <B as Monoid>::identity())
    }

    fn combine(self, other: Self) -> Self {
        (self.0.combine(other.0), self.1.combine(other.1))
    }
}

/// Monoid-valued universal eliminator (§4.3 v0.2 `fold_map`).
///
/// `run_fold_map(container, f)` maps each element to a monoid value and
/// combines them from the identity — the eliminator for "aggregate a
/// metric over the elements" pipelines, where the accumulator type is the
/// metric's monoid rather than a user-supplied `init`.
pub trait CategoricalFoldMap: HKT {
    /// Map every element through `f` and combine from [`Monoid::identity`].
    fn run_fold_map<T, M, F>(container: Self::Type<T>, f: F) -> M
    where
        M: Monoid,
        F: Fn(T) -> M;
}

/// Traversable eliminator (§4.3 v0.3 `traverse`) — the kernel shape §7
/// limitation 4 predicted: unlike every Layer-1 kernel, it consumes an
/// *iterator* rather than a witness container.
///
/// `run_traverse(items, f)` drives `f: T -> Self::Type<U>` — the pointwise
/// effect this witness stands for — across `items` and collects the results
/// into one effect of the same shape around the collected [`Vec`]. The
/// inner container is pinned to `Vec<U>` because the stable-Rust solver
/// (§5.1) cannot normalize the doubly-projected `Self::Type<W::Type<U>>` a
/// shape-generic signature would need; the traversal grammar (§4.3 v0.3)
/// accordingly requires a `Vec` container witness.
///
/// The impls are the idiomatic std spellings — `items.map(f).collect()` —
/// so the kernel *is* the short-circuiting `FromIterator` elimination the
/// §6.1 B-parity target spells by hand, and the first `None`/`Err` stops
/// the traversal without forcing later elements.
pub trait CategoricalTraverse: HKT {
    /// Run the pointwise effect `f` across `items` and collect the results
    /// into `Self::Type<Vec<U>>`.
    fn run_traverse<T, U, F>(items: impl Iterator<Item = T>, f: F) -> Self::Type<Vec<U>>
    where
        F: FnMut(T) -> Self::Type<U>;
}

/// Kleisli composition `f >=> g` (§4.3 v0.2).
///
/// Returns `A -> W::Type<C>` that runs `f` and feeds every success through
/// `g` — value-identical to `bind(f)` followed by `bind(g)`, which is the
/// desugaring the `kleisli:` pipeline entry performs at parse time.
///
/// The witness argument is the §4.1 head anchor in function form: the ZST
/// is never read, it only pins `W` (and thereby the container types) by
/// inference.
#[expect(clippy::inline_always)]
#[inline(always)]
pub fn kleisli<W, A, B, C, F, G>(_witness: W, f: F, g: G) -> impl Fn(A) -> W::Type<C>
where
    W: CategoricalMonad,
    F: Fn(A) -> W::Type<B>,
    G: Fn(B) -> W::Type<C>,
{
    move |a| W::run_bind(f(a), &g)
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
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U,
    {
        container.into_iter().map(f).collect()
    }
}

impl CategoricalMonad for VecWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        container.into_iter().flat_map(f).collect()
    }
}

impl CategoricalPointed for VecWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_pure<T>(value: T) -> Self::Type<T> {
        vec![value]
    }
}

impl CategoricalFilter for VecWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_filter<T, P>(container: Self::Type<T>, p: P) -> Self::Type<T>
    where
        P: Fn(&T) -> bool,
    {
        container.into_iter().filter(p).collect()
    }
}

impl CategoricalFold for VecWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where
        F: Fn(A, T) -> A,
    {
        container.into_iter().fold(init, f)
    }
}

impl CategoricalFoldMap for VecWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_fold_map<T, M, F>(container: Self::Type<T>, f: F) -> M
    where
        M: Monoid,
        F: Fn(T) -> M,
    {
        container
            .into_iter()
            .map(f)
            .fold(<M as Monoid>::identity(), <M as Monoid>::combine)
    }
}

impl CategoricalIntoIter for VecWitness {
    type Item<T> = std::vec::IntoIter<T>;

    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn into_iter<T>(container: Self::Type<T>) -> Self::Item<T> {
        container.into_iter()
    }
}

// ─── OptionWitness kernels ──────────────────────────────────────────────────

impl CategoricalFunctor for OptionWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U,
    {
        container.map(f)
    }
}

impl CategoricalMonad for OptionWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        container.and_then(f)
    }
}

impl CategoricalPointed for OptionWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_pure<T>(value: T) -> Self::Type<T> {
        Some(value)
    }
}

impl CategoricalFilter for OptionWitness {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_filter<T, P>(container: Self::Type<T>, p: P) -> Self::Type<T>
    where
        P: Fn(&T) -> bool,
    {
        container.filter(p)
    }
}

impl CategoricalFold for OptionWitness {
    #[expect(clippy::inline_always)]
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

impl CategoricalFoldMap for OptionWitness {
    // One discriminant test — the idiomatic elimination on a
    // zero-or-one-element container (the §6.1 M4 posture).
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_fold_map<T, M, F>(container: Self::Type<T>, f: F) -> M
    where
        M: Monoid,
        F: Fn(T) -> M,
    {
        container.map_or_else(<M as Monoid>::identity, f)
    }
}

impl CategoricalIntoIter for OptionWitness {
    type Item<T> = std::option::IntoIter<T>;

    #[expect(clippy::inline_always)]
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
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> U,
    {
        container.map(f)
    }
}

impl<E> CategoricalMonad for ResultWitness<E> {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where
        F: Fn(T) -> Self::Type<U>,
    {
        container.and_then(f)
    }
}

impl<E> CategoricalPointed for ResultWitness<E> {
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_pure<T>(value: T) -> Self::Type<T> {
        Ok(value)
    }
}

impl<E> CategoricalFold for ResultWitness<E> {
    #[expect(clippy::inline_always)]
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

impl<E> CategoricalFoldMap for ResultWitness<E> {
    // One discriminant test; `Err` contributes the identity (the same
    // "Err → init" denotation `run_fold` documents).
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_fold_map<T, M, F>(container: Self::Type<T>, f: F) -> M
    where
        M: Monoid,
        F: Fn(T) -> M,
    {
        container.map_or_else(|_| <M as Monoid>::identity(), f)
    }
}

impl<E> CategoricalIntoIter for ResultWitness<E> {
    type Item<T> = std::result::IntoIter<T>;

    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn into_iter<T>(container: Self::Type<T>) -> Self::Item<T> {
        container.into_iter()
    }
}

// ─── CategoricalTraverse kernels (§4.3 v0.3) ────────────────────────────────
//
// The traversal kernel is implemented for the *pointwise effect* witnesses
// only (§2.1 built-ins): the effect is what the per-element arrow lands in,
// and the container side contributes its `CategoricalIntoIter` iterator at
// the call site. `VecWitness` has no impl by design — a `Vec` effect is the
// cartesian traversal, which the grammar rejects.

impl CategoricalTraverse for OptionWitness {
    // The std `FromIterator<Option<T>> for Option<V>` elimination: the
    // first `None` stops the fold without forcing later elements.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_traverse<T, U, F>(items: impl Iterator<Item = T>, f: F) -> Self::Type<Vec<U>>
    where
        F: FnMut(T) -> Self::Type<U>,
    {
        items.map(f).collect::<Option<Vec<U>>>()
    }
}

impl<E> CategoricalTraverse for ResultWitness<E> {
    // The std `FromIterator<Result<T, E>> for Result<V, E>` elimination:
    // the first `Err` stops the fold and is carried out unchanged.
    #[expect(clippy::inline_always)]
    #[inline(always)]
    fn run_traverse<T, U, F>(items: impl Iterator<Item = T>, f: F) -> Self::Type<Vec<U>>
    where
        F: FnMut(T) -> Self::Type<U>,
    {
        items.map(f).collect::<Result<Vec<U>, E>>()
    }
}
