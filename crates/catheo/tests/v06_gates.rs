//! The v0.6 gates — `filter_map`, the partial arrow, deterministically.
//!
//! ARCHITECTURE.md §4.3.3: the partial arrow `T -> Option<U>` on the
//! pipeline surface — std's `filter_map` on `Vec`, `and_then` on `Option`.
//! The gates pin the stage's contract the §6.1 way (deterministic, no
//! wall-clock flakiness):
//!
//! 1. the kernel *is* the idiomatic elimination (`run_filter_map` spells
//!    std's `filter_map` verbatim on `Vec`, `and_then` on `Option`);
//! 2. three-way value agreement over the xorshift sweep — the §4.2 bind
//!    riddle workaround (A), the idiomatic `filter_map` chain (B), the
//!    pipeline stage (C) — in lockstep with `benches/filter_map.rs`;
//! 3. the allocation profile that says the stage is free on the fused
//!    path: C allocates *exactly* like B (zero intermediates), and the
//!    riddle workaround (A) allocates per element;
//! 4. the §3.2 scan refinement: the partial arrow's constructors
//!    (`Some`/`None`) are the effect, not an observation — an inline
//!    `if … { Some(x) } else { None }` closure still fuses;
//! 5. the fused and unfused lowerings agree value-for-value (§6.2 oracle)
//!    for every pipeline shape this stage admits.

use catheo::{
    CategoricalFilterMap, CategoricalFunctor, OptionWitness, ResultWitness, VecWitness, pipeline,
    pipeline_unfused,
};
use catheo_counting_alloc::{Counting, counted};
use catheo_parser::{Lowering, emit_with_lowering, parse_pipeline};
use quote::quote;

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

// ─── The lockstep arrow (mirrored verbatim in benches/filter_map.rs) ──────

/// The partial arrow: keep the evens, widen and double.
fn partial_double(x: i32) -> Option<i64> {
    if x % 2 == 0 {
        Some(i64::from(x) * 2)
    } else {
        None
    }
}

/// A, the pre-v0.6 workaround: the partial arrow hand-rolled through the
/// `bind` kernel, re-collecting the pointed effect into the witness
/// container at every element — the riddle spelling the stage retires.
fn bind_riddle_workaround(input: Vec<i32>) -> i64 {
    let bound = <VecWitness as catheo::CategoricalMonad>::run_bind(input, |x| {
        partial_double(x).into_iter().collect::<Vec<i64>>()
    });
    bound.into_iter().sum()
}

/// B, the idiomatic spelling: one lazy `filter_map` pass.
fn filter_map_idiomatic(input: Vec<i32>) -> i64 {
    input.into_iter().filter_map(partial_double).sum()
}

/// C, the pipeline stage over a pure `fold` terminal — the Rule-4
/// absorbed lowering, one pass, zero intermediates.
fn filter_map_pipeline(input: Vec<i32>) -> i64 {
    pipeline! {
        witness: VecWitness,
        source: input,
        filter_map: partial_double,
        fold: (0_i64, i64::wrapping_add),
    }
}

/// C with the inline constructor spelling — the closure the §3.2 scan
/// refinement exists to keep fused.
fn filter_map_pipeline_inline(input: Vec<i32>) -> i64 {
    pipeline! {
        witness: VecWitness,
        source: input,
        filter_map: |x: i32| if x % 2 == 0 {
            Some(i64::from(x) * 2)
        } else {
            None
        },
        fold: (0_i64, i64::wrapping_add),
    }
}

// ─── 1. The kernel is the idiomatic elimination ───────────────────────────

#[test]
fn the_partial_arrow_kernel_is_the_std_filter_map_elimination() {
    let sweep = sweep_vecs();
    for m in &sweep {
        let by_kernel =
            <VecWitness as CategoricalFilterMap>::run_filter_map(m.clone(), partial_double);
        let by_std: Vec<i64> = m.iter().copied().filter_map(partial_double).collect();
        assert_eq!(by_kernel, by_std, "run_filter_map == std filter_map, {m:?}");

        // The pointwise container: `filter_map` on `Option` is `and_then`.
        let opt = m.first().copied();
        let by_kernel =
            <OptionWitness as CategoricalFilterMap>::run_filter_map(opt, partial_double);
        let by_std = opt.and_then(partial_double);
        assert_eq!(by_kernel, by_std, "run_filter_map == and_then, {opt:?}");
    }
}

