//! The §6.1 v0.4 gates, as deterministic integration tests.
//!
//! The §8 roadmap gates every v0.x milestone on the same protocol: the new
//! grammar entry ships with "benchmark gates re-applied". As for v0.2/v0.3,
//! wall-clock comparisons cannot be enforced on shared CI runners, so this
//! file enforces the *deterministic* consequences of the claims while
//! `benches/join.rs` is the measurement instrument (criterion, published in
//! the README's "v0.4 notes"). The two are kept in lockstep: identical
//! shapes, identical closures.
//!
//! The milestone: `join` (§4.3) — the monadic multiplication, desugared at
//! parse time to `bind(id)`. What is enforced here:
//!
//! 1. **Emission shape.** The join lowers to std's `.flatten()` in the
//!    fused adapter chain — value-identical to `bind(id)`, and the exact
//!    B-parity spelling — with a fused map riding the `.flat_map`
//!    position; the sequential oracle keeps one kernel per stage.
//! 2. **Three-way agreement.** A the two-materialization naive spelling,
//!    B the idiomatic `flat_map`/`flatten` chains, C the macro —
//!    value-identical over seeded sweeps.
//! 3. **Allocation profile.** C allocates exactly like B (the per-element
//!    inner containers only, zero intermediates) and strictly below A
//!    (which materializes the map and the flattened container).
//! 4. **Oracle agreement.** Fused == `pipeline_unfused!` for the join
//!    shapes over every witness: `Vec` (prefix, nested source, after a
//!    traversal), `Option`, and `Result`.
//! 5. **The §3.2 observer posture.** The rescan re-flags the fused view
//!    when an absorbed body observes shape (pinned in catheo-parser's fuse
//!    specs) and the values never move, at either lowering.

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

const XORSHIFT_SEED: u64 = 0x0404_0A2B_C0DE_0404;

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

/// The error type of the Result-witness join cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateError;

// ─── Gate 1: the emission shape ───────────────────────────────────────────────

#[test]
fn join_rides_the_fused_chain_like_any_non_observing_bind() {
    let fused = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        map: |x| vec![x, x],
        join,
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    })
    .expect("parses");
    let out = emit(&fused).expect("emits").to_string();
    assert!(
        !out.contains("run_map") && !out.contains("run_bind") && !out.contains("run_fold"),
        "the pure prefix and the join lower into one imperative chain: {out}"
    );
    assert!(
        out.contains(". flat_map (| x | vec ! [x , x]) . fold (0_i64"),
        "Rule 2 puts the map in the flat_map position and Rule 4 keeps \
         the fold at the end: {out}"
    );
    assert!(
        !out.contains(". flatten ()"),
        "no literal identity rides the fused shape: {out}"
    );

    let sequential_out = emit_with_lowering(&fused, Lowering::Sequential)
        .expect("emits")
        .to_string();
    for kernel in ["run_map", "run_bind", "run_fold"] {
        assert!(
            sequential_out.contains(kernel),
            "the oracle keeps one kernel per stage ({kernel}): {sequential_out}"
        );
    }
}

#[test]
fn join_of_a_nested_source_chains_into_the_collect() {
    let fused = parse_pipeline(quote! {
        witness: VecWitness,
        source: nested,
        join,
        collect,
    })
    .expect("parses");
    let out = emit(&fused).expect("emits").to_string();
    assert!(
        out.contains("into_iter (__catheo_source) . flatten () . collect ()"),
        "a leading join is the whole chain, spelled std's flatten: {out}"
    );
    assert!(!out.contains("run_bind"), "{out}");
}

#[test]
fn join_after_a_traversal_lowers_inside_the_effect_guard() {
    let fused = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        traverse: (OptionWitness, |x| Some(vec![x, x])),
        join,
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("parses");
    let out = emit(&fused).expect("emits").to_string();
    assert_eq!(out.matches("run_traverse").count(), 1, "{out}");
    assert!(
        !out.contains("run_bind") && !out.contains("run_fold") && !out.contains("run_map"),
        "the suffix run (join + fold) absorbs inside the guard: {out}"
    );
    assert!(
        out.contains(":: core :: option :: Option :: Some ("),
        "the guard wraps the suffix: {out}"
    );
    assert_eq!(
        out.matches(". flatten ()").count(),
        1,
        "exactly the join identity, inside the guard: {out}"
    );
}

