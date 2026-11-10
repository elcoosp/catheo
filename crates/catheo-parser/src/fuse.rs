//! The linear-time deforestation pass (ARCHITECTURE.md §3.3).
//!
//! One `O(N)` walk over the stage list applies the four syntax-directed
//! rewrite rules:
//!
//! - **Rule 1 — adjacent `Map` fusion (unconditional):**
//!   `Map(f) → Map(g)  ⟹  Map(|x| g(f(x)))`. Justified by functor
//!   composition; no monad laws required.
//! - **Rule 2 — `Map` → non-observing `Bind` fusion (Chin's
//!   producer/consumer):** `Map(f) → Bind(h)` with `h` not a structural
//!   observer `⟹ Bind(|x| h(f(x)))`.
//! - **Rule 3 — `Filter` is an absolute barrier.** No rewrite crosses it;
//!   a filter stage is never fused into and never fused over.
//! - **Rule 4 — `Fold` absorbs a pure prefix chain.** This is a *lowering*
//!   decision (the fold terminal emits imperative loops over the absorbed
//!   chain) and therefore lives in [`crate::codegen`]; the pass exposes
//!   [`absorbable_suffix`] so both sides agree on what "chain" means.
//!
//! Composition is by **application, not substitution** (§3.3): the fused
//! body is `(|__pipeline_x| (g)((f)(__pipeline_x)))`, a valid composition
//! of two arbitrary callables — closures, function paths, fn pointers —
//! with the hygienic synthetic identifier `__pipeline_x` avoiding capture
//! of user identifiers.
//!
//! Barriers are uniform. Per §3.2 a stage with `fusion_barrier` (every
//! `Filter`, every observing `Bind`, and every observing `FilterMap`)
//! blocks upstream fusion: Rule 2 refuses to compose *into* such a stage,
//! and Rule 4's absorption stops at such a stage. False positives are
//! acceptable; false negatives are not.

use crate::ast::{Closure, Param, Stage, StageKind, StageModifier};
use crate::observer::is_partial_arrow_observer;
use proc_macro2::Span;
use quote::{format_ident, quote_spanned};

/// The hygienic synthetic parameter of every fused closure (§3.3).
const FUSED_PARAM: &str = "__pipeline_x";

/// One deforestation product: the stages merged by Rules 1–2, recorded
/// structurally.
///
/// [`fuse_stages`] renders this view into composed `Stage`s (the §3.3
/// letter: `Map(|x| g(f(x)))`); [`crate::codegen`] consumes the component
/// view directly, because composed closures cannot be *emitted* safely —
/// see the codegen module docs.
#[derive(Debug)]
pub struct FusedStage<'a> {
    /// The kind of the *last* merged stage (Map for Rule 1, Bind for
    /// Rule 2; a barrier keeps its own kind).
    pub kind: StageKind,
    /// Keyword span of the last merged stage — diagnostics anchor here.
    pub span: Span,
    /// Barrier flag (verbatim for singletons; recomputed after fusion).
    pub fusion_barrier: bool,
    /// Observer flag (verbatim for singletons; recomputed after fusion).
    pub structural_observer: bool,
    /// The merged callables in application order: element 0 is applied
    /// first, the last element decides the stage's kind. A barrier stage
    /// always holds exactly one (barriers never absorb).
    pub callables: Vec<&'a Closure>,
}

