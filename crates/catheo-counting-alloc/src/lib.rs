//! Deterministic allocation counting for the §6.1 decision gates (M3).
//!
//! The M3 gate ("C ≥ B on all pipelines; C > A on the map-heavy and
//! fold-heavy pipelines") compares the macro-flattened output (C) against
//! idiomatic iterator chains (B) and naive per-stage-materializing
//! closures (A). Wall-clock comparisons are not enforceable on shared CI
//! runners; *allocation counts* are. They are the deterministic proxy the
//! repo's gate style demands (the M1 kernel-linearity and §6.3 budget
//! gates set the precedent): a fused pipeline that silently regressed to
//! materializing intermediates changes its count, and the gate fails.
//!
//! The counter is **thread-local**: every `#[test]` in a gate binary runs
//! on its own thread, and only the thread inside [`counted`] has a
//! non-zero count, so concurrently running tests cannot pollute each
//! other's measurement.
//!
//! Install the allocator in a test binary with:
//!
//! ```ignore
//! #[global_allocator]
//! static COUNTING: Counting = Counting;
//! ```
//!
//! This crate is the entire `unsafe` surface of the repository: a thin
//! counting shim over [`System`]. It is a dev-dependency only; no shipped
//! artifact links it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Allocations tallied on the current thread since the last reset.
    /// `const`-initialized, so the tallying path never allocates.
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

/// A [`GlobalAlloc`] that forwards every request to [`System`] and tallies
/// each allocation event (`alloc` / `realloc` / `alloc_zeroed`) on the calling
/// thread. Deallocations are not allocations; they are not tallied.
pub struct Counting;

impl Counting {
    fn tally() {
        COUNT.with(|count| count.set(count.get().wrapping_add(1)));
    }
}

// # Safety contract (whole impl)
//
// Every method forwards the call — with the exact layout, pointer and
// size arguments it received — to the corresponding `System` method, after
// bumping a thread-local counter. The forward is itself the safety
// obligation: `System` upholds `GlobalAlloc`'s contract (alignment,
// in-place realloc validity, zeroed contents), and the counter bump is
// exception-free (`Cell` arithmetic; the `const`-initialized thread-local
// never allocates on access). No pointer is dereferenced, stored or
// freed twice here.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::tally();
        // SAFETY: identical request forwarded to `System`, which upholds
        // the `GlobalAlloc` contract for layout and alignment.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was produced by this allocator's `alloc`/`realloc`
        // (i.e. by `System`) with this same `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::tally();
        // SAFETY: `ptr` is live, was allocated with `layout`, and
        // `new_size >= layout.size()` per the `GlobalAlloc` contract;
        // `System::realloc` handles grow/shrink in place or by move.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::tally();
        // SAFETY: identical request forwarded to `System`, which returns
        // memory zeroed up to `layout.size()`.
        unsafe { System.alloc_zeroed(layout) }
    }
}

/// Run `f` with this thread's counter zeroed, returning its result paired
/// with the number of allocation-events it performed.
///
/// Only the calling thread is measured: allocations by other threads (the
/// test harness, unrelated tests) go to their own tallies.
///
/// `f` must be deterministic in its allocation behavior for the gate that
/// consumes the count to be meaningful — which every pipeline in the A/B/C
/// suite is (fixed shapes, fixed sizes, no internal threading).
pub fn counted<T>(f: impl FnOnce() -> T) -> (T, u64) {
    COUNT.with(|count| count.set(0));
    let result = f();
    let allocations = COUNT.with(Cell::get);
    (result, allocations)
}

#[cfg(test)]
mod tests {
    use super::{Counting, counted};

    // The gate design leans on the detector working: a broken counter that
    // always reports zero would "prove" every pipeline allocation-free.
    // These tests run under `cargo test -p catheo-counting-alloc`, where
    // `Counting` is installed as the global allocator below.
    #[global_allocator]
    static COUNTING: Counting = Counting;

    #[test]
    fn counts_a_heap_allocation() {
        let (value, allocations) = counted(|| vec![7_u8; 64]);
        assert_eq!(value.len(), 64);
        assert_eq!(allocations, 1, "one exact-size Vec is one allocation");
    }

    #[test]
    fn counts_each_grow_of_a_pushing_vec() {
        let (_, allocations) = counted(|| {
            let mut xs: Vec<u8> = Vec::new();
            for x in 0..=255_u8 {
                xs.push(x);
            }
            xs
        });
        assert!(
            allocations >= 2,
            "amortized growth reallocates more than once, got {allocations}"
        );
    }

    #[test]
    fn zero_count_for_pure_stack_arithmetic() {
        let (sum, allocations) = counted(|| (0..1_000_i64).sum::<i64>());
        assert_eq!(sum, 499_500);
        assert_eq!(allocations, 0, "arithmetic allocates nothing");
    }
}
