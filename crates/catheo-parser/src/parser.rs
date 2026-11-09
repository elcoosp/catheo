//! The syntax-directed pipeline parser (ARCHITECTURE.md §3.1–§3.2).
//!
//! Design constraints taken from the spec:
//! - Closures are parsed as **raw `TokenStream`** bodies — never
//!   `syn::Expr` — so interpretation cost stays zero for the parts the
//!   compiler does not understand (§5.3). Only the closure *header* is
//!   validated (exactly one identifier parameter).
//! - Keywords are `syn::custom_keyword!` types, avoiding string comparison
//!   in the hot parse path (§3.1).
//! - Barrier decisions are made **at parse time** (§3.2): each `Stage`
//!   carries `fusion_barrier` and `structural_observer` flags computed from
//!   the lexical approximations in [`crate::observer`].
//! - Every error is spanned at the offending user token (§6.2), never inside
//!   macro internals.

use crate::ast::{
    Closure, JOIN_PARAM, Param, Pipeline, Source, Stage, StageKind, StageModifier, Terminal,
    Witness, witness_from_type,
};
use crate::observer::{is_lexically_pure, is_structural_observer};
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote_spanned};
use syn::Token;
use syn::parse::{Parse, ParseStream, Parser};
use syn::spanned::Spanned;

mod kw {
    use syn::custom_keyword;

    custom_keyword!(witness);
    syn::custom_keyword!(source);
    syn::custom_keyword!(map);
    syn::custom_keyword!(bind);
    syn::custom_keyword!(filter);
    syn::custom_keyword!(join);
    syn::custom_keyword!(fold);
    syn::custom_keyword!(collect);
    syn::custom_keyword!(kleisli);
    syn::custom_keyword!(fold_map);
    syn::custom_keyword!(traverse);
    syn::custom_keyword!(force);
    syn::custom_keyword!(materialize);
}

// Aliases: syn's custom_keyword! declares the type under its own name; the
// suffixed aliases keep call sites readable without shadowing local names.

// Entry keywords that terminate an opaque raw-token capture (closure body,
// source expression): `witness`, `source`, `map`, `bind`, `filter`, `fold`,
// `collect`, plus the v0.2 `kleisli` and `fold_map` and the v0.4 `join`. A top-level `ident:`
// sequence naming one of these always starts the next pipeline entry, so a
// missing comma between entries is reported instead of being silently
// swallowed by a greedy capture. A `map!`/`bind!` escape hatch (§4.2)
// starts with the same ident followed by `!`, which the capture boundary
// also honors. The list lives in [`is_boundary_keyword`] as an explicit
// short-circuit chain — no table scan on the hot parse path.

/// Parse a `pipeline! { ... }` token stream into a [`Pipeline`].
///
/// # Errors
/// Returns a [`syn::Error`] spanned at the offending token when the grammar
/// of §4.1 is violated.
pub fn parse_pipeline(tokens: TokenStream) -> syn::Result<Pipeline> {
    syn::parse2(tokens)
}

impl Parse for Pipeline {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        // The witness is head-anchored when written (§4.1) and derived from
        // the source ascription otherwise (§4.2). The derivation decision
        // is taken *after* the entry loop: a `witness:` written below the
        // source is reported as misplaced by the loop, not as a vague
        // "cannot infer" from here.
        let explicit_witness = parse_witness(input)?;
        let source = parse_source(input)?;

        let mut stages = Vec::new();
        let mut terminal = Terminal::None;
        // The terminal consumes the container, so it is only sound as the
        // *last* entry, and there can be only one: a stage written below it
        // would have to be floated ahead of it (silently changing the
        // written semantics), and a second terminal would silently discard
        // the first. Both malformed shapes are rejected here, spanned at
        // the offending keyword (§6.2).
        let mut terminal_seen = false;
        // The traversal re-anchors the pipeline's element flow inside the
        // effect guard, so the grammar admits exactly one boundary (§4.3
        // v0.3): a second `traverse` would re-wrap the already-guarded
        // suffix in an effect the one-witness grammar cannot name.
        let mut traverse_seen = false;

        while !input.is_empty() {
            if let Some(parsed) = parse_entry(input, terminal_seen, traverse_seen, &mut stages)? {
                terminal = parsed;
                terminal_seen = true;
            }
            if stages
                .last()
                .is_some_and(|stage| stage.kind == StageKind::Traverse)
            {
                traverse_seen = true;
            }
            require_separator(input)?;
        }

        // Head-anchor resolution (§4.1 vs §4.2), now that the loop has
        // rejected any misplaced `witness` entry.
        let witness = match explicit_witness {
            Some(witness) => {
                if let Some(ascription) = &source.ascription {
                    return Err(syn::Error::new(
                        ascription.span(),
                        "redundant `as` ascription: the witness is already \
                         head-anchored by the `witness:` entry (§4.1 vs §4.2)",
                    ));
                }
                witness
            }
            None => Witness {
                path: match &source.ascription {
                    Some(ty) => witness_from_type(ty)?,
                    None => {
                        return Err(syn::Error::new(
                            source.span,
                            "cannot infer the witness: name one with \
                             `witness: <Witness>` (§4.1) or ascribe the source \
                             type, e.g. `source: xs as Vec<i32>` (§4.2)",
                        ));
                    }
                },
                span: source.span,
            },
        };

