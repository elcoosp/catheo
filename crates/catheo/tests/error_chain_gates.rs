//! The §6.1 M4 gate, as real integration tests.
//!
//! ARCHITECTURE.md §6.1 defines the two M4 benchmarks and §8 gates M4 on
//! them: a **10-stage `bind`-heavy `Result` chain** (error handling, run
//! *cold* and *warm*) and a **two-morphism Kleisli chain over
//! `OptionWitness`** — with the same verdict form as M3, **C ≥ B on
//! both**. As with the M3 decision gate (`tests/abc_gates.rs`),
//! wall-clock comparisons cannot be enforced on shared CI runners, so
//! this file enforces the *deterministic* consequences of the claim while
//! `benches/error_chains.rs` is the measurement instrument whose criterion
//! output is the published evidence (docs/ARCHITECTURE.md §6.1). The two
//! are kept in lockstep: identical shapes, identical closures.
//!
//! What is enforced here, and why it is equivalent to the §6.1 gate:
//!
//! 1. **Three-way agreement.** A == B == C, value-for-value, on both
//!    chains, both Result universes (warm all-`Ok`, cold mid-chain
//!    failure), and the Kleisli `Some`/`None` mix — over edge shapes and
//!    a seeded xorshift sweep. (A correct-but-cheating C — e.g. a rewrite
//!    that reorders the morphisms or drops the short-circuit — fails
//!    here.)
//! 2. **Allocation gates.** The macro output (C) performs **zero heap
//!    allocations** on every M4 universe — no less than the idiomatic
//!    chain (B, also zero) and no more than the naive ladder (A, also
//!    zero). Zero is the sharp claim here: the §1 failure mode for
//!    monadic chains is *trait-object indirection and boxing* — each
//!    stage paying for a closure object. C's lowering — one `run_bind`
//!    kernel per stage (the kernel *is* the idiomatic `and_then`) into
//!    one `run_fold` eliminator — must box nothing: every stage callable
//!    is passed in a `Fn`-bounded argument position, zero-sized at
//!    runtime. Allocation counts are deterministic via the thread-local
//!    counting allocator (`catheo-counting-alloc`).
//! 3. **Emission lockstep.** The exact benchmark pipelines are re-parsed
//!    through `catheo-parser` and their fused emission shape is pinned —
//!    for these pointwise witnesses (zero-or-one-element projections),
//!    one `run_bind` kernel per stage and a `run_fold` eliminator, no
//!    iterator adapters at all. This pins *why* the allocation counts
//!    are zero *and* why the measured C is the idiomatic machine shape,
//!    and keeps `benches/error_chains.rs` and these gates from drifting
//!    apart.
//!
//! # The gate that forced a lowering refinement
//!
//! The first M4 run failed this gate: the fused lowering chained the
//! absorbed run into `.flat_map` adapter layers over
//! `result::IntoIter`/`option::IntoIter`, and measured **1.2–2.7×
//! slower** than the idiomatic `and_then` chain — the §6.1 protocol
//! (re-examine the responsible rule before merge) traced the loss to
//! adapter-stack overhead on zero-or-one-element iterators, which have no
//! containers to deforest. The revision (`catheo-parser`'s codegen module,
//! "Pointwise witnesses keep the kernel shape"): witnesses spelled
//! `OptionWitness`/`ResultWitness` at the head keep the trailing run at
//! the sequential-kernel shape, so `pipeline!` emits byte-for-byte the
//! idiomatic spelling. These gates pin that shape; the README's "M4
//! notes" publish the measured parity.
//!
//! Every variant closure below is kept in lockstep with
//! `benches/error_chains.rs` (the same discipline as `abc_gates.rs` ↔
//! `benches/abc.rs` and `kernel_gates.rs` ↔ `benches/kernels.rs`).

use catheo::{OptionWitness, ResultWitness};
use catheo_counting_alloc::{Counting, counted};

// The counting allocator is installed for this whole test binary; every
// other test in the file therefore also runs under it, harmlessly.
#[global_allocator]
static COUNTING: Counting = Counting;

// ─── Universe 1: the 10-stage Result chain (§6.1 "Result-chain error
// ─── handling: 10-stage bind-heavy pipeline, cold and warm") ────────────

/// The chain's error type: one overflow, tagged with the failing stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChainError {
    /// The 1-based stage index that rejected the value.
    stage: u8,
}

