//! Unit tests for the syntax-directed pipeline parser (TDD: written first).
//!
//! Each test builds a `TokenStream` with `quote!`, drives
//! [`parse_pipeline`], and asserts on AST structure or on the spanned error
//! message. Span identity is asserted loosely (we check *that* a span points
//! at the offending keyword, not its exact byte range).

use crate::{Pipeline, StageKind, Terminal, parse_pipeline};
use quote::{ToTokens, quote};

fn parse(code: proc_macro2::TokenStream) -> Pipeline {
    parse_pipeline(code).expect("pipeline should parse")
}

fn parse_err(code: proc_macro2::TokenStream) -> String {
    parse_pipeline(code)
        .expect_err("pipeline should be rejected")
        .to_string()
}

#[test]
fn minimal_map_fold_pipeline() {
    let p = parse(quote! {
        witness: VecWitness,
        source: input_vector,
        map: |x| x * 2,
        fold: (0i64, |acc, x| acc + i64::from(x)),
    });
    assert_eq!(p.witness.path.to_token_stream().to_string(), "VecWitness");
    assert_eq!(p.source.tokens.to_string(), "input_vector");
    assert_eq!(p.stages.len(), 1);
    assert_eq!(p.stages[0].kind, StageKind::Map);
    assert!(!p.stages[0].fusion_barrier);
    assert!(!p.stages[0].structural_observer);
    assert_eq!(p.stages[0].closure.stage_param().ident, "x");
    match p.terminal {
        Terminal::Fold {
            is_pure, closure, ..
        } => {
            assert!(is_pure, "fold without &mut is pure");
            assert_eq!(closure.params[0].ident, "acc");
        }
        other => panic!("expected fold terminal, got {other:?}"),
    }
}

#[test]
fn stage_kinds_parse_in_order() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x + 1,
        bind: |x| wrap(x),
        filter: |x| *x > 0,
    });
    let kinds: Vec<_> = p.stages.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [StageKind::Map, StageKind::Bind, StageKind::Filter]);
    assert_eq!(p.terminal, Terminal::None);
}

#[test]
fn filter_is_an_absolute_fusion_barrier() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        filter: |x| *x % 2 == 0,
    });
    assert!(p.stages[0].fusion_barrier, "filter always blocks fusion");
}

#[test]
fn observing_bind_blocks_fusion() {
    let p = parse(quote! {
        witness: VecWitness,
        source: rows,
        bind: |row| row.iter().copied().collect(),
    });
    let bind = &p.stages[0];
    assert!(bind.structural_observer, "`.iter` in body must be detected");
    assert!(bind.fusion_barrier, "observing bind must block fusion");
}

#[test]
fn observer_detection_is_lexical_over_the_whole_table() {
    for body in [
        quote! { |s| s.len() },
        quote! { |s| s.is_empty() },
        quote! { |s| Some(parse(s)) },
        quote! { |s| None },
        quote! { |s| Ok(parse(s)) },
        quote! { |s| Err(decode(s)) },
        quote! { |s| if s.len() > 0 { vec![s] } else { vec![] } },
    ] {
        let p = parse(quote! {
            witness: VecWitness,
            source: xs,
            bind: #body,
        });
        assert!(
            p.stages[0].structural_observer,
            "bind body `{}` must be an observer",
            body.to_token_stream()
        );
    }
}

#[test]
fn plain_bind_is_not_an_observer() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        bind: |x| parse_tokens(&x),
    });
    assert!(!p.stages[0].structural_observer);
    assert!(!p.stages[0].fusion_barrier);
}

#[test]
fn nested_container_tokens_do_not_fool_the_scanner() {
    // `.len` sits inside a group; the walker must still find it.
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        bind: |x| { if x[0].is_empty() { vec![x] } else { vec![] } },
    });
    assert!(p.stages[0].structural_observer);
}

#[test]
fn fold_with_mut_accumulator_is_not_pure() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        fold: ((String::new(), 0usize), |acc, x| { let r = &mut acc.1; *r += x.len(); acc }),
    });
    match p.terminal {
        Terminal::Fold { is_pure, .. } => {
            assert!(!is_pure, "&mut in fold body must mark the fold impure");
        }
        other => panic!("expected fold terminal, got {other:?}"),
    }
}

#[test]
fn closure_type_ascription_is_accepted() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        map: |x: i32| x * 2,
    });
    assert!(p.stages[0].closure.stage_param().ty.is_some());
    assert_eq!(p.stages[0].closure.stage_param().ident, "x");
}

#[test]
fn closure_return_type_ascription_is_accepted() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| -> String { x.to_string() },
    });
    assert!(p.stages[0].closure.ret.is_some());
}

#[test]
fn bare_callable_stages_are_accepted() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        bind: parse_tokens,
        fold: (0, add),
    });
    let bind = &p.stages[0];
    assert!(bind.closure.is_callable(), "fn path must be callable form");
    assert_eq!(bind.closure.body.to_string(), "parse_tokens");
    match p.terminal {
        Terminal::Fold { closure, .. } => {
            assert!(closure.is_callable());
            assert_eq!(closure.body.to_string(), "add");
        }
        other => panic!("expected fold terminal, got {other:?}"),
    }
}