        // The traversal grammar pins the container to `Vec` (§4.3 v0.3):
        // only a multi-element container has a traversal, and the kernel
        // shape (§7 limitation 4) pins the collected inner container to a
        // `Vec`. Pointwise containers are rejected with the traversal's
        // span, which is where the fix belongs.
        if let Some(traverse) = stages
            .iter()
            .find(|stage| stage.kind == StageKind::Traverse)
            && !is_vec_witness(&witness.path)
        {
            return Err(syn::Error::new(
                traverse.span,
                "`traverse` requires a `Vec` container witness (`witness: \
                 VecWitness` or a `Vec` ascription, §4.3 v0.3): only `Vec` \
                 has a multi-element traversal; `Option`/`Result` sources are \
                 pointwise (the §6.1 M4 posture)",
            ));
        }

        Ok(Self {
            witness,
            source,
            stages,
            terminal,
        })
    }
}

/// Spanned diagnostics for `witness:`/`source:` written outside their
/// head-anchor position (§4.1).
fn head_entry_error(span: Span, keyword: &str) -> syn::Error {
    if keyword == "witness" {
        syn::Error::new(
            span,
            "`witness` must appear exactly once, as the head-anchored \
             first entry before `source` (§4.1)",
        )
    } else {
        syn::Error::new(
            span,
            "duplicate `source` entry: a pipeline has exactly one source",
        )
    }
}

/// Spanned diagnostic for a `!` written on an entry that admits no §4.2
/// escape hatch.
fn hatch_not_applicable(span: Span) -> syn::Error {
    syn::Error::new(span, "escape hatches apply to `map` and `bind` stages only")
}

/// Parse the entry at the cursor, pushing any stages onto `stages`.
/// Returns `Some(terminal)` when the entry was a terminal. `terminal_seen`
/// gates the §6.2 ordering rules: nothing after a terminal, and no second
/// terminal. `traverse_seen` gates the §4.3 v0.3 single-boundary rule.
fn parse_entry(
    input: ParseStream<'_>,
    terminal_seen: bool,
    traverse_seen: bool,
    stages: &mut Vec<Stage>,
) -> syn::Result<Option<Terminal>> {
    if input.peek(kw::witness) {
        let kw: kw::witness = input.parse()?;
        return Err(head_entry_error(kw.span(), "witness"));
    }
    if input.peek(kw::source) {
        let kw: kw::source = input.parse()?;
        return Err(head_entry_error(kw.span(), "source"));
    }

    if input.peek(kw::map) {
        let kw: kw::map = input.parse()?;
        if terminal_seen {
            return Err(after_terminal_error(kw.span(), "map"));
        }
        stages.push(parse_hatchable_stage(StageKind::Map, kw.span(), input)?);
    } else if input.peek(kw::bind) {
        let kw: kw::bind = input.parse()?;
        if terminal_seen {
            return Err(after_terminal_error(kw.span(), "bind"));
        }
        stages.push(parse_hatchable_stage(StageKind::Bind, kw.span(), input)?);
    } else if input.peek(kw::filter) {
        let kw: kw::filter = input.parse()?;
        if input.peek(Token![!]) {
            let _: Token![!] = input.parse()?;
            return Err(syn::Error::new(
                kw.span(),
                "`filter!` does not exist: `filter` is an absolute \
                 fusion barrier (§3.3 Rule 3); no escape hatch applies",
            ));
        }
        if terminal_seen {
            return Err(after_terminal_error(kw.span(), "filter"));
        }
        stages.push(parse_stage(StageKind::Filter, kw.span(), input)?);
    } else if input.peek(kw::join) {
        let kw: kw::join = input.parse()?;
        if input.peek(Token![!]) {
            let _: Token![!] = input.parse()?;
            return Err(hatch_not_applicable(kw.span()));
        }
        if terminal_seen {
            return Err(after_terminal_error(kw.span(), "join"));
        }
        stages.push(parse_join(kw.span(), input)?);
    } else if input.peek(kw::kleisli) {
        let kw: kw::kleisli = input.parse()?;
        if input.peek(Token![!]) {
            let _: Token![!] = input.parse()?;
            return Err(hatch_not_applicable(kw.span()));
        }
        if terminal_seen {
            return Err(after_terminal_error(kw.span(), "kleisli"));
        }
        stages.extend(parse_kleisli(kw.span(), input)?);
    } else if input.peek(kw::traverse) {
        let kw: kw::traverse = input.parse()?;
        if input.peek(Token![!]) {
            let _: Token![!] = input.parse()?;
            return Err(hatch_not_applicable(kw.span()));
        }
        if terminal_seen {
            return Err(after_terminal_error(kw.span(), "traverse"));
        }
        if traverse_seen {
            return Err(syn::Error::new(
                kw.span(),
                "duplicate `traverse` entry: a pipeline has exactly one \
                 traversal boundary (§4.3 v0.3)",
            ));
        }
        stages.push(parse_traverse(kw.span(), input)?);
    } else if input.peek(kw::fold) {
        let kw: kw::fold = input.parse()?;
        if terminal_seen {
            return Err(duplicate_terminal_error(kw.span(), "fold"));
        }
        return Ok(Some(parse_fold(kw.span(), input)?));
    } else if input.peek(kw::fold_map) {
        let kw: kw::fold_map = input.parse()?;
        if input.peek(Token![!]) {
            let _: Token![!] = input.parse()?;
            return Err(hatch_not_applicable(kw.span()));
        }
        if terminal_seen {
            return Err(duplicate_terminal_error(kw.span(), "fold_map"));
        }
        return Ok(Some(parse_fold_map(kw.span(), input)?));
    } else if input.peek(kw::collect) {
        let kw: kw::collect = input.parse()?;
        if terminal_seen {
            return Err(duplicate_terminal_error(kw.span(), "collect"));
        }
        return Ok(Some(Terminal::Collect(kw.span())));
    } else {
        return Err(unknown_entry_error(input));
    }
    Ok(None)
}

