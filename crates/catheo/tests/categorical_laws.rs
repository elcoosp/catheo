//! The categorical laws, verified against the runtime kernels (TDD: written
//! first).
//!
//! ARCHITECTURE.md §3.3 makes the weakly-cartesian monad laws the
//! *correctness argument* for the M2 deforestation rewrites; this suite is
//! where that argument is pinned down at the kernel level, per witness, on
//! stable Rust (the M1 gate).
//!
//! The inputs are swept deterministically (edge shapes plus an xorshift64*
//! pseudo-random sweep) so the suite stays dependency-free; the full
//! randomized property harness lands with M2's fusion tests (§6.2).

use catheo::{
    CategoricalFold, CategoricalFunctor, CategoricalIntoIter, CategoricalMonad, CategoricalPointed,
    OptionWitness, ResultWitness, VecWitness,
};

#[derive(Debug, PartialEq)]
struct ParseError {
    at: usize,
}

/// Deterministic xorshift64* sweep: reproducible "random" inputs without a
/// third-party property-testing dependency at M1.
struct XorShift(u64);

impl XorShift {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_i32(&mut self) -> i32 {
        i32::try_from(self.next_u64() % 2_000).unwrap_or(0) - 1_000
    }

    fn next_vec(&mut self, max_len: u64) -> Vec<i32> {
        let len = self.next_u64() % (max_len + 1);
        (0..len).map(|_| self.next_i32()).collect()
    }
}

const XORSHIFT_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// The deterministic input sweep: hand-picked edge shapes first, then
/// pseudo-random vectors of assorted lengths.
fn sweep_vecs() -> Vec<Vec<i32>> {
    let mut rng = XorShift(XORSHIFT_SEED);
    let mut cases = vec![
        Vec::new(),
        vec![0],
        vec![1],
        vec![-1],
        vec![1, 2, 3],
        vec![3, 2, 1],
        vec![7, 7, 7, 7],
    ];
    for _ in 0..24 {
        cases.push(rng.next_vec(8));
    }
    cases
}

/// Representative single elements for the `Option`/`Result` domains: the head
/// of every swept vector, plus a few fixed probes.
fn sweep_elems(cases: &[Vec<i32>]) -> Vec<i32> {
    cases
        .iter()
        .filter_map(|v| v.first().copied())
        .chain([-7, 0, 13])
        .collect()
}

// ─── Functor laws ───────────────────────────────────────────────────────────

/// Law 1 (identity): `map(id) == id`.
#[test]
fn functor_identity_holds_for_every_witness() {
    for case in sweep_vecs() {
        let mapped = <VecWitness as CategoricalFunctor>::run_map(case.clone(), |x| x);
        assert_eq!(mapped, case);
    }
    for &e in &sweep_elems(&sweep_vecs()) {
        let mapped = <OptionWitness as CategoricalFunctor>::run_map(Some(e), |x| x);
        assert_eq!(mapped, Some(e));
        let mapped: Result<i32, ParseError> =
            <ResultWitness<ParseError> as CategoricalFunctor>::run_map(Ok(e), |x| x);
        assert_eq!(mapped, Ok(e));
    }
}

/// Law 2 (composition): `map(g) ∘ map(f) == map(g ∘ f)`. This is exactly
/// deforestation Rule 1, so its kernel-level truth is what licenses the M2
/// rewrite.
#[test]
fn functor_composition_holds_for_every_witness() {
    let f = |x: i32| x.wrapping_mul(3).wrapping_add(1);
    let g = |x: i32| x.wrapping_sub(2);
    let composed = |x: i32| g(f(x));

    for case in sweep_vecs() {
        let lhs = <VecWitness as CategoricalFunctor>::run_map(
            <VecWitness as CategoricalFunctor>::run_map(case.clone(), f),
            g,
        );
        let rhs = <VecWitness as CategoricalFunctor>::run_map(case, composed);
        assert_eq!(lhs, rhs);
    }
    for &e in &sweep_elems(&sweep_vecs()) {
        let lhs = <OptionWitness as CategoricalFunctor>::run_map(
            <OptionWitness as CategoricalFunctor>::run_map(Some(e), f),
            g,
        );
        let rhs = <OptionWitness as CategoricalFunctor>::run_map(Some(e), composed);
        assert_eq!(lhs, rhs);

        let lhs = <ResultWitness<ParseError> as CategoricalFunctor>::run_map(
            <ResultWitness<ParseError> as CategoricalFunctor>::run_map(Ok(e), f),
            g,
        );
        let rhs = <ResultWitness<ParseError> as CategoricalFunctor>::run_map(Ok(e), composed);
        assert_eq!(lhs, rhs);
    }
}

