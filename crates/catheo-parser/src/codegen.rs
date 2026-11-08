//! SSA-flattening codegen (ARCHITECTURE.md §3.4) — M2 fused lowering.
//!
//! Two lowerings share this module:
//!
//! - **[`Lowering::Fused`]** (what `pipeline!` expands to): the stage list
//!   first runs through the §3.3 deforestation pass ([`crate::fuse`]); the
//!   pure-`fold` terminal then *absorbs* its trailing Map/Bind chain
//!   (Rule 4) into nested imperative loops over the pre-barrier value, so
//!   no intermediate container is ever allocated. Every token stays tied to
//!   the originating stage's span via `quote_spanned!`, so `rustc`
//!   diagnostics point at the user's `map:` / `bind:` / `fold:` keyword,
//!   never into macro internals.
//! - **[`Lowering::Sequential`]** (what `pipeline_unfused!` expands to):
//!   the M0 naive lowering — one flat `let` binding per stage, each a
//!   monomorphic kernel call. It is the *oracle* of the §6.2 property
//!   harness: fused and sequential emissions of the same pipeline must
//!   produce identical values.
//!
//! Both emit the shape §3.4 describes: flat `let`-bound virtual registers
//! and flat, monomorphic control flow.
//!
//! # Two stable-Rust constraints the fused lowering works around
//!
//! **Composed closures cannot be emitted.** A §3.3 product
//! `(|__pipeline_x| (g)((f)(__pipeline_x)))` fails to compile: rustc
//! type-checks inner closure literals before any unification could pin
//! their parameters, so a body like `|s| s.len()` is E0282 even in a
//! `Fn`-bounded argument position. The fused lowering therefore chains the
//! *component* callables through `Fn`-bounded adapter positions
//! (`.map`, `.flat_map`, `.fold`), where each callable's parameters are
//! pinned by the expected signature. The composed form stays the AST-level
//! statement of the rules (`fuse_stages`), pinned by its own specs.
//!
//! **Pre-barrier runs cannot be fused into a kernel argument.** Fusing a
//! run into a barrier kernel's container argument requires forming the
//! witness container inside the generated code —
//! `{iterators…}.collect()` against a `<Witness as HKT>::Type<T>`
//! expectation. Empirically (rustc 1.99), that selection only resolves
//! through a *concrete* container type: it works for `Vec` projections but
//! fails for `Option`/`Result` with E0277, and a proc macro has no type
//! information to spell the concrete type. This is the same normalization
//! wall as §5.1's GAT note, so the head (everything before the final
//! barrier-free run) stays at the M0 sequential-kernel shape — one
//! monomorphic kernel per stage — and the fusion wins concentrate where
//! they are sound on stable Rust:
//!
//! - the **absorbed suffix** of a pure `fold` (Rule 4): one adapter pass
//!   ending in a single `.fold(init, g)` eliminator — zero intermediate
//!   containers, every closure `Fn`-pinned, all three witnesses;
//! - `collect` / container terminals: the trailing barrier-free run chains
//!   into the user-annotated `collect()`, a *concrete* expectation that
//!   selects soundly.
//!
//! # Pointwise witnesses keep the kernel shape (the §6.1 M4 evidence)
//!
//! Deforestation eliminates *intermediate containers*. A witness whose
//! projection is a zero-or-one-element container — `OptionWitness`,
//! `ResultWitness<E>` — has none: its per-stage state is a register-sized
//! enum, so chaining the absorbed run into `.flat_map` adapter layers over
//! `option::IntoIter`/`result::IntoIter` is pure overhead against the
//! kernel chain, whose every `run_bind` *is* the idiomatic `and_then`
//! (branch + call, no adapter machinery). The §6.1 M4 benchmark measured
//! the adapter lowering at 1.2–2.7× the idiomatic chain's time on the
//! 10-stage `Result` chain, cold and warm, and on the two-morphism
//! Kleisli chain — a **C ≥ B gate failure**, so per the §6.1 protocol the
//! responsible lowering was re-examined before merge. The revision: for
//! witnesses spelled `OptionWitness`/`ResultWitness` at the head, the
//! trailing barrier-free run stays at the sequential-kernel shape —
//! binds as `run_bind` kernels, the fold terminal as `run_fold` — and
//! `pipeline!` then emits byte-for-byte the idiomatic spelling, measured
//! at parity (README, "M4 notes").
//!
//! The dispatch is on the head witness *spelling* (a `syn::Path`, so
//! `ResultWitness<E>` parses and matches; aliases and custom witnesses do
//! not): a proc macro has no type information, and a miss falls back to
//! the adapter lowering, which is semantically correct for every witness
//! — the §3.2 conservative-approximation posture (false negatives
//! acceptable, false negatives on the *gate* none: the M4 lockstep gate
//! pins the kernel shape for the spelled built-ins). When the
//! next-generation trait solver stabilizes (§5.1) the dispatch can become
//! type-directed without breaking user code.

use crate::ast::{Pipeline, Stage, StageKind, Terminal};
use crate::fuse::{absorbable_suffix, fuse_components};
use proc_macro2::{TokenStream, TokenTree};
use quote::{format_ident, quote, quote_spanned};

/// §6.3 expansion budget, in generated tokens.
///
/// The spec budgets *lines* (≤ 200 per pipeline, measured via
/// `-Zmacro-stats`); printed token streams do not preserve source line
/// breaks, so the budget is enforced on the token count with 10 tokens per
/// line as the documented conversion. Breaching it falls back to the
/// sequential lowering (the fused output can only ever be *smaller* than
/// the user's own closure bodies); the `-Zmacro-stats` artifact in CI is
/// the evidence trail.
pub const EXPANSION_TOKEN_BUDGET: usize = 2_000;