#[test]
fn the_pointwise_partial_arrow_allocates_exactly_nothing() {
    for a in [0_i32, 7, -499] {
        let input = Some(a);
        let (value, count) = counted(|| {
            pipeline! {
                witness: OptionWitness,
                source: input,
                filter_map: partial_double,
            }
        });
        let expected = input.and_then(partial_double);
        assert_eq!(value, expected, "Option filter_map value, a = {a}");
        assert_eq!(
            count, 0,
            "the pointwise partial arrow touches no heap, a = {a}"
        );
    }
}

// ─── 2. Three-way value agreement over the sweep (bench lockstep) ────────

#[test]
fn the_partial_arrow_three_way_agreement_over_the_sweep() {
    for m in &sweep_vecs() {
        let a = bind_riddle_workaround(m.clone());
        let b = filter_map_idiomatic(m.clone());
        let c = filter_map_pipeline(m.clone());
        assert_eq!(a, b, "A == B, {m:?}");
        assert_eq!(b, c, "B == C, {m:?}");

        // The inline constructor spelling agrees too (the §3.2 posture).
        assert_eq!(
            c,
            filter_map_pipeline_inline(m.clone()),
            "inline == C, {m:?}"
        );
    }
}

// ─── 3. The allocation profile: the stage is free on the fused path ──────

#[test]
fn the_fused_partial_arrow_allocates_exactly_like_the_idiomatic_chain() {
    // Cases that keep at least one element: that is where the riddle
    // workaround's per-element materialization shows up at all.
    let cases: Vec<Vec<i32>> = sweep_vecs()
        .into_iter()
        .filter(|m| m.iter().any(|&x| x % 2 == 0))
        .collect();
    let mut pipeline_total: u64 = 0;
    let mut idiomatic_total: u64 = 0;
    let mut riddle_total: u64 = 0;
    for m in &cases {
        let input_c = m.clone();
        let input_b = m.clone();
        let input_a = m.clone();
        let (c_value, c_count) = counted(|| filter_map_pipeline(input_c));
        let (b_value, b_count) = counted(|| filter_map_idiomatic(input_b));
        let (a_value, a_count) = counted(|| bind_riddle_workaround(input_a));
        assert_eq!(c_value, b_value, "C == B value, {m:?}");
        assert_eq!(a_value, b_value, "A == B value, {m:?}");
        assert_eq!(
            c_count, b_count,
            "the fused partial arrow allocates exactly like the idiomatic \
             `filter_map` chain — zero intermediates (C = {c_count}, \
             B = {b_count}, {m:?})"
        );
        assert!(
            a_count > c_count,
            "the riddle workaround materializes a container per element \
             (A = {a_count}, C = {c_count}, {m:?}) — the stage retires it"
        );
        pipeline_total += c_count;
        idiomatic_total += b_count;
        riddle_total += a_count;
    }
    assert_eq!(pipeline_total, idiomatic_total);
    assert!(
        riddle_total > pipeline_total,
        "the sweep exercises real allocations"
    );
}

#[test]
fn a_map_prefix_fuses_into_the_partial_arrow_allocating_nothing_extra() {
    // Rule 2 across the partial arrow: `map(f) → filter_map(g)` rides one
    // adapter chain — the fusion `filter` (an absolute barrier) can never
    // offer. The allocation profile pins it: exactly B's, zero.
    for m in &sweep_vecs() {
        let input = m.clone();
        let (c_value, c_count) = counted(|| {
            pipeline! {
                witness: VecWitness,
                source: input,
                map: |x: i32| x.wrapping_mul(2),
                filter_map: partial_double,
                fold: (0_i64, i64::wrapping_add),
            }
        });
        let (b_value, b_count) = counted(|| {
            m.iter()
                .copied()
                .map(|x| x.wrapping_mul(2))
                .filter_map(partial_double)
                .sum::<i64>()
        });
        let expected: i64 = m
            .iter()
            .copied()
            .map(|x| x.wrapping_mul(2))
            .filter_map(partial_double)
            .sum();
        assert_eq!(c_value, expected, "map → filter_map value, {m:?}");
        assert_eq!(c_value, b_value, "C == B value, {m:?}");
        assert_eq!(
            c_count, b_count,
            "the fused map → filter_map chain allocates exactly like the \
             idiomatic chain (C = {c_count}, B = {b_count}, {m:?})"
        );
    }
}

// ─── 4. The §3.2 scan refinement: constructors are the effect ────────────