// ─── Monad laws ─────────────────────────────────────────────────────────────

/// Law 3 (left identity): `bind(pure(a), k) == k(a)`.
#[test]
fn monad_left_identity_holds_for_every_witness() {
    let ks: [fn(i32) -> Vec<i32>; 4] = [
        |a| vec![a],
        |a| vec![a, a],
        |_a| vec![],
        |a| vec![a, -a, a * 2],
    ];
    for &a in &sweep_elems(&sweep_vecs()) {
        for k in &ks {
            let lhs = <VecWitness as CategoricalMonad>::run_bind(
                <VecWitness as CategoricalPointed>::run_pure(a),
                *k,
            );
            assert_eq!(lhs, k(a));
        }
    }
    for &a in &sweep_elems(&sweep_vecs()) {
        let k = |a: i32| if a % 2 == 0 { Some(a / 2) } else { None };
        let lhs = <OptionWitness as CategoricalMonad>::run_bind(
            <OptionWitness as CategoricalPointed>::run_pure(a),
            k,
        );
        assert_eq!(lhs, k(a));

        let k = |a: i32| {
            if a >= 0 {
                Ok::<i32, ParseError>(a)
            } else {
                Err(ParseError {
                    at: usize::try_from(a.unsigned_abs()).unwrap_or(0),
                })
            }
        };
        let lhs = <ResultWitness<ParseError> as CategoricalMonad>::run_bind(
            <ResultWitness<ParseError> as CategoricalPointed>::run_pure(a),
            k,
        );
        assert_eq!(lhs, k(a));
    }
}

/// Law 4 (right identity): `bind(m, pure) == m`.
#[test]
fn monad_right_identity_holds_for_every_witness() {
    for case in sweep_vecs() {
        let lhs = <VecWitness as CategoricalMonad>::run_bind(
            case.clone(),
            <VecWitness as CategoricalPointed>::run_pure,
        );
        assert_eq!(lhs, case);
    }
    for &e in &sweep_elems(&sweep_vecs()) {
        let lhs = <OptionWitness as CategoricalMonad>::run_bind(
            Some(e),
            <OptionWitness as CategoricalPointed>::run_pure,
        );
        assert_eq!(lhs, Some(e));

        let lhs = <ResultWitness<ParseError> as CategoricalMonad>::run_bind(
            Ok(e),
            <ResultWitness<ParseError> as CategoricalPointed>::run_pure,
        );
        assert_eq!(lhs, Ok::<i32, ParseError>(e));
    }
}

