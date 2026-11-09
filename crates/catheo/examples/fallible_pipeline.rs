//! The traversal boundary over a fallible effect, runnable.
//!
//! ```text
//! cargo run -p catheo --example fallible_pipeline
//! ```
//!
//! A `traverse: (ResultWitness<E>, f)` stage runs a pointwise effect across
//! the container and collects one `Result` around a `Vec` — std's
//! short-circuiting `FromIterator` elimination (§4.3 v0.3). The first
//! failing element forwards its error unchanged and never forces later
//! elements; any pipeline suffix lowers inside the effect guard.

use catheo::{ResultWitness, VecWitness, pipeline};

/// A Kleisli arrow `A -> Result<B, E>`: even elements pass, odd ones fail.
fn parse_even(x: i32) -> Result<i32, String> {
    if x % 2 == 0 {
        Ok(x)
    } else {
        Err(format!("odd element: {x}"))
    }
}

fn main() {
    // Warm path: the prefix map rides the traversal's adapter chain and the
    // `collect` terminal evaluates to the guarded container.
    let warm: Result<Vec<i32>, String> = pipeline! {
        witness: VecWitness,
        source: vec![2_i32, 4, 6, 8],
        map: |x| x.wrapping_mul(5),
        traverse: (ResultWitness::<String>, parse_even),
        collect,
    };
    assert_eq!(warm, Ok(vec![10, 20, 30, 40]));
    println!("warm traversal: {warm:?}");

    // Cold path: the first failing element short-circuits; later elements
    // are never forced, and the original error is carried out unchanged.
    let cold: Result<Vec<i32>, String> = pipeline! {
        witness: VecWitness,
        source: vec![2_i32, 3],
        traverse: (ResultWitness::<String>, parse_even),
        collect,
    };
    assert_eq!(cold, Err("odd element: 3".to_owned()));
    println!("cold traversal: {cold:?}");

    // The suffix lowers inside the effect guard: the fold runs over the
    // collected container only when the traversal succeeded.
    let total: Result<i64, String> = pipeline! {
        witness: VecWitness,
        source: vec![2_i32, 4, 6],
        traverse: (ResultWitness::<String>, parse_even),
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(total, Ok(12));
    println!("guarded fold: {total:?}");

    // Kleisli composition over `Result` (§4.3 v0.2): an `Err` short-circuits
    // the chain without reaching the next morphism.
    let outcome: Result<i32, String> = pipeline! {
        witness: ResultWitness::<String>,
        source: Ok::<i32, String>(21),
        kleisli: (
            |x: i32| x.checked_mul(2).ok_or_else(|| "overflow".to_owned()),
            |x: i32| x.checked_add(1).ok_or_else(|| "overflow".to_owned()),
        ),
    };
    assert_eq!(outcome, Ok(43));
    println!("kleisli chain: {outcome:?}");
}
