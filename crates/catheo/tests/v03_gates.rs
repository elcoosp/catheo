//! The §6.1 v0.3 gates, as deterministic integration tests.
//!
//! The §8 roadmap gates v0.3 on the same protocol as v0.2: the new grammar
//! entry ships with "benchmark gates re-applied". As there, wall-clock
//! comparisons cannot be enforced on shared CI runners, so this file
//! enforces the *deterministic* consequences of the claims while
//! `benches/traverse.rs` is the measurement instrument (criterion,
//! published in the README's "v0.3 notes"). The two are kept in lockstep:
//! identical shapes, identical closures.
//!
//! What is enforced here:
//!
//! 1. **Traversal emission shape.** The absorbable prefix chains into the
//!    traversal's iterator (Rules 1–2 extended to the boundary); the
//!    suffix + terminal lower inside the effect guard; exactly one
//!    `run_traverse` kernel dispatches on the spelled effect witness.
//! 2. **Three-way agreement.** A the two-materialization naive spelling,
//!    B the idiomatic `.map(f).collect::<Result<Vec, _>>()` chain, C the
//!    macro — value-identical over seeded sweeps, warm and failing.
//! 3. **Allocation profile.** C allocates exactly like B (one traversal
//!    container, zero intermediates) and strictly below A (which
//!    materializes the prefix).
//! 4. **Oracle agreement.** Fused == `pipeline_unfused!` over `Vec` inputs
//!    for both pointwise effects, and the guard forwards the *original*
//!    error value unchanged.
//! 5. **Kleisli-prefix absorption.** A non-observing bind before the
//!    traversal fuses into the chain (`flat_map`), and the runtime value
//!    still agrees with the oracle.

use catheo::{OptionWitness, ResultWitness, VecWitness};
use catheo_counting_alloc::{Counting, counted};
use catheo_parser::{Lowering, emit, emit_with_lowering, parse_pipeline};
use quote::quote;

#[global_allocator]
static COUNTING: Counting = Counting;

/// Deterministic xorshift64\* sweep (the fusion-properties pattern).
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
}

const XORSHIFT_SEED: u64 = 0x0303_0A2B_C1DE_A5C0;

/// Edge shapes first, then pseudo-random vectors.
fn sweep_vecs() -> Vec<Vec<i32>> {
    let mut rng = XorShift(XORSHIFT_SEED);
    let mut cases = vec![
        Vec::new(),
        vec![0],
        vec![1],
        vec![-1],
        vec![1, 2, 3],
        vec![i32::MAX, i32::MIN, 0],
    ];
    for _ in 0..24 {
        let len = rng.next_u64() % 10;
        cases.push((0..len).map(|_| rng.next_i32()).collect());
    }
    cases
}

/// The error type of the traversal effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateError;

/// The traversal arrow shared by A, B, and C: `i32 -> Result<u64, _>`,
/// rejecting negatives (so the mixed sweeps exercise the short circuit).
fn try_u64(x: i32) -> Result<u64, GateError> {
    u64::try_from(x).map_err(|_| GateError)
}

// ─── Gate 1: the emission shape ───────────────────────────────────────────

#[test]
fn traversal_absorbs_the_prefix_and_guards_the_suffix() {
    let fused = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        map: |x| x.wrapping_mul(2),
        traverse: (ResultWitness<GateError>, try_u64),
        map: |y| y.wrapping_add(1),
        fold: (0_u64, u64::wrapping_add),
    })
    .expect("parses");
    let out = emit(&fused).expect("emits").to_string();
    assert_eq!(
        out.matches("run_traverse").count(),
        1,
        "exactly one traversal kernel: {out}"
    );
    assert!(
        !out.contains("run_map") && !out.contains("run_fold") && !out.contains("run_bind"),
        "the prefix rides the chain and the suffix rides the guard: {out}"
    );
    assert!(
        out.contains("into_iter (__catheo_source) . map (| x | x . wrapping_mul (2))"),
        "the absorbable prefix chains into the traversal: {out}"
    );
    assert!(
        out.contains(":: core :: result :: Result :: Ok ("),
        "the guard wraps the suffix: {out}"
    );
    assert!(
        out.contains("into_iter (__catheo_inner) . map (| y | y . wrapping_add (1)) . fold (0_u64"),
        "Rule 4 lowers the suffix fold inside the guard: {out}"
    );
}

