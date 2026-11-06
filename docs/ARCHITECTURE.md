# Category Theory at the Speed of Rust

## Architecture Specification — v0.1

**Status:** Design-complete for the v0.1 prototype. Benchmark-gated before any public API commitment.
**Scope:** A proc-macro-backed categorical pipeline library with a syntax-directed compilation engine and an imperative execution core. Zero-cost on the fast path; explicit fallback otherwise.

---

## 1. Vision and Differentiation

The Rust ecosystem contains categorical traits, HKT emulation experiments, and free-monad generators, but no cohesive library treats **compile-time categorical composition as a performance compilation problem**. Existing libraries either accept trait-object indirection and boxing (because the abstraction *is* the runtime representation) or restrict themselves to correctness-first modeling without an execution engine.

This library occupies the gap: **a functional, category-theoretic API that compiles down to flat, fused imperative loops.** The user writes morphisms, functors, and monadic sequences; the proc macro deforests the composition, flattens it into SSA-style `let`-bound virtual registers, and emits monomorphic kernels that sit below LLVM's inlining and SROA thresholds. The categorical algebra is present at the type level and in the macro's validation; it is *absent* from the generated runtime representation.

**The claim we must prove before publishing:** for representative pipelines (`Vec` map/filter/fold chains, `Result` error-handling chains, Kleisli compositions), the macro-flattened output is at least as fast as idiomatic Rust iterator chains, and measurably faster than naive nested-closure equivalents. If the benchmark does not show this, the architecture is revised, not the benchmark.

---

## 2. Architectural Layers

```
┌─────────────────────────────────────────────────────────┐
│  USER SURFACE: pipeline! { ... }                        │
└──────────────────────┬──────────────────────────────────┘
                       │  TokenStream
                       ▼
┌─────────────────────────────────────────────────────────┐
│  PROC MACRO: syntax-directed compiler                   │
│  ┌───────────────────────────────────────────────────┐  │
│  │ 1. Parser (syn, custom keywords, raw TokenStream)  │  │
│  │ 2. Barrier annotation pass (structural_observer)   │  │
│  │ 3. Deforestation pass (linear-time TRS)            │  │
│  │ 4. SSA flattening codegen (quote_spanned)          │  │
│  └───────────────────────────────────────────────────┘  │
└──────────────────────┬──────────────────────────────────┘
                       │  Monomorphic Rust items
                       ▼
┌─────────────────────────────────────────────────────────┐
│  RUNTIME LAYER: witnesses + shallow traits + kernels    │
│  ┌───────────────────────────────────────────────────┐  │
│  │ Layer 0: HKT witness pattern (ZSTs, GATs)          │  │
│  │ Layer 1: Split-Trait Dual-Witness (stable GAT)     │  │
│  │ Layer 2: Monomorphized execution kernels           │  │
│  └───────────────────────────────────────────────────┘  │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
                 rustc / LLVM
```

### 2.1 Layer 0 — HKT Foundation (Witness Pattern)

Zero-sized witness structs proxy for type constructors. GATs provide the type-level projection. Witnesses are `#[repr(ZST)]`-equivalent (zero-sized, no runtime overhead).

```rust
pub trait HKT {
    type Type<T>;
}

pub struct VecWitness;
impl HKT for VecWitness {
    type Type<T> = Vec<T>;
}

pub struct OptionWitness;
impl HKT for OptionWitness {
    type Type<T> = Option<T>;
}
```

The macro generates or validates witnesses; users never write them by hand unless extending the library to a custom type constructor.

### 2.2 Layer 1 — Split-Trait Dual-Witness (Stable GAT Workaround)

Strict `Monad` implementations currently fail on stable Rust with `E0276`/`E0277` GAT normalization errors. The DeepCausality HAFT project has confirmed these compile unchanged under `-Znext-solver`, positioning the dual-witness pattern as a **transitional architecture**. We adopt the same posture: decouple the categorical operations into shallow traits that `rustc` normalizes linearly, and design the API so migration to strict `Monad` is non-breaking.

```rust
pub trait CategoricalFunctor: HKT {
    fn run_map<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where F: Fn(T) -> U;
}

pub trait CategoricalMonad: CategoricalFunctor {
    fn run_bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where F: Fn(T) -> Self::Type<U>;
}

pub trait CategoricalFilter: CategoricalFunctor {
    fn run_filter<T, P>(container: Self::Type<T>, p: P) -> Self::Type<T>
    where P: Fn(&T) -> bool;
}

pub trait CategoricalFold: HKT {
    fn run_fold<T, A, F>(container: Self::Type<T>, init: A, f: F) -> A
    where F: Fn(A, T) -> A;
}

pub trait CategoricalIntoIter: HKT {
    type Item<T>: Iterator<Item = T>;
    fn into_iter<T>(container: Self::Type<T>) -> Self::Item<T>;
}
```

