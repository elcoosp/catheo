//! Built-in witnesses for the standard-library type constructors (§2.1).
//!
//! The macro generates or validates witnesses; users never write them by
//! hand unless extending the library to a custom type constructor.

use core::marker::PhantomData;

use crate::hkt::HKT;

/// Witness for the `Vec` type constructor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct VecWitness;

impl HKT for VecWitness {
    type Type<T> = Vec<T>;
}

/// Witness for the `Option` type constructor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OptionWitness;

impl HKT for OptionWitness {
    type Type<T> = Option<T>;
}

/// Witness for the `Result<T, E>` type constructor, parameterized by the
/// error type: `ResultWitness<E>` projects to `Result<T, E>`.
///
/// Note that [`crate::CategoricalFilter`] is intentionally **not**
/// implemented for this witness: filtering a `Result` has no error-free
/// denotation (a dropped `Ok` value would need a fabricated `Err`). A
/// `filter` stage over `ResultWitness<E>` is a compile error.
pub struct ResultWitness<E> {
    _marker: PhantomData<fn() -> E>,
}

impl<E> ResultWitness<E> {
    /// Construct the witness (zero-sized).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<E> Default for ResultWitness<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Clone for ResultWitness<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E> Copy for ResultWitness<E> {}

impl<E> core::fmt::Debug for ResultWitness<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ResultWitness")
    }
}

impl<E> HKT for ResultWitness<E> {
    type Type<T> = Result<T, E>;
}