/// Spanned diagnostic for a stage written below the terminal (§6.2).
fn after_terminal_error(span: Span, keyword: &str) -> syn::Error {
    syn::Error::new(
        span,
        format!(
            "`{keyword}` after the terminal: `fold`/`collect` must be the \
             last entry of a pipeline"
        ),
    )
}

/// Spanned diagnostic for a second terminal entry (§6.2).
fn duplicate_terminal_error(span: Span, keyword: &str) -> syn::Error {
    syn::Error::new(
        span,
        format!(
            "duplicate terminal entry: a pipeline has exactly one terminal, \
             and `{keyword}` must be its last entry"
        ),
    )
}

/// Span of the next token in the stream, for spanned custom errors.
fn next_span(input: ParseStream<'_>) -> Span {
    input
        .cursor()
        .token_tree()
        .map_or_else(Span::call_site, |(tt, _)| tt.span())
}

/// §4.1 head anchor: `witness: <Path>,` — the *optional* first entry.
///
/// Since v0.2 a pipeline may omit it and derive the witness from a
/// `source: expr as Type` ascription (§4.2); the caller decides. A witness
/// written anywhere but here is rejected by the entry loop (§4.1 keeps it
/// head-anchored).
fn parse_witness(input: ParseStream<'_>) -> syn::Result<Option<Witness>> {
    if input.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "empty pipeline: expected `witness: <Witness>` or a typed \
             `source: expr as Type` as the head anchor",
        ));
    }
    if !input.peek(kw::witness) {
        // Not written: either `source:` follows (fine, derivation) or
        // something else does (parse_source reports the exact problem).
        return Ok(None);
    }
    let kw: kw::witness = input.parse()?;
    let _: Token![:] = input.parse()?;
    let path: syn::Path = input.parse()?;
    require_separator(input)?;
    Ok(Some(Witness {
        span: kw.span(),
        path,
    }))
}

/// §4.1 `source: <expr>,` — required entry; expression captured raw.
///
/// Since v0.2 the expression may carry a §4.2 ascription:
/// `source: expr as Type`. The `as Type` suffix is parsed out — the
/// recorded tokens are the head expression alone — and the type is kept in
/// [`Source::ascription`] for witness derivation and the emitter's type
/// annotation. The type after a top-level `as` is parsed by `syn` itself
/// (angle-bracketed generics like `Result<T, E>` are not groups at token
/// level, so raw capture alone cannot span their commas); only a
/// *top-level* `as` splits — one nested inside parens or brackets is part
/// of the expression.
fn parse_source(input: ParseStream<'_>) -> syn::Result<Source> {
    if input.peek(kw::witness) {
        let kw: kw::witness = input.parse()?;
        return Err(syn::Error::new(
            kw.span(),
            "`witness` must appear exactly once, as the head-anchored \
             first entry before `source` (§4.1)",
        ));
    }
    if !input.peek(kw::source) {
        return Err(syn::Error::new(
            next_span(input),
            "expected `source: <expression>` after the witness",
        ));
    }
    let kw: kw::source = input.parse()?;
    let _: Token![:] = input.parse()?;
    let (tokens, stopped_at_as) = capture_raw_source(input)?;
    if tokens.is_empty() {
        return Err(syn::Error::new(
            kw.span(),
            "expected an expression after `source:`",
        ));
    }
    let span = tokens
        .clone()
        .into_iter()
        .next()
        .map_or_else(Span::call_site, |tt| tt.span());
    let ascription = if stopped_at_as {
        let ty: syn::Type = input.parse().map_err(|err| {
            syn::Error::new(
                span,
                format!("malformed type after `as` in the source ascription: {err}"),
            )
        })?;
        Some(ty)
    } else {
        None
    };
    require_separator(input)?;
    Ok(Source {
        tokens,
        span,
        ascription,
    })
}

/// [`capture_raw`] for the source entry: also stops at a top-level `as`
/// (the §4.2 ascription split point) and reports whether it did, so the
/// caller can hand the following tokens to a `syn::Type` parse.
fn capture_raw_source(input: ParseStream<'_>) -> syn::Result<(TokenStream, bool)> {
    capture_raw_impl(input, true)
}

