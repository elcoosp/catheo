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

use crate::ast::{Closure, Param, Pipeline, Source, Stage, StageKind, Terminal, Witness};
use crate::observer::{is_lexically_pure, is_structural_observer};
use proc_macro2::{Span, TokenStream, TokenTree};
use quote::ToTokens;
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
    syn::custom_keyword!(fold);
    syn::custom_keyword!(collect);
}

// Aliases: syn's custom_keyword! declares the type under its own name; the
// suffixed aliases keep call sites readable without shadowing local names.

/// Entry keywords that terminate an opaque raw-token capture (closure body,
/// source expression). A top-level `ident:` sequence that names one of these
/// always starts the next pipeline entry, so a missing comma between entries
/// is reported instead of being silently swallowed by a greedy capture.
const BOUNDARY_KEYWORDS: [&str; 7] = [
    "witness", "source", "map", "bind", "filter", "fold", "collect",
];

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
        let witness = parse_witness(input)?;
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

        loop {
            if input.is_empty() {
                break;
            }

            if input.peek(kw::witness) {
                let kw: kw::witness = input.parse()?;
                return Err(syn::Error::new(
                    kw.span(),
                    "duplicate `witness` entry: a pipeline has exactly one head-anchored witness",
                ));
            }
            if input.peek(kw::source) {
                let kw: kw::source = input.parse()?;
                return Err(syn::Error::new(
                    kw.span(),
                    "duplicate `source` entry: a pipeline has exactly one source",
                ));
            }

            if input.peek(kw::map) {
                let kw: kw::map = input.parse()?;
                if terminal_seen {
                    return Err(after_terminal_error(kw.span(), "map"));
                }
                stages.push(parse_stage(StageKind::Map, kw.span(), input)?);
            } else if input.peek(kw::bind) {
                let kw: kw::bind = input.parse()?;
                if terminal_seen {
                    return Err(after_terminal_error(kw.span(), "bind"));
                }
                stages.push(parse_stage(StageKind::Bind, kw.span(), input)?);
            } else if input.peek(kw::filter) {
                let kw: kw::filter = input.parse()?;
                if terminal_seen {
                    return Err(after_terminal_error(kw.span(), "filter"));
                }
                stages.push(parse_stage(StageKind::Filter, kw.span(), input)?);
            } else if input.peek(kw::fold) {
                let kw: kw::fold = input.parse()?;
                if terminal_seen {
                    return Err(duplicate_terminal_error(kw.span(), "fold"));
                }
                terminal = parse_fold(kw.span(), input)?;
                terminal_seen = true;
            } else if input.peek(kw::collect) {
                let kw: kw::collect = input.parse()?;
                if terminal_seen {
                    return Err(duplicate_terminal_error(kw.span(), "collect"));
                }
                terminal = Terminal::Collect(kw.span());
                terminal_seen = true;
            } else {
                return Err(unknown_entry_error(input));
            }

            require_separator(input)?;
        }

        Ok(Self {
            witness,
            source,
            stages,
            terminal,
        })
    }
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

/// §4.1 head anchor: `witness: <Path>,` — required first entry.
fn parse_witness(input: ParseStream<'_>) -> syn::Result<Witness> {
    if input.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "empty pipeline: expected `witness: <Witness>` as the head anchor, e.g. `witness: VecWitness,`",
        ));
    }
    if !input.peek(kw::witness) {
        return Err(syn::Error::new(
            next_span(input),
            "expected `witness: <Witness>` as the head anchor of the pipeline (§4.1)",
        ));
    }
    let kw: kw::witness = input.parse()?;
    let _: Token![:] = input.parse()?;
    let path: syn::Path = input.parse()?;
    require_separator(input)?;
    Ok(Witness {
        span: kw.span(),
        path,
    })
}

/// §4.1 `source: <expr>,` — required second entry; expression captured raw.
fn parse_source(input: ParseStream<'_>) -> syn::Result<Source> {
    if input.peek(kw::witness) {
        let kw: kw::witness = input.parse()?;
        return Err(syn::Error::new(
            kw.span(),
            "duplicate `witness` entry: a pipeline has exactly one head-anchored witness",
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
    let tokens = capture_raw(input)?;
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
    require_separator(input)?;
    Ok(Source { tokens, span })
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
                "expected one of `map`, `bind`, `filter`, `fold`, or `collect`, found `{ident}`"
            ),
        ),
        Some((tt, _)) => syn::Error::new(
            tt.span(),
            "expected one of `map`, `bind`, `filter`, `fold`, or `collect`",
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
    let mut captured = TokenStream::new();
    input.step(|cursor| {
        let mut rest = *cursor;
        while let Some((tt, next)) = rest.token_tree() {
            if let TokenTree::Punct(ref punct) = tt
                && punct.as_char() == ','
            {
                break;
            }
            if let TokenTree::Ident(ref ident) = tt
                && BOUNDARY_KEYWORDS.contains(&ident.to_string().as_str())
                && let Some((TokenTree::Punct(ref colon), after)) = next.token_tree()
                && colon.as_char() == ':'
                && after
                    .token_tree()
                    .is_none_or(|(after_tt, _)| {
                        !matches!(after_tt, TokenTree::Punct(ref p) if p.as_char() == ':')
                    })
            {
                break;
            }
            captured.extend(core::iter::once(tt));
            rest = next;
        }
        Ok(((), rest))
    })?;
    Ok(captured)
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

/// Span of a stage keyword for `quote_spanned` diagnostics (§3.4).
pub trait KeywordSpan {
    /// The span this node contributes to generated diagnostics.
    fn keyword_span(&self) -> Span;
}

impl KeywordSpan for Stage {
    fn keyword_span(&self) -> Span {
        self.span
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