/// Stage `n` of the 10-stage chain: a Kleisli morphism
/// `i64 -> Result<i64, ChainError>`. Bare fn items (the parser's callable
/// stage form) — the same shape the benchmark measures.
macro_rules! chain_stage {
    ($name:ident, $n:literal) => {
        fn $name(value: i64) -> Result<i64, ChainError> {
            value.checked_add($n).ok_or(ChainError { stage: $n })
        }
    };
}

chain_stage!(stage_01, 1);
chain_stage!(stage_02, 2);
chain_stage!(stage_03, 3);
chain_stage!(stage_04, 4);
chain_stage!(stage_05, 5);
chain_stage!(stage_06, 6);
chain_stage!(stage_07, 7);
chain_stage!(stage_08, 8);
chain_stage!(stage_09, 9);
chain_stage!(stage_10, 10);

/// A warm seed value: survives all ten stages (far from `i64::MAX`), so
/// the pipeline value is `seed + 55`.
fn warm_seed(index: usize) -> i64 {
    i64::try_from(index % 1_000).unwrap_or(0) + 1
}

/// A cold seed value: fails at stage 3 (`i64::MAX - 3 + 3` overflows),
/// leaving stages 4–10 short-circuited — the error path the cold
/// benchmark isolates.
fn cold_seed() -> i64 {
    i64::MAX - 3
}

/// A: the naive spelling — a hand-rolled, ten-deep nested-match ladder,
/// one lexical continuation per stage. No combinators: the shape the
/// combinators exist to replace (§6.1 "naive nested closures").
fn result_chain_naive(input: Vec<Result<i64, ChainError>>) -> i64 {
    input
        .into_iter()
        .fold(0_i64, |total, element| total + ladder(element))
}

/// The ten-deep ladder, naive per element.
fn ladder(element: Result<i64, ChainError>) -> i64 {
    match element {
        Ok(seed) => match stage_01(seed) {
            Ok(x) => match stage_02(x) {
                Ok(x) => match stage_03(x) {
                    Ok(x) => match stage_04(x) {
                        Ok(x) => match stage_05(x) {
                            Ok(x) => match stage_06(x) {
                                Ok(x) => match stage_07(x) {
                                    Ok(x) => match stage_08(x) {
                                        Ok(x) => match stage_09(x) {
                                            Ok(x) => stage_10(x).unwrap_or_default(),
                                            Err(_) => 0,
                                        },
                                        Err(_) => 0,
                                    },
                                    Err(_) => 0,
                                },
                                Err(_) => 0,
                            },
                            Err(_) => 0,
                        },
                        Err(_) => 0,
                    },
                    Err(_) => 0,
                },
                Err(_) => 0,
            },
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

/// B: the idiomatic spelling — one `and_then` chain, short-circuited by
/// `Result`'s semantics, eliminated with `unwrap_or` at the end.
fn result_chain_idiomatic(input: Vec<Result<i64, ChainError>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + element
                .and_then(stage_01)
                .and_then(stage_02)
                .and_then(stage_03)
                .and_then(stage_04)
                .and_then(stage_05)
                .and_then(stage_06)
                .and_then(stage_07)
                .and_then(stage_08)
                .and_then(stage_09)
                .and_then(stage_10)
                .unwrap_or(0_i64)
    })
}

/// C: the macro-flattened output — the exact benchmark pipeline. On this
/// pointwise witness the fused lowering keeps the kernel chain: every
/// `run_bind` *is* `Result::and_then`, and the fold terminal stays
/// `run_fold` (the §6.1 M4 refinement; pinned by the emission-lockstep
/// gate below). Pedantic's `flat_map_option` only fires on
/// Option-returning closures, so it is [`kleisli_fused`] that carries the
/// documented expectation.
fn result_chain_fused(input: Vec<Result<i64, ChainError>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + catheo::pipeline! {
                witness: ResultWitness<ChainError>,
                source: element,
                bind: stage_01,
                bind: stage_02,
                bind: stage_03,
                bind: stage_04,
                bind: stage_05,
                bind: stage_06,
                bind: stage_07,
                bind: stage_08,
                bind: stage_09,
                bind: stage_10,
                fold: (0_i64, i64::wrapping_add),
            }
    })
}

/// The §6.1 sizes' worth of one universe.
fn result_chain_input(seed: fn(usize) -> i64, size: usize) -> Vec<Result<i64, ChainError>> {
    (0..size).map(|index| Ok(seed(index))).collect()
}

