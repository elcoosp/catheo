//! Valid pipeline: filter barrier plus closure type ascriptions.
use catheo::{VecWitness, pipeline};

fn main() {
    let input = vec![1_i32, 2, 3, 4];
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: input,
        map: |x: i32| x * 10,
        filter: |x: &i32| *x % 20 == 0,
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(total, 60);
}
