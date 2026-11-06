//! Pipeline AST — structure, not semantics.
//!
//! Mirrors [`docs/ARCHITECTURE.md`] §3.1. Closures are stored as raw
//! [`TokenStream`] bodies; the parser validates only the pipeline grammar and
//! the closure *header* (exactly one identifier parameter). Closure bodies are
//! opaque to this crate by design (§5.3): parsing cost is zero for the parts
//! we do not interpret, while `Span` information is preserved for diagnostics.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::spanned::Spanned;

/// Derive a witness path from a `source: expr as Type` ascription (§4.2).
///
/// Only the three §2.1 built-ins derive: the *last* path segment decides
/// (so `std::vec::Vec<T>` derives too, mirroring
/// `is_zero_or_one_element_witness`'s spelling posture). The derived
/// witness is spelled `::catheo::…`, so the expansion needs no user
/// import beyond the macro itself.
///
/// # Errors
/// A spanned [`syn::Error`] at the type when it is not a path type, names
/// no built-in type constructor, or names `Result` without both type
/// arguments.
pub fn witness_from_type(ty: &syn::Type) -> syn::Result<syn::Path> {
    const DERIVABLE: &str = "only `Vec<T>`, `Option<T>` and `Result<T, E>` \
         ascriptions derive a witness; name the witness with `witness:` instead";
    let syn::Type::Path(type_path) = ty else {
        return Err(syn::Error::new(
            ty.span(),
            format!("cannot derive a witness from this type: {DERIVABLE}"),
        ));
    };
    let Some(segment) = type_path.path.segments.last() else {
        return Err(syn::Error::new(
            ty.span(),
            "cannot derive a witness from an empty path",
        ));
    };
    let generic_args = match &segment.arguments {
        syn::PathArguments::AngleBracketed(args) => Some(args),
        syn::PathArguments::None => None,
        syn::PathArguments::Parenthesized(_) => {
            return Err(syn::Error::new(
                segment.ident.span(),
                "cannot derive a witness from a function type",
            ));
        }
    };
    match segment.ident.to_string().as_str() {
        "Vec" => Ok(syn::parse_quote!(::catheo::VecWitness)),
        "Option" => Ok(syn::parse_quote!(::catheo::OptionWitness)),
        "Result" => {
            let args = generic_args.ok_or_else(|| {
                syn::Error::new(
                    segment.ident.span(),
                    "cannot derive a witness from a bare `Result`: ascribe \
                     both type arguments, e.g. `source: r as Result<T, E>`",
                )
            })?;
            let arguments: Vec<&syn::GenericArgument> = args.args.iter().collect();
            let [_, err_arg] = arguments.as_slice() else {
                return Err(syn::Error::new(
                    args.span(),
                    format!(
                        "`Result` takes two type arguments; ascribe both, \
                         e.g. `source: r as Result<T, E>` ({DERIVABLE})"
                    ),
                ));
            };
            let syn::GenericArgument::Type(err_ty) = err_arg else {
                return Err(syn::Error::new(
                    err_arg.span(),
                    "the second `Result` argument must be a type",
                ));
            };
            Ok(syn::parse_quote!(::catheo::ResultWitness<#err_ty>))
        }
        other => Err(syn::Error::new(
            segment.ident.span(),
            format!("cannot derive a witness from `{other}`: {DERIVABLE}"),
        )),
    }
}

/// Head-anchored witness of a pipeline (§4.1).
///
/// The spec's `witness: syn::Ident` is generalized to a [`syn::Path`] so the
/// parameterized `ResultWitness<E>` can be named; a plain identifier parses
/// unchanged.
#[derive(Debug, Clone)]
pub struct Witness {
    /// The witness path, e.g. `VecWitness` or `ResultWitness<MyError>`.
    pub path: syn::Path,
    /// Span of the witness tokens, for diagnostics.
    pub span: Span,
}

/// The pipeline's input expression, captured as raw tokens (§3.1).
#[derive(Debug, Clone)]
pub struct Source {
    /// Raw expression tokens, e.g. `input_vector` or `make_items(1, 8)`.
    /// When [`Self::ascription`] is present this is the expression *head*
    /// only — the `as Type` suffix is parsed out (§4.2).
    pub tokens: TokenStream,
    /// Span of the first source token.
    pub span: Span,
    /// `source: expr as Type` — the ascribed type, recorded for the
    /// emitter (§4.2: the expansion re-anchors the head expression with a
    /// `: Type` annotation so the user's written type still participates
    /// in inference).
    pub ascription: Option<syn::Type>,
}

/// Kind of a pipeline stage (§3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageKind {
    /// Functor map — fusion target (Rule 1, Rule 2).
    Map,
    /// Monadic bind — the hard case; fuses only with a non-observing prefix.
    Bind,
    /// Absolute fusion barrier (Rule 3).
    Filter,
}

impl StageKind {
    /// Canonical keyword spelling of this stage, for diagnostics.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Bind => "bind",
            Self::Filter => "filter",
        }
    }
}