/// Which emission strategy [`emit_with_lowering`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lowering {
    /// Deforestation pass + fold absorption (§3.3, §3.4). The `pipeline!`
    /// lowering.
    #[default]
    Fused,
    /// The M0 naive sequential lowering; the §6.2 unfused oracle behind
    /// `pipeline_unfused!`.
    Sequential,
}

/// Emit monomorphic Rust items for a parsed [`Pipeline`] with the fused
/// M2 lowering (deforestation + fold absorption).
///
/// # Errors
/// Never fails in M2; the signature stays fallible in anticipation of a
/// future diagnostic-carrying pass.
pub fn emit(pipeline: &Pipeline) -> syn::Result<TokenStream> {
    emit_with_lowering(pipeline, Lowering::Fused)
}

/// Emit the naive sequential lowering — the §6.2 unfused oracle.
///
/// # Errors
/// Never fails in M2; see [`emit`].
pub fn emit_sequential(pipeline: &Pipeline) -> syn::Result<TokenStream> {
    emit_with_lowering(pipeline, Lowering::Sequential)
}

/// Emit with an explicit strategy (§6.2: fused and sequential emissions of
/// the same [`Pipeline`] must agree value-for-value).
///
/// # Errors
/// Never fails in M2; see [`emit`].
pub fn emit_with_lowering(pipeline: &Pipeline, lowering: Lowering) -> syn::Result<TokenStream> {
    Ok(emit_with_budget(pipeline, lowering, EXPANSION_TOKEN_BUDGET))
}

/// [`emit_with_lowering`] with the §6.3 budget made explicit (tests).
fn emit_with_budget(pipeline: &Pipeline, lowering: Lowering, budget: usize) -> TokenStream {
    match lowering {
        Lowering::Sequential => emit_pipeline(pipeline, false),
        Lowering::Fused => {
            let fused = emit_pipeline(pipeline, true);
            if token_count(&fused) > budget {
                // §6.3 budget fallback: emit the smaller sequential shape.
                emit_pipeline(pipeline, false)
            } else {
                fused
            }
        }
    }
}

/// Number of token trees in an emitted stream, counted recursively through
/// groups (§6.3 metric).
///
/// The whole emission is one block group, so a top-level count would be 1;
/// the recursive count is deterministic, monotone in pipeline size, and
/// cheap next to parsing.
#[must_use]
pub fn token_count(tokens: &TokenStream) -> usize {
    fn count(tokens: &TokenStream) -> usize {
        tokens
            .clone()
            .into_iter()
            .map(|tree| match tree {
                TokenTree::Group(ref group) => count(&group.stream()) + 1,
                _ => 1,
            })
            .sum()
    }
    count(tokens)
}