/// The §4.2 bang form `map!: force|materialize …` vs the plain stage:
/// the `!` lookahead runs only once the keyword peek has already
/// succeeded, so plain pipelines never pay for it. The `!` itself is
/// *not* consumed here — [`parse_bang_stage`] owns it.
fn parse_hatchable_stage(
    kind: StageKind,
    kw_span: Span,
    input: ParseStream<'_>,
) -> syn::Result<Stage> {
    if input.peek(Token![!]) {
        parse_bang_stage(kind, kw_span, input)
    } else {
        parse_stage(kind, kw_span, input)
    }
}

/// `map:` / `bind:` / `filter:` stage with parse-time barrier annotation.
fn parse_stage(kind: StageKind, kw_span: Span, input: ParseStream<'_>) -> syn::Result<Stage> {
    let _: Token![:] = input.parse()?;
    let closure: Closure = input.parse()?;
    let structural_observer = kind == StageKind::Bind && is_structural_observer(&closure.body);
    let fusion_barrier = kind == StageKind::Filter || structural_observer;
    Ok(Stage {
        kind,
        closure,
        span: kw_span,
        fusion_barrier,
        structural_observer,
        modifier: StageModifier::None,
        effect: None,
    })
}

/// §4.2 escape hatch: `map!: force|materialize <callable>` / the same for
/// `bind!`.
///
/// - `force` suppresses the structural-observer flag, so the stage fuses
///   even though its body looks like a container-shape observer. Documented
///   as unsound if the closure really observes structure. On a `map` it is
///   accepted but inert (maps are unconditional fusion targets).
/// - `materialize` makes the stage a hard barrier: nothing fuses into it
///   and nothing absorbs over it, so the lowering materializes its
///   intermediate container through its kernel call.
fn parse_bang_stage(kind: StageKind, kw_span: Span, input: ParseStream<'_>) -> syn::Result<Stage> {
    let _: Token![!] = input.parse()?;
    let _: Token![:] = input.parse()?;
    let marker = if input.peek(kw::force) {
        let _: kw::force = input.parse()?;
        StageModifier::Force
    } else if input.peek(kw::materialize) {
        let _: kw::materialize = input.parse()?;
        StageModifier::Materialize
    } else {
        return Err(syn::Error::new(
            next_span(input),
            "expected `force` or `materialize` after the escape-hatch keyword, \
             e.g. `bind!: force |row| row.iter().collect()`",
        ));
    };
    let closure: Closure = input.parse()?;
    let structural_observer = match marker {
        // `force` exists to silence exactly this flag (§4.2).
        StageModifier::Force => false,
        StageModifier::Materialize | StageModifier::None => {
            kind == StageKind::Bind && is_structural_observer(&closure.body)
        }
    };
    let fusion_barrier = match marker {
        StageModifier::Force => false,
        StageModifier::Materialize => true,
        StageModifier::None => unreachable!(),
    };
    Ok(Stage {
        kind,
        closure,
        span: kw_span,
        fusion_barrier,
        structural_observer,
        modifier: marker,
        effect: None,
    })
}

