//! The §6.2 correctness gate: fused == unfused, on randomized inputs.
//!
//! Every [`pipeline!`] expansion runs the M2 deforestation pass; every
//! [`pipeline_unfused!`] expansion is the M0 sequential oracle (one
//! monomorphic kernel per stage). The architecture's merge gate for M2 is
//! that the two lowerings agree value-for-value for every pipeline shape
//! the grammar admits, per witness, over randomized inputs (§6.2: "fusion
//! rules preserve semantics: property-based tests over `Vec`, `Option`,
//! `Result` witnesses comparing fused and unfused execution on randomized
//! inputs").
//!
//! The inputs are swept deterministically — hand-picked edge shapes plus an
//! xorshift64\* pseudo-random stream, seeded, so failures reproduce —
//! keeping the suite dependency-free (the pattern M1's law proofs
//! established). Pipeline shapes are a fixed matrix: each rule (§3.3 Rules
//! 1–4), each barrier kind, each terminal, and the escape-relevant closure
//! forms (callables, type ascriptions, environment capture) get at least
//! one shape; randomization covers the data axis.

use catheo::{OptionWitness, ResultWitness, VecWitness, pipeline, pipeline_unfused};

/// Deterministic xorshift64\* sweep: reproducible "random" inputs without a
/// third-party property-testing dependency.
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

    fn next_i64(&mut self) -> i64 {
        i64::from(self.next_i32())
    }
}

const XORSHIFT_SEED: u64 = 0xC0FF_EE12_3456_789A;

/// Edge shapes first, then pseudo-random vectors of assorted lengths.
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
        vec![i32::MAX, i32::MIN, 0],
    ];
    for _ in 0..32 {
        cases.push(rng.next_vec(9));
    }
    cases
}

/// Representative single elements for the `Option`/`Result` domains.
fn sweep_elems() -> Vec<i32> {
    let mut rng = XorShift(XORSHIFT_SEED ^ 0xDEAD_BEEF);
    let mut cases = vec![0, 1, -1, 3, 2, i32::MAX / 2];
    for _ in 0..32 {
        cases.push(rng.next_i32());
    }
    cases
}

/// Strings for the map→String→fold shapes (Rule 1 chains of two maps).
fn sweep_strings() -> Vec<Vec<i64>> {
    let mut rng = XorShift(XORSHIFT_SEED ^ 0x5EED_5EED);
    let mut cases = vec![Vec::new(), vec![0], vec![1, 2, 3, 4]];
    for _ in 0..24 {
        cases.push((0..rng.next_u64() % 6).map(|_| rng.next_i64()).collect());
    }
    cases
}

// ─── Vec: Rules 1, 2, 4 and their barrier interactions ──────────────────────

