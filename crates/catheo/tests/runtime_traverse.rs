//! Runtime kernels for the v0.3 combinator (§4.3): `traverse`.
//!
//! §4.3 defers `traverse` to v0.3 with the note "traversal fusion is
//! significantly more complex", and §7 limitation 4 names the two parts a
//! stable-Rust implementation must supply: "a different rewriting
//! discipline and a different kernel shape". This file pins the *kernel
//! shape* before any grammar work, in the house TDD order:
//!
//! - [`CategoricalTraverse::run_traverse`] per pointwise effect witness,
//!   against the hand-written eliminator;
//! - the traversal laws that justify the macro's rewriting discipline:
//!   identity over the pure arrow (`traverse(pure) == pure(sequence)`),
//!   naturality (`fmap g ∘ traverse f == traverse (fmap g ∘ f)`), and the
//!   empty-container law;
//! - the short-circuit contract the fused lowering leans on: the first
//!   `None`/`Err` stops the traversal, and later elements are never
//!   forced.
//!
//! These are unit gates, not benchmark-dependent: they run in every
//! `cargo test --workspace`.

use catheo::{CategoricalIntoIter, CategoricalTraverse, OptionWitness, ResultWitness, VecWitness};

/// The container side of every case below: traversal runs over the
/// container's owning iterator (§2.2 `CategoricalIntoIter`), which is how
/// the macro's generated code feeds the kernel.
fn traverse_option<T, U, F>(items: Vec<T>, f: F) -> Option<Vec<U>>
where
    F: FnMut(T) -> Option<U>,
{
    <OptionWitness as CategoricalTraverse>::run_traverse(
        <VecWitness as CategoricalIntoIter>::into_iter(items),
        f,
    )
}

fn traverse_result<T, U, E, F>(items: Vec<T>, f: F) -> Result<Vec<U>, E>
where
    F: FnMut(T) -> Result<U, E>,
{
    <ResultWitness<E> as CategoricalTraverse>::run_traverse(
        <VecWitness as CategoricalIntoIter>::into_iter(items),
        f,
    )
}

// ─── Kernel specs, against the hand-written eliminator ───────────────────

#[test]
fn option_run_traverse_collects_every_present_value() {
    let out = traverse_option(vec![-2_i32, -1, 0, 1, 2], |x: i32| Some(x.wrapping_mul(2)));
    assert_eq!(out, Some(vec![-4_i32, -2, 0, 2, 4]));
}

#[test]
fn result_run_traverse_collects_every_ok_value() {
    let out = traverse_result(vec![1_i32, 2, 3], |x: i32| Ok::<i32, GateError>(x * 3));
    assert_eq!(out, Ok(vec![3_i32, 6, 9]));
}

#[test]
fn traverse_preserves_element_order() {
    let out = traverse_option(vec![3_i32, 1, 2], Some);
    assert_eq!(out, Some(vec![3, 1, 2]), "traversal is order-preserving");
}

#[test]
fn traverse_of_an_empty_container_is_the_pure_empty_container() {
    let out = traverse_option(Vec::<i32>::new(), |x: i32| Some(x));
    assert_eq!(out, Some(vec![]), "the empty law: [] ↦ pure []");
    let out = traverse_result(Vec::<i32>::new(), |x: i32| Ok::<i32, GateError>(x));
    assert_eq!(out, Ok(vec![]), "the empty law: [] ↦ pure []");
}

// ─── The short-circuit contract ──────────────────────────────────────────

#[test]
fn option_traverse_short_circuits_at_the_first_none() {
    let calls = core::cell::Cell::new(0_usize);
    let out = traverse_option(vec![1_i32, -2, 3, -4], |x| {
        calls.set(calls.get() + 1);
        if x < 0 { None } else { Some(x) }
    });
    assert_eq!(out, None);
    assert_eq!(
        calls.get(),
        2,
        "elements after the first None are never forced"
    );
}

#[test]
fn result_traverse_short_circuits_at_the_first_err() {
    let calls = core::cell::Cell::new(0_usize);
    let out = traverse_result(vec![1_i32, 2, 3, 4], |x| {
        calls.set(calls.get() + 1);
        if x >= 2 {
            Err(GateError { skipped: 0 })
        } else {
            Ok(x)
        }
    });
    assert_eq!(out, Err(GateError { skipped: 0 }));
    assert_eq!(
        calls.get(),
        2,
        "elements after the first Err are never forced"
    );
}

#[test]
fn short_circuit_preserves_the_failing_element_error_value() {
    // The kernel carries the *original* Err out — no re-wrapping, no
    // identity change (the guard lowering depends on this).
    let out = traverse_result(vec![1_i32, 2], |x: i32| {
        if x == 2 {
            Err(GateError { skipped: 7 })
        } else {
            Ok(x)
        }
    });
    assert_eq!(out, Err(GateError { skipped: 7 }));
}

// ─── Traversal laws (the correctness argument for the rewrites) ──────────

/// Identity over the pure arrow: `traverse(pure) == pure(sequence)`.
/// Justifies absorbing a *prefix* run into the traversal chain: the pure
/// parts of a pipeline never change the effect, only its payload.
#[test]
fn traverse_over_the_pure_arrow_is_the_pure_sequence() {
    for items in [vec![], vec![1_i32], vec![1, 2, 3], vec![-5, 0, 5]] {
        let lhs = traverse_option(items.clone(), Some);
        let rhs = Some(items.clone());
        assert_eq!(lhs, rhs, "traverse(pure) == pure(sequence) over Option");

        let lhs = traverse_result(items.clone(), |x: i32| Ok::<i32, GateError>(x));
        let rhs = Ok(items);
        assert_eq!(lhs, rhs, "traverse(pure) == pure(sequence) over Result");
    }
}

/// Naturality: `fmap(g) ∘ traverse(f) == traverse(fmap(g) ∘ f)`. This is
/// the law that lets the codegen *compose* post-guard transforms with the
/// traversal payload and pre-chain maps with the traversal arrow without
/// changing the value.
#[test]
fn traverse_is_natural_in_the_arrow() {
    let g = |x: i64| x.to_string();
    for items in [vec![], vec![1_i32], vec![-7, 8, 9]] {
        // Option effect.
        let lhs = traverse_option(items.clone(), |x| Some(i64::from(x)))
            .map(|values| values.into_iter().map(g).collect::<Vec<String>>());
        let rhs = traverse_option(items.clone(), |x| Some(g(i64::from(x))));
        assert_eq!(lhs, rhs, "naturality over Option");

        // Result effect.
        let lhs = traverse_result(items.clone(), |x| Ok::<i64, GateError>(i64::from(x)))
            .map(|values| values.into_iter().map(g).collect::<Vec<String>>());
        let rhs = traverse_result(items.clone(), |x: i32| {
            Ok::<String, GateError>(g(i64::from(x)))
        });
        assert_eq!(lhs, rhs, "naturality over Result");
    }
}

/// The error type of the Result-effect cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateError {
    /// Payload proving the original `Err` survives the kernel unchanged.
    skipped: u8,
}
