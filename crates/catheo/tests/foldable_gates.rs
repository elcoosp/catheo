//! The strict `Foldable` layer gates — the eliminator-side unification,
//! deterministically.
//!
//! The §2.4/§4.4 posture extended to the fold side: one trait — `fold`
//! required, `fold_map` provided — over every witness, each kernel
//! delegated to the split layer (§2.2), which remains the single lowering.
//! `fold_map` is the eliminator-side derivation: `fold(m, M::identity(),
//! |acc, x| M::combine(acc, f(x)))` — the fold-side twin of the v0.5
//! `fmap == bind ∘ pure` pin. The gates pin the layer's contract the §6.1
//! way (deterministic, no wall-clock flakiness):
//!
//! 1. the required `fold` and the derived `fold_map` pinned against the
//!    split kernels (`Foldable::fold == run_fold`, `Foldable::fold_map ==
//!    run_fold_map`) per witness over the xorshift sweep (the house
//!    harness of `categorical_laws.rs` and `tests/v05_gates.rs`);
//! 2. the eliminator laws through the trait: `fold(pure(x)) == f(init,
//!    x)` (the pointed singleton folds to one application), the empty
//!    denotations (empty `Vec`, `None`, `Err` all yield `init`), and the
//!    monoidal split (`fold_map` into a pair monoid is the pair of the
//!    component eliminations);
//! 3. three-way value agreement over the sweep, in lockstep with
//!    `benches/foldable.rs`;
//! 4. the allocation profile that says the abstraction is free: the
//!    strict spelling allocates *exactly* like the split-kernel spelling
//!    and exactly like the idiomatic one-pass eliminator — the one-trait
//!    bound adds nothing;
//! 5. the consumer seam: one generic program bounds on `Foldable` alone
//!    and instantiates across every witness, into user `Monoid`s
//!    included.

