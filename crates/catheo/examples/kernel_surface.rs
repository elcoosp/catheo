//! The runtime-kernel surface — the §2.2 layer without the macro, runnable.
//!
//! ```text
//! cargo run -p catheo --example kernel_surface
//! ```
//!
//! The README advertises a second consumer surface: the runtime layer used
//! *directly*, no `pipeline!` in sight. This demo drives it end to end —
//! the §2.2 split kernels ([`CategoricalFunctor::run_map`], the v0.6
//! [`CategoricalFilterMap::run_filter_map`],
//! [`CategoricalMonad::run_join`], [`CategoricalTraverse::run_traverse`]),
//! the monoid eliminator into a **user-defined** [`Monoid`], and the
//! standalone `kleisli` composition the README promises. The observable
//! seam is the house posture: `tracing` spans bracket each section and
//! `miette` help text renders the error path — both dev-only, so the
//! shipped crate stays dependency-free.
//!
//! Everything here is the exact API the generated code calls into (§3.4):
//! what the macro lowers to, driven by hand.

use catheo::{
    CategoricalFilterMap, CategoricalFoldMap, CategoricalFunctor, CategoricalMonad,
    CategoricalTraverse, Monoid, OptionWitness, ResultWitness, VecWitness, kleisli,
};
use miette::Diagnostic;
use thiserror::Error;

/// The fallible arrow the traversal and the Kleisli chain land in.
///
/// The miette attributes are the consumer-facing diagnostic: a code for
/// machine handling and help text that names the fix (§6.2 posture — a
/// rejection is never a riddle).
#[derive(Debug, Error, Diagnostic, PartialEq, Eq)]
#[error("`{input}` is out of the kernel domain")]
#[diagnostic(
    code(catheo::out_of_domain),
    help(
        "the kernels forward the first failure unchanged: the traversal \
          short-circuits before forcing later elements (§4.3 v0.3) and the \
          Kleisli chain never reaches the next morphism"
    )
)]
struct OutOfDomain {
    /// The offending input, carried for the diagnostic rendering.
    input: i64,
}

/// A total arrow `i32 -> i64`: widen and double — the functor kernel's
/// operand.
fn widen(x: i32) -> i64 {
    i64::from(x) * 2
}

/// The partial arrow `i32 -> Option<i64>` (§4.3.3): keep the evens, widen.
/// The `Some`/`None` constructors are the arrow's effect, not an
/// observation of the container it is fed.
fn keep_even(x: i32) -> Option<i64> {
    if x % 2 == 0 { Some(i64::from(x)) } else { None }
}

/// A Kleisli morphism `i32 -> Result<i64, OutOfDomain>`: halve, or reject
/// an odd input.
fn checked_half(x: i32) -> Result<i64, OutOfDomain> {
    if x % 2 == 0 {
        Ok(i64::from(x) / 2)
    } else {
        Err(OutOfDomain {
            input: i64::from(x),
        })
    }
}

/// A second Kleisli morphism `i64 -> Result<i64, OutOfDomain>`: succeed by
/// one, or overflow.
fn checked_add_one(x: i64) -> Result<i64, OutOfDomain> {
    x.checked_add(1).ok_or(OutOfDomain { input: x })
}

/// A user-defined `Monoid`: two metrics in one accumulator, combined
/// elementwise — exactly the algebra `fold_map` eliminates into (§4.3
/// v0.2). The identity is the zero record; `combine` is associative
/// because both components are.
#[derive(Debug, PartialEq, Eq)]
struct Metrics {
    /// How many records contributed.
    count: usize,
    /// The sum of the widened values.
    total: i64,
}

impl Monoid for Metrics {
    fn identity() -> Self {
        Self {
            count: <usize as Monoid>::identity(),
            total: <i64 as Monoid>::identity(),
        }
    }