/// §4.3 `join` — the monadic multiplication, desugared at parse time into
/// one [`StageKind::Bind`] stage whose closure is the hygienic identity.
///
/// This is the §4.3 letter ("join — desugars to `bind(id)`") with the same
/// posture as [`parse_kleisli`]: the desugared stage is ordinary, so the
/// fusion pass needs no new rule. Rules 1–2 apply across it (a preceding
/// `map` absorbs into the identity — functor composition followed by the
/// weakly-cartesian flattening), Rule 4 absorbs over it, and the §3.2
/// rescan still re-flags the product as an observer when an absorbed body
/// mentions container-shape tokens. The runtime kernel is
/// `CategoricalMonad::run_join`, itself a provided `run_bind(id)`.
///
/// The keyword is bare: the desugar has no user-supplied callable, so any
/// argument after the keyword is rejected, spanned at the offending token.
fn parse_join(kw_span: Span, input: ParseStream<'_>) -> syn::Result<Stage> {
    if !input.is_empty() && !input.peek(Token![,]) {
        return Err(syn::Error::new(
            next_span(input),
            "`join` takes no argument: it lowers to `bind(id)` (§4.3), so \
             write the bare keyword, e.g. `join` after a nested stage",
        ));
    }
    let ident = format_ident!("{}", JOIN_PARAM, span = kw_span);
    Ok(Stage {
        kind: StageKind::Bind,
        closure: Closure {
            params: vec![Param {
                ident: ident.clone(),
                ty: None,
            }],
            ret: None,
            body: quote_spanned! {kw_span=> #ident},
            span: kw_span,
        },
        span: kw_span,
        fusion_barrier: false,
        structural_observer: false,
        modifier: StageModifier::None,
        effect: None,
    })
}

/// §4.3 v0.2 `kleisli: (m0, m1, ...)`: a Kleisli chain `m0 >=> m1 >=> ...`,
/// desugared at parse time into one [`StageKind::Bind`] stage per morphism.
///
/// Desugaring here is the fusion-pass extension in miniature: the desugared
/// binds are ordinary stages, so Rules 1–2 apply across the chain boundary
/// (a preceding `map` fuses into the first non-observing morphism) and a
/// terminal's Rule 4 absorption sweeps the whole non-observing suffix. Each
/// desugared stage is spanned at its own morphism, so diagnostics point at
/// the exact member of the chain (§3.4).
fn parse_kleisli(kw_span: Span, input: ParseStream<'_>) -> syn::Result<Vec<Stage>> {
    let _: Token![:] = input.parse()?;
    let group_span;
    let content = {
        let tt: TokenTree = input
            .parse()
            .map_err(|_| syn::Error::new(kw_span, "expected `kleisli: (morphism, ...)`"))?;
        match tt {
            TokenTree::Group(ref g) if g.delimiter() == proc_macro2::Delimiter::Parenthesis => {
                group_span = g.span();
                g.stream()
            }
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "expected `kleisli: (morphism, ...)`, \
                     e.g. `kleisli: (parse_even, halve)`",
                ));
            }
        }
    };

    let segments = split_top_level_commas(&content);
    // One trailing comma is tuple-style and accepted; an empty segment
    // anywhere else is a syntax error.
    let segments = match segments.split_last() {
        Some((last, head)) if last.is_empty() => head,
        _ => &segments[..],
    };
    if segments.len() < 2 || segments.iter().any(proc_macro2::TokenStream::is_empty) {
        return Err(syn::Error::new(
            group_span,
            "expected `kleisli: (morphism, ...)` with at least two Kleisli \
             morphisms, e.g. `kleisli: (parse_even, halve)`",
        ));
    }
    let morphism_parser = |input: ParseStream<'_>| Closure::parse_with_arity(input, 1);
    let mut stages = Vec::with_capacity(segments.len());
    for segment in segments {
        let closure: Closure = morphism_parser.parse2(segment.clone()).map_err(|err| {
            syn::Error::new(
                group_span,
                format!("expected a Kleisli morphism `A -> Witness::Type<B>` ({err})"),
            )
        })?;
        let span = closure.span;
        let structural_observer = is_structural_observer(&closure.body);
        stages.push(Stage {
            kind: StageKind::Bind,
            closure,
            span,
            fusion_barrier: structural_observer,
            structural_observer,
            modifier: StageModifier::None,
            effect: None,
        });
    }
    Ok(stages)
}

/// §4.3 v0.3 `traverse: (G, f)` — the traversal boundary stage.
///
/// `G` must spell one of the built-in pointwise witnesses (`OptionWitness`,
/// `ResultWitness<E>`): the effect is what the arrow `f: A -> G<B>` lands
/// in, and the spelling dispatch decides both the kernel impl and the
/// guard's constructors — the same conservative posture as the codegen's
/// pointwise dispatch (§6.1 M4). `f` is a one-parameter closure or bare
/// callable, exactly like a stage.
///
/// The stage is a *boundary*: `fusion_barrier` is set so Rules 1–2 never
/// merge a neighbor into it (the deforestation pass treats it like a
/// filter), while codegen still chains the absorbable prefix into the
/// traversal's iterator on the lowering side.
fn parse_traverse(kw_span: Span, input: ParseStream<'_>) -> syn::Result<Stage> {
    const TUPLE_FORM: &str = "expected `traverse: (EffectWitness, f)`, e.g. `traverse: \
         (ResultWitness<MyError>, parse_one)`";
    let _: Token![:] = input.parse()?;
    let group_span;
    let content = {
        let tt: TokenTree = input
            .parse()
            .map_err(|_| syn::Error::new(kw_span, TUPLE_FORM))?;
        match tt {
            TokenTree::Group(ref g) if g.delimiter() == proc_macro2::Delimiter::Parenthesis => {
                group_span = g.span();
                g.stream()
            }
            other => {
                return Err(syn::Error::new(other.span(), TUPLE_FORM));
            }
        }
    };

    let segments = split_top_level_commas(&content);
    // One trailing comma is tuple-style and accepted; an empty segment
    // anywhere else is a syntax error.
    let segments = match segments.split_last() {
        Some((last, head)) if last.is_empty() => head,
        _ => &segments[..],
    };
    if segments.len() != 2 || segments.iter().any(proc_macro2::TokenStream::is_empty) {
        return Err(syn::Error::new(group_span, TUPLE_FORM));
    }
    let effect: syn::Path =
        syn::parse2(segments[0].clone()).map_err(|_| syn::Error::new(group_span, TUPLE_FORM))?;
    validate_traverse_effect(&effect)?;
    let closure_parser = |input: ParseStream<'_>| Closure::parse_with_arity(input, 1);
    let closure: Closure = closure_parser.parse2(segments[1].clone()).map_err(|err| {
        syn::Error::new(
            group_span,
            format!("expected a traversal arrow `A -> G<B>` ({err})"),
        )
    })?;
    Ok(Stage {
        kind: StageKind::Traverse,
        closure,
        span: kw_span,
        fusion_barrier: true,
        structural_observer: false,
        modifier: StageModifier::None,
        effect: Some(effect),
    })
}

