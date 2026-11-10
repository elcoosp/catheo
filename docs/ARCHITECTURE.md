# Category Theory at the Speed of Rust

## Architecture Specification — v0.1

**Status:** Design-complete for the v0.1 prototype; maintained through the v0.6 release (§4.3.1 traverse, §4.3.2 join, §4.4 the strict `Monad` layer, §4.3.3 the partial arrow). Benchmark-gated before any public API commitment.
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
│  │ Layer 1.5: strict Monad unification (v0.5, §2.4)   │  │
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

Each trait method is a kernel. Kernels are generic over `(T, U, F)` and monomorphize per call site. To bound compile-time expansion, a pipeline that breaches the §6.3 token budget falls back to the sequential lowering (`pipeline_unfused!`'s shape) — never to a wider generic surface.

### 2.4 Layer 1.5 — the Strict `Monad` Unification (v0.5)

The consumer-facing unification of the split layer: `pure` and `bind` in **one** trait, shipped additively in v0.5 (the section number follows Layer 2 to keep the v0.1–v0.4 §-references stable). Every witness implements the trait by **delegating** to the §2.2 kernels — the split layer remains the single lowering, no kernel body is duplicated, and the `pipeline!` grammar plus emitted code are untouched (the §6.1 gates re-apply verbatim).

```rust
pub trait Monad: HKT {
    fn pure<T>(value: T) -> Self::Type<T>;
    fn bind<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where F: Fn(T) -> Self::Type<U>;

    // provided: join(m) is bind(m, identity) (§4.3.2)
    fn join<T>(container: Self::Type<Self::Type<T>>) -> Self::Type<T> { /* bind(id) */ }
    // provided: the functor operation, derived
    fn fmap<T, U, F>(container: Self::Type<T>, f: F) -> Self::Type<U>
    where F: Fn(T) -> U { /* bind(m, |x| pure(f(x))) */ }
}
```

Why additive and not the §5.1 supertrait migration: folding the split traits into a `Monad` **supertrait with default bodies over a generic `Self`** still fails on stable Rust with `E0276`/`E0277` (the shape-generic wall). A fresh per-witness trait with required kernels needs no such normalization — single-level `Self::Type<T>` projections are exactly the shape `rustc` already accepts (`kleisli`, `run_join`). The deliverable is therefore: one trait to bound on at the consumer seam, laws-tested per witness (`tests/v05_gates.rs`), `fmap == run_map` and `join == run_join == bind(id)` pinned against the split kernels, and the allocation profile pinning strict == split-kernel exactly — the abstraction is free. Full surface migration stays tracked against `-Znext-solver` stabilization (§7 limitation 3, §8 roadmap).

The runnable demonstration is `crates/catheo/examples/strict_monad.rs` (the §6.1 observable seam posture: `tracing` spans around the program, `miette` help text on the error path, both dev-only). The traversal boundary stays the §4.3.1 split-kernel discipline — `traverse` is not part of the `Monad` layer, and the example crosses it explicitly.

---

## 3. The Macro Compiler

### 3.1 Parser

The parser consumes a `TokenStream` and produces a `Pipeline` AST. The AST records **structure**, not semantics. Closures are stored as raw `TokenStream` — the macro does not parse closure bodies into `syn::Expr`. This is a deliberate choice: parsing cost is zero for the parts we do not interpret, while `Span` information is preserved for diagnostics.

```rust
pub struct Pipeline {
    pub witness: Witness,           // path + span (§4.1 head anchor)
    pub source: Source,             // raw tokens + optional `as` ascription
    pub stages: Vec<Stage>,
    pub terminal: Terminal,
}

pub struct Stage {
    pub kind: StageKind,
    pub closure: Closure,           // validated header + opaque body tokens
    pub span: Span,
    pub fusion_barrier: bool,       // blocks upstream fusion
    pub structural_observer: bool,  // Bind inspects container shape
    pub modifier: StageModifier,    // §4.2: None | Force | Materialize
    pub effect: Option<syn::Path>,  // §4.3 v0.3: the traverse effect witness
}

pub enum StageKind { Map, Bind, Filter, Traverse }

pub enum Terminal {
    Fold { init: TokenStream, closure: Box<Closure>, span: Span, is_pure: bool },
    Collect(Span),
    FoldMap { closure: Box<Closure>, span: Span },  // §4.3 v0.2
    None,
}
```

The sketch tracks the v0.4 grammar; the authoritative definitions live in
`crates/catheo-parser/src/ast.rs`.

Custom keywords (`witness`, `source`, `map`, `bind`, `filter`, `join`,
`kleisli`, `traverse`, `fold`, `fold_map`, `collect`, plus the §4.2 hatch
markers `force`/`materialize`) are declared via `syn::custom_keyword!` to
avoid string comparison in the hot parse path.

### 3.2 Barrier Annotation

Barrier decisions are made **at parse time**, not in a separate pass. Each `Stage` carries two flags:

- `fusion_barrier` — `true` for `Filter` (absolute barrier) and for `Bind` whose closure is a structural observer.
- `structural_observer` — a conservative lexical approximation: the closure's token text contains `.len`, `.is_empty`, `.iter`, `Some`, `None`, `Ok`, or `Err`.

Both checks live in the `observer` module (`crates/catheo-parser/src/observer.rs`): `is_structural_observer` (the dot-method and constructor scan) and `is_lexically_pure` (the `&mut` scan feeding Rule 4's `is_pure`).

This is a **syntax-level conservative approximation**, not semantic analysis. It may produce false positives (rejecting a fusion that would have been safe). False positives are acceptable; false negatives are not. The user can override with an explicit escape hatch (see §4.2).

Since v0.6 the scan is **kind-aware for the partial arrow** (`filter_map`, §4.3.3): a `filter_map` arrow lands in `Option`, so its `Some`/`None` tokens are the arrow's *effect constructors* — flattened away by the kernel before anything downstream could observe them — and scanning them would flag every idiomatic spelling as a barrier (a false positive the stage could never recover from). The partial-arrow scan (`crates/catheo-parser/src/observer.rs::is_partial_arrow_observer`) keeps the method idents (`.len`, `.is_empty`, `.iter`) and drops the constructor idents; the composed-body rescan in the deforestation pass applies the same refinement when an upstream producer absorbs into a partial arrow. The soundness argument is unchanged: a closure whose body never mentions the shape methods cannot inspect the container it is fed.

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
| `filter_map` | v0.6 | The partial arrow (§4.3.3); bind-shaped fusion, the methods-only scan |
| `fold` | v0.1 | Universal eliminator; absorbs pure prefixes |
| `pure` | v0.1 | Runtime kernel only — `CategoricalPointed::run_pure` (§2.2, the pointed layer); never a pipeline stage keyword |
| `join` | v0.4 | Bare-keyword stage; desugars to `bind(id)` (§4.3.2) |
| Kleisli `>=>` | v0.2 | Desugars to nested bind; fusion pass extended |
| `traverse` | v0.3 | Traversable eliminator; two-sided boundary discipline (§4.3.1) |
| `fold_map` | v0.2 | Fused eliminator; needs its own kernel |

#### 4.3.1 The Traversal Boundary (v0.3)

v0.3 delivers `traverse` as a *boundary* stage, not a kernel-shaped one — the "different rewriting discipline and a different kernel shape" limitation 4 predicted. The grammar:

```rust
let total: Result<u64, GateError> = pipeline! {
    witness: VecWitness,
    source: input,
    map: |x| x.wrapping_mul(3),
    traverse: (ResultWitness<GateError>, try_u64),
    fold: (0_u64, u64::wrapping_add),
};
```

Parse-enforced constraints, each with a spanned rejection at the offending token:

- **Exactly one boundary per pipeline.** The traversal re-anchors the element flow inside the effect guard; a second `traverse` would re-wrap an already-guarded suffix in an effect the one-witness grammar cannot name.
- **The effect is a built-in pointwise witness:** `OptionWitness` or `ResultWitness<E>`. `VecWitness` is rejected with an explanation (`Vec` traversal is cartesian, not pointwise); other effects are outside the kernel set.
- **The container is `Vec`.** Only a multi-element container has a traversal; `Option`/`Result` sources are pointwise (the §6.1 M4 posture). The witness entry or a `Vec` ascription both satisfy this.
- **The arrow is a one-argument closure** `A -> G<B>`; no `!` modifier exists for this stage.

The lowering is two-sided:

1. **The absorbable prefix chains into the traversal's iterator.** The maximal non-barrier run before the boundary rides the traversal's own adapter chain — `into_iter(xs).map(g)` feeds `run_traverse`, which appends its arrow and collects. No intermediate container exists between the source and the traversal's collected `Vec`. Barriered head stages stay sequential kernels, and the chain starts at their register.
2. **The suffix + terminal lower inside the effect guard.** The traversal's result is the effect around the collected container, so the suffix pipeline is emitted inside the `Ok`/`Some` arm over the guard's inner binding; the `Err`/`None` arm forwards the effect unchanged. A pure fold terminal absorbs its own trailing run inside the arm (Rule 4 composes across the guard). The guard is a semantic shape — emitted identically by the fused and unfused lowerings — not a fusion artifact.

The kernel is [`CategoricalTraverse::run_traverse`](../crates/catheo/src/categorical.rs), whose impls are the idiomatic std spellings — `items.map(f).collect::<Result<Vec<U>, E>>()` — so the fused output *is* the B-parity elimination by construction. The inner container is pinned to `Vec<U>`: the stable solver (§5.1) cannot normalize the doubly-projected `Self::Type<W::Type<U>>` a shape-generic signature would need.

Validation: `tests/v03_gates.rs` pins emission shape, three-way value agreement, and the allocation profile deterministically (§6.1 style, no wall-clock gates); `benches/traverse.rs` re-applies the §6.1 three-way protocol over warm and cold universes — the verdict and the re-examination record are in §6.1.

#### 4.3.2 The Join Stage (v0.4)

§4.3 promised `join` — "desugars to `bind(id)`" — from the first cut of the combinator table, but the grammar never admitted it: the only spelling was a hand-written `bind: |x| x`, which reads as a no-op and carries no documentation or diagnostics. v0.4 closes that runtime-to-grammar seam. The entry is the bare keyword — the desugar has no user-supplied callable, so `join:` or any argument is rejected, spanned at the offending token, as are the §4.2 hatches and a position after the terminal.

The desugar is the `parse_kleisli` posture in miniature: one ordinary `StageKind::Bind` stage whose closure is the hygienic identity (`__pipeline_join`), so the fusion pass needs no new rule — Rules 1–2 apply across it, Rule 4 absorbs over it, and the §3.2 rescan re-flags the product when an absorbed body observes shape. The lowering is the one deliberate refinement: in adapter chains the identity renders as std's `.flatten()`, and a map fused in front of it (Rule 2) takes the `.flat_map` position instead of forming `.map(f).flatten()` — value-identical eliminations on every witness, and the spellings user-side lints (`clippy::flat_map_identity`, `clippy::map_flatten`) rightly demand. The kernel path is untouched: `CategoricalMonad::run_join` (the provided `run_bind(id)`) dispatches for the pointwise witnesses exactly as before.

Validation: `tests/v04_gates.rs` pins the emission shape, three-way value agreement, the allocation profile, and oracle agreement over every witness (deterministic, §6.1 style); `benches/join.rs` re-applies the §6.1 three-way protocol — over the map-prefix flatten shape (10k/100k/1M), the fused `flat_map` chain holds parity with B (C ≈ B within noise) and A, the two-materialization spelling, is ~27× slower. Multiple `join` stages are admitted (each flattens one level); the runtime half (`run_join`, `tests/runtime_pointed_join.rs`) shipped with the v0.1 kernels and is unchanged.

#### 4.4 The Strict Monad Layer (v0.5)

The consumer-facing one-trait surface (§2.4): `catheo::Monad` carries `pure` + `bind`, with `join` provided (`bind(id)`, §4.3.2) and `fmap` derived (`bind ∘ pure`). A consumer program bounds on a single `Monad` parameter and runs, monomorphized, across every witness — the generic body `fn checked_doubling<M: Monad>(program: M::Type<i32>) -> M::Type<i64>` instantiates for `VecWitness`, `OptionWitness`, and `ResultWitness<E>` without naming the split-trait pair.

The runtime layer is the *eager* monad semantics, by design (§2.3): each `bind` collects its container and each `pure` allocates its single-element one. B-parity on the fused path remains the macro's compile-time job (`benches/abc.rs`); the pointwise witnesses, with no container to build, hold C == B at the runtime layer too. The allocation profile is pinned exactly: strict == split-kernel spelling (the bound is free), and the pointwise strict chain == the idiomatic one-pass chain.

| Entry | Role |
|---|---|
| `Monad::pure` | the pointed kernel, unified (§4.3 `pure`) |
| `Monad::bind` | the monadic kernel, unified — the required pair |
| `Monad::join` | provided: `bind(id)` (§4.3.2) |
| `Monad::fmap` | provided, derived: `bind(m, \|x\| pure(f(x)))` |

Validation: `tests/v05_gates.rs` pins the monad laws per witness over the xorshift sweep, the derived/provided kernels against the split layer, three-way value agreement, and the allocation profile (deterministic, §6.1 style); `benches/monad.rs` re-applies the §6.1 three-way protocol over the pointwise universes — warm and cold, `Option` and `Result`: C ≈ B within noise on every universe (the bound erases under monomorphization). The runnable consumer demonstration, with the observable seam, is `examples/strict_monad.rs`.

#### 4.3.3 The Partial Arrow (v0.6)

§4.3's grammar carried a hole on the seam between the total arrow (`map:`) and the container-to-container arrow (`bind:`): the most idiomatic Rust arrow of all — the *partial* arrow `T -> Option<U>` — had no spelling. The only workaround was the riddle: hand-rolling the arrow through the `bind` kernel with a per-element re-collect (`bind: |x| f(x).into_iter().collect::<Vec<_>>()`), a shape that contradicts the §6.2 posture on the positive surface and allocates a container per kept element. v0.6 closes the seam with `filter_map:` — the kernel the ecosystem already names: std's `filter_map` on `Vec`, `and_then` on `Option`.

The categorical reading is the pointed-arrow translation: a bind whose arrow lands in the pointed sub-effect `Option` instead of the witness's own container, flattened by construction (`Option: IntoIterator`, so the kernel *is* std's `filter_map` elimination — `into_iter(xs).filter_map(f).collect()`). That makes the fusion story Rule 2 verbatim: `map(f) → filter_map(g)` composes by application into one adapter pass, Rule 4 absorbs a non-observing partial arrow into a pure `fold`, and the stage rides the absorbable suffix and the traversal's prefix chain exactly like a bind. Where `filter` is an absolute barrier by design (Rule 3), `filter_map` delivers the filter+map *semantics* with bind's fusion *discipline* — the pre-v0.6 filter+map spelling paid a barrier, a container, and a pass; the partial arrow pays neither.

Two boundaries keep the surface honest, mirroring the `filter` posture: `ResultWitness<E>` has **no** kernel impl (a dropped `Ok` has no error-free denotation — a fabricated `Err` would be a value the user never produced), so a `filter_map` over that witness is an `E0277` spanned at the witness token; and the §3.2 scan is kind-aware — the arrow's `Some`/`None` constructors are its effect, so only the shape methods (`.len`, `.is_empty`, `.iter`) flag the stage as an observing barrier (the §4.2 hatches apply: `filter_map!: force` overrides the methods scan, `filter_map!: materialize` pins the intermediate).

Validation: `tests/v06_gates.rs` pins the kernel against the std eliminations, three-way value agreement over the sweep (A the riddle, B the idiomatic chain, C the pipeline), the allocation profile (C == B exactly — zero intermediates; A allocates per kept element — the riddle retired), the scan refinement (the inline constructor spelling still fuses; `.iter` still barriers), and fused == unfused oracle agreement over every admitted shape (deterministic, §6.1 style). `benches/filter_map.rs` re-applies the §6.1 three-way protocol — the verdict is in §6.1. The runnable consumer demonstration, with the observable seam, is `examples/filter_map_pipeline.rs`.

---

## 5. Compiler Constraints and Mitigations

### 5.1 GAT Normalization (E0276/E0277)

The split-trait design avoids the strict-`Monad` **supertrait** on stable. Since v0.5 the consumer-facing unification is available additively (§2.4/§4.4): a fresh per-witness `Monad` trait with required `pure`/`bind` kernels needs no shape-generic normalization, so it compiles on stable — while the supertrait migration of the *macro surface* (split traits folded into a GAT hierarchy with default bodies over generic `Self`) still waits. Migration path: when `-Znext-solver` stabilizes, the split layer is superseded without breaking user code (the `run_bind` method signature is compatible with `Monad::bind`). The dual-witness layer stays as the single lowering and documentation of the transitional period; it is not a permanent API commitment.

### 5.2 Monomorphization Explosion

Zero-sized witnesses are runtime-free but compile-time expensive: each `(witness, element type)` pair instantiates every kernel. The shipped mitigations:

- **Shallow traits, linear normalization (§2.2)**: every kernel signature projects `Self::Type<T>` exactly one level, so each monomorphization normalizes linearly — the shape-generic wall is what the split layer exists to route around.
- **Pointwise witnesses keep the kernel shape (§6.1 M4)**: on `OptionWitness`/`ResultWitness<E>` there is no container to deforest, so the fused lowering stays at one monomorphic kernel per stage instead of layering adapters over a zero-or-one-element iterator — measured at parity, not assumed.
- **Per-pipeline expansion budget**: if a single `pipeline!` would emit more than the §6.3 token budget, the macro falls back to the sequential lowering — the M0 shape, semantically identical (§6.2), and never larger than the user's own closure bodies (`codegen.rs`).

### 5.3 Proc-Macro Compile Cost

`syn` parsing ~11k lines costs ~100ms per invocation; heavy proc-macro use has been measured to inflate code volume over 3×. Our mitigations:

- Parse closures as raw `TokenStream`, not `syn::Expr`. The parser only validates the pipeline grammar; closure bodies are opaque.
- Custom keywords avoid the `Ident`-to-`String` comparison path.
- The deforestation pass is `O(N)` in stage count, not `O(N²)` or worse.
- `-Zmacro-stats` runs on every CI build as the *advisory* nightly job (nightly drift must not block unrelated work): its artifact is the evidence trail the §6.3 budget is audited against, not a merge gate.

### 5.4 The LLVM Boundary

The macro cannot force LLVM to vectorize or to preserve the flat structure. The claim is *plausible*, not guaranteed: flat `let`-bound code avoids the deep nested continuation structures that trigger SROA timeouts and inline-heuristic failures on nested closure chains. This is a bet on LLVM's cost model, and it is validated only by benchmark.

---

## 6. Validation Plan

### 6.1 Benchmark Suite (Criterion)

Three-way comparison over representative pipelines, on `Vec<i32>` and `Vec<String>` inputs of 10k, 100k, and 1M elements:

- **A. Naive nested closures** — the baseline the macro is meant to beat.
- **B. Idiomatic Rust iterator chains** — the baseline the macro must match.
- **C. Macro-flattened output** — the target.

Wall-clock is never the CI gate: the deterministic instruments are the allocation-counting gates of `crates/catheo-counting-alloc` (a thread-local counting allocator over `std::alloc::System`, dev-only — the crate is the repository's entire `unsafe` surface and no shipped artifact links it). `tests/v02_gates.rs`–`tests/v05_gates.rs` pin three-way value agreement, allocation profiles, and — where the macro is involved — fused-emission shape lockstep as plain `cargo test` gates, so a fused pipeline that silently regresses to materializing intermediates changes its count and fails the gate on shared runners without wall-clock flakiness.

Success criterion for v0.1: **C ≥ B** on all pipelines, and **C > A** on at least the map-heavy and fold-heavy pipelines. If C < B on any pipeline, the deforestation rule responsible is re-examined before merge.

**v0.3 record (traverse).** The §8 v0.3 gate re-applied this protocol over `benches/traverse.rs`: `Vec<i32>` sources, an `i32 -> Result<u64, GateError>` traversal, warm (every element passes) and cold (first element fails) universes, 10k/100k/1M. Warm parity holds at all three sizes — C within noise of B, A ≈ 3× slower. Cold, the timed region is a first-element short-circuit plus a buffer free, and absolute cross-run numbers drift heavily on shared hardware (B itself moved 73 → 138 ns across runs of the *same* binary), so only within-run comparisons were treated as evidence. Within one long-window run (10 s measurement, 100 samples, CI ≈ ±0.5%): 100k parity (C +0.4%), 1M with C 6% *faster*, and 10k with C +16 ns under non-overlapping CIs. Per the protocol, the rule responsible — prefix absorption into the traversal's iterator — was re-examined before merge: the kernel is verbatim B's elimination (`items.map(f).collect()`, §4.3.1), the cold path carries no extra container or pass, and the residual delta does not scale with *n* and inverts sign at 1M — the signature of per-binary code layout, not of a rewriting-rule defect. Delivered on that evidence.

**v0.5 record (the strict Monad layer).** The §8 v0.5 gate re-applied the protocol over `benches/monad.rs` — the pointwise universes (`Option` warm/cold, `Result` warm/cold, per-call): C ≈ B within noise on every universe (all four A/B/C routines land in the same ~1.1 ns warm / ~2.0 ns cold band), and C ≈ A — the one-trait bound erases under monomorphization, leaving B's spelling. The eager `Vec` shape publishes its floor deterministically instead of by wall-clock: `tests/v05_gates.rs` pins strict == split-kernel allocation counts exactly (the abstraction adds nothing over the kernels), the pointwise strict chain == the idiomatic one-pass chain, and the C > B/C > A eager gap is asserted with its §2.3 rationale — the fused answer lives in the macro pass (§4.4).

**v0.6 record (the partial arrow).** The §8 v0.6 gate re-applied the protocol over `benches/filter_map.rs`. The container universes (`Vec<i32>`, the evens-kept `partial_double` arrow, a pure fold terminal, 10k/100k/1M): C within ±1.5% of B on every size (10k −0.8%, 100k +0.6%, 1M +1.5% — the kernel *is* the B spelling, so the deltas are per-binary code layout), and A — the pre-v0.6 bind riddle — 15.9–23.4× slower, the per-element container materialization the stage retires. The pointwise universes (`Option` warm/cold, per-call) land in the same ~1.12 ns band across A/B/C: on a zero-or-one-element container the naive match spelling *is* the idiomatic one, and the stage erases into `and_then`. The deterministic consequences (C == B allocations exactly, the riddle's per-element profile, the scan-refinement shapes) are pinned in `tests/v06_gates.rs`.

Additional benchmarks:

- **`Result`-chain error handling**: 10-stage `bind`-heavy pipeline, cold and warm.
- **Kleisli composition**: two-morphism Kleisli chain over `OptionWitness`.
- **Compile-time**: `cargo build --timings` and `-Zmacro-stats` on a 50-pipeline fixture; expansion time and generated token volume recorded.

### 6.2 Correctness Tests (`trybuild`)

- Valid pipelines compile.
- Malformed pipelines (missing witness, mismatched closure arity, unsupported combinator) produce spanned errors pointing to the offending token.
- Fusion rules preserve semantics: property-based tests over `Vec`, `Option`, `Result` witnesses comparing fused and unfused execution on randomized inputs.
- The consumer-facing diagnostic posture ("a rejection is never a riddle") is carried by the runnable demos: `miette` help text on every error path (`examples/instrumented_pipeline.rs`, `examples/monadic_surface.rs`, `examples/strict_monad.rs`, `examples/escape_hatches.rs`), pinned by the consumer-surface gate (`tests/consumer_examples.rs`).

### 6.3 Expansion Budget

Each `pipeline!` expansion is measured for:
- Generated token count — the enforced budget: **2,000 tokens** (`codegen.rs`'s `EXPANSION_TOKEN_BUDGET`, the documented 10-tokens-per-line conversion of the original ≤ 200-line budget; printed token streams do not preserve line breaks).
- Generated line count and expansion wall time — advisory, recorded by the nightly `-Zmacro-stats` CI artifact.

Pipelines exceeding the token budget fall back to the sequential lowering — byte-for-byte `pipeline_unfused!`'s shape, semantically identical (§6.2), never wider than the user's own closure bodies. No warning is emitted: the fused output can only ever be smaller than the sequential one, so the fallback is silent by design and the `-Zmacro-stats` artifact is the evidence trail.

---

## 7. Known Limitations (Published in README)

1. **Closure shape is constrained.** Fused closures are composed by application, not by substitution. A user closure that pattern-matches its argument destructures at the call boundary, not inside the fused body. Documented with examples.
2. **Structural observers are detected lexically.** A `bind` closure containing `.iter` anywhere in its body is treated as an observer, even if the call is dead code. Users can force fusion with `bind!: force`.
3. **No strict `Monad` on stable.** The macro surface keeps the dual-witness split-trait layer — folding the split traits into a GAT supertrait hierarchy over a generic `Self` still hits the `E0276`/`E0277` shape-generic wall; migration is tracked against `-Znext-solver` stabilization. The consumer-facing unification shipped additively in v0.5 (§2.4/§4.4): one `Monad` trait, delegating to the split kernels, laws- and allocation-gated.
4. **`traverse` arrived in v0.3, and its shape is constrained.** The kernel collects into a pinned `Vec<U>` (the stable solver cannot normalize a shape-generic signature, §5.1), the effect must be a built-in pointwise witness, and a pipeline admits exactly one traversal boundary. The two-sided lowering discipline is documented in §4.3.1.
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
| **v0.3** | `traverse` — two-sided boundary discipline, `CategoricalTraverse` kernel | §6.1 gates re-applied; cold re-examination recorded (§6.1) |
| **v0.4** | `join` — the §4.3 monadic multiplication on the pipeline surface | §6.1 gates re-applied (deterministic forms in `tests/v04_gates.rs`) |
| **v0.5** | the strict `Monad` layer — the §2.4/§4.4 one-trait unification, additive | §6.1 gates re-applied (laws + allocation profile in `tests/v05_gates.rs`; pointwise verdict in `benches/monad.rs`); surface migration still tracked against `-Znext-solver` |
| **v0.6** | `filter_map` — the §4.3.3 partial arrow, additive over the kernel set | §6.1 gates re-applied (value + allocation profile in `tests/v06_gates.rs`; three-way verdict in `benches/filter_map.rs`: C ≈ B, C > A) |

**M3 is the decision gate.** If the macro-flattened output does not match idiomatic iterator chains on at least the map-heavy and fold-heavy pipelines, the architecture returns to design review. No public API is committed until M3 passes.

---

## 9. Summary

The library's value proposition is narrow and falsifiable: **category-theoretic composition compiled to imperatively-fused loops, with a benchmark that proves it.** Every architectural decision in this document — raw `TokenStream` closures, linear-time deforestation, split-trait GAT workaround, explicit expansion budgets — is in service of that falsifiable claim. The category theory is the correctness argument for the rewrites and the vocabulary of the API. The imperative engine is the reason the library exists. The benchmark is the reason it will be believed.