/// One closure parameter: an identifier with optional `: Type` ascription
/// (v0.1 restriction: identifier parameters only, §7 limitation 1).
#[derive(Debug, Clone)]
pub struct Param {
    /// The parameter identifier.
    pub ident: syn::Ident,
    /// Optional `: Type` ascription after the identifier.
    pub ty: Option<syn::Type>,
}

/// A validated stage callable, in one of two forms:
///
/// - **Closure form** `|x: T| -> R body`: identifier parameters (one for
///   stage combinators, two for `fold`'s `Fn(A, T) -> A`), optional type
///   ascriptions, optional return type, and an opaque body (§7 limitation
///   1: composition happens by application, so pattern-matching parameters
///   are a v0.1 restriction).
/// - **Callable form** `parse_tokens`: a fn path or any other callable
///   expression, passed to the kernel verbatim. Arity is checked by the
///   kernel's `Fn` bound, not by the parser.
#[derive(Debug, Clone)]
pub struct Closure {
    /// Parameter identifiers in order; **empty** for the callable form.
    pub params: Vec<Param>,
    /// Optional `-> Ret` ascription (closure form only).
    pub ret: Option<syn::Type>,
    /// Opaque body tokens (between the bars, or the callable expression).
    pub body: TokenStream,
    /// Span of the first parameter identifier, or of the callable.
    pub span: Span,
}

impl Closure {
    /// The single stage-closure parameter; stages take exactly one.
    ///
    /// # Panics
    /// If called on a callable-form [`Closure`] (empty params); check
    /// [`Closure::is_callable`] first.
    #[must_use]
    pub fn stage_param(&self) -> &Param {
        &self.params[0]
    }

    /// `true` when this is a bare callable expression rather than a closure.
    #[must_use]
    pub fn is_callable(&self) -> bool {
        self.params.is_empty()
    }

