//! Runtime kernels for the v0.2 combinators (§4.3): `fold_map` and
//! Kleisli `>=>`.
//!
//! The §4.3 table defers both to v0.2: `fold_map` is a "fused eliminator;
//! needs its own kernel", Kleisli `>=>` "desugars to nested bind". This
//! file pins the runtime side of both before any grammar work:
//!
//! - [`Monoid`] laws (identity + associativity) over every provided impl;
//! - `run_fold_map` per witness, against the hand-written eliminator;
//! - `kleisli` composition: correctness, and the associativity law that
//!   justifies the pipeline grammar's desugaring to nested binds.
//!
//! These are unit gates, not benchmark-dependent: they run in every
//! `cargo test --workspace`.

use catheo::{
    CategoricalFoldMap, CategoricalMonad, Monoid, OptionWitness, ResultWitness, VecWitness, kleisli,
};

// ─── Monoid laws over the provided impls ─────────────────────────────────

/// Samples per type for the law sweeps.
macro_rules! monoid_laws {
    ($($sample:expr => $ty:ty),* $(,)?) => {$({
        type M = $ty;
        let samples: [M; 3] = [$sample, <$ty as Monoid>::identity(), $sample];
        for a in samples {
            // left identity: e ∘ a == a
            assert_eq!(
                <M as Monoid>::identity().combine(a),
                a,
                "left identity violated"
            );
            // right identity: a ∘ e == a
            assert_eq!(
                a.combine(<M as Monoid>::identity()),
                a,
                "right identity violated"
            );
            for b in samples {
                for c in samples {
                    // associativity: (a ∘ b) ∘ c == a ∘ (b ∘ c)
                    assert_eq!(
                        a.combine(b).combine(c),
                        a.combine(b.combine(c)),
                        "associativity violated"
                    );
                }
            }
        }
    })*};
}

#[test]
fn monoid_laws_hold_for_every_provided_impl() {
    monoid_laws! {
        1_i8 => i8, 2_i16 => i16, 3_i32 => i32, 4_i64 => i64, 5_i128 => i128,
        1_isize => isize,
        1_u8 => u8, 2_u16 => u16, 3_u32 => u32, 4_u64 => u64, 5_u128 => u128,
        1_usize => usize,
        1.5_f32 => f32, 2.5_f64 => f64,
    }
}

#[test]
fn monoid_laws_hold_for_string_vec_option_and_tuples() {
    // String: concatenation.
    monoid_string_laws();
    // Vec<T>: concatenation.
    monoid_vec_laws();
    // Option<T: Monoid>: pointwise combine over the inner monoid.
    monoid_option_laws();
    // Tuples: componentwise combine.
    monoid_tuple_laws();
}

fn monoid_string_laws() {
    type M = String;
    let samples = [String::new(), "a".to_owned(), "bb".to_owned()];
    for a in &samples {
        assert_eq!(
            <M as Monoid>::identity().combine(a.clone()),
            *a,
            "String left identity"
        );
        for b in &samples {
            for c in &samples {
                assert_eq!(
                    a.clone().combine(b.clone()).combine(c.clone()),
                    a.clone().combine(b.clone().combine(c.clone())),
                    "String associativity"
                );
            }
        }
    }
}

fn monoid_vec_laws() {
    type M = Vec<i32>;
    let samples = [vec![], vec![1], vec![2, 3]];
    for a in &samples {
        assert_eq!(
            <M as Monoid>::identity().combine(a.clone()),
            *a,
            "Vec left identity"
        );
        for b in &samples {
            for c in &samples {
                assert_eq!(
                    a.clone().combine(b.clone()).combine(c.clone()),
                    a.clone().combine(b.clone().combine(c.clone())),
                    "Vec associativity"
                );
            }
        }
    }
}

fn monoid_option_laws() {
    type M = Option<usize>;
    let samples = [None, Some(0_usize), Some(3_usize)];
    for a in &samples {
        assert_eq!(
            <M as Monoid>::identity().combine(*a),
            *a,
            "Option left identity"
        );
        for b in &samples {
            for c in &samples {
                assert_eq!(
                    a.combine(*b).combine(*c),
                    a.combine(b.combine(*c)),
                    "Option associativity"
                );
            }
        }
    }
}

fn monoid_tuple_laws() {
    type M = (usize, String);
    let samples = [
        (0_usize, String::new()),
        (2_usize, "x".to_owned()),
        (5_usize, "yy".to_owned()),
    ];
    for a in &samples {
        assert_eq!(
            <M as Monoid>::identity().combine(a.clone()),
            *a,
            "tuple left identity"
        );
        for b in &samples {
            for c in &samples {
                assert_eq!(
                    a.clone().combine(b.clone()).combine(c.clone()),
                    a.clone().combine(b.clone().combine(c.clone())),
                    "tuple associativity"
                );
            }
        }
    }
}

// ─── run_fold_map kernels ─────────────────────────────────────────────────

#[test]
fn vec_run_fold_map_sums_with_zero_intermediates_from_the_kernel() {
    let total: usize =
        VecWitness::run_fold_map(vec![1_i32, 2, 3, 4], |x| usize::try_from(x).unwrap_or(0));
    assert_eq!(total, 10);
    assert_eq!(
        VecWitness::run_fold_map(Vec::<i32>::new(), |x: i32| usize::try_from(x).unwrap_or(0)),
        0,
        "empty container folds to the identity"
    );
}