/// Validate that the spelled effect witness is one of the built-in
/// pointwise witnesses (§4.3 v0.3): `OptionWitness` or `ResultWitness<E>`,
/// matched by the *last* path segment (the spelling posture of
/// [`crate::ast::witness_from_type`] and the codegen's pointwise dispatch).
///
/// # Errors
/// A spanned [`syn::Error`] at the effect path when it names no built-in
/// pointwise witness.
fn validate_traverse_effect(effect: &syn::Path) -> syn::Result<()> {
    let Some(segment) = effect.segments.last() else {
        return Err(syn::Error::new(
            effect.span(),
            "the `traverse` effect witness cannot be an empty path",
        ));
    };
    match segment.ident.to_string().as_str() {
        "OptionWitness" | "ResultWitness" => Ok(()),
        "VecWitness" => Err(syn::Error::new(
            effect.span(),
            "`Vec` is not a pointwise effect: a `Vec` traversal is cartesian \
             (each element expands to a container, the results combine \
             combinatorially); use `bind:` for flattening stages",
        )),
        other => Err(syn::Error::new(
            effect.span(),
            format!(
                "the `traverse` effect witness must be spelled `OptionWitness` \
                 or `ResultWitness<E>` (§4.3 v0.3), found `{other}`"
            ),
        )),
    }
}

/// `true` when the witness path's last segment spells the `Vec` witness —
/// the spelling posture shared with [`crate::ast::witness_from_type`] and
/// the codegen's pointwise dispatch.
fn is_vec_witness(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "VecWitness")
}

/// §4.3 v0.2 `fold_map: <callable>` — the monoid-valued eliminator.
///
/// No `init` and no purity question: the accumulator is the metric's
/// monoid identity, never user code, so Rule 4 absorption is always sound.
fn parse_fold_map(kw_span: Span, input: ParseStream<'_>) -> syn::Result<Terminal> {
    let _: Token![:] = input.parse()?;
    let closure: Closure = input.parse().map_err(|err| {
        syn::Error::new(
            kw_span,
            format!(
                "expected `fold_map: <Fn(T) -> M>` with `M: Monoid`, \
                 e.g. `fold_map: |x| x.len()` ({err})"
            ),
        )
    })?;
    Ok(Terminal::FoldMap {
        closure: Box::new(closure),
        span: kw_span,
    })
}

/// §3.1 `fold: (init, closure)` — the universal eliminator.
fn parse_fold(kw_span: Span, input: ParseStream<'_>) -> syn::Result<Terminal> {
    let _: Token![:] = input.parse()?;
    let group_span;
    let content = {
        let tt: TokenTree = input
            .parse()
            .map_err(|_| syn::Error::new(kw_span, "expected `fold: (init, closure)`"))?;
        match tt {
            TokenTree::Group(ref g) if g.delimiter() == proc_macro2::Delimiter::Parenthesis => {
                group_span = g.span();
                g.stream()
            }
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "expected `fold: (init, closure)`, e.g. `fold: (0, |acc, x| acc + x)`",
                ));
            }
        }
    };

    let (init, rest) = split_at_top_level_comma(&content);
    if init.is_empty() {
        return Err(syn::Error::new(
            group_span,
            "expected `fold: (init, closure)`",
        ));
    }
    let fold_parser = |input: ParseStream<'_>| Closure::parse_with_arity(input, 2);
    let closure: Closure = fold_parser.parse2(rest).map_err(|err| {
        syn::Error::new(
            group_span,
            format!(
                "expected `fold: (init, closure)` with a two-parameter `Fn(A, T) -> A` closure ({err})"
            ),
        )
    })?;
    let is_pure = is_lexically_pure(&closure.body);
    Ok(Terminal::Fold {
        init,
        closure: Box::new(closure),
        span: kw_span,
        is_pure,
    })
}

/// Comma between entries is mandatory unless the pipeline ends here.
fn require_separator(input: ParseStream<'_>) -> syn::Result<()> {
    if input.is_empty() {
        return Ok(());
    }
    if !input.peek(Token![,]) {
        return Err(syn::Error::new(
            next_span(input),
            "expected `,` between pipeline entries",
        ));
    }
    let _: Token![,] = input.parse()?;
    Ok(())
}

/// Diagnostics for anything that is not a known entry keyword.
fn unknown_entry_error(input: ParseStream<'_>) -> syn::Error {
    match input.cursor().token_tree() {
        Some((TokenTree::Ident(ident), _)) => syn::Error::new(
            ident.span(),
            format!(
                "expected one of `map`, `bind!`, `filter`, `join`, `kleisli`, \
                 `traverse`, `fold`, `fold_map`, or `collect`, found `{ident}`"
            ),
        ),
        Some((tt, _)) => syn::Error::new(
            tt.span(),
            "expected one of `map`, `bind!`, `filter`, `join`, `kleisli`, `traverse`, \
             `fold`, `fold_map`, or `collect`",
        ),
        None => syn::Error::new(Span::call_site(), "unexpected end of pipeline"),
    }
}

/// Consume raw token trees from `input`, stopping at the capture boundary.
///
/// Groups are consumed whole, so commas and `kw:` sequences *inside* parens,
/// brackets, or braces never terminate the capture — the grammar is
/// syntax-directed (§3.1), no semantic analysis involved.
fn capture_raw(input: ParseStream<'_>) -> syn::Result<TokenStream> {
    Ok(capture_raw_impl(input, false)?.0)
}

