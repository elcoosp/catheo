//! Unit tests for the syntax-directed pipeline parser (TDD: written first).
//!
//! Each test builds a `TokenStream` with `quote!`, drives
//! [`parse_pipeline`], and asserts on AST structure or on the spanned error
//! message. Span identity is asserted loosely (we check *that* a span points
//! at the offending keyword, not its exact byte range).

use crate::{Pipeline, StageKind, StageModifier, Terminal, parse_pipeline};
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
        err.contains("exactly once"),
        "error must mention the single-anchor rule: {err}"
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

// ─── v0.2 grammar: witness derivation from `source: expr as Type` ─────────

#[test]
fn vec_ascription_derives_the_vec_witness() {
    let p = parse(quote! {
        source: xs as Vec<i32>,
        map: |x| x * 2,
        collect,
    });
    assert_eq!(
        p.witness.path.to_token_stream().to_string(),
        ":: catheo :: VecWitness"
    );
    assert_eq!(
        p.source.tokens.to_string(),
        "xs",
        "cast head survives alone"
    );
    assert!(p.source.ascription.is_some(), "ascription recorded");
}

#[test]
fn option_ascription_derives_the_option_witness() {
    let p = parse(quote! {
        source: o as Option<i32>,
        bind: |x| wrap(x),
    });
    assert_eq!(
        p.witness.path.to_token_stream().to_string(),
        ":: catheo :: OptionWitness"
    );
}

#[test]
fn result_ascription_derives_the_parameterized_witness() {
    let p = parse(quote! {
        source: r as Result<i32, MyError>,
        bind: |x| step(x),
    });
    assert_eq!(
        p.witness.path.to_token_stream().to_string(),
        ":: catheo :: ResultWitness < MyError >"
    );
}

#[test]
fn qualified_path_ascription_uses_the_last_segment() {
    let p = parse(quote! {
        source: xs as std::vec::Vec<String>,
        collect,
    });
    assert_eq!(
        p.witness.path.to_token_stream().to_string(),
        ":: catheo :: VecWitness"
    );
}

#[test]
fn ascription_strips_the_cast_but_keeps_the_type_for_the_emitter() {
    let p = parse(quote! {
        source: make_items(1, 8) as Vec<u64>,
        fold: (0, |acc, x| acc + x),
    });
    assert_eq!(p.source.tokens.to_string(), "make_items (1 , 8)");
    let ty = p.source.ascription.as_ref().expect("ascription present");
    assert_eq!(ty.to_token_stream().to_string(), "Vec < u64 >");
}

#[test]
fn witness_and_ascription_together_are_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs as Vec<i32>,
        collect,
    });
    assert!(
        err.contains("head-anchored"),
        "double witness anchor must be an error: {err}"
    );
}

#[test]
fn non_derivable_ascription_is_rejected() {
    let err = parse_err(quote! {
        source: xs as MyAlias,
        collect,
    });
    assert!(
        err.contains("cannot derive"),
        "alias ascription must be an error: {err}"
    );
}

#[test]
fn result_ascription_missing_error_type_is_rejected() {
    let err = parse_err(quote! {
        source: r as Result<i32>,
        bind: |x| step(x),
    });
    assert!(
        err.contains("two type arguments"),
        "Result<T> ascription must be an error: {err}"
    );
}

#[test]
fn missing_witness_and_missing_ascription_still_error() {
    let err = parse_err(quote! {
        source: xs,
        map: |x| x,
    });
    assert!(
        err.contains("witness"),
        "must ask for a witness anchor: {err}"
    );
}

#[test]
fn witness_after_source_is_rejected_as_misanchored() {
    let err = parse_err(quote! {
        source: xs,
        witness: VecWitness,
        collect,
    });
    assert!(
        err.to_lowercase().contains("first"),
        "witness must precede source: {err}"
    );
}

// ─── v0.2 grammar: the `kleisli:` entry (§4.3, desugars to nested binds) ──

#[test]
fn kleisli_chain_desugars_to_bind_stages() {
    let p = parse(quote! {
        witness: OptionWitness,
        source: o,
        kleisli: (parse_even, halve),
        fold: (0, |acc, x| acc + x),
    });
    assert_eq!(p.stages.len(), 2, "two morphisms, two binds");
    assert!(
        p.stages.iter().all(|s| s.kind == StageKind::Bind),
        "every desugared stage is a bind"
    );
    assert_eq!(p.stages[0].closure.body.to_string(), "parse_even");
    assert_eq!(p.stages[1].closure.body.to_string(), "halve");
}

#[test]
fn kleisli_accepts_closure_morphisms_and_flags_observers() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        kleisli: (|x| wrap(x), |row| row.iter().collect(), |y| other(y)),
    });
    assert_eq!(p.stages.len(), 3);
    assert!(!p.stages[0].structural_observer);
    assert!(
        p.stages[1].structural_observer,
        "per-morphism observer scan"
    );
    assert!(p.stages[1].fusion_barrier);
    assert!(!p.stages[2].structural_observer);
}

#[test]
fn single_morphism_kleisli_is_rejected() {
    let err = parse_err(quote! {
        witness: OptionWitness,
        source: o,
        kleisli: (f),
    });
    assert!(err.contains("at least two"), "kleisli needs a chain: {err}");
}

