use catheo::{VecWitness, pipeline};

fn main() {
    let rows: Vec<Vec<i32>> = vec![vec![1, 2], vec![3, 4]];

    // `bind!: force` fuses past a false-positive structural observer.
    let forced: i64 = pipeline! {
        witness: VecWitness,
        source: rows.clone(),
        map: |row: Vec<i32>| row.iter().copied().sum::<i32>(),
        bind!: force |sum: i32| vec![sum, sum],
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    };
    assert_eq!(forced, 20);

    // `bind!: materialize` forces the intermediate container; semantics
    // stay the oracle's.
    let materialized: i64 = pipeline! {
        witness: VecWitness,
        source: rows,
        bind!: materialize |row: Vec<i32>| row,
        map: |x: i32| x * 2,
        fold: (0_i64, |acc, x| acc + i64::from(x)),
    };
    assert_eq!(materialized, 20);

    // `map!: force` is accepted (inert on maps).
    let xs: Vec<i32> = vec![1, 2, 3];
    let inert: Vec<i32> = pipeline! {
        witness: VecWitness,
        source: xs,
        map!: force |x| x * 2,
        collect,
    };
    assert_eq!(inert, vec![2, 4, 6]);
}