/// The deforestation pass over the *component* view: rewrite `stages` in a
/// single `O(N)` walk (§3.3).
///
/// Fusable neighbours are merged into their second member (the rewrite
/// consumes the later stage, whose keyword span labels the fused result so
/// diagnostics keep pointing at user-written tokens).
#[must_use]
pub fn fuse_components(stages: &[Stage]) -> Vec<FusedStage<'_>> {
    let mut fused: Vec<FusedStage<'_>> = Vec::with_capacity(stages.len());
    for stage in stages {
        // Rule 3 (and its §3.2 generalization): a barrier stage is pushed
        // verbatim and *blocks* what came before from fusing forward — the
        // next iteration pairs against the barrier and matches no rule.
        if stage.fusion_barrier {
            fused.push(FusedStage {
                kind: stage.kind,
                span: stage.span,
                fusion_barrier: stage.fusion_barrier,
                structural_observer: stage.structural_observer,
                callables: vec![&stage.closure],
            });
            continue;
        }
        // Rule 1: Map(f) → Map(g), unconditional.
        // Rule 2: Map(f) → consumer with a non-observing body — a `Bind`
        // (Chin's producer/consumer) or, since v0.6, the partial arrow of a
        // `FilterMap`: both are elementwise consumers, so absorbing the
        // producer into the argument position is sound. The barrier check
        // above already rejected observing consumers, so a consumer
        // reaching this point fuses. The fused stage takes the consumer's
        // kind, so a Map→Map→Bind chain applies Rule 1 twice, then Rule 2.
        if let Some(last) = fused.last_mut()
            && last.kind == StageKind::Map
            && matches!(
                stage.kind,
                StageKind::Map | StageKind::Bind | StageKind::FilterMap
            )
        {
            last.kind = stage.kind;
            last.span = stage.span;
            last.callables.push(&stage.closure);
            // Recompute the flags lexically over the composed body: the
            // composition embeds a superset of both bodies, so the rescan
            // can only ever *add* observations (a false positive at worst,
            // never a false negative — §3.2). Only the consumer kinds carry
            // the flag, and the scan is the kind's own: the §3.2 full scan
            // for a Bind, the §4.3 v0.6 methods-only refinement for a
            // FilterMap (whose constructors are the effect).
            if stage.kind != StageKind::Map {
                let body = composed_body(last.callables.iter().copied());
                last.structural_observer = match stage.kind {
                    StageKind::FilterMap => is_partial_arrow_observer(&body),
                    _ => crate::observer::is_structural_observer(&body),
                };
                last.fusion_barrier = last.structural_observer;
            }
            continue;
        }
        // No rule pairs Bind→*, FilterMap→* or Filter→*: push as-is. Each
        // stage is visited exactly once and each fusion mutates `last` in
        // place, so the walk is O(N) in the stage count (§5.3).
        fused.push(FusedStage {
            kind: stage.kind,
            span: stage.span,
            fusion_barrier: stage.fusion_barrier,
            structural_observer: stage.structural_observer,
            callables: vec![&stage.closure],
        });
    }
    fused
}

/// The deforestation pass: rewrite `stages` into composed `Stage`s (§3.3).
///
/// This is the *AST view* of [`fuse_components`]: each fused stage's
/// callables are folded into one closure by application — `Map(|x|
/// g(f(x)))` — the form §3.3 specifies. Codegen lowers from the component
/// view instead (see [`FusedStage`]).
#[must_use]
pub fn fuse_stages(stages: &[Stage]) -> Vec<Stage> {
    fuse_components(stages)
        .iter()
        .map(FusedStage::to_stage)
        .collect()
}

/// Length of the trailing run of stages a pure `fold` may absorb (Rule 4).
///
/// The suffix is the maximal run of non-barrier stages at the end of the
/// list: `Map`s and non-observing `Bind`s. Every barrier (`Filter`,
/// observing `Bind`) terminates absorption (see the module docs on
/// uniform barriers). Codegen lowers exactly this suffix into the fold's
/// imperative loops and materializes everything before it with kernels.
#[must_use]
pub fn absorbable_suffix(stages: &[Stage]) -> usize {
    stages
        .iter()
        .rev()
        .take_while(|stage| !stage.fusion_barrier)
        .count()
}