// ─── Gate 2: three-way agreement ──────────────────────────────────────────

/// A: the naive spelling — the map materializes its nested container, the
/// join materializes the flattened one, and the fold loops over that.
fn join_naive(input: Vec<i32>) -> i64 {
    let mapped: Vec<Vec<i32>> = input.into_iter().map(|x| vec![x, x]).collect();
    let mut flattened: Vec<i32> = Vec::new();
    for inner in mapped {
        flattened.extend(inner);
    }
    let mut total = 0_i64;
    for x in flattened {
        total += i64::from(x);
    }
    total
}

/// B: the idiomatic spelling — one adapter pass, no intermediate container.
fn join_idiomatic(input: Vec<i32>) -> i64 {
    input
        .into_iter()
        .flat_map(|x| vec![x, x])
        .fold(0_i64, |acc, x| acc + i64::from(x))
}

/// C: the macro-flattened output — the exact benchmark pipeline.
fn join_macro_pipeline(input: Vec<i32>) -> i64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| vec![x, x],
        join,
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    }
}

#[test]
fn join_three_way_agreement_over_the_sweep() {
    for input in sweep_vecs() {
        let a = join_naive(input.clone());
        let b = join_idiomatic(input.clone());
        let c = join_macro_pipeline(input.clone());
        assert_eq!((a, b), (b, c), "input {input:?}");
    }
}

// ─── Gate 3: the allocation profile ───────────────────────────────────────

#[test]
fn join_allocation_profile_matches_the_idiomatic_chain() {
    let input: Vec<i32> = (0..1_000).collect();
    // The clones happen *outside* the counted sections: they measure the
    // pipeline shapes, not the harness. The per-element `vec![x, x]` is
    // part of every spelling's shape, so the *fused* claim is exactly:
    // zero intermediate containers beyond B's own.
    let for_a = input.clone();
    let for_b = input.clone();
    let (a_value, a_count) = counted(|| join_naive(for_a));
    let (b_value, b_count) = counted(|| join_idiomatic(for_b));
    let (c_value, c_count) = counted(|| join_macro_pipeline(input));
    assert_eq!(a_value, b_value);
    assert_eq!(b_value, c_value);
    assert_eq!(
        c_count, b_count,
        "C allocates exactly like the one-pass idiomatic chain — the join \
         never materializes the nested or flattened container \
         (B = {b_count}, C = {c_count})"
    );
    assert!(
        a_count > c_count,
        "A materializes two intermediates A = {a_count}, C = {c_count}"
    );
}

// ─── Gate 4: oracle agreement over every witness ──────────────────────────

#[test]
fn join_agrees_with_the_oracle_over_the_vec_prefix() {
    for input in sweep_vecs() {
        let fused: i64 = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| vec![x, x],
            join,
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let oracle: i64 = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| vec![x, x],
            join,
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, oracle, "input {input:?}");
    }
}

#[test]
fn join_of_a_nested_source_agrees_with_the_oracle() {
    for input in sweep_vecs() {
        let nested: Vec<Vec<i32>> = input.iter().map(|x| vec![*x, *x]).collect();
        let fused: Vec<i32> = catheo::pipeline! {
            witness: VecWitness,
            source: nested.clone(),
            join,
            collect,
        };
        let oracle: Vec<i32> = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: nested.clone(),
            join,
            collect,
        };
        assert_eq!(fused, oracle, "input {input:?}");
        let idiomatic: Vec<i32> = nested.into_iter().flatten().collect();
        assert_eq!(fused, idiomatic, "the B-parity spelling, input {input:?}");
    }
}