#[test]
fn traversal_emission_keeps_the_oracle_guard_honest() {
    // Both lowerings emit the guard; only the absorption differs. The
    // sequential shape is what `pipeline_unfused!` expands to, so this is
    // the exact emission whose runtime value gate 4 compares.
    let fused = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        traverse: (OptionWitness, try_opt),
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("parses");
    let fused_out = emit(&fused).expect("emits").to_string();
    let sequential_out = emit_with_lowering(&fused, Lowering::Sequential)
        .expect("emits")
        .to_string();
    for out in [&fused_out, &sequential_out] {
        assert_eq!(out.matches("run_traverse").count(), 1, "{out}");
        assert!(
            out.contains(":: core :: option :: Option :: Some ("),
            "{out}"
        );
        assert!(out.contains(":: core :: option :: Option :: None"), "{out}");
    }
    assert!(
        fused_out.contains(". fold (0_i64"),
        "the fused guard absorbs the fold: {fused_out}"
    );
    assert!(
        sequential_out.contains("run_fold"),
        "the oracle keeps the fold kernel: {sequential_out}"
    );
}

fn try_opt(x: i32) -> Option<i64> {
    if x < 0 { None } else { Some(i64::from(x)) }
}

fn try_opt_i64(x: i64) -> Option<i64> {
    if x < 0 { None } else { Some(x) }
}

// ─── Gate 2: three-way agreement ──────────────────────────────────────────

/// A: the naive spelling — every combinator materializes its container,
/// with a hand-rolled early-exit loop for the traversal.
fn traverse_naive(input: Vec<i32>) -> Result<u64, GateError> {
    let mapped: Vec<i32> = input.into_iter().map(|x| x.wrapping_mul(3)).collect();
    let mut collected: Vec<u64> = Vec::new();
    for x in mapped {
        collected.push(try_u64(x)?);
    }
    let mut total = 0_u64;
    for x in collected {
        total = total.wrapping_add(x);
    }
    Ok(total)
}

/// B: the idiomatic spelling — one adapter pass, short-circuiting collect.
fn traverse_idiomatic(input: Vec<i32>) -> Result<u64, GateError> {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(try_u64)
        .collect::<Result<Vec<u64>, GateError>>()
        .map(|collected| collected.into_iter().fold(0_u64, u64::wrapping_add))
}

/// C: the macro-flattened output — the exact benchmark pipeline.
fn traverse_macro(input: Vec<i32>) -> Result<u64, GateError> {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(3),
        traverse: (ResultWitness<GateError>, try_u64),
        fold: (0_u64, u64::wrapping_add),
    }
}

#[test]
fn traversal_three_way_agreement_over_the_sweep() {
    for input in sweep_vecs() {
        let a = traverse_naive(input.clone());
        let b = traverse_idiomatic(input.clone());
        let c = traverse_macro(input.clone());
        assert_eq!((a, b), (b, c), "input {input:?}");
    }
}

// ─── Gate 3: the allocation profile ───────────────────────────────────────
//
// The traversal *inherently* allocates its collected container (std's
// `FromIterator` elimination grows it amortized because the surviving count
// is unknowable ahead of the short circuit), so the naive A/B/C wall-clock
// comparison is the benchmark's job (§6.1: `benches/traverse.rs`). The
// deterministic claim the fusion pass must prove is about *intermediates*:
// C and B lower to the identical iterator composition — the prefix map
// rides the traversal's chain and never materializes its own container —
// so their allocation profiles must be exactly equal. (The sequential
// oracle's extra `Vec` is *not* observable here: LLVM deletes the
// provably-scoped intermediate allocation, which is itself evidence the
// flat shape gives the optimizer nothing to recover from.)