// ─── Universe 2: the two-morphism Kleisli chain over OptionWitness
// ─── (§6.1 "Kleisli composition: two-morphism Kleisli chain") ───────────

/// First Kleisli morphism `i64 -> Option<i64>`: keep the tripler's even
/// outputs — a deterministic mix of `Some` and `None` over the sweep.
fn kleisli_f(value: i64) -> Option<i64> {
    let tripled = value.checked_mul(3)?;
    (tripled % 2 == 0).then_some(tripled)
}

/// Second Kleisli morphism `i64 -> Option<i64>`: keep the shifted value
/// when it does not divide by three — the complement mix.
fn kleisli_g(value: i64) -> Option<i64> {
    let shifted = value.checked_add(7)?;
    (shifted % 3 != 0).then_some(shifted)
}

/// A: the naive spelling — the composition unrolled as a nested ladder.
fn kleisli_naive(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + match element {
                Some(x) => match kleisli_f(x) {
                    Some(y) => kleisli_g(y).unwrap_or_default(),
                    None => 0,
                },
                None => 0,
            }
    })
}

/// B: the idiomatic spelling — `(f >=> g)(x)` as an `and_then` chain.
fn kleisli_idiomatic(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + element
                .and_then(kleisli_f)
                .and_then(kleisli_g)
                .unwrap_or(0_i64)
    })
}

/// C: the macro-flattened output — the exact benchmark pipeline, on the
/// `OptionWitness` spelling of the pointwise dispatch (see
/// [`result_chain_fused`]).
fn kleisli_fused(input: Vec<Option<i64>>) -> i64 {
    input.into_iter().fold(0_i64, |total, element| {
        total
            + catheo::pipeline! {
                witness: OptionWitness,
                source: element,
                bind: kleisli_f,
                bind: kleisli_g,
                fold: (0_i64, i64::wrapping_add),
            }
    })
}

/// Edge shapes first, then a seeded xorshift sweep — the §6.2 idiom.
fn option_inputs() -> Vec<Vec<Option<i64>>> {
    let mut rng_first = XorShift(0xC0FF_EE12_3456_789A);
    let mut rng_second = XorShift(0x5EED_C0DE_1234_ABCD);
    let mut cases = vec![
        Vec::new(),
        vec![None],
        vec![Some(0)],
        vec![Some(2)],
        vec![Some(1)],
        vec![Some(6), Some(1), None, Some(-2)],
        vec![Some(i64::MAX), Some(i64::MIN), None],
        vec![Some(-1_000); 8],
    ];
    for _ in 0..4 {
        let len = rng_first.next_u64() % 64;
        let case = (0..len)
            .map(|_| match rng_first.next_u64() % 4 {
                0 => None,
                _ => Some(rng_first.next_i64()),
            })
            .collect();
        cases.push(case);
    }
    // A second sweep exercised through the same pipelines.
    for _ in 0..2 {
        let len = rng_second.next_u64() % 64;
        let case = (0..len)
            .map(|_| match rng_second.next_u64() % 3 {
                0 => None,
                _ => Some(rng_second.next_i64()),
            })
            .collect();
        cases.push(case);
    }
    cases
}

/// Edge Result shapes, both universes, plus a seeded xorshift sweep.
fn result_inputs() -> Vec<Vec<Result<i64, ChainError>>> {
    let mut rng_first = XorShift(0xABCD_EF01_2345_6789);
    let mut rng_second = XorShift(0xFEED_FACE_0DEF_E8A8);
    let mut cases = vec![
        Vec::new(),
        vec![Err(ChainError { stage: 0 })],
        vec![Ok(warm_seed(0))],
        vec![Ok(cold_seed())],
        vec![
            Ok(warm_seed(7)),
            Ok(cold_seed()),
            Err(ChainError { stage: 2 }),
        ],
        // Overflow-adjacent and underflow-adjacent edges for every stage
        // position the sweep can reach.
        vec![Ok(i64::MAX - 1), Ok(i64::MAX - 9), Ok(i64::MIN), Ok(0)],
    ];
    for _ in 0..4 {
        let len = rng_first.next_u64() % 64;
        let case = (0..len)
            .map(|_| match rng_first.next_u64() % 5 {
                0 => Err(ChainError {
                    stage: u8::try_from(rng_first.next_u64() % 11).unwrap_or(0),
                }),
                1 => Ok(cold_seed()),
                _ => Ok(rng_first.next_i64()),
            })
            .collect();
        cases.push(case);
    }
    for _ in 0..2 {
        let len = rng_second.next_u64() % 64;
        let case = (0..len)
            .map(|_| match rng_second.next_u64() % 6 {
                0 => Err(ChainError { stage: 1 }),
                _ => Ok(rng_second.next_i64()),
            })
            .collect();
        cases.push(case);
    }
    cases
}

