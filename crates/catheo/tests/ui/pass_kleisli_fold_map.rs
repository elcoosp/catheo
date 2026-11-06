use catheo::{Monoid, OptionWitness, ResultWitness, VecWitness, pipeline};

fn main() {
    // Kleisli chain over Option, fused and eliminated by a fold.
    let total: i64 = pipeline! {
        witness: OptionWitness,
        source: Some(2_i32),
        kleisli: (
            |x: i32| x.checked_mul(10),
            |x: i32| x.checked_add(3),
            |x: i32| x.checked_mul(2),
        ),
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    };
    assert_eq!(total, 46);

    // Kleisli over Result: the error short-circuits the chain. No
    // terminal: the pipeline evaluates to the last container, the
    // `Result` itself (a `collect` would gather the *unwrapped* value).
    let outcome: Result<i32, String> = pipeline! {
        witness: ResultWitness::<String>,
        source: Ok::<i32, String>(5),
        kleisli: (
            |x: i32| x.checked_mul(2).ok_or_else(|| "overflow".to_string()),
            |x: i32| x.checked_add(1).ok_or_else(|| "overflow".to_string()),
        ),
    };
    assert_eq!(outcome, Ok(11_i32));

    // fold_map: the metric's monoid is the accumulator.
    let words: Vec<String> = vec!["alpha".into(), "be".into(), "gamma".into()];
    let letters: usize = pipeline! {
        witness: VecWitness,
        source: words,
        map: |w: String| w.trim().to_owned(),
        fold_map: |w: String| w.len(),
    };
    assert_eq!(letters, 12); // 5 + 2 + 5

    // fold_map over a tuple monoid: (count, total length).
    let stats: (usize, usize) = pipeline! {
        witness: VecWitness,
        source: vec!["a".to_string(), "bbb".to_string()],
        fold_map: |w: String| (1_usize, w.len()),
    };
    assert_eq!(stats, (2, 4));

    // fold_map over Option and Result witnesses.
    let from_option: usize = pipeline! {
        witness: OptionWitness,
        source: Some("hello".to_string()),
        fold_map: |s: String| s.len(),
    };
    assert_eq!(from_option, 5);
    let from_result: usize = pipeline! {
        witness: ResultWitness::<String>,
        source: Err::<String, String>("no".into()),
        fold_map: |s: String| s.len(),
    };
    assert_eq!(from_result, 0);

    // String and Vec monoids.
    let joined: String = pipeline! {
        witness: VecWitness,
        source: vec!["x".to_string(), "y".to_string()],
        fold_map: |s: String| s,
    };
    assert_eq!(joined, "xy");
    let flattened: Vec<i32> = pipeline! {
        witness: VecWitness,
        source: vec![vec![1, 2], vec![3]],
        fold_map: |v: Vec<i32>| v,
    };
    assert_eq!(flattened, vec![1, 2, 3]);
    assert_eq!(<usize as Monoid>::identity(), 0);
}