/// The shared capture stepper behind [`capture_raw`] and
/// [`capture_raw_source`].
///
/// Termination conditions, all at the token-tree top level:
///
/// - a `,` — the entry separator;
/// - a [`BOUNDARY_KEYWORDS`] ident followed by `:` (not a `::` path joint)
///   or by `!` (a §4.2 escape hatch) — the next entry starting unseparated;
/// - with `stop_at_as`, an `as` — the §4.2 ascription split point, consumed
///   so the `syn::Type` parse starts at the type.
///
/// The one non-group construct whose commas must *not* split is the
/// turbofish `path::<A, B>(…)`: angle brackets are not token groups, so a
/// generic-list comma is token-level top level. The stepper enters
/// angle-tracking mode only after a `::<` joint (where `<` can only begin a
/// generic argument list, never a comparison) and exits at the matching
/// `>`, consuming `->` arrows of fn-typed arguments as pairs so they are
/// not mistaken for closers. Outside turbofish position `<`/`>` are left
/// untouched — comparison operators never swallow a separator.
fn capture_raw_impl(input: ParseStream<'_>, stop_at_as: bool) -> syn::Result<(TokenStream, bool)> {
    let mut captured = TokenStream::new();
    let mut stopped_at_as = false;
    input.step(|cursor| {
        let mut rest = *cursor;
        'capture: while let Some((tt, next)) = rest.token_tree() {
            // Ordinary expression position: a comma or the next entry
            // keyword ends the capture.
            if let TokenTree::Punct(ref punct) = tt {
                if punct.as_char() == ',' {
                    break;
                }
                // `::<` opens a turbofish generic list: an inner labeled
                // loop scans it (angle brackets are not token groups, so
                // its commas must not split) and resumes the capture after
                // the matching `>`.
                if punct.as_char() == ':'
                    && punct.spacing() == proc_macro2::Spacing::Joint
                    && let Some((second, after)) = next.token_tree()
                    && matches!(second, TokenTree::Punct(ref p) if p.as_char() == ':')
                    && let Some((open, after_open)) = after.token_tree()
                    && matches!(open, TokenTree::Punct(ref p) if p.as_char() == '<')
                {
                    captured.extend([tt.clone(), second, open.clone()]);
                    rest = after_open;
                    let mut depth = 1_usize;
                    while let Some((tf, tf_next)) = rest.token_tree() {
                        if let TokenTree::Punct(p) = &tf {
                            match p.as_char() {
                                '<' => depth += 1,
                                '>' => depth -= 1,
                                // `->` of a fn-typed generic argument:
                                // consume both tokens so the `>` is not
                                // read as a closer.
                                '-' if matches!(
                                    tf_next.token_tree(),
                                    Some((TokenTree::Punct(gt), _)) if gt.as_char() == '>'
                                ) =>
                                {
                                    let (arrow_gt, after_arrow) =
                                        tf_next.token_tree().expect("checked above");
                                    captured.extend([tf, arrow_gt]);
                                    rest = after_arrow;
                                    continue;
                                }
                                // A generic-argument comma is part of
                                // the capture.
                                _ => {}
                            }
                        }
                        captured.extend(core::iter::once(tf));
                        rest = tf_next;
                        if depth == 0 {
                            break;
                        }
                    }
                    continue 'capture;
                }
            } else if let TokenTree::Ident(ref ident) = tt {
                if stop_at_as && ident == "as" {
                    stopped_at_as = true;
                    rest = next;
                    break;
                }
                if is_boundary_keyword(ident)
                    && let Some((TokenTree::Punct(ref colon), after)) = next.token_tree()
                    && (colon.as_char() == ':' || colon.as_char() == '!')
                    && (colon.as_char() == '!'
                        || after
                            .token_tree()
                            .is_none_or(|(after_tt, _)| {
                                !matches!(after_tt, TokenTree::Punct(ref p) if p.as_char() == ':')
                            }))
                {
                    break;
                }
            }
            captured.extend(core::iter::once(tt));
            rest = next;
        }
        Ok(((), rest))
    })?;
    Ok((captured, stopped_at_as))
}

/// `true` when the ident is one of the [`BOUNDARY_KEYWORDS`] — compared
/// without building a `String` (§3.1: no string machinery on the hot
/// parse path; `Ident: PartialEq<&str>` compares in place). An explicit
/// short-circuit chain, most frequent keywords first: the check runs for
/// every ident of every captured body.
fn is_boundary_keyword(ident: &proc_macro2::Ident) -> bool {
    ident == "map"
        || ident == "bind"
        || ident == "collect"
        || ident == "fold"
        || ident == "filter"
        || ident == "join"
        || ident == "fold_map"
        || ident == "kleisli"
        || ident == "traverse"
        || ident == "source"
        || ident == "witness"
}

/// Split a raw token stream at its first top-level comma (fold tuple §3.1).
fn split_at_top_level_comma(tokens: &TokenStream) -> (TokenStream, TokenStream) {
    let mut head = TokenStream::new();
    let mut tail = TokenStream::new();
    let mut in_tail = false;
    for tt in tokens.clone() {
        if in_tail {
            tail.extend(core::iter::once(tt));
            continue;
        }
        if let TokenTree::Punct(ref punct) = tt
            && punct.as_char() == ','
        {
            in_tail = true;
            continue;
        }
        head.extend(core::iter::once(tt));
    }
    (head, tail)
}