/// Deterministic xorshift64\* (the repo's dependency-free input idiom).
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

    fn next_i64(&mut self) -> i64 {
        i64::try_from(self.next_u64() % 4_000).unwrap_or(0) - 2_000
    }
}

// ─── Gate 1: three-way agreement IS the correctness precondition ────────

#[test]
fn three_way_agreement_on_the_result_chain() {
    // Spot values first: if all three variants shared one wrong rewrite,
    // pairwise equality alone could not catch it.
    assert_eq!(
        result_chain_naive(vec![Ok(warm_seed(0))]),
        56,
        "A warm spot"
    );
    assert_eq!(
        result_chain_idiomatic(vec![Ok(warm_seed(0))]),
        56,
        "B warm spot"
    );
    assert_eq!(
        result_chain_fused(vec![Ok(warm_seed(0))]),
        56,
        "C warm spot"
    );
    assert_eq!(result_chain_naive(vec![Ok(cold_seed())]), 0, "A cold spot");
    assert_eq!(
        result_chain_idiomatic(vec![Ok(cold_seed())]),
        0,
        "B cold spot"
    );
    assert_eq!(result_chain_fused(vec![Ok(cold_seed())]), 0, "C cold spot");
    assert_eq!(
        result_chain_naive(vec![Err(ChainError { stage: 0 })]),
        0,
        "A pre-failed spot"
    );
    assert_eq!(
        result_chain_fused(vec![Err(ChainError { stage: 0 })]),
        0,
        "C pre-failed spot"
    );

    for input in result_inputs() {
        let (naive, _) = counted({
            let input = input.clone();
            move || result_chain_naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || result_chain_idiomatic(input)
        });
        let (fused, _) = counted(move || result_chain_fused(input));
        assert_eq!(naive, idiomatic, "A == B, result chain");
        assert_eq!(naive, fused, "A == C, result chain");
    }
}

#[test]
fn three_way_agreement_on_the_kleisli_chain() {
    // Spot values: Some(2) -> 13, Some(1) -> f rejects (0), None -> 0.
    assert_eq!(kleisli_naive(vec![Some(2)]), 13, "A spot Some(2)");
    assert_eq!(kleisli_idiomatic(vec![Some(2)]), 13, "B spot Some(2)");
    assert_eq!(kleisli_fused(vec![Some(2)]), 13, "C spot Some(2)");
    assert_eq!(kleisli_naive(vec![Some(1)]), 0, "A spot f-rejects");
    assert_eq!(kleisli_fused(vec![Some(1)]), 0, "C spot f-rejects");
    assert_eq!(kleisli_naive(vec![None]), 0, "A spot None");
    assert_eq!(kleisli_fused(vec![None]), 0, "C spot None");
    assert_eq!(kleisli_naive(vec![Some(6), Some(1), None, Some(-2)]), 26);
    assert_eq!(kleisli_fused(vec![Some(6), Some(1), None, Some(-2)]), 26);

    for input in option_inputs() {
        let (naive, _) = counted({
            let input = input.clone();
            move || kleisli_naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || kleisli_idiomatic(input)
        });
        let (fused, _) = counted(move || kleisli_fused(input));
        assert_eq!(naive, idiomatic, "A == B, Kleisli chain");
        assert_eq!(naive, fused, "A == C, Kleisli chain");
    }
}

// ─── Gate 2: the allocation profile IS the §6.1 comparison ──────────────

#[test]
fn the_result_chain_boxes_nothing_cold_or_warm() {
    let warm: Vec<Result<i64, ChainError>> = result_chain_input(warm_seed, 1_024);
    let cold: Vec<Result<i64, ChainError>> = result_chain_input(|_| cold_seed(), 1_024);

    for (universe, input) in [("warm", &warm), ("cold", &cold)] {
        let (_, naive_allocs) = counted({
            let input = input.clone();
            move || result_chain_naive(input)
        });
        let (_, idiomatic_allocs) = counted({
            let input = input.clone();
            move || result_chain_idiomatic(input)
        });
        let (_, fused_allocs) = counted({
            let input = input.clone();
            move || result_chain_fused(input)
        });
        assert_eq!(naive_allocs, 0, "A allocates nothing, {universe}");
        assert_eq!(
            idiomatic_allocs, 0,
            "B allocates nothing, {universe}: C ≥ B"
        );
        assert_eq!(
            fused_allocs, 0,
            "C must box nothing on the {universe} path (the §1 failure mode)"
        );
    }
}