#[test]
fn rule1_adjacent_maps_then_fold_agree() {
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(3),
            map: |y| y.wrapping_add(1),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(3),
            map: |y| y.wrapping_add(1),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn rule1_triple_map_string_chain_agrees() {
    for input in &sweep_strings() {
        let fused: usize = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            map: |x| x.to_string(),
            map: |s| s.len(),
            fold: (0_usize, usize::wrapping_add),
        };
        let unfused: usize = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            map: |x| x.to_string(),
            map: |s| s.len(),
            fold: (0_usize, usize::wrapping_add),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn rule2_map_into_non_observing_bind_agrees() {
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_add(1),
            bind: |x| vec![x, x.wrapping_neg()],
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_add(1),
            bind: |x| vec![x, x.wrapping_neg()],
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn rule2_blocked_by_observing_bind_agrees() {
    for input in &sweep_vecs() {
        // `row.iter()` marks the bind a structural observer: Rule 2 must
        // not compose into it, and Rule 4's absorption must stop at it.
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| vec![x.wrapping_mul(10), x],
            bind: |row| row.iter().fold(Vec::new(), |mut copy, item| {
                copy.push(*item);
                copy
            }),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| vec![x.wrapping_mul(10), x],
            bind: |row| row.iter().fold(Vec::new(), |mut copy, item| {
                copy.push(*item);
                copy
            }),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn rule3_filter_barrier_agrees() {
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(10),
            filter: |x| x % 20 == 0,
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(10),
            filter: |x| x % 20 == 0,
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn rule3_barrier_then_absorbable_suffix_agrees() {
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(10),
            filter: |x| x % 20 == 0,
            map: |y| y + 1,
            bind: |y| vec![y, 0],
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(10),
            filter: |x| x % 20 == 0,
            map: |y| y + 1,
            bind: |y| vec![y, 0],
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn nested_binds_absorb_and_agree() {
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            bind: |x| vec![x, x.wrapping_add(1)],
            bind: |y| vec![y, y.wrapping_neg()],
            fold: (0_i64, |acc, x| acc.wrapping_add(i64::from(x))),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            bind: |x| vec![x, x.wrapping_add(1)],
            bind: |y| vec![y, y.wrapping_neg()],
            fold: (0_i64, |acc, x| acc.wrapping_add(i64::from(x))),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn map_after_bind_never_fuses_but_agrees() {
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            bind: |x| vec![x, x.wrapping_neg()],
            map: |y| y.wrapping_mul(2),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            bind: |x| vec![x, x.wrapping_neg()],
            map: |y| y.wrapping_mul(2),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn rule1_with_collect_terminal_agrees() {
    for input in &sweep_vecs() {
        let fused: Vec<i32> = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            map: |y| y.wrapping_add(1),
            collect,
        };
        let unfused: Vec<i32> = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            map: |y| y.wrapping_add(1),
            collect,
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn impure_fold_falls_back_and_agrees() {
    // `&mut` inside the fold closure flips the §3.4 lexical purity check:
    // the lowering must materialize and use `run_fold`. (A helper carries
    // the `&mut` because closure parameters are bare identifiers.)
    fn accumulate(acc: &mut i64, x: i32) {
        *acc += i64::from(x);
    }
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            fold: (0_i64, |acc, x| {
                let mut total = acc;
                accumulate(&mut total, x);
                total
            }),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x| x.wrapping_mul(2),
            fold: (0_i64, |acc, x| {
                let mut total = acc;
                accumulate(&mut total, x);
                total
            }),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn callable_bind_form_agrees() {
    fn split_into_pair(x: i32) -> Vec<i32> {
        vec![x, x.wrapping_add(1)]
    }
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            bind: split_into_pair,
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            bind: split_into_pair,
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

#[test]
fn environment_capture_survives_fusion() {
    for factor in [1_i64, -3, 7] {
        for input in &sweep_strings() {
            let fused: i64 = pipeline! {
                witness: VecWitness,
                source: input.clone(),
                map: |x| x * factor,
                fold: (0_i64, i64::wrapping_add),
            };
            let unfused: i64 = pipeline_unfused! {
                witness: VecWitness,
                source: input.clone(),
                map: |x| x * factor,
                fold: (0_i64, i64::wrapping_add),
            };
            assert_eq!(fused, unfused, "factor {factor}, input {input:?}");
        }
    }
}

#[test]
fn type_ascribed_closures_agree() {
    for input in &sweep_vecs() {
        let fused: i64 = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            map: |x: i32| x.wrapping_mul(10),
            filter: |x: &i32| *x % 20 == 0,
            fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
        };
        let unfused: i64 = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            map: |x: i32| x.wrapping_mul(10),
            filter: |x: &i32| *x % 20 == 0,
            fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}

// ─── Option: short-circuit semantics through fusion ─────────────────────────

#[test]
fn option_map_bind_fold_agrees() {
    for &e in &sweep_elems() {
        let fused: i32 = pipeline! {
            witness: OptionWitness,
            source: Some(e),
            map: |x| x.wrapping_add(1),
            bind: |x| if x > 2 { Some(x.wrapping_mul(10)) } else { None },
            fold: (0_i32, i32::wrapping_add),
        };
        let unfused: i32 = pipeline_unfused! {
            witness: OptionWitness,
            source: Some(e),
            map: |x| x.wrapping_add(1),
            bind: |x| if x > 2 { Some(x.wrapping_mul(10)) } else { None },
            fold: (0_i32, i32::wrapping_add),
        };
        assert_eq!(fused, unfused, "elem {e}");
    }
}

#[test]
fn option_none_source_agrees() {
    for &e in &sweep_elems() {
        // always None, but typed:
        let fused: i32 = pipeline! {
            witness: OptionWitness,
            source: e.checked_mul(0),
            map: |x: i32| x.wrapping_add(1),
            bind: |x| if x > 2 { Some(x.wrapping_mul(10)) } else { None },
            fold: (0_i32, i32::wrapping_add),
        };
        let unfused: i32 = pipeline_unfused! {
            witness: OptionWitness,
            source: e.checked_mul(0),
            map: |x: i32| x.wrapping_add(1),
            bind: |x| if x > 2 { Some(x.wrapping_mul(10)) } else { None },
            fold: (0_i32, i32::wrapping_add),
        };
        assert_eq!(fused, unfused, "elem {e}");
        assert_eq!(fused, 0, "None leaves the accumulator untouched");
    }
}

// No Option/Result `collect` shape exists to fuse: `FromIterator<i32>` is
// not implemented for `Option<i32>` or `Result<i32, _>` (the std impls
// flatten `Option<A>` / `Result<A, E>` *items*), so the `collect` terminal
// is a `Vec`-witness feature on stable Rust — for the at-most-one
// containers a pipeline ends in `fold` (above). Both lowerings reject the
// combination identically at type check.

// ─── Result: error short-circuit through fusion ─────────────────────────────

#[derive(Debug, PartialEq)]
struct ParseError;

fn ok_input(e: i32) -> Result<i64, ParseError> {
    let doubled = e.checked_mul(2);
    match doubled {
        Some(doubled) => Ok(i64::from(doubled)),
        None => Err(ParseError),
    }
}

fn err_input() -> Result<i64, ParseError> {
    if u8::try_from(0).is_ok() {
        Err(ParseError)
    } else {
        unreachable!("u8 accepts zero")
    }
}

fn digits_input(e: i32) -> Result<Vec<u8>, ParseError> {
    if e == i32::MIN {
        Err(ParseError)
    } else {
        Ok(digits_of(i64::from(e)).unwrap_or_default())
    }
}

fn digits_of(s: i64) -> Result<Vec<u8>, ParseError> {
    s.to_string()
        .trim_start_matches('-')
        .chars()
        .map(|c| {
            c.to_digit(10)
                .map_or(Err(ParseError), |d| u8::try_from(d).map_err(|_| ParseError))
        })
        .collect()
}

#[test]
fn result_map_fold_agrees() {
    for &e in &sweep_elems() {
        let fused: usize = pipeline! {
            witness: ResultWitness<ParseError>,
            source: if e >= 0 { Ok(e) } else { Err(ParseError) },
            map: |x: i32| usize::try_from(x).unwrap_or(0),
            fold: (0_usize, usize::wrapping_add),
        };
        let unfused: usize = pipeline_unfused! {
            witness: ResultWitness<ParseError>,
            source: if e >= 0 { Ok(e) } else { Err(ParseError) },
            map: |x: i32| usize::try_from(x).unwrap_or(0),
            fold: (0_usize, usize::wrapping_add),
        };
        assert_eq!(fused, unfused, "elem {e}");
    }
}

#[test]
fn result_bind_chain_agrees() {
    for &e in &sweep_elems() {
        let fused: usize = pipeline! {
            witness: ResultWitness<ParseError>,
            source: ok_input(e),
            bind: digits_of,
            fold: (0_usize, |acc, digits: Vec<u8>| acc + digits.len()),
        };
        let unfused: usize = pipeline_unfused! {
            witness: ResultWitness<ParseError>,
            source: ok_input(e),
            bind: digits_of,
            fold: (0_usize, |acc, digits: Vec<u8>| acc + digits.len()),
        };
        assert_eq!(fused, unfused, "elem {e}");

        let fused: usize = pipeline! {
            witness: ResultWitness<ParseError>,
            source: err_input(),
            bind: digits_of,
            fold: (0_usize, |acc, digits: Vec<u8>| acc + digits.len()),
        };
        let unfused: usize = pipeline_unfused! {
            witness: ResultWitness<ParseError>,
            source: err_input(),
            bind: digits_of,
            fold: (0_usize, |acc, digits: Vec<u8>| acc + digits.len()),
        };
        assert_eq!(fused, unfused, "elem {e} (Err)");
        assert_eq!(fused, 0, "Err leaves the accumulator untouched");
    }
}

#[test]
fn result_impure_fold_agrees() {
    fn accumulate(acc: &mut usize, digits: &[u8]) {
        *acc += digits.len();
    }
    for &e in &sweep_elems() {
        let fused: usize = pipeline! {
            witness: ResultWitness<ParseError>,
            source: digits_input(e),
            fold: (0_usize, |acc, digits: Vec<u8>| {
                let mut total = acc;
                accumulate(&mut total, &digits);
                total
            }),
        };
        let unfused: usize = pipeline_unfused! {
            witness: ResultWitness<ParseError>,
            source: digits_input(e),
            fold: (0_usize, |acc, digits: Vec<u8>| {
                let mut total = acc;
                accumulate(&mut total, &digits);
                total
            }),
        };
        assert_eq!(fused, unfused, "elem {e}");
    }
}

// ─── Container terminals without a trailing stage ───────────────────────────

#[test]
fn identity_pipeline_agrees() {
    for input in &sweep_vecs() {
        let fused: Vec<i32> = pipeline! {
            witness: VecWitness,
            source: input.clone(),
            collect,
        };
        let unfused: Vec<i32> = pipeline_unfused! {
            witness: VecWitness,
            source: input.clone(),
            collect,
        };
        assert_eq!(fused, unfused, "input {input:?}");
    }
}