#[test]
fn option_run_fold_map_skips_none() {
    let total: usize =
        OptionWitness::run_fold_map(Some(3_i32), |x| usize::try_from(x).unwrap_or(0));
    assert_eq!(total, 3);
    assert_eq!(
        OptionWitness::run_fold_map(None::<i32>, |x: i32| usize::try_from(x).unwrap_or(0)),
        0,
        "None contributes the identity"
    );
}

#[test]
fn result_run_fold_map_skips_err() {
    let total: usize = ResultWitness::<String>::run_fold_map(Ok::<i32, String>(4), |x| {
        usize::try_from(x).unwrap_or(0)
    });
    assert_eq!(total, 4);
    assert_eq!(
        ResultWitness::<String>::run_fold_map(Err::<i32, String>("boom".into()), |x: i32| {
            usize::try_from(x).unwrap_or(0)
        }),
        0,
        "Err contributes the identity"
    );
}

#[test]
fn fold_map_grees_with_the_hand_written_eliminator() {
    // Vec: map-then-fold over the owned iterator.
    let xs: Vec<i64> = (-50..50).collect();
    let kernel: i64 = VecWitness::run_fold_map(xs.clone(), |x| x.wrapping_mul(3));
    let hand: i64 = xs.into_iter().map(|x| x.wrapping_mul(3)).sum();
    assert_eq!(kernel, hand);

    // Option / Result: the kernel is the idiomatic elimination — the value
    // on the filled arm, the identity on the empty arm.
    let filled = OptionWitness::run_fold_map(Some(7_i64), |x| x);
    assert_eq!(filled, 7_i64);
    let empty = OptionWitness::run_fold_map(None::<i64>, |x: i64| x);
    assert_eq!(empty, 0_i64);
    let err_arm = ResultWitness::<u8>::run_fold_map(Err::<i64, u8>(9), |x| x);
    assert_eq!(err_arm, 0_i64);
    let ok_arm = ResultWitness::<u8>::run_fold_map(Ok::<i64, u8>(6), |x| x);
    assert_eq!(ok_arm, 6_i64);
}

// ─── Kleisli composition ──────────────────────────────────────────────────

fn parse_even(x: i32) -> Option<i32> {
    if x % 2 == 0 { Some(x) } else { None }
}

fn halve(x: i32) -> Option<i32> {
    if x % 2 == 0 { Some(x / 2) } else { None }
}

#[test]
fn kleisli_composes_two_morphisms() {
    let pipeline = kleisli(OptionWitness, parse_even, halve);
    assert_eq!(pipeline(4), Some(2));
    assert_eq!(pipeline(6), Some(3));
    assert_eq!(pipeline(3), None, "first morphism rejects");
    assert_eq!(pipeline(2), Some(1));
}

#[test]
fn kleisli_agrees_with_nested_bind() {
    // The §4.3 desugaring: `f >=> g == bind(f) then bind(g)`.
    for x in [-4, -2, 0, 1, 2, 3, 4, 8] {
        let composed = kleisli(OptionWitness, parse_even, halve)(x);
        let nested = OptionWitness::run_bind(parse_even(x), halve);
        assert_eq!(composed, nested, "x = {x}");
    }
}

#[test]
fn kleisli_is_associative() {
    // (f >=> g) >=> h == f >=> (g >=> h), the law that makes the grammar's
    // desugaring to a sequence of binds sound for chains of any length.
    // Same law over Vec (kleisli arrows return the witness's own container).
    let keep_positive = |x: i32| if x > 0 { vec![x] } else { vec![] };
    let bump = |x: i32| vec![x + 1];
    let wrap = |x: i32| vec![x, x];
    for x in [-1, 0, 1, 2] {
        let left = kleisli(VecWitness, kleisli(VecWitness, keep_positive, bump), wrap)(x);
        let right = kleisli(VecWitness, keep_positive, kleisli(VecWitness, bump, wrap))(x);
        assert_eq!(left, right, "x = {x}");
    }

    // Same law over Result.
    let checked = |x: i64| x.checked_mul(2).ok_or("overflow");
    let sum_digit = |x: i64| Ok::<i64, &str>(x % 10);
    for x in [0_i64, 3, 1_000_000_000_000] {
        let left = kleisli(
            ResultWitness::<&str>::new(),
            kleisli(ResultWitness::<&str>::new(), checked, sum_digit),
            checked,
        )(x);
        let right = kleisli(
            ResultWitness::<&str>::new(),
            checked,
            kleisli(ResultWitness::<&str>::new(), sum_digit, checked),
        )(x);
        assert_eq!(left, right, "x = {x}");
    }
}

#[test]
fn kleisli_works_over_result_witness_with_error_type() {
    let f = |x: i32| Ok::<i32, String>(x + 1);
    let g = |x: i32| if x > 0 { Ok(x) } else { Err("negative".into()) };
    let h = kleisli(ResultWitness::<String>::new(), f, g);
    assert_eq!(h(0), Ok(1));
    assert_eq!(h(-2), Err("negative".to_owned()));
}