Concrete implementations live in the same module and are `#[inline(always)]`.

### 2.3 Layer 2 — Monomorphized Execution Kernels

Each trait method is a kernel. Kernels are generic over `(T, U, F)` and monomorphize per call site. To bound binary size, the macro generates a **unified non-generic state wrapper** for pipelines that exceed a complexity threshold (see §6.3), rather than emitting a fresh kernel copy per stage.

---

## 3. The Macro Compiler

### 3.1 Parser

The parser consumes a `TokenStream` and produces a `Pipeline` AST. The AST records **structure**, not semantics. Closures are stored as raw `TokenStream` — the macro does not parse closure bodies into `syn::Expr`. This is a deliberate choice: parsing cost is zero for the parts we do not interpret, while `Span` information is preserved for diagnostics.

```rust
pub struct Pipeline {
    pub witness: syn::Ident,
    pub source: TokenStream,
    pub stages: Vec<Stage>,
    pub terminal: Terminal,
}

pub struct Stage {
    pub kind: StageKind,
    pub closure: TokenStream,
    pub span: Span,
    pub fusion_barrier: bool,       // blocks upstream fusion
    pub structural_observer: bool,  // Bind inspects container shape
}

pub enum StageKind { Map, Bind, Filter }

pub enum Terminal {
    Fold { init: TokenStream, closure: TokenStream, span: Span, is_pure: bool },
    Collect(Span),
    None,
}
```

Custom keywords (`witness`, `source`, `map`, `bind`, `filter`, `fold`) are declared via `syn::custom_keyword!` to avoid string comparison in the hot parse path.

### 3.2 Barrier Annotation

Barrier decisions are made **at parse time**, not in a separate pass. Each `Stage` carries two flags:

- `fusion_barrier` — `true` for `Filter` (absolute barrier) and for `Bind` whose closure is a structural observer.
- `structural_observer` — a conservative lexical approximation: the closure's token text contains `.len`, `.is_empty`, `.iter`, `Some`, `None`, `Ok`, or `Err`.

This is a **syntax-level conservative approximation**, not semantic analysis. It may produce false positives (rejecting a fusion that would have been safe). False positives are acceptable; false negatives are not. The user can override with an explicit escape hatch (see §4.2).

### 3.3 Deforestation Pass (Linear-Time TRS)

The pass walks the `Vec<Stage>` once, in `O(N)` time, applying four syntax-directed rewrite rules. The weakly-cartesian property of the monad is the **correctness argument** for the rewrites; the implementation is a straightforward pattern-match loop.

**Rule 1 — Adjacent `Map` fusion (unconditional).**
```
Map(f) → Map(g)   ⟹   Map(|x| g(f(x)))
```
Justified by functor composition: `map(g) ∘ map(f) = map(g ∘ f)`. No monad laws required.