use catheo::{
    CategoricalFold, CategoricalFoldMap, CategoricalPointed, Foldable, Monad, Monoid,
    OptionWitness, ResultWitness, VecWitness,
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

// ─── The lockstep programs (mirrored verbatim in benches/foldable.rs) ─────

/// C, the strict spelling: the eliminator composed over ONE `Foldable`
/// bound — the "one trait to fold through" surface, monomorphized per
/// witness at the call site. Lockstep with the bench's `vec` universe.
fn total<W: Foldable>(xs: W::Type<i32>) -> i64 {
    W::fold(xs, 0_i64, |acc, x| acc + i64::from(x))
}

/// The parity twin: the same elimination spelled over the split kernel
/// (`CategoricalFold::run_fold`). The allocation gate pins strict ==
/// split to the allocation.
fn total_kernel(xs: Vec<i32>) -> i64 {
    <VecWitness as CategoricalFold>::run_fold(xs, 0_i64, |acc, x| acc + i64::from(x))
}

/// A: the two-pass spelling — a materialized intermediate container, then
/// the fold over it.
fn total_naive(xs: Vec<i32>) -> i64 {
    let widened: Vec<i64> = xs.into_iter().map(i64::from).collect();
    widened.into_iter().sum()
}

/// B: one lazy iterator pass, no intermediate container.
fn total_idiomatic(xs: Vec<i32>) -> i64 {
    xs.into_iter().map(i64::from).sum()
}

/// C, the strict `fold_map`: the monoid eliminator over one `Foldable`
/// bound. Lockstep with the bench's `vec_fold_map` universe.
fn letters<W: Foldable>(xs: W::Type<String>) -> usize {
    W::fold_map(xs, |word: String| word.len())
}

/// The parity twin of `letters`: the split `run_fold_map` kernel.
fn letters_kernel(xs: Vec<String>) -> usize {
    <VecWitness as CategoricalFoldMap>::run_fold_map(xs, |word: String| word.len())
}

/// A: the two-pass `fold_map` — map into a materialized `Vec`, then sum.
fn letters_naive(xs: Vec<String>) -> usize {
    let lengths: Vec<usize> = xs.into_iter().map(|word: String| word.len()).collect();
    lengths.into_iter().sum()
}

/// B: the idiomatic one-pass chain.
fn letters_idiomatic(xs: Vec<String>) -> usize {
    xs.into_iter().map(|word: String| word.len()).sum()
}

// ─── 1. The kernels pinned against the split layer ────────────────────────

#[test]
fn the_required_fold_agrees_with_the_split_kernel() {
    let sweep = sweep_vecs();
    for m in &sweep {
        assert_eq!(
            total::<VecWitness>(m.clone()),
            total_kernel(m.clone()),
            "Foldable::fold == run_fold, Vec, {m:?}"
        );
        let opt = m.first().copied();
        assert_eq!(
            <OptionWitness as Foldable>::fold(opt, 0_i64, |acc, x| acc + i64::from(x)),
            <OptionWitness as CategoricalFold>::run_fold(opt, 0_i64, |acc, x| {
                acc + i64::from(x)
            }),
            "Foldable::fold == run_fold, Option of {m:?}"
        );
        let res: Result<i32, String> = m.first().copied().ok_or_else(|| "empty".to_owned());
        assert_eq!(
            <ResultWitness<String> as Foldable>::fold(res.clone(), 0_i64, |acc, x| {
                acc + i64::from(x)
            }),
            <ResultWitness<String> as CategoricalFold>::run_fold(res, 0_i64, |acc, x| {
                acc + i64::from(x)
            }),
            "Foldable::fold == run_fold, Result of {m:?}"
        );
    }
}

#[test]
fn the_derived_fold_map_agrees_with_the_split_kernel() {
    let words: Vec<Vec<String>> = sweep_vecs()
        .into_iter()
        .map(|m| {
            m.into_iter()
                .map(|x| {
                    if x < 0 {
                        "neg".to_owned()
                    } else {
                        "pos".to_owned()
                    }
                })
                .collect()
        })
        .collect();
    for words in &words {
        assert_eq!(
            letters::<VecWitness>(words.clone()),
            letters_kernel(words.clone()),
            "Foldable::fold_map == run_fold_map, Vec, {words:?}"
        );
        let opt = words.first().cloned();
        assert_eq!(
            <OptionWitness as Foldable>::fold_map(opt.clone(), |word: String| word.len()),
            <OptionWitness as CategoricalFoldMap>::run_fold_map(opt, |word: String| word.len()),
            "Foldable::fold_map == run_fold_map, Option of {words:?}"
        );
        let res: Result<String, String> = words.first().cloned().ok_or_else(|| "empty".to_owned());
        assert_eq!(
            <ResultWitness<String> as Foldable>::fold_map(res.clone(), |word: String| word.len()),
            <ResultWitness<String> as CategoricalFoldMap>::run_fold_map(res, |word: String| word
                .len()),
            "Foldable::fold_map == run_fold_map, Result of {words:?}"
        );
    }
}

// ─── 2. The eliminator laws through the trait ─────────────────────────────

#[test]
fn the_strict_foldable_laws_hold_over_the_sweep() {
    let sweep = sweep_vecs();

    // The pointed singleton folds to one application: `fold(pure(x)) ==
    // f(init, x)` — the eliminator-side twin of the monad left identity.
    for a in [0_i32, 7, -499] {
        let lhs =
            <VecWitness as Foldable>::fold(<VecWitness as Monad>::pure(a), 0_i64, |acc, x| {
                acc + i64::from(x)
            });
        assert_eq!(lhs, i64::from(a), "fold ∘ pure == apply, Vec, a = {a}");
        let lhs = <OptionWitness as Foldable>::fold(
            <OptionWitness as Monad>::pure(a),
            0_i64,
            |acc, x| acc + i64::from(x),
        );
        assert_eq!(lhs, i64::from(a), "fold ∘ pure == apply, Option, a = {a}");
        let lhs = <ResultWitness<String> as Foldable>::fold(
            <ResultWitness<String> as Monad>::pure(a),
            0_i64,
            |acc, x| acc + i64::from(x),
        );
        assert_eq!(lhs, i64::from(a), "fold ∘ pure == apply, Result, a = {a}");
    }

    // The empty denotations: every witness's zero-element shape yields
    // `init` untouched — the identity of the elimination.
    assert_eq!(total::<VecWitness>(Vec::new()), 0, "fold of empty Vec");
    assert_eq!(
        <OptionWitness as Foldable>::fold(None::<i32>, 0_i64, |acc, x| acc + i64::from(x)),
        0,
        "fold of None"
    );
    let err: Result<i32, String> = Err("cold".to_owned());
    assert_eq!(
        <ResultWitness<String> as Foldable>::fold(err, 0_i64, |acc, x| acc + i64::from(x)),
        0,
        "fold of Err forwards init"
    );

    // The monoidal split: `fold_map` into a pair monoid is the pair of
    // the component eliminations — the law that makes the derived
    // `fold_map` kernel sound for every `Monoid`.
    for m in &sweep {
        let pair = <VecWitness as Foldable>::fold_map(m.clone(), |x: i32| {
            (i64::from(x), usize::try_from(x.unsigned_abs()).unwrap_or(0))
        });
        let left = <VecWitness as Foldable>::fold_map(m.clone(), i64::from);
        let right = <VecWitness as Foldable>::fold_map(m.clone(), |x: i32| {
            usize::try_from(x.unsigned_abs()).unwrap_or(0)
        });
        assert_eq!(pair, (left, right), "monoidal split, Vec, {m:?}");
    }

    // The identity contribution: mapping every element to `M::identity()`
    // eliminates to the identity itself — `fold_map` starts there.
    for m in &sweep {
        assert_eq!(
            <VecWitness as Foldable>::fold_map(m.clone(), |_: i32| String::new()),
            String::new(),
            "identity contribution, Vec, {m:?}"
        );
    }

    // The pointed singleton through `fold_map`: `fold_map(pure(x), f) ==
    // f(x)` — identity ∘ f is f (left identity of the monoid).
    for a in [0_i32, 13] {
        let singleton = <VecWitness as CategoricalPointed>::run_pure(a);
        let lhs = <VecWitness as Foldable>::fold_map(singleton, |x: i32| i64::from(x) * 2);
        assert_eq!(lhs, i64::from(a) * 2, "fold_map ∘ pure == apply, a = {a}");
    }
}

// ─── 3. Three-way value agreement over the sweep (bench lockstep) ────────

#[test]
fn the_strict_foldable_three_way_agreement_over_the_sweep() {
    for m in &sweep_vecs() {
        let a = total_naive(m.clone());
        let b = total_idiomatic(m.clone());
        let c = total::<VecWitness>(m.clone());
        assert_eq!(a, b, "A == B, {m:?}");
        assert_eq!(b, c, "B == C, {m:?}");
    }
    let words: Vec<Vec<String>> = sweep_vecs()
        .into_iter()
        .map(|m| {
            m.into_iter()
                .map(|x| x.unsigned_abs().to_string())
                .collect()
        })
        .collect();
    for words in &words {
        let a = letters_naive(words.clone());
        let b = letters_idiomatic(words.clone());
        let c = letters::<VecWitness>(words.clone());
        assert_eq!(a, b, "A == B (fold_map), {words:?}");
        assert_eq!(b, c, "B == C (fold_map), {words:?}");
    }
}

// ─── 4. The allocation profile: the abstraction is free ──────────────────

#[test]
fn the_strict_foldable_allocates_exactly_like_the_split_kernels() {
    let cases: Vec<Vec<i32>> = sweep_vecs().into_iter().filter(|m| !m.is_empty()).collect();
    let mut strict_total: u64 = 0;
    let mut kernel_total: u64 = 0;
    let mut idiomatic_total: u64 = 0;
    for m in &cases {
        let (c_value, c_count) = counted(|| total::<VecWitness>(m.clone()));
        let (k_value, k_count) = counted(|| total_kernel(m.clone()));
        let (_b_value, b_count) = counted(|| total_idiomatic(m.clone()));
        let (_, a_count) = counted(|| total_naive(m.clone()));
        assert_eq!(c_value, k_value, "strict == split-kernel value, {m:?}");
        assert_eq!(
            c_count, k_count,
            "C allocates exactly like the split-kernel spelling — the one-trait \
             bound adds nothing over the kernels (strict = {c_count}, \
             kernels = {k_count}, {m:?})"
        );
        assert_eq!(
            c_count, b_count,
            "the eliminator allocates exactly like the idiomatic one-pass \
             chain — a fold is not the eager bind (C = {c_count}, \
             B = {b_count}, {m:?})"
        );
        assert!(
            a_count > c_count,
            "the two-pass spelling materializes its intermediate (A = {a_count}, \
             C = {c_count}, {m:?})"
        );
        strict_total += c_count;
        kernel_total += k_count;
        idiomatic_total += b_count;
    }
    assert!(strict_total > 0, "the sweep exercises real allocations");
    assert_eq!(strict_total, kernel_total);
    assert_eq!(strict_total, idiomatic_total);
}

#[test]
fn the_pointwise_foldable_allocates_exactly_like_the_idiomatic_chain() {
    let cases: Vec<Option<i32>> = sweep_vecs()
        .into_iter()
        .map(|m| m.first().copied())
        .collect();
    for input in &cases {
        let (c_value, c_count) = counted(|| {
            <OptionWitness as Foldable>::fold(*input, 0_i64, |acc, x| acc + i64::from(x))
        });
        let (b_value, b_count) = counted(|| input.map_or(0, i64::from));
        assert_eq!(c_value, b_value, "Option: strict == idiomatic, {input:?}");
        assert_eq!(
            c_count, b_count,
            "the pointwise eliminator allocates exactly like the idiomatic \
             one-pass chain — one discriminant, no heap (C = {c_count}, \
             B = {b_count}, {input:?})"
        );
    }
    for input in cases.iter().copied() {
        // The Err variant's `String` is heap-carried: clone outside the
        // counted regions so the gate measures the eliminator, not the
        // copy (the v05_gates posture for the pointwise Result arm).
        let res: Result<i32, String> = input.ok_or_else(|| "empty".to_owned());
        let strict_input = res.clone();
        let idiomatic_input = res;
        let (c_value, c_count) = counted(|| {
            <ResultWitness<String> as Foldable>::fold(strict_input, 0_i64, |acc, x| {
                acc + i64::from(x)
            })
        });
        let (b_value, b_count) = counted(|| idiomatic_input.map_or(0, i64::from));
        assert_eq!(c_value, b_value, "Result: strict == idiomatic, {input:?}");
        assert_eq!(
            c_count, b_count,
            "the Result eliminator allocates exactly like the idiomatic chain \
             (C = {c_count}, B = {b_count}, {input:?})"
        );
    }
}

// ─── 5. The consumer seam: one bound, every witness, any Monoid ──────────

/// A user `Monoid` for the seam program: two metrics in one accumulator,
/// combined elementwise — the algebra `fold_map` eliminates into.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct WordStats {
    /// How many records contributed.
    words: usize,
    /// The total letter count.
    letters: usize,
}