#[test]
fn turbofish_path_inside_closure_body_is_not_an_entry_boundary() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        bind: |row| row.iter().copied().collect::<Vec<i32>>(),
        collect,
    });
    let body = p.stages[0].closure.body.to_string();
    assert!(
        body.contains("collect :: < Vec < i32 >>"),
        "the `collect::<…>()` turbofish belongs to the body, got: {body}"
    );
    assert!(matches!(p.terminal, Terminal::Collect(_)));
}

#[test]
fn source_expression_may_contain_top_level_commas_in_groups() {
    let p = parse(quote! {
        witness: VecWitness,
        source: make_items(1, 8),
        fold: (0, |acc, x| acc + x),
    });
    // proc-macro2's Display inserts spaces around punctuation; the token
    // sequence is what matters: `make_items` `(` `1` `,` `8` `)`.
    assert_eq!(p.source.tokens.to_string(), "make_items (1 , 8)");
}

#[test]
fn generic_result_witness_parses_as_a_path() {
    let p = parse(quote! {
        witness: ResultWitness<ParseError>,
        source: raw,
        bind: |s| parse_line(&s),
        fold: (0, |acc, n| acc + n),
    });
    assert_eq!(
        p.witness.path.to_token_stream().to_string(),
        "ResultWitness < ParseError >"
    );
}

#[test]
fn collect_terminal_parses() {
    let p = parse(quote! {
        witness: OptionWitness,
        source: maybe,
        map: |x| x + 1,
        collect,
    });
    assert!(matches!(p.terminal, Terminal::Collect(_)));
}

#[test]
fn trailing_commas_are_allowed_everywhere() {
    parse(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x,
        filter: |x| *x > 0,
        collect,
    });
}

#[test]
fn no_stages_and_no_terminal_is_a_valid_identity() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
    });
    assert!(p.stages.is_empty());
    assert_eq!(p.terminal, Terminal::None);
}

#[test]
fn empty_pipeline_is_rejected_at_the_call_site() {
    let err = parse_err(quote! {});
    assert!(
        err.contains("witness"),
        "error must mention `witness`: {err}"
    );
}

#[test]
fn missing_witness_is_rejected_spanning_the_first_token() {
    let err = parse_err(quote! {
        source: xs,
        map: |x| x,
    });
    assert!(
        err.contains("witness"),
        "error must mention `witness`: {err}"
    );
}

#[test]
fn missing_source_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        map: |x| x,
    });
    assert!(err.contains("source"), "error must mention `source`: {err}");
}

#[test]
fn duplicate_witness_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        witness: OptionWitness,
        source: xs,
    });
    assert!(
        err.contains("duplicate"),
        "error must mention duplication: {err}"
    );
}

#[test]
fn stage_after_a_fold_terminal_is_rejected() {
    // A stage written below the terminal cannot keep its written position:
    // the terminal consumes the container, so emitting the stage first (the
    // only sound lowering) would silently change the user's semantics.
    // §6.2: malformed pipelines are rejected with a spanned error.
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fold: (0, |acc, x| acc + x),
        map: |x| x * 2,
    });
    assert!(
        err.contains("after the terminal"),
        "error must name the terminal-ordering rule: {err}"
    );
    assert!(err.contains("map"), "error must name the offender: {err}");
}

#[test]
fn stage_after_a_collect_terminal_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        collect,
        filter: |x| *x > 0,
    });
    assert!(
        err.contains("after the terminal"),
        "error must name the terminal-ordering rule: {err}"
    );
    assert!(
        err.contains("filter"),
        "error must name the offender: {err}"
    );
}

#[test]
fn duplicate_fold_terminal_is_rejected() {
    // A second terminal silently replacing the first would drop user code
    // without a diagnostic — the same class as the duplicate-witness error.
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fold: (0, |acc, x| acc + x),
        fold: (1, |acc, x| acc * x),
    });
    assert!(
        err.contains("duplicate terminal"),
        "error must name the duplication: {err}"
    );
    assert!(err.contains("fold"), "error must name the offender: {err}");
}

#[test]
fn collect_after_a_fold_terminal_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x * 2,
        fold: (0, |acc, x| acc + x),
        collect,
    });
    assert!(
        err.contains("duplicate terminal"),
        "collect below a fold is a second terminal: {err}"
    );
    assert!(
        err.contains("collect"),
        "error must name the offender: {err}"
    );
}

#[test]
fn unknown_combinator_is_rejected_spanning_the_keyword() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fmap: |x| x,
    });
    assert!(
        err.contains("fmap") && err.contains("map"),
        "error must name the offender and the options: {err}"
    );
}

#[test]
fn zero_parameter_closure_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        map: || 42,
    });
    assert!(
        err.contains("exactly one"),
        "error must explain the arity rule: {err}"
    );
}

#[test]
fn two_parameter_closure_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        map: |x, y| x + y,
    });
    assert!(err.contains("exactly one"), "arity error, got: {err}");
}

#[test]
fn tuple_pattern_closure_is_rejected_with_guidance() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        map: |(a, b)| a + b,
    });
    assert!(
        err.contains("single identifier"),
        "v0.1 restriction must be stated: {err}"
    );
}

#[test]
fn malformed_fold_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fold: |acc, x| acc + x,
    });
    assert!(
        err.contains("(init, closure)"),
        "error must show the fold shape: {err}"
    );
}

#[test]
fn fold_without_closure_part_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fold: (0),
    });
    assert!(err.contains("(init, closure)"), "got: {err}");
}

#[test]
fn stage_without_comma_separator_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x + 1 map: |x| x * 2,
    });
    assert!(err.contains("`,`"), "separator error, got: {err}");
}