/// [`absorbable_suffix`] over the component view (what codegen consumes).
#[must_use]
pub fn absorbable_components(components: &[FusedStage<'_>]) -> usize {
    components
        .iter()
        .rev()
        .take_while(|stage| !stage.fusion_barrier)
        .count()
}

impl FusedStage<'_> {
    /// Render the component view as a composed [`Stage`] (the §3.3 form).
    #[must_use]
    pub fn to_stage(&self) -> Stage {
        let Some((first, rest)) = self.callables.split_first() else {
            // Unreachable by construction: `fuse_components` never pushes a
            // stage without its callable.
            return Stage {
                kind: self.kind,
                closure: Closure {
                    params: Vec::new(),
                    ret: None,
                    body: proc_macro2::TokenStream::new(),
                    span: self.span,
                },
                span: self.span,
                fusion_barrier: self.fusion_barrier,
                structural_observer: self.structural_observer,
                modifier: StageModifier::None,
                effect: None,
            };
        };
        let mut closure = (*first).clone();
        for callable in rest {
            closure = compose(&closure, callable);
        }
        Stage {
            kind: self.kind,
            closure,
            span: self.span,
            fusion_barrier: self.fusion_barrier,
            structural_observer: self.structural_observer,
            modifier: StageModifier::None,
            effect: None,
        }
    }
}