/// Split a raw token stream at every top-level comma (kleisli chain §4.3).
/// Nested groups are single token trees, so commas inside parens, brackets
/// or braces never split.
fn split_top_level_commas(tokens: &TokenStream) -> Vec<TokenStream> {
    let mut segments = vec![TokenStream::new()];
    for tt in tokens.clone() {
        if let TokenTree::Punct(ref punct) = tt
            && punct.as_char() == ','
        {
            segments.push(TokenStream::new());
            continue;
        }
        segments
            .last_mut()
            .expect("at least one segment always present")
            .extend(core::iter::once(tt));
    }
    segments
}

/// A validated single-parameter closure (§7 limitation 1).
impl Closure {
    /// Parse a closure with exactly `arity` identifier parameters: stage
    /// combinators take one, `fold` takes two (`Fn(A, T) -> A`).
    ///
    /// # Errors
    /// A spanned [`syn::Error`] when the input is neither a callable
    /// expression nor a closure with exactly `arity` identifier parameters
    /// (mismatched arity, a non-identifier parameter, or a malformed
    /// `-> Ret` ascription). Diagnostics point at the offending token (§6.2).
    pub fn parse_with_arity(input: ParseStream<'_>, arity: usize) -> syn::Result<Self> {
        // Callable form (`bind: parse_tokens`): anything that is not a
        // closure header is captured raw and let the kernel's `Fn` bound
        // type-check it.
        if !input.peek(Token![|]) {
            let tokens = capture_raw(input)?;
            if tokens.is_empty() {
                return Err(syn::Error::new(
                    next_span(input),
                    "expected a closure `|x| …` or a callable expression",
                ));
            }
            let span = tokens
                .clone()
                .into_iter()
                .next()
                .map_or_else(Span::call_site, |tt| tt.span());
            return Ok(Self {
                params: Vec::new(),
                ret: None,
                body: tokens,
                span,
            });
        }

        let arity_word = if arity == 1 { "one" } else { "two" };
        let _open: Token![|] = input.parse()?;
        if input.peek(Token![|]) {
            let empty: Token![|] = input.parse()?;
            return Err(syn::Error::new(
                empty.span(),
                format!("pipeline closures take exactly {arity_word} parameter; found none"),
            ));
        }
        let mut params = Vec::with_capacity(arity);
        let ident_err_msg = if arity == 1 {
            "expected a single identifier closure parameter \
             (v0.1 restriction: pattern parameters are not supported, \
             destructure inside the body)"
                .to_string()
        } else {
            format!(
                "expected exactly {arity_word} identifier closure parameters \
                 (v0.1 restriction: pattern parameters are not supported, \
                 destructure inside the body)"
            )
        };
        for index in 0..arity {
            let ident: syn::Ident = input
                .parse()
                .map_err(|_| syn::Error::new(next_span(input), ident_err_msg.clone()))?;
            let ty = if input.peek(Token![:]) {
                let _: Token![:] = input.parse()?;
                Some(input.parse::<syn::Type>()?)
            } else {
                None
            };
            params.push(Param { ident, ty });
            if index + 1 < arity {
                let _: Token![,] = input.parse().map_err(|_| {
                    syn::Error::new(
                        next_span(input),
                        format!("pipeline closures take exactly {arity_word} parameters"),
                    )
                })?;
            }
        }
        if input.peek(Token![,]) {
            let comma: Token![,] = input.parse()?;
            return Err(syn::Error::new(
                comma.span(),
                format!(
                    "pipeline closures take exactly {arity_word} parameter; expected closing `|`"
                ),
            ));
        }
        let _: Token![|] = input.parse().map_err(|_| {
            syn::Error::new(
                next_span(input),
                format!(
                    "pipeline closures take exactly {arity_word} parameter; expected closing `|`"
                ),
            )
        })?;
        let ret = if input.peek(Token![->]) {
            let _: Token![->] = input.parse()?;
            Some(input.parse::<syn::Type>()?)
        } else {
            None
        };
        let body = capture_raw(input)?;
        Ok(Self {
            span: params[0].ident.span(),
            params,
            ret,
            body,
        })
    }
}

impl Parse for Closure {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        Self::parse_with_arity(input, 1)
    }
}

impl ToTokens for Witness {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.path.to_tokens(tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::split_at_top_level_comma;
    use quote::quote;

    #[test]
    fn split_finds_first_top_level_comma() {
        let ts = quote!(0usize, |acc, x| acc + x);
        let (head, tail) = split_at_top_level_comma(&ts);
        assert_eq!(head.to_string(), "0usize");
        assert_eq!(tail.to_string(), "| acc , x | acc + x");
    }

    #[test]
    fn split_without_comma_returns_everything_as_head() {
        let ts = quote!(just_init);
        let (head, tail) = split_at_top_level_comma(&ts);
        assert_eq!(head.to_string(), "just_init");
        assert!(tail.is_empty());
    }
}
