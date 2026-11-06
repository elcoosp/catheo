//! SSA-flattening codegen (ARCHITECTURE.md §3.4) — M0 lowering.
//!
//! M0 emits the **naive sequential lowering**: one flat `let` binding per
//! stage, each a monomorphic kernel call on the witness. There is no
//! deforestation yet (that is M2) and no unified state wrapper (§6.3
//! fallback, also M2). The emitted shape is already what §3.4 describes:
//! flat `let`-bound virtual registers with every token tied to the
//! originating stage's span via `quote_spanned!`, so `rustc` diagnostics
//! point at the user's `map:` / `bind:` keyword, never into macro internals.

use crate::ast::{Closure, Pipeline, StageKind, Terminal};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};

/// Emit monomorphic Rust items for a parsed [`Pipeline`].
///
/// # Errors
/// Never fails in M0; the signature stays fallible in anticipation of the
/// expansion-budget fallback of §6.3.
pub fn emit(pipeline: &Pipeline) -> syn::Result<TokenStream> {
    let witness = &pipeline.witness.path;

    let mut parts: Vec<TokenStream> = Vec::with_capacity(pipeline.stages.len() + 2);

    // state_source — the raw source expression, bound directly: a `let`
    // initializer needs no extra grouping.
    let source = &pipeline.source.tokens;
    parts.push(quote_spanned! {pipeline.source.span=>
        let __catheo_source = #source;
    });

    let mut current = quote!(__catheo_source);
    for (index, stage) in pipeline.stages.iter().enumerate() {
        let state = format_ident!("__catheo_state_{index}");
        let closure = emit_closure(&stage.closure);
        let kernel = match stage.kind {
            StageKind::Map => quote! { ::catheo::CategoricalFunctor },
            StageKind::Bind => quote! { ::catheo::CategoricalMonad },
            StageKind::Filter => quote! { ::catheo::CategoricalFilter },
        };
        let run = match stage.kind {
            StageKind::Map => quote! { run_map },
            StageKind::Bind => quote! { run_bind },
            StageKind::Filter => quote! { run_filter },
        };
        // Every token of the binding is tied to the stage keyword's span
        // (§3.4): a type error inside the stage points at `map:`/`bind:` as
        // written by the user.
        parts.push(quote_spanned! {stage.span=>
            let #state = <#witness as #kernel>::#run(#current, #closure);
        });
        current = quote!(#state);
    }

    parts.push(match &pipeline.terminal {
        Terminal::Fold {
            init,
            closure,
            span,
            ..
        } => {
            let fold_closure = emit_closure(closure);
            quote_spanned! {*span=>
                <#witness as ::catheo::CategoricalFold>::run_fold(#current, #init, #fold_closure)
            }
        }
        Terminal::Collect(span) => quote_spanned! {*span=>
            <#witness as ::catheo::CategoricalIntoIter>::into_iter(#current).collect()
        },
        Terminal::None => current,
    });

    Ok(quote! {{ #(#parts)* }})
}

/// Reassemble a validated closure: reconstructed header (identifier
/// parameters with optional type ascriptions) plus the opaque body verbatim.
///
/// Per §3.3, fused composition later happens by *application*, not
/// substitution; the naive lowering emits the closure exactly as parsed.
fn emit_closure(closure: &Closure) -> TokenStream {
    let body = &closure.body;
    let span = closure.span;
    if closure.is_callable() {
        // Callable form: pass the expression to the kernel verbatim.
        quote_spanned! {span=> #body}
    } else {
        let params = closure.params.iter().map(|param| {
            let ident = &param.ident;
            if let Some(ty) = &param.ty {
                quote! { #ident : #ty }
            } else {
                quote! { #ident }
            }
        });
        let ret = closure.ret.as_ref();
        if let Some(ret) = ret {
            quote_spanned! {span=> | #(#params),* | -> #ret #body }
        } else {
            quote_spanned! {span=> | #(#params),* | #body }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::StageKind;
    use crate::parser::parse_pipeline;
    use quote::quote;

    fn emitted(tokens: proc_macro2::TokenStream) -> String {
        let pipeline = parse_pipeline(tokens).expect("pipeline parses");
        crate::codegen::emit(&pipeline)
            .expect("emission succeeds")
            .to_string()
    }

    #[test]
    fn map_stages_lower_to_the_functor_kernel() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x| x * 2,
            fold: (0, |acc, x| acc + x),
        });
        assert!(out.contains("CategoricalFunctor"), "{out}");
        assert!(out.contains("run_map"), "{out}");
        assert!(out.contains("CategoricalFold"), "{out}");
        assert!(out.contains("run_fold"), "{out}");
        assert!(out.contains("__catheo_state_0"), "{out}");
    }

    #[test]
    fn bind_and_filter_lower_to_their_kernels() {
        let out = emitted(quote! {
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
    fn closures_are_reassembled_verbatim() {
        let out = emitted(quote! {
            witness: VecWitness,
            source: xs,
            map: |x: i32| x * 2,
            collect,
        });
        assert!(out.contains("| x : i32 | x * 2"), "{out}");
    }

    #[test]
    fn witness_path_is_used_verbatim() {
        let out = emitted(quote! {
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
        let out = crate::codegen::emit(&pipeline).expect("emits").to_string();
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
}
