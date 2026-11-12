//! The strict `Filterable` layer gates — the partial-arrow-side
//! unification, deterministically.
//!
//! The §2.4 posture extended to the partial-arrow side: one trait —
//! `filter_map` required, `filter` provided — over the witnesses that
//! admit a partial arrow (`VecWitness`, `OptionWitness`;
//! `ResultWitness<E>` is excluded by the §4.3.3 posture — a dropped `Ok`
//! has no error-free denotation). Each kernel delegates to the split
//! layer (§2.2), which remains the single lowering. The provided
//! `filter` is the derivation: `filter_map(m, |x| if p(&x) { Some(x) }
//! else { None })` — the filter-side twin of the v0.5 `fmap == bind ∘
//! pure` pin. The gates pin the layer's contract the §6.1 way
//! (deterministic, no wall-clock flakiness):
//!
//! 1. the required `filter_map` and the derived `filter` pinned against
//!    the split kernels (`Filterable::filter_map == run_filter_map`,
//!    `Filterable::filter == run_filter`) per witness over the xorshift
//!    sweep (the house harness);
//! 2. the filter laws through the trait: the trivial predicates
//!    (`always-true` is the identity, `always-false` the empty
//!    denotation), idempotence (`filter(p)` twice is once), and
//!    composition (`filter(p && q) == filter(p) ∘ filter(q)`);
//! 3. three-way value agreement over the sweep, in lockstep with
//!    `benches/filterable.rs`;
//! 4. the allocation profile that says the abstraction is free: the
//!    strict spelling allocates *exactly* like the split-kernel spelling
//!    and exactly like the idiomatic one-pass chain;
//! 5. the consumer seam: one generic program bounds on `Filterable`
//!    alone and instantiates across both admitting witnesses.

use catheo::{
    CategoricalFilter, CategoricalFilterMap, CategoricalIntoIter, Filterable, OptionWitness,
    VecWitness,
};
use catheo_counting_alloc::{Counting, counted};

#[global_allocator]
static COUNTING: Counting = Counting;

// ─── The xorshift sweep (house harness, seed shared with the law gates) ───

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
        // A value in [-500, 500): representative, wrap-free for the
        // multipliers below, and distinct across consecutive draws.
        let raw = i64::from((self.next_u64() >> 33) as u32) % 1_000 - 500;
        i32::try_from(raw).unwrap_or(0)
    }
}

const XORSHIFT_SEED: u64 = 0x9E37_79B9_7F4A_7C15;

fn sweep_vecs() -> Vec<Vec<i32>> {
    let mut rng = XorShift(XORSHIFT_SEED);
    let mut cases: Vec<Vec<i32>> = vec![
        vec![],
        vec![0],
        vec![1, 2],
        vec![-3, 0, 7],
        vec![499, -499, 1],
    ];
    for _ in 0..12 {
        let len = (rng.next_u64() % 6) as usize;
        cases.push((0..len).map(|_| rng.next_i32()).collect());
    }
    cases
}

// ─── The lockstep programs (mirrored verbatim in benches/filterable.rs) ───

/// The partial arrow of the universes (lockstep with the bench): keep the
/// evens, halve them.
fn keep_half(x: i32) -> Option<i64> {
    if x % 2 == 0 {
        Some(i64::from(x) / 2)
    } else {
        None
    }
}

/// C, the strict spelling: the partial arrow composed over ONE
/// `Filterable` bound, eliminated through the split into-iter kernel —
/// the same composition shape as the v05 gates' strict-then-fold program.
/// Lockstep with the bench's `vec` universe.
fn kept_total<W: Filterable + CategoricalIntoIter>(xs: W::Type<i32>) -> i64 {
    W::into_iter(W::filter_map(xs, keep_half)).sum()
}

/// The parity twin: the same program spelled over the split kernels
/// (`run_filter_map` + `run_fold`). The allocation gate pins strict ==
/// split to the allocation.
fn kept_total_kernel(xs: Vec<i32>) -> i64 {
    let kept: Vec<i64> = <VecWitness as CategoricalFilterMap>::run_filter_map(xs, keep_half);
    <VecWitness as catheo::CategoricalFold>::run_fold(kept, 0_i64, |acc, x| acc + x)
}

/// A: the two-barrier spelling — filter into one materialized container,
/// map into a second, then the fold.
fn kept_total_naive(xs: Vec<i32>) -> i64 {
    let evens: Vec<i32> = xs.into_iter().filter(|x| x % 2 == 0).collect();
    let halves: Vec<i64> = evens.into_iter().map(i64::from).map(|x| x / 2).collect();
    halves.into_iter().sum()
}

