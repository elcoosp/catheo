use catheo::{VecWitness, pipeline};

fn main() {
    let xs: Vec<i32> = vec![1, 2, 3];
    let _ = pipeline! {
        witness: VecWitness,
        source: xs,
        bind!: |x| wrap(x),
    };
}