#[test]
fn join_agrees_with_the_oracle_over_the_pointwise_witnesses() {
    // Option: `Option<Option<T>>` flattens; `None` at either level wins.
    let nested_opt: Option<Option<i32>> = Some(Some(5));
    let fused: Option<i32> = catheo::pipeline! {
        witness: OptionWitness,
        source: nested_opt,
        join,
    };
    assert_eq!(fused, Some(5));
    let fused: Option<i32> = catheo::pipeline! {
        witness: OptionWitness,
        source: Some(None::<i32>),
        join,
    };
    assert_eq!(fused, None, "an outer None forwards");
    let fused: Option<i32> = catheo::pipeline! {
        witness: OptionWitness,
        source: None::<Option<i32>>,
        join,
    };
    assert_eq!(fused, None, "an inner None flattens out");

    // Result: both layers must share the error type, which is exactly the
    // `Result<Result<T, E>, E>` shape the kernel's `Fn` bound pins.
    let ok_ok: Result<Result<i32, GateError>, GateError> = Ok(Ok(7));
    let fused: Result<i32, GateError> = catheo::pipeline! {
        witness: ResultWitness<GateError>,
        source: ok_ok,
        join,
    };
    assert_eq!(fused, Ok(7));
    let err_outer: Result<Result<i32, GateError>, GateError> = Err(GateError);
    let fused: Result<i32, GateError> = catheo::pipeline! {
        witness: ResultWitness<GateError>,
        source: err_outer,
        join,
    };
    assert_eq!(fused, Err(GateError), "an outer Err forwards unchanged");
}

#[test]
fn join_after_a_traversal_agrees_with_the_oracle() {
    for input in sweep_vecs() {
        let fused: Option<i64> = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            traverse: (OptionWitness, |x| Some(vec![i64::from(x), i64::from(x)])),
            join,
            fold: (0_i64, i64::wrapping_add),
        };
        let oracle: Option<i64> = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            traverse: (OptionWitness, |x| Some(vec![i64::from(x), i64::from(x)])),
            join,
            fold: (0_i64, i64::wrapping_add),
        };
        assert_eq!(fused, oracle, "input {input:?}");
        // B parity for this warm shape: the arrow never rejects, so the
        // traversal+join value equals the direct two-element flat_map.
        let sum: i64 = input
            .iter()
            .copied()
            .flat_map(|x| vec![i64::from(x), i64::from(x)])
            .fold(0_i64, i64::wrapping_add);
        assert_eq!(fused, Some(sum), "the B-parity value, input {input:?}");
    }
}

// ─── Gate 5: the §3.2 observer false-positive posture ─────────────────────

#[test]
fn the_conservative_rescan_around_join_never_changes_values() {
    // `.iter` in the map body re-flags the fused bind in the *AST view* of
    // the deforestation pass (§3.2, false positives acceptable — pinned by
    // `an_observer_map_before_join_rescans_to_a_barrier` in catheo-parser).
    // The emission still lowers by application-composition, whose values
    // the flag never touches: this gate pins exactly that, fused ==
    // oracle == the hand-written chain.
    let fused_ast = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        map: |x: i32| [x, x].iter().map(|&v| v.wrapping_mul(2)).collect::<Vec<i32>>(),
        join,
        collect,
    })
    .expect("parses");
    let out = emit(&fused_ast).expect("emits").to_string();
    assert!(
        out.contains(
            ". iter () . map (| & v | v . wrapping_mul (2)) . collect :: < Vec < i32 >> ()"
        ),
        "the map body survives verbatim in the chain: {out}"
    );
    assert!(
        out.contains(
            ". flat_map (| x : i32 | [x , x] . iter () . map (| & v | v . wrapping_mul (2)) . collect :: < Vec < i32 >> ())"
        ),
        "the observer map takes the flat_map position, identity absorbed: {out}"
    );

    for input in sweep_vecs() {
        let fused: Vec<i32> = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x: i32| [x, x].iter().map(|&v| v.wrapping_mul(2)).collect::<Vec<i32>>(),
            join,
            collect,
        };
        let oracle: Vec<i32> = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x: i32| [x, x].iter().map(|&v| v.wrapping_mul(2)).collect::<Vec<i32>>(),
            join,
            collect,
        };
        let idiomatic: Vec<i32> = input
            .clone()
            .into_iter()
            .flat_map(|x: i32| {
                [x, x]
                    .iter()
                    .map(|&v| v.wrapping_mul(2))
                    .collect::<Vec<i32>>()
            })
            .collect();
        assert_eq!(fused, oracle, "input {input:?}");
        assert_eq!(fused, idiomatic, "the B-parity spelling, input {input:?}");
    }
}
