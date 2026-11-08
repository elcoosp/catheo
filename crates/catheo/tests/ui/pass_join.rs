use catheo::{OptionWitness, ResultWitness, VecWitness, pipeline};

// Valid joins compile and run: the §4.3 monadic multiplication, desugared
// to `bind(id)` — one nesting level flattened per stage.
fn main() {
    // A leading join flattens the nested source container.
    let flattened: Vec<i32> = pipeline! {
        witness: VecWitness,
        source: vec![vec![1_i32, 2], vec![3, 4]],
        join,
        collect,
    };
    assert_eq!(flattened, vec![1, 2, 3, 4]);

    // A map prefix feeds the join; a pure fold absorbs both (Rule 4).
    let total: i64 = pipeline! {
        witness: VecWitness,
        source: vec![1_i32, 2, 3],
        map: |x| vec![x, x],
        join,
        fold: (0_i64, |acc: i64, x: i32| acc + i64::from(x)),
    };
    assert_eq!(total, 12);

    // Pointwise witnesses join too: `Option<Option<T>>` and
    // `Result<Result<T, E>, E>` flatten through the same kernel.
    let opt: Option<i32> = pipeline! {
        witness: OptionWitness,
        source: Some(Some(5_i32)),
        join,
    };
    assert_eq!(opt, Some(5));

    let res: Result<i32, String> = pipeline! {
        witness: ResultWitness<String>,
        source: Ok::<Result<i32, String>, String>(Ok(7)),
        join,
    };
    assert_eq!(res, Ok(7));

    // Two joins flatten two nesting levels.
    let twice: Vec<i32> = pipeline! {
        witness: VecWitness,
        source: vec![vec![vec![1_i32, 2], vec![3]], vec![vec![4]]],
        join,
        join,
        collect,
    };
    assert_eq!(twice, vec![1, 2, 3, 4]);
}
