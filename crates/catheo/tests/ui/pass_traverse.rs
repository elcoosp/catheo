use catheo::{ResultWitness, VecWitness, pipeline};

// Valid traversals compile and run: a `Result` effect over a `Vec`
// container, with a fused prefix, a fused suffix fold, and the
// short-circuiting error path.
fn parse_even(x: i32) -> Result<i32, String> {
    if x % 2 == 0 {
        Ok(x)
    } else {
        Err(format!("odd: {x}"))
    }
}

fn main() {
    // Warm path: the prefix map rides the traversal chain, `collect`
    // evaluates to the guarded container.
    let doubled: Result<Vec<i32>, String> = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3, 4],
        map: |x| x * 2,
        traverse: (ResultWitness<String>, parse_even),
        collect,
    };
    assert_eq!(doubled, Ok(vec![2, 4, 6, 8]));

    // The first failing element short-circuits the traversal and carries
    // its original error out of the guard.
    let odd: Result<Vec<i32>, String> = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 3],
        traverse: (ResultWitness<String>, parse_even),
        collect,
    };
    assert_eq!(odd, Err("odd: 1".to_owned()));

    // A suffix after the traversal lowers inside the effect guard: the
    // fold runs over the collected container, wrapped by the effect.
    let total: Result<i64, String> = pipeline! {
        witness: VecWitness,
        source: vec![2_i32, 4, 6],
        traverse: (ResultWitness<String>, parse_even),
        map: |x| x.wrapping_mul(10),
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(total, Ok(20 + 40 + 60));
}