/// Emit the whole pipeline; `fuse` selects the fused lowering.
///
/// The sequential lowering emits one kernel binding per original stage
/// (M0). The fused lowering keeps that shape for the *prefix* (everything
/// before the trailing barrier-free run — see the module docs for why a
/// stable-Rust macro cannot form witness containers mid-chain) and fuses
/// the trailing run into one adapter pass that ends in the terminal: a
/// pure `fold` absorbs it into a single `.fold(init, g)` eliminator
/// (Rule 4); `collect`/container terminals chain into the user-annotated
/// `collect()`. Every user callable lands in a `Fn`-bounded argument
/// position, so closure parameters are pinned by the expected signature.
///
/// A `traverse:` entry (§4.3 v0.3) splits the emission at the traversal
/// boundary: see [`emit_traversal`].
fn emit_pipeline(pipeline: &Pipeline, fuse: bool) -> TokenStream {
    let witness = &pipeline.witness.path;

    let mut parts: Vec<TokenStream> = Vec::new();

    // state_source — the raw source expression, bound directly: a `let`
    // initializer needs no extra grouping. A §4.2 ascription re-anchors the
    // head expression with the user's written `: Type`, so the type they
    // spelled still participates in inference (the `as` cast itself is
    // never emitted — casting into a container is not valid Rust).
    let source = &pipeline.source.tokens;
    match &pipeline.source.ascription {
        Some(ty) => parts.push(quote_spanned! {pipeline.source.span=>
            let __catheo_source : #ty = #source;
        }),
        None => parts.push(quote_spanned! {pipeline.source.span=>
            let __catheo_source = #source;
        }),
    }

    let source_register = quote!(__catheo_source);

    if let Some(index) = pipeline
        .stages
        .iter()
        .position(|stage| stage.kind == StageKind::Traverse)
    {
        emit_traversal(pipeline, index, source_register, fuse, &mut parts);
    } else {
        let run = emit_stage_run(
            witness,
            source_register,
            &pipeline.stages,
            &pipeline.terminal,
            fuse,
        );
        parts.push(run);
    }

    quote! {{ #(#parts)* }}
}

/// Emit one stage run: sequential kernel bindings for the barriered head,
/// then the terminal over the current register (§3.4 flat `let` registers).
///
/// `fuse` selects the fused absorption for the terminal's trailing run —
/// the same shape whether the run starts at the source (the boundary-free
/// pipeline) or at the traversal guard's inner binding (the §4.3 v0.3
/// suffix), which is what makes Rule 4 compose across the guard.
fn emit_stage_run(
    witness: &syn::Path,
    source_register: TokenStream,
    stages: &[Stage],
    terminal: &Terminal,
    fuse: bool,
) -> TokenStream {
    let mut parts: Vec<TokenStream> = Vec::new();

    // Pointwise witnesses (zero-or-one-element projections) keep the whole
    // run at the kernel shape: there is no container to deforest, so the
    // adapter-chain absorption is pure overhead against the kernel chain
    // (see the module docs, the §6.1 M4 evidence).
    let pointwise = fuse && is_zero_or_one_element_witness(witness);

    // The trailing barrier-free run: absorbed by a pure fold or a
    // `fold_map`, chained into a `collect`/container terminal, or (impure
    // fold) absent. Barrier flags are identical in the original and fused
    // views, so the split is computed on the original stages and the run is
    // re-fused for emission. `fold_map` has no purity question — the
    // accumulator never appears in user code — so it always absorbs.
    let split = if fuse && !pointwise {
        match terminal {
            Terminal::Fold { is_pure: true, .. }
            | Terminal::FoldMap { .. }
            | Terminal::Collect(_)
            | Terminal::None => absorbable_suffix(stages),
            Terminal::Fold { is_pure: false, .. } => 0,
        }
    } else {
        0
    };
    let prefix = &stages[..stages.len() - split];
    let suffix = &stages[stages.len() - split..];

    let mut current = source_register;

    // Head: one monomorphic kernel binding per barriered stage (§3.4 flat
    // `let` registers; sequential in both lowerings — see module docs).
    for (index, stage) in prefix.iter().enumerate() {
        let state = format_ident!("__catheo_state_{index}");
        let closure = stage.closure.to_callable_tokens();
        let kernel = kernel_trait(stage.kind);
        let run = kernel_method(stage.kind);
        // Every token of the binding is tied to the stage keyword's span
        // (§3.4): a type error inside the stage points at `map:`/`bind:` as
        // written by the user.
        parts.push(quote_spanned! {stage.span=>
            let #state = <#witness as #kernel>::#run(#current, #closure);
        });
        current = quote!(#state);
    }

    // Terminal.
    let tail = match terminal {
        Terminal::Fold {
            init,
            closure,
            span,
            is_pure,
        } => {
            let fold_callable = closure.to_callable_tokens();
            if *is_pure && fuse && !pointwise {
                // Rule 4: the absorbed run (possibly empty) lowers into the
                // fold's imperative loop — a single fused `.fold`
                // eliminator over one adapter pass. Nothing between the
                // loop source and the accumulator is ever materialized.
                let chain = chain_adapters(suffix);
                let iter = quote_spanned! {*span=>
                    <#witness as ::catheo::CategoricalIntoIter>::into_iter(#current) #chain
                };
                quote_spanned! {*span=> #iter . fold (#init , #fold_callable)}
            } else {
                // Impure fold (§3.4), or any fold on a pointwise witness:
                // `run_fold` on the last register. For Option/Result the
                // kernel *is* the idiomatic elimination (one discriminant
                // test); see the module docs, the §6.1 M4 evidence.
                quote_spanned! {*span=>
                    <#witness as ::catheo::CategoricalFold>::run_fold(#current, #init, #fold_callable)
                }
            }
        }
        Terminal::FoldMap { closure, span } => {
            let fm_callable = closure.to_callable_tokens();
            if fuse && !pointwise {
                // Rule 4 for the monoid eliminator: the absorbed run (possibly
                // empty) chains into one pass ending `.map(f).fold(identity,
                // combine)` — zero intermediate containers. `combine`'s
                // `(Self, Self)` signature ties the fold's accumulator type to
                // the mapped item type, so the monoid resolves from `f`'s
                // return even without a user-side annotation.
                let chain = chain_adapters(suffix);
                let iter = quote_spanned! {*span=>
                    <#witness as ::catheo::CategoricalIntoIter>::into_iter(#current) #chain . map (#fm_callable)
                };
                quote_spanned! {*span=>
                    #iter . fold (
                        ::catheo::Monoid::identity(),
                        ::catheo::Monoid::combine,
                    )
                }
            } else {
                // Pointwise witnesses (or the sequential lowering): the
                // `run_fold_map` kernel is the idiomatic elimination — one
                // discriminant test on Option/Result (the §6.1 M4 posture).
                quote_spanned! {*span=>
                    <#witness as ::catheo::CategoricalFoldMap>::run_fold_map(#current, #fm_callable)
                }
            }
        }
        Terminal::Collect(span) => {
            let chain = if fuse {
                chain_adapters(suffix)
            } else {
                TokenStream::new()
            };
            let iter = quote_spanned! {*span=>
                <#witness as ::catheo::CategoricalIntoIter>::into_iter(#current) #chain
            };
            quote_spanned! {*span=> #iter . collect ()}
        }
        Terminal::None => {
            if fuse && !suffix.is_empty() {
                // The trailing run still fuses; the user's `let` annotation
                // provides the concrete collection target.
                let chain = chain_adapters(suffix);
                quote! {
                    { <#witness as ::catheo::CategoricalIntoIter>::into_iter(#current) #chain }.collect()
                }
            } else {
                current
            }
        }
    };
    parts.push(tail);

    quote! { #(#parts)* }
}

/// Emit the §4.3 v0.3 traversal boundary at `stages[index]` (the first and
/// only `traverse:` entry — the parser enforces the cardinality).
///
/// The lowering is the *different rewriting discipline* §7 limitation 4
/// predicted, split around the boundary:
///
/// - **The absorbable prefix chains into the traversal's iterator.** The
///   maximal non-barrier run before the boundary (Rules 1–2 view) rides
///   the traversal's adapter chain, exactly like a fold's absorbed
///   suffix: `into_iter(xs).map(g).map(f)` lands in `run_traverse`, which
///   appends its own arrow and collects — no intermediate container
///   between the source and the traversal's own `Vec`. Barriered stages
///   (filters, observing binds) stay sequential kernels and the chain
///   starts from their register.
/// - **The suffix + terminal lower inside the effect guard.** The
///   traversal's result is the effect around the collected container, so
///   the suffix pipeline is emitted inside the effect's `Ok`/`Some` arm
///   over the guard's inner binding, and the `Err`/`None` arm forwards the
///   effect unchanged. A pure fold terminal absorbs its own trailing run
///   *inside* the arm (Rule 4 composes across the guard); the sequential
///   lowering emits the same guard with per-stage kernels — the guard is a
///   semantic shape, not a fusion artifact.
/// - **No identity guard.** An empty suffix with no terminal evaluates to
///   the traversal itself; wrapping it in a match would emit an identity.
fn emit_traversal(
    pipeline: &Pipeline,
    index: usize,
    source_register: TokenStream,
    fuse: bool,
    parts: &mut Vec<TokenStream>,
) {
    let witness = &pipeline.witness.path;
    let traverse = &pipeline.stages[index];
    let effect = traverse
        .effect
        .as_ref()
        .expect("traverse stages carry their effect witness (parse-validated)");
    let pre = &pipeline.stages[..index];
    let post = &pipeline.stages[index + 1..];

    // The maximal non-barrier run before the boundary chains into the
    // traversal's iterator; everything before it stays at the kernel shape.
    let absorbable = if fuse { absorbable_suffix(pre) } else { 0 };
    let head = &pre[..pre.len() - absorbable];
    let chain_stages = &pre[pre.len() - absorbable..];

    let mut current = source_register;
    for (stage_index, stage) in head.iter().enumerate() {
        let state = format_ident!("__catheo_state_{stage_index}");
        let closure = stage.closure.to_callable_tokens();
        let kernel = kernel_trait(stage.kind);
        let run = kernel_method(stage.kind);
        parts.push(quote_spanned! {stage.span=>
            let #state = <#witness as #kernel>::#run(#current, #closure);
        });
        current = quote!(#state);
    }

    let traversal_state = format_ident!("__catheo_traversal");
    let arrow = traverse.closure.to_callable_tokens();
    let chain = if fuse {
        chain_adapters(chain_stages)
    } else {
        TokenStream::new()
    };
    parts.push(quote_spanned! {traverse.span=>
        let #traversal_state = <#effect as ::catheo::CategoricalTraverse>::run_traverse (
            <#witness as ::catheo::CategoricalIntoIter>::into_iter(#current) #chain ,
            #arrow ,
        );
    });

    if post.is_empty() && matches!(pipeline.terminal, Terminal::None) {
        parts.push(quote!(#traversal_state));
        return;
    }

    let inner = format_ident!("__catheo_inner");
    let stage_run = emit_stage_run(witness, quote!(#inner), post, &pipeline.terminal, fuse);
    parts.push(effect_guard(
        effect,
        traverse.span,
        &traversal_state,
        &inner,
        &stage_run,
    ));
}

/// Wrap a suffix stage-run in the effect guard: the collected container is
/// bound by the pure arm, the impure arm forwards the effect unchanged.
///
/// The constructor spelling dispatches on the effect witness's *last path
/// segment* — the same conservative posture as the codegen's pointwise
/// dispatch (§6.1 M4): the parser has already validated the spelling, so a
/// miss is unreachable here.
fn effect_guard(
    effect: &syn::Path,
    span: proc_macro2::Span,
    traversed: &syn::Ident,
    inner: &syn::Ident,
    stage_run: &TokenStream,
) -> TokenStream {
    let effect_name = effect
        .segments
        .last()
        .map_or_else(String::new, |segment| segment.ident.to_string());
    // The guarded suffix is bound to a register *before* the pure arm
    // re-wraps it: `Ok({ block })` would put braces in function-argument
    // position (`unused_braces` fires on the user's `traverse:` span), and
    // the braces around a `let`-leading initializer are necessary, so the
    // lint can never fire on them.
    match effect_name.as_str() {
        "OptionWitness" => quote_spanned! {span=>
            match #traversed {
                :: core :: option :: Option :: Some (#inner) => {
                    let __catheo_guarded = { #stage_run };
                    :: core :: option :: Option :: Some (__catheo_guarded)
                }
                :: core :: option :: Option :: None => :: core :: option :: Option :: None ,
            }
        },
        "ResultWitness" => quote_spanned! {span=>
            match #traversed {
                :: core :: result :: Result :: Ok (#inner) => {
                    let __catheo_guarded = { #stage_run };
                    :: core :: result :: Result :: Ok (__catheo_guarded)
                }
                :: core :: result :: Result :: Err (__catheo_err) => {
                    :: core :: result :: Result :: Err (__catheo_err)
                }
            }
        },
        other => {
            unreachable!("parse_traverse validated the effect spelling, found `{other}`")
        }
    }
}

/// Render a run of non-barrier stages as chained `Fn`-bounded adapters:
/// producers become `.map`, binds become `.flat_map` (`.filter` never
/// appears — filters are barriers and stay in the sequential prefix).
///
/// The stages are fused first so Rule 1/2 products apply in application
/// order; spans ride on the user's callable tokens (§3.4).
fn chain_adapters(stages: &[Stage]) -> TokenStream {
    let mut chain = TokenStream::new();
    for component in fuse_components(stages) {
        let last = component.callables.len().saturating_sub(1);
        for (index, callable) in component.callables.iter().enumerate() {
            let call = callable.to_callable_tokens();
            if component.kind == StageKind::Bind && index == last {
                chain.extend(quote_spanned! {callable.span=> . flat_map (#call)});
            } else {
                chain.extend(quote_spanned! {callable.span=> . map (#call)});
            }
        }
    }
    chain
}

fn kernel_trait(kind: StageKind) -> TokenStream {
    match kind {
        StageKind::Map => quote! { ::catheo::CategoricalFunctor },
        StageKind::Bind => quote! { ::catheo::CategoricalMonad },
        StageKind::Filter => quote! { ::catheo::CategoricalFilter },
        // Traversal stages lower through [`emit_traversal`] — the boundary
        // splits the emission and is never a kernel binding.
        StageKind::Traverse => {
            unreachable!("traverse stages are the emission boundary, not kernels")
        }
    }
}

fn kernel_method(kind: StageKind) -> TokenStream {
    match kind {
        StageKind::Map => quote! { run_map },
        StageKind::Bind => quote! { run_bind },
        StageKind::Filter => quote! { run_filter },
        StageKind::Traverse => {
            unreachable!("traverse stages are the emission boundary, not kernels")
        }
    }
}

/// `true` when the head witness spells a zero-or-one-element projection:
/// `OptionWitness` or `ResultWitness<…>` (§2.1 built-ins; the last path
/// segment decides, so a fully qualified spelling matches too).
///
/// A conservative *spelling* check by necessity — a proc macro has no type
/// information (§5.1) — with the §3.2 posture: a miss (aliased or custom
/// pointwise witnesses) falls back to the adapter lowering, which is
/// semantically correct for every witness, merely not optimal for those.
/// False negatives are acceptable; false positives are impossible (both
/// spelled witnesses project to zero-or-one-element containers by
/// definition).
fn is_zero_or_one_element_witness(witness: &syn::Path) -> bool {
    witness
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "OptionWitness" || segment.ident == "ResultWitness")
}

#[cfg(test)]
mod tests {
    use crate::ast::StageKind;
    use crate::codegen::{Lowering, emit, emit_sequential, emit_with_budget, emit_with_lowering};
    use crate::parser::parse_pipeline;
    use quote::quote;

    fn emitted_with(tokens: proc_macro2::TokenStream, lowering: Lowering) -> String {
        let pipeline = parse_pipeline(tokens).expect("pipeline parses");
        emit_with_lowering(&pipeline, lowering)
            .expect("emission succeeds")
            .to_string()
    }

    fn emitted(tokens: proc_macro2::TokenStream) -> String {
        emitted_with(tokens, Lowering::Fused)
    }

    fn sequential(tokens: proc_macro2::TokenStream) -> String {
        emitted_with(tokens, Lowering::Sequential)
    }

    // ─── Sequential lowering (the §6.2 oracle, pinned since M0) ────────────

    #[test]
    fn sequential_map_stages_lower_to_the_functor_kernel() {
        let out = sequential(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| y + 1,
            fold: (0, |acc, x| acc + x),
        });
        assert!(out.contains("CategoricalFunctor"), "{out}");
        assert!(out.contains("run_map"), "{out}");
        assert!(out.contains("CategoricalFold"), "{out}");
        assert!(out.contains("run_fold"), "{out}");
        assert!(out.contains("__catheo_state_0"), "{out}");
        assert!(out.contains("__catheo_state_1"), "{out}");
    }

    #[test]
    fn sequential_bind_and_filter_lower_to_their_kernels() {
        let out = sequential(quote! {
            witness: VecWitness,
            source: xs,
            bind: |x| split(x),
            filter: |x| *x > 0,
            collect,
        });
        assert!(out.contains("CategoricalMonad"), "{out}");
        assert!(out.contains("run_bind"), "{out}");
        assert!(out.contains("CategoricalFilter"), "{out}");
        assert!(out.contains("run_filter"), "{out}");
        assert!(out.contains("into_iter"), "{out}");
    }

    #[test]
    fn sequential_closures_are_reassembled_verbatim() {
        let out = sequential(quote! {
            witness: VecWitness,
            source: xs,
            map: |x: i32| x * 2,
            collect,
        });
        assert!(out.contains("| x : i32 | x * 2"), "{out}");
    }

    #[test]
    fn sequential_witness_path_is_used_verbatim() {
        let out = sequential(quote! {
            witness: ResultWitness<MyError>,
            source: r,
            fold: (0, |acc, x| acc + x),
        });
        assert!(out.contains("ResultWitness < MyError >"), "{out}");
    }

    #[test]
    fn identity_pipeline_evaluates_to_the_source_binding() {
        let pipeline = parse_pipeline(quote! {
            witness: VecWitness,
            source: xs,
        })
        .expect("parses");
        let out = emit(&pipeline).expect("emits").to_string();
        assert!(!out.contains("run_map"), "{out}");
        assert!(out.contains("__catheo_source"), "{out}");
        assert_eq!(pipeline.stages.len(), 0);
    }

    #[test]
    fn stage_kinds_reach_codegen_in_order() {
        let pipeline = parse_pipeline(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x,
            bind: |x| vec![x],
            filter: |x| *x > 0,
        })
        .expect("parses");
        let kinds: Vec<StageKind> = pipeline.stages.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [StageKind::Map, StageKind::Bind, StageKind::Filter]);
    }

    // ─── Fused lowering (M2: deforestation + fold absorption) ──────────────
    //
    // The fused emission chains the *component* callables of each fused
    // stage through `Fn`-bounded adapter positions (`.map` / `.flat_map` /
    // `.fold`). Applying composed closures directly — `(|x| g(f(x)))(v)` —
    // is not an option: the inner closure literals would be type-checked
    // before any unification pins their parameters (E0282 on `s.len()`-
    // style bodies), which the §6.2 property gate caught.

    #[test]
    fn pure_fold_absorbs_the_whole_map_chain() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| y + 1,
            fold: (0, |acc, x| acc + x),
        });
        assert!(!out.contains("run_map"), "no kernel survives Rule 4: {out}");
        assert!(!out.contains("CategoricalFunctor"), "{out}");
        assert!(
            !out.contains("run_fold"),
            "the fold itself is the loop now: {out}"
        );
        assert!(
            !out.contains("__catheo_state"),
            "nothing materializes between source and fold: {out}"
        );
        assert!(
            out.contains("into_iter (__catheo_source) . map (| x | x * 2) . map (| y | y + 1)"),
            "one pass, adapters chained: {out}"
        );
        assert!(out.contains(". fold (0 , | acc , x | acc + x)"), "{out}");
    }

    #[test]
    fn impure_fold_materializes_through_run_fold() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            fold: (Vec::new(), |acc, x| { acc.push(&mut x); acc }),
        });
        // Rule 4's fallback: an impure fold never absorbs — the prefix
        // stays sequential and the fold lowers to run_fold on the
        // materialized intermediate.
        assert!(out.contains("run_fold"), "{out}");
        assert!(
            out.contains("run_map"),
            "impure folds keep the sequential prefix: {out}"
        );
        assert!(
            !out.contains(". fold ("),
            "no fused eliminator for an impure fold: {out}"
        );
    }

    #[test]
    fn absorption_stops_at_an_observing_bind() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: rows,
            map: |x| transform(x),
            bind: |row| row.iter().collect(),
            fold: (0, |acc, x| acc + x),
        });
        // The observing bind is a barrier: absorption stops, the prefix
        // stays sequential, and the fold loops over the materialized bind
        // result.
        assert!(out.contains("run_bind"), "{out}");
        assert!(out.contains("run_map"), "prefix stays sequential: {out}");
        assert!(
            out.contains("into_iter (__catheo_state_1)"),
            "fold loops over the materialized bind result: {out}"
        );
    }

    #[test]
    fn absorption_stops_at_a_filter() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            filter: |x| *x > 0,
            fold: (0, |acc, x| acc + x),
        });
        assert!(out.contains("run_filter"), "{out}");
        assert!(
            out.contains("run_map"),
            "prefix stays sequential behind the barrier: {out}"
        );
        assert!(
            out.contains("into_iter (__catheo_state_1)"),
            "fold loops over the filter's materialized output: {out}"
        );
        assert!(
            !out.contains("run_fold"),
            "the pure fold is the fused eliminator: {out}"
        );
    }

    #[test]
    fn absorbed_binds_chain_into_one_fold() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            bind: |x| vec![x, x],
            bind: |y| wrap(y),
            fold: (0, |acc, x| acc + x),
        });
        assert!(!out.contains("run_map"), "{out}");
        assert!(!out.contains("run_bind"), "{out}");
        assert_eq!(
            out.matches(". flat_map (").count(),
            2,
            "one flat_map per bind: {out}"
        );
        assert_eq!(
            out.matches(". fold (").count(),
            1,
            "exactly one eliminator, at the end: {out}"
        );
        assert!(
            out.contains(
                ". map (| x | x * 2) . flat_map (| x | vec ! [x , x]) . flat_map (| y | wrap (y))"
            ),
            "linear chain in application order: {out}"
        );
    }

    #[test]
    fn collect_terminal_chains_maps_into_one_collect() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| y + 1,
            collect,
        });
        assert!(
            !out.contains("run_"),
            "no kernel: the chain collects straight into the target: {out}"
        );
        assert!(
            out.contains(". map (| x | x * 2) . map (| y | y + 1) . collect ()"),
            "{out}"
        );
    }

    #[test]
    fn barriers_keep_the_prefix_sequential() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| describe(y),
            filter: |x| *x == 3,
            collect,
        });
        // Rule 3: nothing crosses the filter, and a stable-Rust macro
        // cannot form a witness container mid-chain (module docs) — so the
        // prefix stays one monomorphic kernel per stage and the collect
        // consumes the filter's materialized state.
        assert_eq!(out.matches("run_map").count(), 2, "{out}");
        assert!(out.contains("run_filter"), "{out}");
        assert!(
            out.contains("into_iter (__catheo_state_2) . collect ()"),
            "{out}"
        );
        assert!(
            !out.contains("__catheo_state_3"),
            "no registers past the barrier: {out}"
        );
    }

    #[test]
    fn callable_bind_absorbs_into_the_chain() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            bind: split_tokens,
            fold: (0, |acc, x| acc + x),
        });
        assert!(!out.contains("run_bind"), "{out}");
        assert!(
            out.contains(". flat_map (split_tokens)"),
            "applied as a Fn-bounded argument: {out}"
        );
    }

    #[test]
    fn pointwise_witnesses_keep_the_kernel_chain() {
        // The §6.1 M4 evidence: on a zero-or-one-element witness the
        // adapter-chain absorption is pure overhead against the kernel
        // chain (whose run_bind *is* the idiomatic `and_then`), so the
        // fused lowering keeps every stage a monomorphic kernel and the
        // fold terminal at run_fold — byte-for-byte the idiomatic shape.
        let out = emitted(quote! {
            witness: ResultWitness<MyError>,
            source: r,
            bind: stage_01,
            bind: stage_02,
            fold: (0_i64, |acc, x| acc.wrapping_add(x)),
        });
        assert_eq!(
            out.matches("run_bind").count(),
            2,
            "one run_bind kernel per bind: {out}"
        );
        assert!(
            !out.contains("flat_map"),
            "no adapter chain over a 1-element iterator: {out}"
        );
        assert!(!out.contains("into_iter"), "no iterator is formed: {out}");
        assert_eq!(
            out.matches("run_fold").count(),
            1,
            "the fold terminal stays the run_fold kernel: {out}"
        );
        assert!(
            out.contains("__catheo_state_1"),
            "flat registers between kernels (§3.4): {out}"
        );

        // The Option spelling dispatches the same way, and the parameterized
        // Result path matches on the last path segment.
        let qualified = syn::parse_str::<syn::Path>("catheo::OptionWitness").expect("parses");
        for witness in [
            syn::parse_str::<syn::Path>("OptionWitness").expect("parses"),
            qualified,
        ] {
            let out = emitted(quote! {
                witness: #witness,
                source: o,
                bind: kleisli_f,
                fold: (0_i64, |acc, x| acc.wrapping_add(x)),
            });
            assert!(out.contains("run_bind"), "{out}");
            assert!(!out.contains("flat_map"), "{out}");
            assert!(out.contains("run_fold"), "{out}");
        }

        // Vec keeps the Rule 4 absorption: the two lowerings coexist.
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            bind: split_tokens,
            fold: (0, |acc, x| acc + x),
        });
        assert!(out.contains("flat_map"), "Vec still absorbs: {out}");
        assert!(!out.contains("run_bind"), "Vec still absorbs: {out}");
    }

    #[test]
    fn budget_breach_falls_back_to_the_sequential_shape() {
        // With budget headroom the two maps chain into zero kernels;
        // squeezing the §6.3 budget forces the sequential fallback (one
        // kernel per original stage).
        let tokens = quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| y + 1,
            collect,
        };
        let pipeline = parse_pipeline(tokens).expect("parses");
        let roomy = emit_with_budget(&pipeline, Lowering::Fused, usize::MAX).to_string();
        assert_eq!(
            roomy.matches("run_map").count(),
            0,
            "adapters chain with headroom: {roomy}"
        );
        let squeezed = emit_with_budget(&pipeline, Lowering::Fused, 10).to_string();
        assert_eq!(
            squeezed.matches("run_map").count(),
            2,
            "breach falls back to one kernel per stage: {squeezed}"
        );
    }

    // ─── v0.2: fold_map terminal (§4.3) ──────────────────────────────────

    #[test]
    fn fold_map_absorbs_the_map_chain_into_one_fold() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| y.to_string(),
            fold_map: |s| s.len(),
        });
        assert!(
            !out.contains("run_"),
            "no kernel survives absorption: {out}"
        );
        assert!(
            out.contains(
                ". map (| x | x * 2) . map (| y | y . to_string ()) . map (| s | s . len ())"
            ),
            "one pass ending in the metric map: {out}"
        );
        assert_eq!(out.matches(". fold (").count(), 1, "one eliminator: {out}");
        assert!(
            out.contains("Monoid :: identity ()"),
            "identity seeds the eliminator: {out}"
        );
        assert!(
            out.contains("Monoid :: combine"),
            "combine is the eliminator closure: {out}"
        );
    }

    #[test]
    fn fold_map_on_a_pointwise_witness_keeps_the_kernel() {
        let out = emitted(quote! {
            witness: OptionWitness,
            source: o,
            bind: |x| wrap(x),
            fold_map: |x| score(x),
        });
        assert!(
            out.contains("CategoricalFoldMap"),
            "pointwise keeps the run_fold_map kernel: {out}"
        );
        assert!(out.contains("run_fold_map"), "{out}");
        assert!(!out.contains("flat_map"), "no adapter chain: {out}");
        assert!(!out.contains("Monoid"), "no inline monoid fold: {out}");
    }

    #[test]
    fn fold_map_lowers_to_the_kernel_in_the_sequential_oracle() {
        let out = sequential(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            fold_map: |x| x.len(),
        });
        assert!(out.contains("run_map"), "{out}");
        assert!(out.contains("run_fold_map"), "{out}");
        assert!(!out.contains("flat_map"), "{out}");
    }

    #[test]
    fn fold_map_after_a_barrier_materializes_the_prefix() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            filter: |x| *x > 0,
            map: |y| y + 1,
            fold_map: |z| z,
        });
        assert!(out.contains("run_filter"), "{out}");
        assert!(
            !out.contains("__catheo_state_1"),
            "the barrier keeps the prefix at one register: {out}"
        );
        assert!(
            out.contains("into_iter (__catheo_state_0) . map (| y | y + 1) . map (| z | z)"),
            "absorption starts after the barrier: {out}"
        );
    }

    // ─── v0.2: §4.2 escape hatches ────────────────────────────────────────

    #[test]
    fn forced_observing_bind_chains_into_the_adapters() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: rows,
            map: |x| x * 2,
            bind!: force |row| row.iter().collect(),
            fold: (0, |acc, x| acc + x),
        });
        assert!(
            !out.contains("run_bind"),
            "force cleared the barrier: {out}"
        );
        assert!(!out.contains("run_map"), "{out}");
        assert!(
            out.contains(". map (| x | x * 2) . flat_map (| row | row . iter () . collect ())"),
            "the observer rides the fused pass: {out}"
        );
        assert_eq!(out.matches(". fold (").count(), 1, "{out}");
    }

    #[test]
    fn materialized_bind_stays_a_kernel_and_blocks_absorption() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            bind!: materialize |x| wrap(x),
            map: |y| y + 1,
            fold: (0, |acc, x| acc + x),
        });
        assert!(
            out.contains("run_bind"),
            "the materialized bind is its own kernel: {out}"
        );
        assert!(
            out.contains("run_map"),
            "the prefix map also stays sequential: {out}"
        );
        assert!(
            out.contains("into_iter (__catheo_state_1) . map (| y | y + 1)"),
            "the suffix map fuses into the fold, not through the barrier: {out}"
        );
    }

    // ─── v0.2: §4.2 source ascription ─────────────────────────────────────

    #[test]
    fn ascribed_source_binds_with_a_type_annotation() {
        let out = emitted(quote! {
            source: xs as Vec<i32>,
            map: |x| x * 2,
            fold: (0, |acc, x| acc + x),
        });
        assert!(
            out.contains("let __catheo_source : Vec < i32 > = xs ;"),
            "the ascription re-anchors the head expression: {out}"
        );
        assert!(
            out.contains(":: catheo :: VecWitness"),
            "the derived witness is absolute: {out}"
        );
        assert!(!out.contains("as Vec"), "the cast is never emitted: {out}");
    }

    #[test]
    fn emit_sequential_matches_the_lowering_enum() {
        let tokens = quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| y + 1,
            fold: (0, |acc, x| acc + x),
        };
        let pipeline = parse_pipeline(tokens).expect("parses");
        let direct = emit_sequential(&pipeline).expect("emits");
        let via_enum = emit_with_lowering(&pipeline, Lowering::Sequential).expect("emits");
        assert_eq!(
            direct.to_string(),
            via_enum.to_string(),
            "emit_sequential is Lowering::Sequential"
        );
    }

    // ─── v0.3: the §4.3 traversal boundary ─────────────────────────────────
    //
    // The traversal is the one stage whose neighbors lower on *both* sides
    // of it: the non-barrier prefix chains into the traversal's iterator
    // (Rules 1–2 extended to the boundary), and the suffix + terminal lower
    // *inside* the effect guard. These tests pin the emission shape.

    #[test]
    fn pure_prefix_absorbs_into_the_traversal_chain() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            traverse: (OptionWitness, |x| checked(x)),
            fold: (0_i64, |acc, x| acc + x),
        });
        assert_eq!(out.matches("run_traverse").count(), 1, "{out}");
        assert!(
            !out.contains("run_map"),
            "the prefix map rides the traversal chain: {out}"
        );
        assert!(
            !out.contains("run_fold"),
            "the fold lowers inside the guard: {out}"
        );
        assert!(
            out.contains("into_iter (__catheo_source) . map (| x | x * 2)",),
            "the absorbable prefix chains into the traversal's iterator: {out}"
        );
        assert!(
            out.contains("< OptionWitness as :: catheo :: CategoricalTraverse > :: run_traverse"),
            "the kernel dispatches on the effect witness: {out}"
        );
        assert!(
            out.contains(":: core :: option :: Option :: Some ("),
            "the guard wraps the suffix: {out}"
        );
        assert!(
            out.contains(
                ":: core :: option :: Option :: None => :: core :: option :: Option :: None"
            ),
            "the empty arm preserves the effect: {out}"
        );
    }

    #[test]
    fn traversal_guard_spells_the_result_arms() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            traverse: (ResultWitness<MyError>, parse_one),
            fold: (0_i64, |acc, x| acc + x),
        });
        assert!(out.contains(":: core :: result :: Result :: Ok ("), "{out}");
        assert!(
            out.contains(":: core :: result :: Result :: Err (__catheo_err)"),
            "the Err arm forwards the original error: {out}"
        );
    }

    #[test]
    fn whole_prefix_chain_fuses_into_the_traversal() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            map: |y| y.to_string(),
            bind: |s| wrap(s),
            traverse: (OptionWitness, |x| checked(x)),
            collect,
        });
        assert!(
            out.contains(
                "into_iter (__catheo_source) . map (| x | x * 2) . map (| y | y . to_string ()) . flat_map (| s | wrap (s))"
            ),
            "Rules 1–2 compose across the boundary into the traversal: {out}"
        );
        assert!(!out.contains("run_bind"), "{out}");
        assert_eq!(out.matches("run_traverse").count(), 1, "{out}");
    }

    #[test]
    fn observing_bind_before_the_traversal_stays_a_kernel() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            bind: |row| row.iter().collect(),
            traverse: (OptionWitness, |x| checked(x)),
            collect,
        });
        assert!(
            out.contains("run_bind"),
            "the observer is a barrier for the traversal too: {out}"
        );
        assert!(
            out.contains("into_iter (__catheo_state_0)"),
            "the traversal chains from the barrier's register: {out}"
        );
    }

    #[test]
    fn suffix_lowers_inside_the_effect_guard() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            traverse: (OptionWitness, |x| checked(x)),
            filter: |x| *x > 0,
            collect,
        });
        assert!(out.contains("run_filter"), "filters stay kernels: {out}");
        assert!(
            out.contains(":: run_filter (__catheo_inner , | x | * x > 0)"),
            "the guard's inner binding is the suffix's source: {out}"
        );
        // Order: the guard's Some arm contains the suffix work.
        let guard = out
            .find(":: core :: option :: Option :: Some (")
            .expect("guard present");
        let filter = out.find("run_filter").expect("filter present");
        assert!(guard < filter, "run_filter lives inside the guard: {out}");
    }

    #[test]
    fn pure_suffix_fold_absorbs_inside_the_guard() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            traverse: (OptionWitness, |x| checked(x)),
            map: |x| x.wrapping_add(1),
            fold: (0_i64, |acc, x| acc + x),
        });
        assert!(
            out.contains(
                "into_iter (__catheo_inner) . map (| x | x . wrapping_add (1)) . fold (0_i64"
            ),
            "Rule 4 applies to the suffix inside the guard: {out}"
        );
        assert!(!out.contains("run_fold"), "{out}");
        assert!(!out.contains("run_map"), "{out}");
    }

    #[test]
    fn traversal_without_a_suffix_or_terminal_is_the_traversal() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            traverse: (OptionWitness, |x| Some(x)),
        });
        assert_eq!(out.matches("run_traverse").count(), 1, "{out}");
        assert!(
            !out.contains(":: core :: option :: Option :: Some ("),
            "no identity guard is emitted over an empty suffix: {out}"
        );
    }

    #[test]
    fn sequential_traversal_keeps_kernels_on_both_sides() {
        let out = sequential(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            traverse: (OptionWitness, |x| checked(x)),
            map: |y| y.wrapping_add(1),
            fold: (0_i64, |acc, x| acc + x),
        });
        assert!(
            out.contains("run_map"),
            "the prefix map stays a kernel: {out}"
        );
        assert_eq!(out.matches("run_traverse").count(), 1, "{out}");
        assert!(
            out.contains("run_fold"),
            "the suffix fold stays a kernel: {out}"
        );
        assert!(
            out.contains(":: core :: option :: Option :: Some ("),
            "the guard is a semantic shape, not a fusion artifact: {out}"
        );
        assert!(
            !out.contains(". map ("),
            "no absorption in the oracle: {out}"
        );
    }

    #[test]
    fn traverse_collect_terminal_chains_inside_the_guard() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            traverse: (ResultWitness<MyError>, parse_one),
            map: |x| x.wrapping_mul(2),
            collect,
        });
        assert!(
            out.contains("into_iter (__catheo_inner) . map (| x | x . wrapping_mul (2))"),
            "the collect chains the suffix inside the guard: {out}"
        );
        assert!(out.contains(". collect ()"), "{out}");
    }
}
