// The effect witness must be a spelled built-in pointwise witness: a proc
// macro has no type information, and a custom witness has no `run_traverse`
// kernel (§3.2 conservative-posture analogue for the traversal dispatch).
struct MyEffect;

fn main() {
    let xs = vec![1_i32, 2, 3];
    let _ = catheo::pipeline! {
        witness: VecWitness,
        source: xs,
        traverse: (MyEffect, |x| Some(x)),
        collect,
    };
}
