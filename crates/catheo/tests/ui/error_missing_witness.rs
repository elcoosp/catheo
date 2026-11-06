//! Missing head-anchored witness must be rejected (§4.1).
fn main() {
    let xs = vec![1_i32];
    let _ = catheo::pipeline! {
        source: xs,
        map: |x| x,
    };
}
