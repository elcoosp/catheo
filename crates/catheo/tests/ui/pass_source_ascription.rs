use catheo::pipeline;

fn main() {
    // Vec<T> derives VecWitness.
    let doubled: Vec<i32> = pipeline! {
        source: vec![1, 2, 3] as Vec<i32>,
        map: |x| x * 2,
        collect,
    };
    assert_eq!(doubled, vec![2, 4, 6]);

    // Option<T> derives OptionWitness.
    let len: usize = pipeline! {
        source: Some("hello".to_string()) as Option<String>,
        fold_map: |s: String| s.len(),
    };
    assert_eq!(len, 5);

    // Result<T, E> derives ResultWitness<E>, fully qualified path included.
    let value: i64 = pipeline! {
        source: Ok::<i32, String>(21) as Result<i32, String>,
        map: |x| i64::from(x) * 2,
        fold: (0_i64, |acc, x| acc + x),
    };
    assert_eq!(value, 42);

    // std::vec::Vec spelling derives too.
    let total: usize = pipeline! {
        source: vec![1_i32, 2] as std::vec::Vec<i32>,
        fold_map: |x: i32| usize::try_from(i64::from(x)).unwrap_or(0),
    };
    assert_eq!(total, 3);
}