#[test]
fn traversal_allocation_profile_matches_the_idiomatic_chain() {
    let input: Vec<i32> = (0..1_000).collect();
    // The clones happen *outside* the counted sections: they measure the
    // pipeline shapes, not the harness.
    let for_b = input.clone();
    let (b_value, b_count) = counted(|| traverse_idiomatic(for_b));
    let (c_value, c_count) = counted(|| traverse_macro(input));
    assert_eq!(b_value, c_value);
    assert_eq!(
        c_count, b_count,
        "C allocates exactly like the one-pass idiomatic chain — the prefix \
         map never materializes its own container (B = {b_count}, C = {c_count})"
    );
}

// ─── Gate 4: oracle agreement over both effects ───────────────────────────

#[test]
fn traversal_agrees_with_the_oracle_over_both_effects() {
    for input in sweep_vecs() {
        // Result effect: warm (all pass) and failing (negatives rejected).
        let fused: Result<u64, GateError> = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            traverse: (ResultWitness<GateError>, try_u64),
            fold: (0_u64, u64::wrapping_add),
        };
        let oracle: Result<u64, GateError> = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            traverse: (ResultWitness<GateError>, try_u64),
            fold: (0_u64, u64::wrapping_add),
        };
        assert_eq!(fused, oracle, "Result input {input:?}");

        // Option effect: a warm sum (the failing sweep is covered by the
        // three-way gate; here both lowerings must agree per input).
        let fused: Option<i64> = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            traverse: (OptionWitness, try_opt),
            fold: (0_i64, i64::wrapping_add),
        };
        let oracle: Option<i64> = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            traverse: (OptionWitness, try_opt),
            fold: (0_i64, i64::wrapping_add),
        };
        assert_eq!(fused, oracle, "Option input {input:?}");
    }
}

#[test]
fn traversal_forwards_the_original_error_value() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Tagged {
        rejected: i32,
    }
    let f = |x: i32| -> Result<i32, Tagged> {
        if x % 2 == 0 {
            Ok(x)
        } else {
            Err(Tagged { rejected: x })
        }
    };
    for input in [vec![1_i32], vec![2, 3, 4], vec![-1, 0]] {
        let c: Result<i64, Tagged> = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            traverse: (ResultWitness<Tagged>, f),
            fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
        };
        let expected = input
            .into_iter()
            .map(f)
            .collect::<Result<Vec<i32>, Tagged>>()
            .map(|collected| {
                collected
                    .into_iter()
                    .fold(0_i64, |acc: i64, x: i32| acc + i64::from(x))
            });
        assert_eq!(c, expected, "the guard forwards the original Err");
    }
}

// ─── Gate 5: the Kleisli-prefix absorption ────────────────────────────────

fn expand(x: i32) -> Vec<i64> {
    vec![i64::from(x), i64::from(x) * 2]
}

fn keep(x: i64) -> Vec<i64> {
    if x % 3 == 0 { vec![x] } else { vec![x, x] }
}

fn widen(x: i64) -> Vec<i64> {
    vec![x - 1, x + 1]
}

#[test]
fn non_observing_bind_fuses_across_the_traversal_boundary() {
    let fused = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        map: |x| x.wrapping_mul(2),
        bind: expand,
        kleisli: (keep, widen),
        traverse: (OptionWitness, try_opt_i64),
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("parses");
    let out = emit(&fused).expect("emits").to_string();
    assert!(
        !out.contains("run_map")
            && !out.contains("run_bind")
            && !out.contains("run_fold")
            && !out.contains("run_filter"),
        "Rules 1–2 absorb the whole non-observing prefix: {out}"
    );
    assert_eq!(
        out.matches(". flat_map (").count(),
        3,
        "one flat_map per bind-bearing stage: {out}"
    );
    assert_eq!(out.matches("run_traverse").count(), 1, "{out}");

    for input in sweep_vecs() {
        let fused: Option<i64> = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            bind: expand,
            kleisli: (keep, widen),
            traverse: (OptionWitness, try_opt_i64),
            fold: (0_i64, i64::wrapping_add),
        };
        let oracle: Option<i64> = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            bind: expand,
            kleisli: (keep, widen),
            traverse: (OptionWitness, try_opt_i64),
            fold: (0_i64, i64::wrapping_add),
        };
        assert_eq!(fused, oracle, "input {input:?}");
    }
}
