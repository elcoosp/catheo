//! The §6.1 v0.2 gates, as deterministic integration tests.
//!
//! The §8 roadmap gates v0.2 on the same protocol as M3/M4: "Kleisli,
//! `fold_map`, escape hatches — benchmark gates re-applied". As there,
//! wall-clock comparisons cannot be enforced on shared CI runners, so this
//! file enforces the *deterministic* consequences of the claims while
//! `benches/combinators.rs` is the measurement instrument (criterion,
//! published in the README's "v0.2 notes"). The two are kept in lockstep:
//! identical shapes, identical closures.
//!
//! What is enforced here:
//!
//! 1. **Kleisli desugar lockstep.** A `kleisli: (f, g, …)` entry emits
//!    byte-for-byte the same expansion as the equivalent `bind:` spelling
//!    (§4.3: "desugars to nested bind"), and a preceding `map` fuses into
//!    the first non-observing morphism (the §3.3 rules extend across the
//!    chain boundary). Runtime agreement A == B == C over seeded sweeps.
//! 2. **`fold_map` gates.** Three-way agreement (naive materializing A,
//!    idiomatic `.map(f).fold(identity, combine)` B, macro C); C's
//!    allocation profile equals B's (zero intermediate containers) and is
//!    strictly below A's; pointwise witnesses keep the `run_fold_map`
//!    kernel with no adapter stack (the §6.1 M4 posture).
//! 3. **Escape hatches.** `bind!: force` chains an observer-marked bind
//!    into the fused pass (emission + agreement); `bind!: materialize`
//!    stays a kernel and blocks absorption; the `source: expr as Type`
//!    ascription derives the correct built-in witness (§4.2).

use catheo::{Monoid, OptionWitness, ResultWitness, VecWitness};
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

const XORSHIFT_SEED: u64 = 0x0202_0A2B_C1DE_A5C0;

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

/// Representative single elements for the pointwise domains.
fn sweep_elems() -> Vec<i32> {
    let mut rng = XorShift(XORSHIFT_SEED ^ 0xFEED_BEEF);
    let mut cases = vec![0, 1, -1, 2, 3, i32::MAX / 2, -(i32::MAX / 2)];
    for _ in 0..24 {
        cases.push(rng.next_i32());
    }
    cases
}

// ─── Gate 1: the kleisli entry ────────────────────────────────────────────