**Rule 2 — `Map` → non-observing `Bind` fusion (Chin's producer/consumer).**
```
Map(f) → Bind(h)   where h is not a structural_observer
   ⟹   Bind(|x| h(f(x)))
```
Justified by the producer/consumer classification: `h` acts only on the element, not the container's shape, so absorbing the producer `f` into `h`'s argument position is safe.

**Rule 3 — `Filter` is an absolute barrier.** No rewrite crosses it. `Filter` is not a functor operation and the fusion would change evaluation semantics.

**Rule 4 — `Fold` absorbs a pure prefix chain.** If the `Fold` closure contains no `&mut` (lexical check: `is_pure`), the entire upstream `Map`/`Bind` chain is lowered directly into the fold's imperative loop body, eliminating intermediate container allocation. If `is_pure` is false, the pipeline falls back to `run_fold` on a materialized intermediate.

The fused closures use a hygienic synthetic identifier (`__pipeline_x`) to avoid capture of user identifiers. Because the closure bodies are opaque `TokenStream`, the macro composes by *application*, not by substitution — the generated code is `(|__pipeline_x| (g)((f)(__pipeline_x)))`, which is a valid composition of two arbitrary callables (function paths, closures, fn pointers).

### 3.4 SSA Flattening Codegen

The output is a flat sequence of `let` bindings, each binding one stage's result. `quote_spanned!` ties every token to the originating `Stage::span`, so `rustc` diagnostics point to the user's `map:` or `bind:` token rather than into macro internals.

```rust
let optimized_result = {
    let state_source = input_vector;

    let state_0 = <VecWitness as HKT>::Type::<_>::run_map(
        state_source,
        |__pipeline_x| { (|x| x * 2)(__pipeline_x) }   // fused two maps
    );

    let state_1 = <VecWitness as HKT>::Type::<_>::run_bind(
        state_0,
        |s| parse_tokens(&s)
    );

    let state_terminal = {
        let mut acc = 0;
        let loop_closure = |acc, s: String| acc + s.len();
        for item in <VecWitness as HKT>::Type::<_>::into_iter(state_1) {
            acc = loop_closure(acc, item);
        }
        acc
    };

    state_terminal
};
```

**What the flattening buys:** the generated code has no nested closure chain, no intermediate generic `Adapter` iterator structs layered by the standard library, and no deep recursive monomorphization. LLVM's SROA and inliner see flat basic blocks; the cost model does not time out on nested adapter layers.

**What it does not buy:** LLVM still owns final scheduling, vectorization, and register allocation. The macro shapes the input to the optimizer; it does not replace it.

---

## 4. Public API Surface

### 4.1 Primary Syntax — Head-Anchored Witness

The witness is the one piece of type information the macro cannot infer. Pin it at the pipeline head; element types are inferred by `rustc` down the chain.

```rust
let result = pipeline! {
    witness: VecWitness,
    source: input_vector,
    map: |x| x * 2,
    map: |x| x.to_string(),
    bind: |s| parse_tokens(&s),
    fold: (0usize, |acc, s| acc + s.len()),
};
```

### 4.2 Escape Hatches

- **Forced fusion past a false-positive observer:** `map!: force` / `bind!: force` — an explicit marker that suppresses the `structural_observer` flag for that stage. Documented as unsound if the closure actually observes structure.
- **Type ascription at source:** `source: input as Vec<i32>` — the macro derives `VecWitness` automatically, for users who prefer not to name witnesses.
- **Fallback to materialized semantics:** `bind!: materialize` forces an intermediate container, useful when the optimizer's fusion interacts badly with user code (e.g., when a later stage relies on `Vec`'s capacity or ordering guarantees that fusion would obscure).

### 4.3 Combinator Set (v0.1)

| Combinator | Status | Notes |
|---|---|---|
| `map` | v0.1 | Functor operation; fusion target |
| `bind` | v0.1 | Monadic sequence; the hard case |
| `filter` | v0.1 | Absolute barrier; no fusion |
| `fold` | v0.1 | Universal eliminator; absorbs pure prefixes |
| `pure` / `return` | v0.1 | Compile-time identity |
| `join` | v0.1 | Desugars to `bind(id)` |
| Kleisli `>=>` | v0.2 | Desugars to nested bind; fusion pass extended |
| `traverse` | v0.3+ | Deferred; traversal fusion is significantly more complex |
| `fold_map` | v0.2 | Fused eliminator; needs its own kernel |

---

## 5. Compiler Constraints and Mitigations

### 5.1 GAT Normalization (E0276/E0277)

The split-trait design avoids strict `Monad` on stable. Migration path: when `-Znext-solver` stabilizes, `CategoricalMonad` is superseded by a strict `Monad` supertrait without breaking user code (the `run_bind` method signature is compatible). The dual-witness layer stays as documentation of the transitional period; it is not a permanent API commitment.

### 5.2 Monomorphization Explosion

Zero-sized witnesses are runtime-free but compile-time expensive: each `(witness, element type)` pair instantiates every kernel. Mitigations:

- **Unified non-generic state wrapper** for complex pipelines: the macro emits a single `struct __PipelineState` with a non-generic iteration protocol, so the inner loop is monomorphized once per pipeline, not once per stage.
- **`Copy`-element fast path / generic fallback**: identical to the subtrait-layering pattern. Fast paths for `Copy` elements; boxed generic interpreter for the rest. Documented; not a silent cliff.
- **Per-pipeline expansion budget**: if a single `pipeline!` would emit more than ~200 lines (measured via `-Zmacro-stats`), the macro falls back to the unified wrapper.

### 5.3 Proc-Macro Compile Cost

`syn` parsing ~11k lines costs ~100ms per invocation; heavy proc-macro use has been measured to inflate code volume over 3×. Our mitigations:

- Parse closures as raw `TokenStream`, not `syn::Expr`. The parser only validates the pipeline grammar; closure bodies are opaque.
- Custom keywords avoid the `Ident`-to-`String` comparison path.
- The deforestation pass is `O(N)` in stage count, not `O(N²)` or worse.
- `-Zmacro-stats` is run on every CI build from the first commit. If expansion time or code volume exceeds budget, the PR is rejected.