#[test]
fn the_inline_partial_closure_still_fuses() {
    // The inline spelling mentions `Some`/`None` — the partial arrow's
    // constructors, not container-shape observation. The fused emission
    // must stay the one-pass adapter chain: no kernel call, no state
    // register, no intermediate container.
    let tokens = quote! {
        witness: VecWitness,
        source: input,
        filter_map: |x: i32| if x % 2 == 0 { Some(i64::from(x) * 2) } else { None },
        fold: (0_i64, i64::wrapping_add),
    };
    let parsed = parse_pipeline(tokens).expect("the inline partial pipeline parses");
    let fused = emit_with_lowering(&parsed, Lowering::Fused)
        .expect("emission succeeds")
        .to_string();
    assert!(
        fused.contains("filter_map"),
        "the partial arrow rides the adapter chain: {fused}"
    );
    assert!(
        !fused.contains("run_filter_map") && !fused.contains("run_bind"),
        "no kernel survives Rule 4 absorption: {fused}"
    );
    assert!(
        !fused.contains("__catheo_state"),
        "nothing materializes between source and fold: {fused}"
    );
}

#[test]
fn an_observing_partial_closure_stays_a_barrier() {
    // The refined scan still catches the shape observers: `.len`/`.iter`/
    // `.is_empty` in a partial arrow block the fusion exactly like a bind.
    let tokens = quote! {
        witness: VecWitness,
        source: input,
        filter_map: |x: i32| if x.iter().count() > 0 { Some(x) } else { None },
        collect,
    };
    let parsed = parse_pipeline(tokens).expect("the observing partial pipeline parses");
    let fused = emit_with_lowering(&parsed, Lowering::Fused)
        .expect("emission succeeds")
        .to_string();
    assert!(
        fused.contains("run_filter_map"),
        "the observing partial arrow stays at the kernel shape: {fused}"
    );
}

// ─── 5. The §6.2 oracle: fused == unfused for every admitted shape ────────

#[test]
fn the_partial_arrow_survives_the_unfused_oracle() {
    let sweep = sweep_vecs();
    for m in &sweep {
        let fused = pipeline! {
            witness: VecWitness,
            source: m.clone(),
            map: |x: i32| x.wrapping_mul(2),
            filter_map: partial_double,
            fold: (0_i64, i64::wrapping_add),
        };
        let sequential = pipeline_unfused! {
            witness: VecWitness,
            source: m.clone(),
            map: |x: i32| x.wrapping_mul(2),
            filter_map: partial_double,
            fold: (0_i64, i64::wrapping_add),
        };
        assert_eq!(fused, sequential, "fused == unfused, {m:?}");

        // The `collect` terminal keeps the container.
        let fused: Vec<i64> = pipeline! {
            witness: VecWitness,
            source: m.clone(),
            filter_map: partial_double,
            collect,
        };
        let sequential: Vec<i64> = pipeline_unfused! {
            witness: VecWitness,
            source: m.clone(),
            filter_map: partial_double,
            collect,
        };
        assert_eq!(fused, sequential, "fused == unfused, collect, {m:?}");
        let expected: Vec<i64> = m.iter().copied().filter_map(partial_double).collect();
        assert_eq!(fused, expected, "collect == std filter_map, {m:?}");

        // The pointwise witness: `and_then` parity through the macro.
        let opt = m.first().copied();
        let fused: Option<i64> = pipeline! {
            witness: OptionWitness,
            source: opt,
            filter_map: partial_double,
        };
        assert_eq!(
            fused,
            opt.and_then(partial_double),
            "Option == and_then, {m:?}"
        );
    }
}

// ─── 6. The `ResultWitness` boundary is type-level, spanned ──────────────

#[test]
fn the_result_witness_rejection_stays_the_documented_posture() {
    // No `CategoricalFilterMap` impl for `ResultWitness<E>` — dropping an
    // `Ok` has the same error-free-denotation problem the `filter` kernel
    // documents. The rejection is the E0277 at the witness token, pinned
    // by the trybuild case `error_filter_map_on_result_witness.rs`; here
    // the same shape is exercised value-level: the kernel set covers every
    // witness *but* the one the docs name.
    fn assert_kernel_implementations<W: CategoricalFilterMap>(input: W::Type<i32>) -> W::Type<i64> {
        <W as CategoricalFilterMap>::run_filter_map(input, partial_double)
    }
    assert_eq!(
        assert_kernel_implementations::<VecWitness>(vec![2_i32, 3]),
        vec![4_i64]
    );
    assert_eq!(
        assert_kernel_implementations::<OptionWitness>(Some(2_i32)),
        Some(4_i64)
    );
    // `ResultWitness<String>` has no impl — the bound would not hold; the
    // UI case pins the spanned rejection. This line only records the
    // posture: the two implemented witnesses are the whole surface.
    let _ = ResultWitness::<String>::default();
    let _ = <VecWitness as CategoricalFunctor>::run_map(vec![1_i32], |x| x + 1);
}