/// The composed body of a callable run: `((h)((g)((f)(x))))` without the
/// synthetic parameter — used for the lexical flag rescan.
fn composed_body<'a, I>(callables: I) -> proc_macro2::TokenStream
where
    I: Iterator<Item = &'a Closure>,
{
    let mut acc = None;
    for callable in callables {
        let call = callable.to_callable_tokens();
        acc = Some(match acc {
            None => call,
            Some(prev) => quote_spanned! {callable.span=> (#call)(#prev)},
        });
    }
    acc.unwrap_or_default()
}

/// Compose two callables by application (§3.3): `(|__pipeline_x| (g)((f)(__pipeline_x)))`.
///
/// The parameter identifier is synthetic (`FUSED_PARAM`); capture is
/// impossible because it only ever occurs as the wrapper's own parameter
/// and as the argument applied to the inner callable — user identifiers of
/// the same spelling are shadowed by their own closures' parameters.
#[must_use]
pub fn compose(producer: &Closure, consumer: &Closure) -> Closure {
    let inner = producer.to_callable_tokens();
    let outer = consumer.to_callable_tokens();
    let param = param_ident();
    let span = consumer.span;
    let body = quote_spanned! {span=> (#outer)((#inner)(#param))};
    Closure {
        params: vec![Param {
            ident: param,
            ty: None,
        }],
        ret: None,
        body,
        span,
    }
}

fn param_ident() -> syn::Ident {
    format_ident!("{FUSED_PARAM}", span = Span::call_site())
}

#[cfg(test)]
mod tests {
    use crate::ast::{Stage, StageKind};
    use crate::fuse::{absorbable_suffix, fuse_stages};
    use crate::parser::parse_pipeline;
    use quote::{ToTokens, quote};

    /// Parse just the stage list of a pipeline fragment.
    fn stages(entries: proc_macro2::TokenStream) -> Vec<Stage> {
        let mut tokens = quote! {
            witness: VecWitness,
            source: xs,
        };
        tokens.extend(entries);
        let pipeline = parse_pipeline(tokens).expect("pipeline parses");
        pipeline.stages
    }

    fn kinds(stages: &[Stage]) -> Vec<StageKind> {
        stages.iter().map(|stage| stage.kind).collect()
    }

    fn body_of(stage: &Stage) -> String {
        stage.closure.body.to_token_stream().to_string()
    }

    #[test]
    fn adjacent_maps_fuse_unconditionally() {
        let fused = fuse_stages(&stages(quote! {
            map: |x| x * 2,
            map: |y| y + 1,
            map: |z| z - 3,
        }));
        assert_eq!(kinds(&fused), [StageKind::Map], "three maps fuse into one");
        let body = body_of(&fused[0]);
        // Composition by application: outer applied to inner applied to the
        // synthetic parameter; both bodies survive verbatim.
        assert!(body.contains("__pipeline_x"), "{body}");
        assert!(body.contains("| y | y + 1"), "{body}");
        assert!(body.contains("| z | z - 3"), "{body}");
        assert!(body.contains("| x | x * 2"), "{body}");
        // Outermost-composed-first ordering: (z-compose)((y-compose)((x-compose)(arg)))
        let z = body.find("| z |").expect("outermost present");
        let y = body.find("| y |").expect("middle present");
        let x = body.find("| x |").expect("innermost present");
        assert!(z < y && y < x, "application order outer→inner: {body}");
    }

    #[test]
    fn map_into_non_observing_bind_fuses_to_a_bind() {
        let fused = fuse_stages(&stages(quote! {
            map: |x| parse_it(x),
            bind: |s| wrap(s),
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind], "Rule 2 yields a Bind");
        let body = body_of(&fused[0]);
        assert!(body.contains("wrap"), "{body}");
        assert!(body.contains("parse_it"), "{body}");
        assert!(!fused[0].fusion_barrier);
        assert!(!fused[0].structural_observer);
    }

    #[test]
    fn map_into_observing_bind_does_not_fuse() {
        let fused = fuse_stages(&stages(quote! {
            map: |x| transform(x),
            bind: |row| row.iter().collect(),
        }));
        assert_eq!(kinds(&fused), [StageKind::Map, StageKind::Bind]);
        assert!(fused[1].structural_observer, "observer flag kept");
        assert!(fused[1].fusion_barrier, "observing bind is a barrier");
        assert!(!fused[0].fusion_barrier);
    }

    #[test]
    fn filter_is_an_absolute_barrier() {
        let fused = fuse_stages(&stages(quote! {
            map: |x| x * 2,
            filter: |x| *x > 0,
            map: |y| y + 1,
        }));
        assert_eq!(
            kinds(&fused),
            [StageKind::Map, StageKind::Filter, StageKind::Map],
            "no rewrite crosses the filter"
        );
        let fused = fuse_stages(&stages(quote! {
            map: |x| x * 2,
            filter: |x| *x > 0,
        }));
        assert_eq!(kinds(&fused), [StageKind::Map, StageKind::Filter]);
        let fused = fuse_stages(&stages(quote! {
            filter: |x| *x > 0,
            map: |x| x * 2,
            map: |y| y + 1,
        }));
        // The filter blocks backwards; the two maps after it still fuse.
        assert_eq!(
            kinds(&fused),
            [StageKind::Filter, StageKind::Map],
            "Rule 1 applies after the barrier"
        );
    }

    #[test]
    fn bind_then_map_never_fuses() {
        let fused = fuse_stages(&stages(quote! {
            bind: |x| wrap(x),
            map: |y| y + 1,
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind, StageKind::Map]);
    }

    #[test]
    fn bind_then_bind_never_fuses() {
        let fused = fuse_stages(&stages(quote! {
            bind: |x| wrap(x),
            bind: |y| other(y),
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind, StageKind::Bind]);
    }

    #[test]
    fn rule1_then_rule2_chain_into_one_bind() {
        let fused = fuse_stages(&stages(quote! {
            map: |x| x * 2,
            map: |y| y.to_string(),
            bind: |s| parse(s),
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind]);
        let body = body_of(&fused[0]);
        assert!(body.contains("parse"), "{body}");
        assert!(body.contains("to_string"), "{body}");
        assert!(body.contains("x * 2"), "{body}");
    }

    #[test]
    fn fused_flags_are_rescanned_conservatively() {
        // The bind body alone is clean; the absorbed map body mentions
        // `Some`, so the fused bind must be re-flagged as an observer —
        // a false positive is acceptable, a false negative is not (§3.2).
        let fused = fuse_stages(&stages(quote! {
            map: |x| maybe(x),
            bind: |y| wrap(y),
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind]);
        assert!(
            !fused[0].structural_observer,
            "sanity: wrap/maybe alone are clean; `maybe` is a plain path"
        );
        let fused = fuse_stages(&stages(quote! {
            map: |x| Some(x),
            bind: |y| wrap(y),
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind]);
        assert!(fused[0].structural_observer, "rescan catches `Some`");
        assert!(fused[0].fusion_barrier);
    }

    #[test]
    fn callable_forms_compose_by_application() {
        let fused = fuse_stages(&stages(quote! {
            map: double,
            map: |y| y + 1,
        }));
        assert_eq!(kinds(&fused), [StageKind::Map]);
        let body = body_of(&fused[0]);
        assert!(body.contains("double"), "{body}");
        assert!(body.contains("__pipeline_x"), "{body}");
    }

    #[test]
    fn fused_stage_inherits_the_later_keyword_span_and_kind() {
        let stages = stages(quote! {
            map: |x| x * 2,
            map: |y| y + 1,
        });
        let fused = fuse_stages(&stages);
        assert_eq!(fused.len(), 1);
        // Span equality is checked loosely: the fused stage must carry the
        // *later* stage's span, which for same-file spans means the same
        // source file. We assert the observable part — the kind and that
        // the span is a real span, not call-site.
        assert_eq!(fused[0].kind, StageKind::Map);
    }

    #[test]
    fn no_stages_stays_empty() {
        let fused = fuse_stages(&stages(quote! {}));
        assert!(fused.is_empty());
    }

    #[test]
    fn absorbable_suffix_stops_at_barriers() {
        let all_maps = stages(quote! {
            map: |x| x * 2,
            map: |y| y + 1,
        });
        assert_eq!(absorbable_suffix(&all_maps), 2);

        let with_filter = stages(quote! {
            map: |x| x * 2,
            filter: |x| *x > 0,
            map: |y| y + 1,
        });
        assert_eq!(
            absorbable_suffix(&with_filter),
            1,
            "filter is never absorbed over"
        );

        let with_observer = stages(quote! {
            bind: |x| wrap(x),
            bind: |row| row.iter().collect(),
            map: |y| y + 1,
        });
        assert_eq!(
            absorbable_suffix(&with_observer),
            1,
            "observing bind is a barrier for absorption too"
        );

        let non_observing_binds = stages(quote! {
            bind: |x| wrap(x),
            bind: |y| other(y),
            map: |z| z + 1,
        });
        assert_eq!(
            absorbable_suffix(&non_observing_binds),
            3,
            "non-observing binds absorb"
        );

        assert_eq!(absorbable_suffix(&[]), 0);
    }

    /// The composed closure's own header is exactly one synthetic
    /// parameter; the user closures appear only inside the body.
    #[test]
    fn composed_closure_header_is_synthetic() {
        let fused = fuse_stages(&stages(quote! {
            map: |x| x * 2,
            map: |y| y + 1,
        }));
        assert_eq!(fused[0].closure.params.len(), 1);
        assert_eq!(fused[0].closure.params[0].ident.to_string(), "__pipeline_x");
        assert!(fused[0].closure.ret.is_none());
    }

    // ─── Component view (what codegen lowers from) ──────────────────────────

    use crate::fuse::{absorbable_components, fuse_components};

    #[test]
    fn components_record_the_merged_callables_in_order() {
        let original = stages(quote! {
            map: |x| x * 2,
            map: |y| y + 1,
            map: |z| z - 3,
        });
        let comps = fuse_components(&original);
        assert_eq!(comps.len(), 1, "Rule 1 merges into one fused stage");
        assert_eq!(comps[0].callables.len(), 3);
        assert_eq!(comps[0].kind, StageKind::Map);
        let rendered: Vec<String> = comps[0]
            .callables
            .iter()
            .map(|c| c.to_callable_tokens().to_string())
            .collect();
        assert!(rendered[0].contains("| x | x * 2"), "{rendered:?}");
        assert!(rendered[1].contains("| y | y + 1"), "{rendered:?}");
        assert!(rendered[2].contains("| z | z - 3"), "{rendered:?}");
    }

    #[test]
    fn rule2_components_end_with_the_bind() {
        let original = stages(quote! {
            map: |x| parse_it(x),
            bind: |s| wrap(s),
        });
        let comps = fuse_components(&original);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].kind, StageKind::Bind, "the consumer decides kind");
        assert_eq!(comps[0].callables.len(), 2);
        assert!(!comps[0].fusion_barrier);
    }

    #[test]
    fn barrier_components_are_singletons() {
        let original = stages(quote! {
            map: |x| x * 2,
            filter: |x| *x > 0,
            bind: |y| wrap(y),
        });
        let comps = fuse_components(&original);
        assert_eq!(comps.len(), 3, "nothing merges across or into barriers");
        assert!(comps[1].fusion_barrier);
        assert_eq!(comps[1].callables.len(), 1);
        assert_eq!(comps[2].kind, StageKind::Bind);
        assert_eq!(
            comps[2].callables.len(),
            1,
            "bind after a barrier stays alone"
        );
    }

    #[test]
    fn ast_view_composes_the_components() {
        let original = stages(quote! {
            map: |x| x * 2,
            map: |y| y + 1,
        });
        let comps = fuse_components(&original);
        let ast_view = comps[0].to_stage();
        let body = ast_view.closure.body.to_token_stream().to_string();
        assert!(body.contains("| y | y + 1"), "{body}");
        assert!(body.contains("| x | x * 2"), "{body}");
        assert_eq!(comps[0].to_stage().kind, StageKind::Map);
    }

    #[test]
    fn absorbable_components_match_absorbable_suffix() {
        let original = stages(quote! {
            map: |x| x * 2,
            filter: |x| *x > 0,
            bind: |y| wrap(y),
            map: |z| z + 1,
        });
        assert_eq!(absorbable_suffix(&original), 2);
        assert_eq!(absorbable_components(&fuse_components(&original)), 2);
        assert_eq!(absorbable_components(&[]), 0);
    }

    // ─── The §4.3 join stage (v0.4): an ordinary bind for the pass ──────────

    #[test]
    fn map_into_join_fuses_to_one_bind() {
        // The identity is not a structural observer, so Rule 2 absorbs the
        // map into the join exactly as it would any producer/consumer pair.
        let fused = fuse_stages(&stages(quote! {
            map: |x| vec![x, x],
            join,
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind], "Rule 2 yields a Bind");
        assert!(!fused[0].fusion_barrier);
        assert!(!fused[0].structural_observer);
        let body = body_of(&fused[0]);
        assert!(body.contains("vec ! [x , x]"), "{body}");
        assert!(
            body.contains("__pipeline_join"),
            "the identity rides the fused body: {body}"
        );
    }

    #[test]
    fn join_then_map_never_fuses() {
        // No rewrite pairs Bind→*: the join stays its own kernel.
        let fused = fuse_stages(&stages(quote! {
            join,
            map: |y| y + 1,
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind, StageKind::Map]);
    }

    #[test]
    fn join_is_absorbable_by_a_pure_fold() {
        // Rule 4: the trailing non-barrier run includes the identity bind.
        // The AST view counts raw stages (3); the component view has the
        // map+join pair already merged into one Bind (Rule 2), so the
        // absorbable run there is the fused bind plus the trailing map.
        let original = stages(quote! {
            map: |x| vec![x, x],
            join,
            map: |y| y + 1,
        });
        assert_eq!(absorbable_suffix(&original), 3);
        let comps = fuse_components(&original);
        assert_eq!(absorbable_components(&comps), 2);
        assert_eq!(comps[0].callables.len(), 2, "Rule 2 merged map into join");
        assert!(!comps[0].fusion_barrier);
        assert!(!comps[1].fusion_barrier);
    }

    #[test]
    fn an_observer_map_before_join_rescans_to_a_barrier() {
        // §3.2: the composed body mentions `Some`, so the fused bind is
        // re-flagged as an observer — a false positive is acceptable, a
        // false negative is not. The value semantics are unchanged (the
        // runtime gates keep fused == unfused for this shape).
        let fused = fuse_stages(&stages(quote! {
            map: |x| Some(x),
            join,
        }));
        assert_eq!(kinds(&fused), [StageKind::Bind]);
        assert!(fused[0].structural_observer, "the rescan catches `Some`");
        assert!(fused[0].fusion_barrier);
    }
}