impl Monoid for WordStats {
    fn identity() -> Self {
        Self {
            words: <usize as Monoid>::identity(),
            letters: <usize as Monoid>::identity(),
        }
    }

    fn combine(self, other: Self) -> Self {
        Self {
            words: self.words.combine(other.words),
            letters: self.letters.combine(other.letters),
        }
    }
}

#[test]
fn a_generic_program_bounds_on_foldable_alone_across_every_witness() {
    // The same generic body, instantiated per witness — the consumer seam
    // the layer exists for (the §4.4 posture, fold side).
    fn stats_strict<W: Foldable>(xs: W::Type<String>) -> WordStats {
        W::fold_map(xs, |word: String| WordStats {
            words: 1,
            letters: word.len(),
        })
    }

    let words = vec!["alpha".to_owned(), "be".to_owned(), "gamma".to_owned()];
    let stats = |xs: Vec<String>| {
        <VecWitness as Foldable>::fold_map(xs, |word: String| WordStats {
            words: 1,
            letters: word.len(),
        })
    };
    assert_eq!(
        stats(words.clone()),
        WordStats {
            words: 3,
            letters: 12
        },
        "the user Monoid eliminates through the strict layer, Vec"
    );

    assert_eq!(
        stats_strict::<VecWitness>(words.clone()),
        WordStats {
            words: 3,
            letters: 12
        },
        "generic over Vec"
    );
    assert_eq!(
        stats_strict::<OptionWitness>(words.first().cloned()),
        WordStats {
            words: 1,
            letters: 5
        },
        "generic over Option"
    );
    assert_eq!(
        stats_strict::<OptionWitness>(None),
        WordStats::identity(),
        "None contributes the identity"
    );
    let res: Result<String, String> = words.first().cloned().ok_or_else(|| "empty".to_owned());
    assert_eq!(
        stats_strict::<ResultWitness<String>>(res),
        WordStats {
            words: 1,
            letters: 5
        },
        "generic over Result"
    );
    let cold: Result<String, String> = Err("cold".to_owned());
    assert_eq!(
        stats_strict::<ResultWitness<String>>(cold),
        WordStats::identity(),
        "Err contributes the identity (the documented denotation)"
    );
}
