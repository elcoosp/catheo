//! The v0.5 gates — the strict `Monad` layer, deterministically.
//!
//! ARCHITECTURE.md §2.4/§4.4: one trait — `pure` + `bind` — over every
//! witness, each kernel delegated to the split layer (§2.2), which remains
//! the single lowering. The gates pin the layer's contract the §6.1 way
//! (deterministic, no wall-clock flakiness):
//!
//! 1. the monad laws per witness over the xorshift sweep (the house
//!    harness of `categorical_laws.rs`);
//! 2. the derived `fmap` pinned against the split functor kernel
//!    (`fmap == run_map`) and the provided `join` pinned against
//!    `run_join` and the `bind(id)` desugar;
//! 3. three-way value agreement over the sweep, in lockstep with
//!    `benches/monad.rs`;
//! 4. the allocation profile that says the abstraction is free: the
//!    strict spelling allocates *exactly* like the split-kernel spelling,
//!    and the pointwise witnesses allocate exactly like the idiomatic
//!    one-pass chain;
//! 5. the lints-migration contract: `extern crate self` stays the only
//!    declared `extern crate` in the tree (the rationale documented on
//!    `[workspace.lints.rust] unused_extern_crates`).

use catheo::{
    CategoricalFunctor, CategoricalIntoIter, CategoricalMonad, CategoricalPointed, Monad,
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

// ─── The lockstep programs (mirrored verbatim in benches/monad.rs) ────────

/// C, the strict spelling: the whole program composed over ONE `Monad`
/// bound — the "one trait to bound on" surface, monomorphized per
/// witness at the call site.
fn bind_chain_strict<M: Monad>(input: M::Type<i32>) -> M::Type<i64> {
    M::bind(M::bind(input, |x| M::pure(x.wrapping_mul(3))), |x| {
        M::pure(i64::from(x) + 7)
    })
}

/// The parity twin: the same program spelled over the split kernels
/// (`CategoricalMonad::run_bind` + `CategoricalPointed::run_pure`). The
/// allocation gate pins strict == split to the allocation.
fn bind_chain_kernels(input: Vec<i32>) -> Vec<i64> {
    let bound = <VecWitness as CategoricalMonad>::run_bind(input, |x| {
        <VecWitness as CategoricalPointed>::run_pure(x.wrapping_mul(3))
    });
    <VecWitness as CategoricalMonad>::run_bind(bound, |x| {
        <VecWitness as CategoricalPointed>::run_pure(i64::from(x) + 7)
    })
}

/// A: every stage materializes its own container.
fn bind_chain_naive(input: Vec<i32>) -> i64 {
    let step_1: Vec<i32> = input.into_iter().map(|x| x.wrapping_mul(3)).collect();
    let step_2: Vec<i64> = step_1.into_iter().map(|x| i64::from(x) + 7).collect();
    step_2.into_iter().sum()
}

/// B: one lazy iterator pass, no intermediate container.
fn bind_chain_idiomatic(input: Vec<i32>) -> i64 {
    input
        .into_iter()
        .map(|x| x.wrapping_mul(3))
        .map(|x| i64::from(x) + 7)
        .sum()
}

/// C over `Vec`: the strict layer plus the terminal elimination.
fn bind_chain_strict_folded(input: Vec<i32>) -> i64 {
    bind_chain_strict::<VecWitness>(input).into_iter().sum()
}

/// The pointwise program, strict spelling.
fn pointwise_strict<M: Monad>(input: M::Type<i32>) -> M::Type<i64> {
    M::bind(input, |x| M::pure(i64::from(x) * 2))
}

/// The idiomatic pointwise spellings — `Option`'s lazy `map` chain and
/// `Result`'s `map`. No container, no heap: one discriminant test.
fn pointwise_idiomatic_option(input: Option<i32>) -> Option<i64> {
    input.map(i64::from).map(|x| x * 2)
}

fn pointwise_idiomatic_result<E>(input: Result<i32, E>) -> Result<i64, E> {
    input.map(i64::from).map(|x| x * 2)
}

// ─── 1. The monad laws, per witness over the sweep ────────────────────────

#[test]
fn the_strict_monad_laws_hold_over_the_sweep() {
    let sweep = sweep_vecs();

    // Left identity: `bind(pure(a), k) == k(a)` — `Vec`.
    for a in [0_i32, 7, -499] {
        let lhs = <VecWitness as Monad>::bind(<VecWitness as Monad>::pure(a), |x| {
            vec![i64::from(x) + 1, i64::from(x) - 1]
        });
        let rhs = vec![i64::from(a) + 1, i64::from(a) - 1];
        assert_eq!(lhs, rhs, "left identity, Vec, a = {a}");
    }

    // Left identity — `Option` and `Result` (one discriminant each).
    let lhs = <OptionWitness as Monad>::bind(<OptionWitness as Monad>::pure(7_i32), |x| {
        Some(i64::from(x) * 2)
    });
    assert_eq!(lhs, Some(14_i64), "left identity, Option");
    let lhs = <ResultWitness<String> as Monad>::bind(
        <ResultWitness<String> as Monad>::pure(7_i32),
        |x| Ok::<i64, String>(i64::from(x) * 2),
    );
    assert_eq!(lhs, Ok(14_i64), "left identity, Result");

    // Right identity: `bind(m, pure) == m` — over the whole sweep.
    for m in &sweep {
        let lhs = <VecWitness as Monad>::bind(m.clone(), <VecWitness as Monad>::pure);
        assert_eq!(lhs, *m, "right identity, Vec, {m:?}");
        let opt = m.first().copied();
        let lhs = <OptionWitness as Monad>::bind(opt, <OptionWitness as Monad>::pure);
        assert_eq!(lhs, opt, "right identity, Option of {m:?}");
        let res: Result<i32, String> = m.first().copied().ok_or_else(|| "empty".to_owned());
        let expected = res.clone();
        let lhs =
            <ResultWitness<String> as Monad>::bind(res, <ResultWitness<String> as Monad>::pure);
        assert_eq!(lhs, expected, "right identity, Result of {m:?}");
    }

    // Associativity: `bind(bind(m, f), g) == bind(m, |x| bind(f(x), g))`
    // — `Vec` over the sweep; the pointwise witnesses over their heads.
    let f = |x: i32| vec![x.wrapping_mul(2), x.wrapping_mul(2) + 1];
    let g = |x: i32| vec![i64::from(x), i64::from(x) * 10];
    for m in &sweep {
        let lhs = <VecWitness as Monad>::bind(<VecWitness as Monad>::bind(m.clone(), f), g);
        let rhs = <VecWitness as Monad>::bind(m.clone(), |x| <VecWitness as Monad>::bind(f(x), g));
        assert_eq!(lhs, rhs, "associativity, Vec, {m:?}");
    }
    for a in [0_i32, 13, -499] {
        let lhs = <OptionWitness as Monad>::bind(
            <OptionWitness as Monad>::bind(Some(a), |x| Some(x.wrapping_add(1))),
            |x| Some(i64::from(x) * 2),
        );
        let rhs = <OptionWitness as Monad>::bind(Some(a), |x| {
            <OptionWitness as Monad>::bind(Some(x.wrapping_add(1)), |y| {
                <OptionWitness as Monad>::pure(i64::from(y) * 2)
            })
        });
        assert_eq!(lhs, rhs, "associativity, Option, a = {a}");
        let lhs = <ResultWitness<String> as Monad>::bind(
            <ResultWitness<String> as Monad>::bind(Ok(a), |x| Ok::<i32, String>(x.wrapping_add(1))),
            |x| Ok::<i64, String>(i64::from(x) * 2),
        );
        let rhs = <ResultWitness<String> as Monad>::bind(Ok(a), |x| {
            <ResultWitness<String> as Monad>::bind(Ok::<i32, String>(x.wrapping_add(1)), |y| {
                <ResultWitness<String> as Monad>::pure(i64::from(y) * 2)
            })
        });
        assert_eq!(lhs, rhs, "associativity, Result, a = {a}");
    }
}

// ─── 2. The provided kernels pinned against the split layer ──────────────

#[test]
fn the_derived_fmap_agrees_with_the_split_functor_kernel() {
    let sweep = sweep_vecs();
    let f = |x: i32| i64::from(x) * 3 + 1;
    for m in &sweep {
        assert_eq!(
            <VecWitness as Monad>::fmap(m.clone(), f),
            <VecWitness as CategoricalFunctor>::run_map(m.clone(), f),
            "fmap == run_map, Vec, {m:?}"
        );
        let opt = m.first().copied();
        assert_eq!(
            <OptionWitness as Monad>::fmap(opt, f),
            <OptionWitness as CategoricalFunctor>::run_map(opt, f),
            "fmap == run_map, Option of {m:?}"
        );
        let res: Result<i32, String> = m.first().copied().ok_or_else(|| "empty".to_owned());
        assert_eq!(
            <ResultWitness<String> as Monad>::fmap(res.clone(), f),
            <ResultWitness<String> as CategoricalFunctor>::run_map(res, f),
            "fmap == run_map, Result of {m:?}"
        );
    }
}

#[test]
fn the_provided_join_agrees_with_the_split_kernel_and_the_bind_identity() {
    let sweep = sweep_vecs();
    for m in &sweep {
        let nested: Vec<Vec<i32>> = m.iter().map(|&x| vec![x, x]).collect();
        let by_join = <VecWitness as Monad>::join(nested.clone());
        let by_kernel = <VecWitness as CategoricalMonad>::run_join(nested.clone());
        let by_desugar = <VecWitness as CategoricalMonad>::run_bind(nested, |inner| inner);
        assert_eq!(by_join, by_kernel, "join == run_join, {m:?}");
        assert_eq!(by_join, by_desugar, "join == bind(id), {m:?}");
        let expected: Vec<i32> = m.iter().flat_map(|&x| vec![x, x]).collect();
        assert_eq!(
            by_join, expected,
            "join flattens the doubled nesting, {m:?}"
        );

        let opt = m.first().copied();
        let nested_opt = opt.map(|x| Some(x.wrapping_mul(2)));
        let by_join = <OptionWitness as Monad>::join(nested_opt);
        let by_kernel =
            <OptionWitness as CategoricalMonad>::run_join(opt.map(|x| Some(x.wrapping_mul(2))));
        assert_eq!(by_join, by_kernel, "join == run_join, Option of {m:?}");

        let res: Result<i32, String> = m.first().copied().ok_or_else(|| "empty".to_owned());
        let nested_res = res.clone().map(|x| Ok::<i32, String>(x.wrapping_mul(2)));
        let by_join = <ResultWitness<String> as Monad>::join(nested_res);
        let by_kernel = <ResultWitness<String> as CategoricalMonad>::run_join(
            res.map(|x| Ok::<i32, String>(x.wrapping_mul(2))),
        );
        assert_eq!(by_join, by_kernel, "join == run_join, Result of {m:?}");
    }
}

// ─── 3. Three-way value agreement over the sweep (bench lockstep) ────────

#[test]
fn the_strict_layer_three_way_agreement_over_the_sweep() {
    for m in &sweep_vecs() {
        let a = bind_chain_naive(m.clone());
        let b = bind_chain_idiomatic(m.clone());
        let c = bind_chain_strict_folded(m.clone());
        assert_eq!(a, b, "A == B, {m:?}");
        assert_eq!(b, c, "B == C, {m:?}");
    }
}

// ─── 4. The allocation profile: the abstraction is free ──────────────────

#[test]
fn the_strict_vec_chain_allocates_exactly_like_the_split_kernels() {
    let cases: Vec<Vec<i32>> = sweep_vecs().into_iter().filter(|m| !m.is_empty()).collect();
    let mut strict_total: u64 = 0;
    let mut kernel_total: u64 = 0;
    let mut idiomatic_total: u64 = 0;
    for m in &cases {
        let (c_value, c_count) = counted(|| bind_chain_strict_folded(m.clone()));
        let (k_value, k_count) = counted(|| bind_chain_kernels(m.clone()).into_iter().sum());
        let (_, b_count) = counted(|| bind_chain_idiomatic(m.clone()));
        let (_, a_count) = counted(|| bind_chain_naive(m.clone()));
        assert_eq!(c_value, k_value, "strict == split-kernel value, {m:?}");
        assert_eq!(
            c_count, k_count,
            "C allocates exactly like the split-kernel spelling — the one-trait \
             bound adds nothing over the kernels (strict = {c_count}, \
             kernels = {k_count}, {m:?})"
        );
        // The eager-monad honesty gate: each runtime `bind` collects its
        // container and every `pure` allocates its single-element one
        // (§2.3) — so the strict layer allocates MORE than both the naive
        // two-collect spelling and the lazy idiomatic chain. That gap is
        // precisely the gap the macro's fusion pass exists to close; the
        // fused B-parity story lives in `benches/abc.rs`, and the
        // pointwise witnesses (no container to build) hold C == B by gate.
        assert!(
            a_count > 0,
            "A materializes its intermediates (A = {a_count}, {m:?})"
        );
        assert!(
            c_count > a_count,
            "the eager runtime bind collects per stage and pure per element \
             (C = {c_count}, A = {a_count}, {m:?}) — the fusion pass is the \
             compile-time answer, the strict layer documents the eager floor"
        );
        assert!(
            c_count > b_count,
            "the runtime layer is not the fused path (C = {c_count}, \
             B = {b_count}, {m:?})"
        );
        strict_total += c_count;
        kernel_total += k_count;
        idiomatic_total += b_count;
    }
    assert!(strict_total > 0, "the sweep exercises real allocations");
    assert_eq!(strict_total, kernel_total);
    assert!(strict_total > idiomatic_total);
}

#[test]
fn the_pointwise_strict_layer_allocates_exactly_like_the_idiomatic_chain() {
    let cases: Vec<Option<i32>> = sweep_vecs()
        .into_iter()
        .map(|m| m.first().copied())
        .collect();
    for input in &cases {
        let (c_value, c_count) = counted(|| pointwise_strict::<OptionWitness>(*input));
        let (b_value, b_count) = counted(|| pointwise_idiomatic_option(*input));
        assert_eq!(c_value, b_value, "Option: strict == idiomatic, {input:?}");
        assert_eq!(
            c_count, b_count,
            "the pointwise strict chain allocates exactly like the idiomatic \
             one-pass chain — one discriminant, no heap (C = {c_count}, \
             B = {b_count}, {input:?})"
        );
    }
    for input in cases.iter().copied() {
        let res: Result<i32, String> = input.ok_or_else(|| "empty".to_owned());
        let strict_input = res.clone();
        let idiomatic_input = res;
        let (c_value, c_count) =
            counted(|| pointwise_strict::<ResultWitness<String>>(strict_input));
        let (b_value, b_count) = counted(|| pointwise_idiomatic_result::<String>(idiomatic_input));
        assert_eq!(c_value, b_value, "Result: strict == idiomatic, {input:?}");
        assert_eq!(
            c_count, b_count,
            "the Result strict chain allocates exactly like the idiomatic \
             chain (C = {c_count}, B = {b_count}, {input:?})"
        );
    }
}

// ─── 5. The lints-migration contract ─────────────────────────────────────

#[test]
fn the_only_declared_extern_crate_is_the_facade_self_alias() {
    // The `[workspace.lints.rust] unused_extern_crates` allowance carries a
    // rationale tied to exactly one site: the facade's
    // `extern crate self as catheo` (crates/catheo/src/lib.rs), which makes
    // macro-generated `::catheo::…` paths resolve inside the facade's own
    // unit tests and doctests. This gate keeps that rationale honest: the
    // tree declares no other `extern crate`, so the allowance can never
    // silently license a second one.
    let mut sites: Vec<String> = Vec::new();
    let sources: [(&str, &str); 5] = [
        ("crates/catheo/src/lib.rs", "catheo/src/lib.rs"),
        (
            "crates/catheo/src/categorical.rs",
            "catheo/src/categorical.rs",
        ),
        ("crates/catheo/src/monad.rs", "catheo/src/monad.rs"),
        ("crates/catheo/src/witness.rs", "catheo/src/witness.rs"),
        ("crates/catheo/src/hkt.rs", "catheo/src/hkt.rs"),
    ];
    for (relative, label) in sources {
        let source = read_workspace_file(relative);
        for (line_no, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if line.contains("extern crate") {
                sites.push(format!("{label}:{}", line_no + 1));
            }
        }
    }
    assert_eq!(
        sites.len(),
        1,
        "exactly one `extern crate` may exist in the runtime sources (the \
         facade self-alias), found: {sites:?}"
    );
    let lib = read_workspace_file("crates/catheo/src/lib.rs");
    assert!(
        lib.contains("extern crate self as catheo"),
        "the single declared `extern crate` is the facade self-alias"
    );
    assert!(
        !lib.contains("#[expect(unused_extern_crates)]"),
        "the suppression lives in [workspace.lints.rust] with its rationale, \
         not as a site-level attribute"
    );
}

fn read_workspace_file(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative);
    let display = path.display();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{relative} must be readable at {display}: {err}"))
}

// Keep the eliminator import honest: the three-way fold terminal above is
// the container-to-value step the layer composes with; this assertion pins
// that `CategoricalIntoIter` still drives it per witness.
#[test]
fn the_terminal_fold_rides_the_container_iterator() {
    let sweep = sweep_vecs();
    for m in &sweep {
        let via_into_iter: i64 = <VecWitness as CategoricalIntoIter>::into_iter(m.clone())
            .fold(0_i64, |acc, x| acc + i64::from(x));
        let via_fold =
            <VecWitness as catheo::CategoricalFold>::run_fold(m.clone(), 0_i64, |acc, x| {
                acc + i64::from(x)
            });
        assert_eq!(via_into_iter, via_fold, "{m:?}");
    }
}