### 5.4 The LLVM Boundary

The macro cannot force LLVM to vectorize or to preserve the flat structure. The claim is *plausible*, not guaranteed: flat `let`-bound code avoids the deep nested continuation structures that trigger SROA timeouts and inline-heuristic failures on nested closure chains. This is a bet on LLVM's cost model, and it is validated only by benchmark.

---

## 6. Validation Plan

### 6.1 Benchmark Suite (Criterion)

Three-way comparison over representative pipelines, on `Vec<i32>` and `Vec<String>` inputs of 10k, 100k, and 1M elements:

- **A. Naive nested closures** — the baseline the macro is meant to beat.
- **B. Idiomatic Rust iterator chains** — the baseline the macro must match.
- **C. Macro-flattened output** — the target.

Success criterion for v0.1: **C ≥ B** on all pipelines, and **C > A** on at least the map-heavy and fold-heavy pipelines. If C < B on any pipeline, the deforestation rule responsible is re-examined before merge.

Additional benchmarks:

- **`Result`-chain error handling**: 10-stage `bind`-heavy pipeline, cold and warm.
- **Kleisli composition**: two-morphism Kleisli chain over `OptionWitness`.
- **Compile-time**: `cargo build --timings` and `-Zmacro-stats` on a 50-pipeline fixture; expansion time and generated token volume recorded.

### 6.2 Correctness Tests (`trybuild`)

- Valid pipelines compile.
- Malformed pipelines (missing witness, mismatched closure arity, unsupported combinator) produce spanned errors pointing to the offending token.
- Fusion rules preserve semantics: property-based tests over `Vec`, `Option`, `Result` witnesses comparing fused and unfused execution on randomized inputs.

### 6.3 Expansion Budget

Each `pipeline!` expansion is measured for:
- Generated token count.
- Generated line count (post-formatting).
- Expansion wall time.

Budgets (initial, tunable):
- ≤ 200 lines per pipeline.
- ≤ 5 ms expansion per pipeline (excluding the macro crate's own compilation).
- No single kernel instantiated more than once per pipeline.

Pipelines exceeding budget fall back to the unified state wrapper, with a `#[deprecated]`-style warning emitted via `proc_macro::Diagnostic` if the threshold is breached in a user build.

---

## 7. Known Limitations (Published in README)

1. **Closure shape is constrained.** Fused closures are composed by application, not by substitution. A user closure that pattern-matches its argument destructures at the call boundary, not inside the fused body. Documented with examples.
2. **Structural observers are detected lexically.** A `bind` closure containing `.iter` anywhere in its body is treated as an observer, even if the call is dead code. Users can force fusion with `bind!: force`.
3. **No strict `Monad` on stable.** The dual-witness layer is a documented transitional architecture; migration is tracked against `-Znext-solver` stabilization.
4. **No `traverse` in v0.1.** Traversal fusion requires a different rewriting discipline and a different kernel shape.
5. **LLVM is not controlled.** The performance claims are empirically validated per-pipeline, not guaranteed universally.

---

## 8. Roadmap

| Milestone | Deliverable | Gate |
|---|---|---|
| **M0** | Parser + AST + `trybuild` error tests | All error tests pass; expansion budget met |
| **M1** | Split-trait runtime + `Vec`/`Option`/`Result` kernels | Kernel unit tests pass under stable Rust |
| **M2** | Deforestation pass + SSA codegen | Fused == unfused semantics (property tests) |
| **M3** | Criterion benchmark suite | C ≥ B on all pipelines; C > A on ≥ 2 |
| **M4** | `Result`-chain and Kleisli benchmarks | C ≥ B on both |
| **M5** | Public v0.1 release | M0–M4 gates met; README limitations published |
| **v0.2** | Kleisli, `fold_map`, escape hatches | Benchmark gates re-applied |
| **v0.3+** | `traverse`, strict `Monad` migration | Deferred pending `-Znext-solver` |

**M3 is the decision gate.** If the macro-flattened output does not match idiomatic iterator chains on at least the map-heavy and fold-heavy pipelines, the architecture returns to design review. No public API is committed until M3 passes.

---

## 9. Summary

The library's value proposition is narrow and falsifiable: **category-theoretic composition compiled to imperatively-fused loops, with a benchmark that proves it.** Every architectural decision in this document — raw `TokenStream` closures, linear-time deforestation, split-trait GAT workaround, explicit expansion budgets — is in service of that falsifiable claim. The category theory is the correctness argument for the rewrites and the vocabulary of the API. The imperative engine is the reason the library exists. The benchmark is the reason it will be believed.