#[test]
fn kleisli_entry_emits_byte_identical_to_the_bind_spelling() {
    let desugared = parse_pipeline(quote! {
        witness: OptionWitness,
        source: element,
        kleisli: (kleisli_f, kleisli_g),
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("kleisli entry parses");
    let spelled = parse_pipeline(quote! {
        witness: OptionWitness,
        source: element,
        bind: kleisli_f,
        bind: kleisli_g,
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("bind spelling parses");
    let desugared_out = emit(&desugared).expect("kleisli emits").to_string();
    let spelled_out = emit(&spelled).expect("binds emit").to_string();
    assert_eq!(
        desugared_out, spelled_out,
        "the §4.3 desugar is emission-identical to nested binds"
    );
    assert_eq!(spelled_out.matches("run_bind").count(), 2);
    assert_eq!(spelled_out.matches("run_fold").count(), 1);
}

#[test]
fn kleisli_three_morphism_chain_emits_three_binds() {
    let chain = parse_pipeline(quote! {
        witness: ResultWitness<GateError>,
        source: element,
        kleisli: (stage_a, stage_b, stage_c),
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("chain parses");
    let out = emit(&chain).expect("chain emits").to_string();
    assert_eq!(out.matches("run_bind").count(), 3, "{out}");
    assert_eq!(out.matches("run_fold").count(), 1, "{out}");
    assert!(
        !out.contains("flat_map"),
        "pointwise stays kernel-shaped: {out}"
    );
}

/// A preceding map fuses into the first non-observing kleisli morphism:
/// the §3.3 rules extend across the desugared chain boundary.
#[test]
fn map_fuses_across_the_kleisli_chain_boundary() {
    let with_kleisli = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        map: |x| x.wrapping_mul(2),
        kleisli: (expand, keep),
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("parses");
    let out = emit(&with_kleisli).expect("emits").to_string();
    assert!(
        !out.contains("run_"),
        "Rule 2 + Rule 4 absorb the map and the chain: {out}"
    );
    assert_eq!(
        out.matches(". flat_map (").count(),
        2,
        "one flat_map per morphism: {out}"
    );
}

/// The morphisms behind the emission-shape tests above.
fn expand(x: i32) -> Vec<i64> {
    vec![i64::from(x), i64::from(x) * 2]
}

fn keep(x: i64) -> Vec<i64> {
    if x % 3 == 0 { vec![x] } else { vec![x, x] }
}

/// The error type of the three-stage Result chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateError;

fn stage_a(value: i64) -> Result<i64, GateError> {
    value.checked_add(1).ok_or(GateError)
}

fn stage_b(value: i64) -> Result<i64, GateError> {
    value.checked_mul(2).ok_or(GateError)
}

fn stage_c(value: i64) -> Result<i64, GateError> {
    value.checked_sub(3).ok_or(GateError)
}

/// Runtime agreement for the kleisli entry over `VecWitness` (flattening
/// binds) and over `ResultWitness<GateError>` (short-circuiting binds):
/// pipeline vs `pipeline_unfused!`, the §6.2 gate applied to the desugar.
#[test]
fn kleisli_entry_agrees_with_the_oracle_over_vec_and_result() {
    for input in sweep_vecs() {
        let fused: i64 = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            kleisli: (expand, keep),
            fold: (0_i64, i64::wrapping_add),
        };
        let oracle: i64 = catheo::pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            kleisli: (expand, keep),
            fold: (0_i64, i64::wrapping_add),
        };
        assert_eq!(fused, oracle, "Vec input {input:?}");
    }

    let ok_universe: Vec<Result<i64, GateError>> = sweep_elems()
        .into_iter()
        .map(|x| Ok(i64::from(x)))
        .collect();
    let mixed_universe: Vec<Result<i64, GateError>> = ok_universe
        .iter()
        .copied()
        .enumerate()
        .map(|(index, element)| {
            if index % 3 == 0 {
                Err(GateError)
            } else {
                element
            }
        })
        .collect();
    for universe in [ok_universe, mixed_universe] {
        // The witness holds one element at a time: the universe is folded
        // outside, exactly like the M4 error-chain gates.
        let fused: i64 = universe.iter().copied().fold(0_i64, |total, element| {
            total
                + catheo::pipeline! {
                    witness: ResultWitness::<GateError>,
                    source: element,
                    kleisli: (stage_a, stage_b, stage_c),
                    fold: (0_i64, |acc, x| acc + x),
                }
        });
        let oracle: i64 = universe.iter().copied().fold(0_i64, |total, element| {
            total
                + catheo::pipeline_unfused! {
                    witness: ResultWitness::<GateError>,
                    source: element,
                    kleisli: (stage_a, stage_b, stage_c),
                    fold: (0_i64, |acc, x| acc + x),
                }
        });
        assert_eq!(fused, oracle, "Result universe {universe:?}");
    }
}

/// Three-way runtime agreement for the kleisli entry over `OptionWitness`:
/// A the naive nested match, B the idiomatic `and_then` chain, C the
/// `kleisli:` entry — the M4 gate form, re-applied to the new grammar.
#[test]
fn kleisli_entry_three_way_agreement_over_the_sweep() {
    fn naive(x: i32) -> i64 {
        match x.checked_add(1) {
            Some(x) => match x.checked_mul(3) {
                Some(x) => match x.checked_sub(2) {
                    Some(x) => i64::from(x),
                    None => 0,
                },
                None => 0,
            },
            None => 0,
        }
    }

    fn idiomatic(x: i32) -> i64 {
        x.checked_add(1)
            .and_then(|x| x.checked_mul(3))
            .and_then(|x| x.checked_sub(2))
            .map_or(0, i64::from)
    }

    fn macro_kleisli(x: i32) -> i64 {
        catheo::pipeline! {
            witness: OptionWitness,
            source: x.checked_add(1),
            kleisli: (
                |x: i32| x.checked_mul(3),
                |x: i32| x.checked_sub(2),
            ),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        }
    }

    for x in sweep_elems() {
        let a = naive(x);
        let b = idiomatic(x);
        let c = macro_kleisli(x);
        assert_eq!((a, b), (b, c), "x = {x}");
    }
}

// ─── Gate 2: fold_map ─────────────────────────────────────────────────────

/// A: the naive spelling — every combinator materializes its container.
fn fold_map_naive(input: Vec<i32>) -> u64 {
    let mapped: Vec<u64> = input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(u64::try_from)
        .map(|x| x.unwrap_or(0))
        .collect();
    let mut total = 0_u64;
    for x in mapped {
        total += x;
    }
    total
}

/// B: the idiomatic spelling — one adapter pass, monoid eliminator.
fn fold_map_idiomatic(input: Vec<i32>) -> u64 {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(|x| u64::try_from(x).unwrap_or(0))
        .fold(<u64 as Monoid>::identity(), <u64 as Monoid>::combine)
}

/// C: the macro-flattened output — the exact benchmark pipeline.
fn fold_map_macro(input: Vec<i32>) -> u64 {
    catheo::pipeline! {
        witness: VecWitness,
        source: input,
        map: |x| x.wrapping_mul(3),
        fold_map: |x: i32| u64::try_from(x).unwrap_or(0),
    }
}

#[test]
fn fold_map_three_way_agreement_over_the_sweep() {
    for input in sweep_vecs() {
        let a = fold_map_naive(input.clone());
        let b = fold_map_idiomatic(input.clone());
        let c = fold_map_macro(input.clone());
        assert_eq!((a, b), (b, c), "input {input:?}");
    }
}

#[test]
fn fold_map_allocation_profile_matches_the_idiomatic_chain() {
    let input: Vec<i32> = (0..1_000).collect();
    // The clones happen *outside* the counted sections: they measure the
    // pipeline shapes, not the harness.
    let for_a = input.clone();
    let for_b = input.clone();
    let (a_value, a_count) = counted(|| fold_map_naive(for_a));
    let (b_value, b_count) = counted(|| fold_map_idiomatic(for_b));
    let (c_value, c_count) = counted(|| fold_map_macro(input));
    assert_eq!(a_value, b_value);
    assert_eq!(b_value, c_value);
    assert_eq!(
        c_count, b_count,
        "C allocates exactly like the one-pass idiomatic chain (A = {a_count}, B = {b_count}, C = {c_count})"
    );
    assert!(
        c_count < a_count,
        "C is strictly below the materializing naive form (A = {a_count}, C = {c_count})"
    );
}

#[test]
fn fold_map_pointwise_witnesses_keep_the_run_fold_map_kernel() {
    // Option: one discriminant test, no iterator is ever formed.
    let option_chain = parse_pipeline(quote! {
        witness: OptionWitness,
        source: element,
        bind: kleisli_opt,
        fold_map: i64::from,
    })
    .expect("parses");
    let out = emit(&option_chain).expect("emits").to_string();
    assert!(out.contains("run_bind"), "{out}");
    assert_eq!(out.matches("run_fold_map").count(), 1, "{out}");
    assert!(!out.contains("flat_map"), "{out}");
    assert!(!out.contains("into_iter"), "{out}");
    assert!(!out.contains("Monoid"), "no inline monoid fold: {out}");

    // Result: the same kernel posture (Err contributes the identity).
    let result_chain = parse_pipeline(quote! {
        witness: ResultWitness<GateError>,
        source: element,
        bind: stage_a,
        fold_map: i64::from,
    })
    .expect("parses");
    let out = emit(&result_chain).expect("emits").to_string();
    assert_eq!(out.matches("run_fold_map").count(), 1, "{out}");
    assert!(!out.contains("into_iter"), "{out}");

    // The sequential oracle keeps the kernel at every stage too.
    let sequential = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        map: |x| x.wrapping_mul(3),
        fold_map: |x: i32| u64::try_from(x).unwrap_or(0),
    })
    .expect("parses");
    let out = emit_with_lowering(&sequential, Lowering::Sequential)
        .expect("emits")
        .to_string();
    assert!(out.contains("run_map"), "{out}");
    assert!(out.contains("run_fold_map"), "{out}");
}

fn kleisli_opt(x: i32) -> Option<i64> {
    x.checked_add(1).map(i64::from)
}

/// Runtime agreement for the `Option` `fold_map` chain used by the
/// emission-shape gate above (kernel chain + `run_fold_map`).
#[test]
fn option_fold_map_chain_agrees_with_the_oracle() {
    for x in sweep_elems() {
        let fused: i64 = catheo::pipeline! {
            witness: OptionWitness,
            source: x.checked_add(1),
            bind: kleisli_opt,
            fold_map: i64::from,
        };
        let oracle: i64 = catheo::pipeline_unfused! {
            witness: OptionWitness,
            source: x.checked_add(1),
            bind: kleisli_opt,
            fold_map: i64::from,
        };
        assert_eq!(fused, oracle, "x = {x}");
    }
}

// ─── Gate 3: escape hatches (§4.2) ────────────────────────────────────────

#[test]
fn forced_observing_bind_chains_into_the_fused_pass() {
    let forced = parse_pipeline(quote! {
        witness: VecWitness,
        source: rows,
        map: |row| row.iter().copied().collect::<Vec<i32>>(),
        bind!: force |row: Vec<i32>| row.iter().fold(0_i32, i32::wrapping_add),
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    })
    .expect("parses");
    let out = emit(&forced).expect("emits").to_string();
    assert!(
        !out.contains("run_bind"),
        "force clears the observer barrier: {out}"
    );
    assert!(
        out.contains(". flat_map ("),
        "the observer rides the fused adapter pass: {out}"
    );
    assert_eq!(
        out.matches(". fold (0_i64").count(),
        1,
        "one terminal eliminator (the fold inside the user's closure body is theirs): {out}"
    );

    // And the un-forced spelling is the barrier the flag promises — the
    // contrast that proves the hatch did something.
    let plain = parse_pipeline(quote! {
        witness: VecWitness,
        source: rows,
        map: |row| row.iter().copied().collect::<Vec<i32>>(),
        bind: |row: Vec<i32>| row.iter().fold(0_i32, i32::wrapping_add),
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    })
    .expect("parses");
    let out = emit(&plain).expect("emits").to_string();
    assert!(out.contains("run_bind"), "unforced stays a kernel: {out}");
}

#[test]
fn materialized_bind_stays_a_kernel_and_blocks_absorption() {
    let materialized = parse_pipeline(quote! {
        witness: VecWitness,
        source: elements,
        map: |x| x.wrapping_mul(2),
        bind!: materialize |x| vec![x, x],
        map: |y| y.wrapping_add(1),
        fold: (0_i64, i64::wrapping_add),
    })
    .expect("parses");
    let out = emit(&materialized).expect("emits").to_string();
    assert!(
        out.contains("run_bind"),
        "materialize keeps the kernel: {out}"
    );
    assert!(
        out.contains("run_map"),
        "the prefix map stays sequential behind the barrier: {out}"
    );
    assert!(
        out.contains("into_iter (__catheo_state_1) . map (| y | y . wrapping_add (1))"),
        "the suffix folds over the materialized state: {out}"
    );
}

#[test]
fn ascription_derives_the_builtin_witnesses() {
    let vec_case = parse_pipeline(quote! {
        source: elements as Vec<i32>,
        map: |x| x.wrapping_mul(2),
        collect,
    })
    .expect("parses");
    let out = emit(&vec_case).expect("emits").to_string();
    assert!(
        out.contains(":: catheo :: VecWitness"),
        "Vec derives the Vec witness: {out}"
    );
    assert!(
        out.contains("let __catheo_source : Vec < i32 > = elements ;"),
        "the ascription re-anchors the head expression: {out}"
    );

    let option_case = parse_pipeline(quote! {
        source: element as Option<i32>,
        bind: kleisli_opt,
        collect,
    })
    .expect("parses");
    let out = emit(&option_case).expect("emits").to_string();
    assert!(
        out.contains(":: catheo :: OptionWitness"),
        "Option derives the Option witness: {out}"
    );

    let result_case = parse_pipeline(quote! {
        source: element as Result<i32, GateError>,
        bind: stage_a,
        collect,
    })
    .expect("parses");
    let out = emit(&result_case).expect("emits").to_string();
    assert!(
        out.contains(":: catheo :: ResultWitness < GateError >"),
        "Result<T, E> derives the parameterized witness: {out}"
    );
}

#[test]
fn ascribed_pipelines_agree_with_the_explicit_witness() {
    for input in sweep_vecs() {
        let derived: i64 = catheo::pipeline! {
            source: input.clone() as Vec<i32>,
            map: |x| x.wrapping_mul(3),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let explicit: i64 = catheo::pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(3),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(derived, explicit, "input {input:?}");
    }
}
