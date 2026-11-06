//! Layer 0 — the HKT foundation (ARCHITECTURE.md §2.1).
//!
//! Zero-sized witness structs proxy for type constructors; the GAT provides
//! the type-level projection. Witnesses are runtime-free: no byte, no cycle
//! (verified by `witnesses_are_zero_sized`).

/// Higher-kinded type projection: `W: HKT<Type<T> = F<T>>` says "the witness
/// `W` stands for the type constructor `F`".
pub trait HKT {
    /// The type constructor this witness proxies, applied to `T`.
    type Type<T>;
}