/// B: one lazy iterator pass, no intermediate container.
fn kept_total_idiomatic(xs: Vec<i32>) -> i64 {
    xs.into_iter().filter_map(keep_half).sum()
}

/// C, the derived `filter`: the predicate eliminator over one bound.
fn evens<W: Filterable>(xs: W::Type<i32>) -> W::Type<i32> {
    W::filter(xs, |x: &i32| x % 2 == 0)
}

// ─── 1. The kernels pinned against the split layer ────────────────────────

#[test]
fn the_required_filter_map_agrees_with_the_split_kernel() {
    let sweep = sweep_vecs();
    for m in &sweep {
        assert_eq!(
            <VecWitness as Filterable>::filter_map(m.clone(), keep_half),
            <VecWitness as CategoricalFilterMap>::run_filter_map(m.clone(), keep_half),
            "Filterable::filter_map == run_filter_map, Vec, {m:?}"
        );
        let opt = m.first().copied();
        assert_eq!(
            <OptionWitness as Filterable>::filter_map(opt, keep_half),
            <OptionWitness as CategoricalFilterMap>::run_filter_map(opt, keep_half),
            "Filterable::filter_map == run_filter_map, Option of {m:?}"
        );
    }
}

#[test]
fn the_derived_filter_agrees_with_the_split_kernel_and_the_desugar() {
    let sweep = sweep_vecs();
    for m in &sweep {
        // The derivation pin: `filter(p)` is the partial arrow that keeps
        // or drops — pinned against the split `run_filter` kernel ...
        assert_eq!(
            evens::<VecWitness>(m.clone()),
            <VecWitness as CategoricalFilter>::run_filter(m.clone(), |x: &i32| x % 2 == 0),
            "Filterable::filter == run_filter, Vec, {m:?}"
        );
        // ... and against the desugar itself: the provided body spelled
        // through the required kernel.
        assert_eq!(
            evens::<VecWitness>(m.clone()),
            <VecWitness as Filterable>::filter_map(m.clone(), |x: i32| if x % 2 == 0 {
                Some(x)
            } else {
                None
            }),
            "filter == filter_map desugar, Vec, {m:?}"
        );
        let opt = m.first().copied();
        assert_eq!(
            evens::<OptionWitness>(opt),
            <OptionWitness as CategoricalFilter>::run_filter(opt, |x: &i32| x % 2 == 0),
            "Filterable::filter == run_filter, Option of {m:?}"
        );
    }
}

// ─── 2. The filter laws through the trait ─────────────────────────────────

#[test]
fn the_strict_filterable_laws_hold_over_the_sweep() {
    let sweep = sweep_vecs();
    let always_true = |_: &i32| true;
    let always_false = |_: &i32| false;

    for m in &sweep {
        // Identity: `filter(always-true) == m` — the trivial predicate
        // keeps every element in place.
        assert_eq!(
            <VecWitness as Filterable>::filter(m.clone(), always_true),
            m.clone(),
            "filter identity, Vec, {m:?}"
        );
        // The empty denotation: `filter(always-false)` yields the
        // witness's zero-element shape.
        assert!(
            <VecWitness as Filterable>::filter(m.clone(), always_false).is_empty(),
            "filter always-false is empty, Vec, {m:?}"
        );

        // Idempotence: `filter(p)` twice is once.
        let once = evens::<VecWitness>(m.clone());
        let twice = evens::<VecWitness>(once.clone());
        assert_eq!(once, twice, "filter idempotence, Vec, {m:?}");

        // Composition: `filter(p && q) == filter(p) ∘ filter(q)` — the
        // law that makes chains of predicates composable without
        // re-traversal semantics.
        let p = |x: &i32| x % 2 == 0;
        let q = |x: &i32| *x >= 0;
        let lhs = <VecWitness as Filterable>::filter(m.clone(), |x: &i32| p(x) && q(x));
        let rhs =
            <VecWitness as Filterable>::filter(<VecWitness as Filterable>::filter(m.clone(), p), q);
        assert_eq!(lhs, rhs, "filter composition, Vec, {m:?}");

        // The pointwise shapes carry the same laws (one discriminant).
        let opt = m.first().copied();
        assert_eq!(
            <OptionWitness as Filterable>::filter(opt, always_true),
            opt,
            "filter identity, Option of {m:?}"
        );
        assert_eq!(
            <OptionWitness as Filterable>::filter(opt, always_false),
            None,
            "filter always-false is None, Option of {m:?}"
        );
    }
}

// ─── 3. Three-way value agreement over the sweep (bench lockstep) ────────