#[test]
fn empty_kleisli_is_rejected() {
    let err = parse_err(quote! {
        witness: OptionWitness,
        source: o,
        kleisli: (),
    });
    assert!(err.contains("at least two"), "got: {err}");
}

#[test]
fn kleisli_without_parens_is_rejected() {
    let err = parse_err(quote! {
        witness: OptionWitness,
        source: o,
        kleisli: f,
    });
    assert!(
        err.contains("(morphism, ...)"),
        "error must show the kleisli shape: {err}"
    );
}

#[test]
fn kleisli_after_terminal_is_rejected() {
    let err = parse_err(quote! {
        witness: OptionWitness,
        source: o,
        fold: (0, |acc, x| acc + x),
        kleisli: (f, g),
    });
    assert!(
        err.contains("after the terminal"),
        "terminal ordering applies to kleisli: {err}"
    );
}

#[test]
fn unparenthesized_kleisli_does_not_swallow_the_next_entry() {
    // `kleisli` joined the capture-boundary list with v0.2: a missing comma
    // must surface as a separator error, not as a swallowed closure body.
    let err = parse_err(quote! {
        witness: OptionWitness,
        source: o,
        bind: |x| wrap(x) kleisli: (f, g),
    });
    assert!(err.contains("`,`"), "separator error, got: {err}");
}

// ─── v0.2 grammar: the `fold_map:` terminal (§4.3) ────────────────────────

#[test]
fn fold_map_parses_to_its_terminal() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x * 2,
        fold_map: |x| x.len(),
    });
    match p.terminal {
        Terminal::FoldMap { closure, .. } => {
            assert_eq!(closure.params.len(), 1);
            assert_eq!(closure.params[0].ident, "x");
        }
        other => panic!("expected fold_map terminal, got {other:?}"),
    }
}

#[test]
fn fold_map_accepts_the_callable_form() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        fold_map: word_score,
    });
    match p.terminal {
        Terminal::FoldMap { closure, .. } => {
            assert!(closure.is_callable());
            assert_eq!(closure.body.to_string(), "word_score");
        }
        other => panic!("expected fold_map terminal, got {other:?}"),
    }
}

#[test]
fn fold_map_is_a_terminal_ordering_rule_applies() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fold_map: |x| x.len(),
        map: |y| y + 1,
    });
    assert!(err.contains("after the terminal"), "got: {err}");

    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fold: (0, |acc, x| acc + x),
        fold_map: |x| x.len(),
    });
    assert!(
        err.contains("duplicate terminal"),
        "fold_map counts as a terminal: {err}"
    );
}

// ─── v0.2 grammar: escape hatches (§4.2) ──────────────────────────────────

#[test]
fn bang_force_clears_the_observer_barrier() {
    let p = parse(quote! {
        witness: VecWitness,
        source: rows,
        bind!: force |row| row.iter().collect(),
        fold: (0, |acc, x| acc + x),
    });
    let bind = &p.stages[0];
    assert_eq!(bind.modifier, StageModifier::Force);
    assert!(
        !bind.fusion_barrier,
        "force removes the barrier the observer would raise"
    );
    assert!(!bind.structural_observer);
}

#[test]
fn bang_materialize_forces_the_barrier() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x * 2,
        bind!: materialize |x| wrap(x),
        map: |y| y + 1,
        collect,
    });
    let bind = &p.stages[1];
    assert_eq!(bind.modifier, StageModifier::Materialize);
    assert!(bind.fusion_barrier, "materialize is a hard barrier");
    assert!(
        !p.stages[0].fusion_barrier && !p.stages[2].fusion_barrier,
        "neighbours are untouched"
    );
}

#[test]
fn map_force_is_accepted_and_inert() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        map!: force |x| x * 2,
        collect,
    });
    assert_eq!(p.stages[0].modifier, StageModifier::Force);
    assert!(!p.stages[0].fusion_barrier, "maps were never barriers");
}

#[test]
fn map_materialize_blocks_rule_one_fusion() {
    let p = parse(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x * 2,
        map!: materialize |y| y + 1,
        collect,
    });
    assert!(p.stages[1].fusion_barrier, "materialized map is a barrier");
}

#[test]
fn bang_without_marker_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        bind!: |x| wrap(x),
    });
    assert!(
        err.contains("`force` or `materialize`"),
        "marker is mandatory: {err}"
    );
}

#[test]
fn bang_with_unknown_marker_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        map!: explode |x| x * 2,
    });
    assert!(
        err.contains("`force` or `materialize`"),
        "unknown marker: {err}"
    );
}

#[test]
fn filter_bang_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        filter!: force |x| *x > 0,
        collect,
    });
    assert!(err.contains("absolute"), "rule 3 admits no hatch: {err}");
}

#[test]
fn terminal_bang_is_rejected() {
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        fold_map!: force |x| x.len(),
    });
    assert!(
        err.contains("map") && err.contains("bind"),
        "hatches are stage-only: {err}"
    );
}

#[test]
fn bang_does_not_swallow_the_next_entry() {
    // The capture boundary must also stop at `map!`/`bind!` — otherwise a
    // missing comma would be swallowed into the previous closure body.
    let err = parse_err(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x + 1 bind!: force |y| y,
    });
    assert!(err.contains("`,`"), "separator error, got: {err}");
}