#[test]
fn the_kleisli_chain_boxes_nothing() {
    let input: Vec<Option<i64>> = (0..1_024)
        .map(|i| match i % 3 {
            0 => None,
            _ => Some(i64::from(i % 401) - 200),
        })
        .collect();

    let (_, naive_allocs) = counted({
        let input = input.clone();
        move || kleisli_naive(input)
    });
    let (_, idiomatic_allocs) = counted({
        let input = input.clone();
        move || kleisli_idiomatic(input)
    });
    let (_, fused_allocs) = counted(move || kleisli_fused(input));
    assert_eq!(naive_allocs, 0, "A allocates nothing");
    assert_eq!(idiomatic_allocs, 0, "B allocates nothing: C ≥ B");
    assert_eq!(
        fused_allocs, 0,
        "C must box nothing on the Kleisli chain (the §1 failure mode)"
    );
}

// ─── Gate 3: emission lockstep (why the counts above are what they are) ─

use catheo_parser::{emit, parse_pipeline};
use quote::quote;

#[test]
fn fused_emission_shape_lockstep_with_the_bench_pipelines() {
    // The 10-stage Result chain over a pointwise witness: every bind is a
    // bare callable (never a structural observer) and the fold is pure,
    // but there is no container to deforest — so the pointwise dispatch
    // keeps the whole chain at the kernel shape: one `run_bind` per stage
    // (the kernel *is* `Result::and_then`) and one `run_fold`
    // eliminator, no iterator adapters. This is the emission that makes
    // C the idiomatic machine shape and the zero-allocation gate above
    // structural, not accidental.
    let result_chain = parse_pipeline(quote! {
        witness: ResultWitness<ChainError>,
        source: element,
        bind: stage_01,
        bind: stage_02,
        bind: stage_03,
        bind: stage_04,
        bind: stage_05,
        bind: stage_06,
        bind: stage_07,
        bind: stage_08,
        bind: stage_09,
        bind: stage_10,
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("result chain parses");
    let out = emit(&result_chain).expect("result chain emits").to_string();
    assert_eq!(
        out.matches("run_bind").count(),
        10,
        "one run_bind kernel per stage: {out}"
    );
    assert_eq!(
        out.matches("run_fold").count(),
        1,
        "exactly one eliminator: {out}"
    );
    assert!(
        !out.contains("flat_map"),
        "no adapter stack over a 1-element iterator: {out}"
    );
    assert!(
        !out.contains("into_iter"),
        "no iterator is formed at all: {out}"
    );
    assert!(
        out.contains("__catheo_state_9"),
        "flat let-bound registers (§3.4): {out}"
    );

    // The two-morphism Kleisli chain: same pointwise kernel shape.
    let kleisli = parse_pipeline(quote! {
        witness: OptionWitness,
        source: element,
        bind: kleisli_f,
        bind: kleisli_g,
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("Kleisli chain parses");
    let out = emit(&kleisli).expect("Kleisli chain emits").to_string();
    assert_eq!(
        out.matches("run_bind").count(),
        2,
        "one run_bind per morphism: {out}"
    );
    assert_eq!(
        out.matches("run_fold").count(),
        1,
        "exactly one eliminator: {out}"
    );
    assert!(!out.contains("flat_map"), "no adapters: {out}");
    assert!(!out.contains("into_iter"), "no iterators: {out}");

    // And the contrast that keeps the refinement honest: the same shape
    // over VecWitness still absorbs into the Rule 4 adapter chain.
    let vec_chain = parse_pipeline(quote! {
        witness: VecWitness,
        source: element,
        bind: kleisli_f,
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("Vec chain parses");
    let out = emit(&vec_chain).expect("Vec chain emits").to_string();
    assert!(
        !out.contains("run_bind"),
        "Vec keeps the Rule 4 absorption: {out}"
    );
    assert_eq!(
        out.matches(". flat_map (").count(),
        1,
        "Vec keeps the adapter chain: {out}"
    );
}
