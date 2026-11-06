//! Valid pipeline: map → map → fold over `Vec<i32>` (§4.1 headline example).
use catheo::{VecWitness, pipeline};

fn main() {
    let input_vector = vec![1_i32, 2, 3, 4];
    let total: usize = pipeline! {
        witness: VecWitness,
        source: input_vector,
        map: |x| x * 2,
        map: |x| x.to_string(),
        fold: (0_usize, |acc, s| acc + s.len()),
    };
    assert_eq!(total, 4);
}
