use catheo::pipeline;

fn main() {
    let xs: Vec<MyVec> = Vec::new();
    let _ = pipeline! {
        source: xs as MyVec,
        collect,
    };
}