/// Law 5 (associativity): `bind(bind(m, f), g) == bind(m, |x| bind(f(x), g))`.
#[test]
fn monad_associativity_holds_for_every_witness() {
    let f = |a: i32| vec![a, a.wrapping_add(1)];
    let g = |a: i32| if a == 0 { vec![] } else { vec![a; 2] };

    for case in sweep_vecs() {
        let lhs = <VecWitness as CategoricalMonad>::run_bind(
            <VecWitness as CategoricalMonad>::run_bind(case.clone(), f),
            g,
        );
        // The right-hand side is spelled as the M2 macro will desugar a
        // two-morphism chain: one bind whose closure runs the next arrow.
        let rhs = <VecWitness as CategoricalMonad>::run_bind(case, move |x| {
            <VecWitness as CategoricalMonad>::run_bind(f(x), g)
        });
        assert_eq!(lhs, rhs);
    }

    let f_opt = |a: i32| if a % 3 == 0 { None } else { Some(a * 2) };
    let g_opt = |a: i32| if a > 100 { None } else { Some(a + 1) };
    for &e in &sweep_elems(&sweep_vecs()) {
        let lhs = <OptionWitness as CategoricalMonad>::run_bind(
            <OptionWitness as CategoricalMonad>::run_bind(Some(e), f_opt),
            g_opt,
        );
        let rhs = <OptionWitness as CategoricalMonad>::run_bind(Some(e), move |x| {
            <OptionWitness as CategoricalMonad>::run_bind(f_opt(x), g_opt)
        });
        assert_eq!(lhs, rhs);

        let f_res = |a: i32| -> Result<i32, ParseError> {
            if a == i32::MIN {
                Err(ParseError { at: 0 })
            } else {
                Ok(a.wrapping_mul(2))
            }
        };
        let g_res = |a: i32| -> Result<i32, ParseError> {
            if a > 500 {
                Err(ParseError { at: 1 })
            } else {
                Ok(a + 1)
            }
        };
        let lhs = <ResultWitness<ParseError> as CategoricalMonad>::run_bind(
            <ResultWitness<ParseError> as CategoricalMonad>::run_bind(Ok(e), f_res),
            g_res,
        );
        let rhs = <ResultWitness<ParseError> as CategoricalMonad>::run_bind(Ok(e), move |x| {
            <ResultWitness<ParseError> as CategoricalMonad>::run_bind(f_res(x), g_res)
        });
        assert_eq!(lhs, rhs);
    }
}

// ─── join law ───────────────────────────────────────────────────────────────

/// `join(map(m, pure)) == m`: a mapped layer of pures is exactly `m` squashed.
#[test]
fn join_of_map_pure_is_the_identity() {
    for case in sweep_vecs() {
        let lhs = <VecWitness as CategoricalMonad>::run_join(
            <VecWitness as CategoricalFunctor>::run_map(
                case.clone(),
                <VecWitness as CategoricalPointed>::run_pure,
            ),
        );
        assert_eq!(lhs, case);
    }
    for &e in &sweep_elems(&sweep_vecs()) {
        let lhs = <OptionWitness as CategoricalMonad>::run_join(Some(Some(e)));
        assert_eq!(lhs, Some(e));
        let lhs = <ResultWitness<ParseError> as CategoricalMonad>::run_join(Ok(Ok(e)));
        assert_eq!(lhs, Ok::<i32, ParseError>(e));
    }
}

// ─── Kernel equivalence ─────────────────────────────────────────────────────

/// `run_fold` must agree with a fold driven through `CategoricalIntoIter`:
/// the eliminator and the iterator lowering observe the same container.
#[test]
fn fold_kernel_agrees_with_into_iter_fold() {
    for case in sweep_vecs() {
        let via_kernel =
            <VecWitness as CategoricalFold>::run_fold(case.clone(), 0_i64, |acc, x| {
                acc + i64::from(x)
            });
        let via_iter = <VecWitness as CategoricalIntoIter>::into_iter(case)
            .fold(0_i64, |acc, x| acc + i64::from(x));
        assert_eq!(via_kernel, via_iter);
    }
}

/// `HKT` projections stay coherent under the sweep: the witness round-trips
/// through `Type<T>` without changing element counts.
#[test]
fn hkt_projection_preserves_element_count() {
    fn count<W, T>(container: W::Type<T>) -> usize
    where
        W: CategoricalIntoIter,
    {
        <W as CategoricalIntoIter>::into_iter(container).count()
    }
    for case in sweep_vecs() {
        assert_eq!(count::<VecWitness, i32>(case.clone()), case.len());
    }
}