    fn combine(self, other: Self) -> Self {
        Self {
            count: self.count.combine(other.count),
            total: self.total.combine(other.total),
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let input = vec![1_i32, 2, 3, 4, 5, 6];

    // ── The functor kernel, driven by hand (§2.2) ──────────────────────────
    let span = tracing::info_span!("kernel.functor", elements = input.len());
    let _guard = span.enter();
    let widened: Vec<i64> = <VecWitness as CategoricalFunctor>::run_map(input.clone(), widen);
    assert_eq!(widened, vec![2, 4, 6, 8, 10, 12]);
    println!("run_map (the functor arrow): {widened:?}");

    // ── The v0.6 partial arrow as a plain trait (§4.3.3) ───────────────────
    let span = tracing::info_span!("kernel.partial_arrow", elements = input.len());
    let _guard = span.enter();
    let kept: Vec<i64> =
        <VecWitness as CategoricalFilterMap>::run_filter_map(input.clone(), keep_even);
    assert_eq!(kept, vec![2, 4, 6]);
    println!("run_filter_map on Vec (std's filter_map): {kept:?}");

    let pointwise: Option<i64> =
        <OptionWitness as CategoricalFilterMap>::run_filter_map(Some(8_i32), keep_even);
    assert_eq!(pointwise, Some(8));
    println!("run_filter_map on Option (and_then): {pointwise:?}");

    // ── The monadic multiplication and the traversal boundary ──────────────
    let span = tracing::info_span!("kernel.join_and_traverse");
    let _guard = span.enter();
    let nested: Vec<Vec<i32>> = vec![vec![1, 2], vec![3], vec![4, 5]];
    let flat: Vec<i32> = <VecWitness as CategoricalMonad>::run_join(nested);
    assert_eq!(flat, vec![1, 2, 3, 4, 5]);
    println!("run_join (bind(id)): {flat:?}");

    let warm = <ResultWitness<OutOfDomain> as CategoricalTraverse>::run_traverse(
        [2_i32, 4, 6].into_iter(),
        checked_half,
    );
    assert_eq!(warm, Ok(vec![1, 2, 3]));
    println!("run_traverse (warm): {warm:?}");

    // The cold path: the first failing element forwards its error unchanged
    // and later elements are never forced — the kernel is std's
    // short-circuiting `FromIterator` elimination (§4.3 v0.3).
    let cold = <ResultWitness<OutOfDomain> as CategoricalTraverse>::run_traverse(
        [2_i32, 5, 6].into_iter(),
        checked_half,
    );
    match cold {
        Ok(parsed) => println!("run_traverse (cold): {parsed:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("run_traverse (cold) short-circuited:\n{report:?}");
        }
    }

    // ── The standalone Kleisli composition (§4.3 v0.2) ─────────────────────
    let span = tracing::info_span!("kernel.kleisli");
    let _guard = span.enter();
    let chain = kleisli(
        ResultWitness::<OutOfDomain>::new(),
        checked_half,
        checked_add_one,
    );
    let warm: Result<i64, OutOfDomain> = chain(84_i32);
    assert_eq!(warm, Ok(43));
    println!("kleisli chain (warm): {warm:?}");

    // The chain never reaches the second morphism on a failure: the error
    // carries the offending input, rendered with its help text.
    let cold: Result<i64, OutOfDomain> = chain(7_i32);
    match cold {
        Ok(value) => println!("kleisli chain (cold): {value:?}"),
        Err(err) => {
            let report = miette::Report::new(err);
            eprintln!("kleisli chain (cold) rejected:\n{report:?}");
        }
    }

    // ── A user-defined Monoid, the fold_map algebra (§4.3 v0.2) ────────────
    let span = tracing::info_span!("kernel.fold_map", elements = input.len());
    let _guard = span.enter();
    let metrics =
        <VecWitness as CategoricalFoldMap>::run_fold_map(input.clone(), |x: i32| Metrics {
            count: 1,
            total: i64::from(x),
        });
    assert_eq!(
        metrics,
        Metrics {
            count: 6,
            total: 21
        }
    );
    println!("run_fold_map into a user Monoid: {metrics:?}");
}
