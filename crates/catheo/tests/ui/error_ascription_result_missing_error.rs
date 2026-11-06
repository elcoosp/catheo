use catheo::pipeline;

fn main() {
    let r: Result<i32, String> = Ok(2);
    let _ = pipeline! {
        source: r as Result<i32>,
        bind: |x| step(x),
    };
}