#[test]
fn the_strict_filterable_three_way_agreement_over_the_sweep() {
    for m in &sweep_vecs() {
        let a = kept_total_naive(m.clone());
        let b = kept_total_idiomatic(m.clone());
        let c = kept_total::<VecWitness>(m.clone());
        assert_eq!(a, b, "A == B, {m:?}");
        assert_eq!(b, c, "B == C, {m:?}");
    }
}

// ─── 4. The allocation profile: the abstraction is free ──────────────────

#[test]
fn the_strict_filterable_allocates_exactly_like_the_split_kernels() {
    let cases: Vec<Vec<i32>> = sweep_vecs().into_iter().filter(|m| !m.is_empty()).collect();
    let mut strict_total: u64 = 0;
    let mut kernel_total: u64 = 0;
    let mut idiomatic_total: u64 = 0;
    for m in &cases {
        let (c_value, c_count) = counted(|| kept_total::<VecWitness>(m.clone()));
        let (k_value, k_count) = counted(|| kept_total_kernel(m.clone()));
        let (b_value, b_count) = counted(|| kept_total_idiomatic(m.clone()));
        let (_, a_count) = counted(|| kept_total_naive(m.clone()));
        assert_eq!(c_value, k_value, "strict == split-kernel value, {m:?}");
        assert_eq!(c_value, b_value, "strict == idiomatic value, {m:?}");
        assert_eq!(
            c_count, k_count,
            "C allocates exactly like the split-kernel spelling — the one-trait \
             bound adds nothing over the kernels (strict = {c_count}, \
             kernels = {k_count}, {m:?})"
        );
        // The eager-semantics honesty gate (the §2.3 posture, as in the
        // v05 gates for the runtime bind): the split kernel *returns* its
        // container, so the strict spelling pays one allocation for the
        // kept set whenever it is non-empty — the idiomatic chain
        // eliminates lazily and pays none. That gap is precisely the gap
        // the macro's fusion pass exists to close (the v0.6 gates pin the
        // fused C == B profile); the runtime layer documents its floor.
        assert!(
            c_count >= b_count,
            "the eager kernel collects its kept container (C = {c_count}, \
             B = {b_count}, {m:?})"
        );
        assert!(
            a_count >= c_count,
            "the two-barrier spelling materializes a superset of the strict \
             spelling's containers (A = {a_count}, C = {c_count}, {m:?})"
        );
        strict_total += c_count;
        kernel_total += k_count;
        idiomatic_total += b_count;
    }
    assert!(strict_total > 0, "the sweep exercises real allocations");
    assert_eq!(strict_total, kernel_total);
    assert!(strict_total >= idiomatic_total);
}

#[test]
fn the_pointwise_filterable_allocates_exactly_like_the_idiomatic_chain() {
    let cases: Vec<Option<i32>> = sweep_vecs()
        .into_iter()
        .map(|m| m.first().copied())
        .collect();
    for input in &cases {
        let (c_value, c_count) =
            counted(|| <OptionWitness as Filterable>::filter_map(*input, keep_half));
        let (b_value, b_count) = counted(|| input.and_then(keep_half));
        assert_eq!(c_value, b_value, "Option: strict == idiomatic, {input:?}");
        assert_eq!(
            c_count, b_count,
            "the pointwise partial arrow allocates exactly like the idiomatic \
             `and_then` — one discriminant, no heap (C = {c_count}, \
             B = {b_count}, {input:?})"
        );
    }
}

// ─── 5. The consumer seam: one bound, both admitting witnesses ───────────

#[test]
fn a_generic_program_bounds_on_filterable_alone_across_both_witnesses() {
    // The same generic body, instantiated per witness — the consumer seam
    // the layer exists for (the §4.4 posture, partial-arrow side).
    fn kept_strict<W: Filterable>(xs: W::Type<i32>) -> W::Type<i64> {
        W::filter_map(xs, keep_half)
    }

    assert_eq!(
        kept_strict::<VecWitness>(vec![2_i32, 3, 8, 7]),
        vec![1_i64, 4],
        "generic over Vec: the evens halved, the odds dropped"
    );
    assert_eq!(
        kept_strict::<OptionWitness>(Some(8_i32)),
        Some(4_i64),
        "generic over Option: and_then on the kept arm"
    );
    assert_eq!(
        kept_strict::<OptionWitness>(None::<i32>),
        None,
        "None forwards unchanged"
    );

    // The derived `filter` rides the same bound.
    assert_eq!(
        evens::<VecWitness>(vec![1_i32, 2, 3, 4]),
        vec![2, 4],
        "the derived filter through the one bound, Vec"
    );
    assert_eq!(evens::<OptionWitness>(Some(3_i32)), None, "Option, dropped");
    assert_eq!(evens::<OptionWitness>(Some(4_i32)), Some(4), "Option, kept");
}