    /// Reassemble the validated closure into a callable expression:
    /// reconstructed header (identifier parameters with optional type
    /// ascriptions) plus the opaque body verbatim, every token spanned at
    /// the closure's own span (§3.4).
    ///
    /// Callable form (a fn path or other expression) is passed through
    /// verbatim. Composition later happens by *application* (§3.3), so
    /// this reconstruction is the single place a closure becomes a value.
    #[must_use]
    pub fn to_callable_tokens(&self) -> TokenStream {
        let body = &self.body;
        let span = self.span;
        if self.is_callable() {
            // Callable form: pass the expression through verbatim.
            quote_spanned! {span=> #body}
        } else {
            let params = self.params.iter().map(|param| {
                let ident = &param.ident;
                if let Some(ty) = &param.ty {
                    quote! { #ident : #ty }
                } else {
                    quote! { #ident }
                }
            });
            if let Some(ret) = self.ret.as_ref() {
                quote_spanned! {span=> | #(#params),* | -> #ret #body }
            } else {
                quote_spanned! {span=> | #(#params),* | #body }
            }
        }
    }
}

/// The §4.2 escape hatch written on a stage, if any.
///
/// The flags this drives are computed at parse time into
/// [`Stage::fusion_barrier`] / [`Stage::structural_observer`]; the
/// modifier is kept so tests and diagnostics can see *why* a stage is
/// (or is not) a barrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StageModifier {
    /// Written plainly (`map:`, `bind:`) — flags from the §3.2 heuristics.
    #[default]
    None,
    /// `map!: force` / `bind!: force` — the structural-observer flag is
    /// suppressed and the stage fuses (documented as unsound if the
    /// closure really observes structure). On a `map` the hatch is
    /// accepted but inert: maps are unconditional fusion targets.
    Force,
    /// `map!: materialize` / `bind!: materialize` — the stage is a hard
    /// barrier and lowers to its own kernel call, so an intermediate
    /// container is materialized no matter what the fusion pass would
    /// otherwise do.
    Materialize,
}

/// One pipeline stage with its parse-time barrier annotations (§3.2).
#[derive(Debug, Clone)]
pub struct Stage {
    /// Which combinator this stage applies.
    pub kind: StageKind,
    /// The stage closure.
    pub closure: Closure,
    /// Span of the stage keyword (`map:`, `bind:`, `filter:`) — or, for a
    /// `kleisli:`-desugared stage, of the morphism itself, so diagnostics
    /// point at the exact member of the chain (§3.4).
    pub span: Span,
    /// `true` blocks upstream fusion: always for [`StageKind::Filter`],
    /// for a [`StageKind::Bind`] whose closure is a structural observer,
    /// and for every [`StageModifier::Materialize`] stage.
    pub fusion_barrier: bool,
    /// Conservative lexical approximation (§3.2): the closure body mentions
    /// container-shape tokens (`.len`, `.is_empty`, `.iter`, `Some`, `None`,
    /// `Ok`, `Err`). False positives are acceptable; false negatives are not.
    pub structural_observer: bool,
    /// The §4.2 escape hatch written on this stage, if any.
    pub modifier: StageModifier,
}

/// Pipeline terminal (§3.1).
///
/// The `Fold` closure is boxed so every variant stays uniformly small: the
/// M2 deforestation pass rebuilds and moves `Terminal`s inside
/// `Vec<Stage>`-shaped buffers, and a small enum makes those moves cheap
/// while the boxed hot-path closure keeps its identity.
#[derive(Debug, Clone)]
pub enum Terminal {
    /// Universal eliminator; absorbs a pure upstream prefix (Rule 4).
    Fold {
        /// Initial accumulator, raw tokens.
        init: TokenStream,
        /// `Fn(A, T) -> A` closure over the accumulated value and each item.
        closure: Box<Closure>,
        /// Span of the `fold` keyword.
        span: Span,
        /// Lexical check (§3.4): the fold closure body contains no `&mut`.
        is_pure: bool,
    },
    /// Collect the final container back out of the witness.
    Collect(Span),
    /// Monoid-valued eliminator (§4.3 v0.2): map each element to a
    /// [`crate::ast`] closure's monoid value and combine from the identity.
    /// No `init` and no purity question: the accumulator never appears in
    /// user code, so Rule 4 absorption is always sound for it.
    FoldMap {
        /// `Fn(T) -> M` closure producing the per-element monoid value.
        closure: Box<Closure>,
        /// Span of the `fold_map` keyword.
        span: Span,
    },
    /// No terminal: the pipeline evaluates to the last stage's container.
    None,
}

/// A fully parsed `pipeline! { ... }` invocation.
#[derive(Debug, Clone)]
pub struct Pipeline {
    /// Head-anchored witness (§4.1) — the one piece of type information the
    /// macro cannot infer.
    pub witness: Witness,
    /// Input expression.
    pub source: Source,
    /// Ordered stages, top to bottom as written.
    pub stages: Vec<Stage>,
    /// Terminal eliminator, if any.
    pub terminal: Terminal,
}

/// Span-insensitive equality: two closures are equal when they denote the
/// same computation, regardless of where they were written. Used by tests.
impl PartialEq for Closure {
    fn eq(&self, other: &Self) -> bool {
        let param_str = |c: &Self| {
            c.params
                .iter()
                .map(|p| {
                    let ty =
                        p.ty.as_ref()
                            .map_or(String::new(), |t| format!(": {}", t.to_token_stream()));
                    format!("{}{}", p.ident, ty)
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        param_str(self) == param_str(other) && self.body.to_string() == other.body.to_string()
    }
}

impl PartialEq for Terminal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Fold {
                    init,
                    closure,
                    is_pure,
                    ..
                },
                Self::Fold {
                    init: their_init,
                    closure: their_closure,
                    is_pure: their_is_pure,
                    ..
                },
            ) => {
                init.to_string() == their_init.to_string()
                    && closure == their_closure
                    && is_pure == their_is_pure
            }
            (Self::None | Self::Collect(_) | Self::FoldMap { .. }, _) => {
                core::mem::discriminant(self) == core::mem::discriminant(other)
            }
            _ => false,
        }
    }
}
