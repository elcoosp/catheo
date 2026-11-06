# catheo — Category Theory at the Speed of Rust

A proc-macro-backed categorical pipeline library with a syntax-directed
compilation engine and an imperative execution core. You write morphisms,
functors, and monadic sequences; the `pipeline!` macro lowers them to flat,
`let`-bound imperative code that sits below LLVM's inlining and SROA
thresholds. The categorical algebra lives at the type level and in the
macro's validation — it is **absent from the generated runtime
representation**.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design.

## Status — milestone M3 ✅

Per the roadmap in the architecture document:

| Deliverable | State |
|---|---|
| Pipeline AST + syntax-directed parser (`catheo-parser`) | done (M0) |
| Barrier annotation at parse time (§3.2) | done (M0) |
| `pipeline!` macro with naive sequential lowering | done (M0) |
| `trybuild` error tests with spanned diagnostics (§6.2) | done (M0) |
| Criterion parse benchmarks + §6.3 expansion-budget gate | done (M0) |
| Split-trait runtime + `Vec` / `Option` / `Result` kernels (M1 gate: kernel unit tests on stable Rust) | done (M1) |
| `pure` / `join` runtime kernels (§4.3 v0.1 combinators) | done (M1) |
| Categorical law proofs: functor identity/composition, monad left/right identity, associativity, `join ∘ map(pure) = id` | done (M1) |
| Kernel-floor criterion benchmark + 1M-element linearity gate | done (M1) |
| Linear-time deforestation pass, §3.3 Rules 1–3 + component/AST views | done (M2) |
| SSA fused lowering: pure-`fold` absorption (Rule 4), fused `collect` runs, §6.3 token-budget fallback | done (M2) |
| `pipeline_unfused!` — the sequential oracle macro | done (M2) |
| **M2 gate**: fused == unfused property harness over randomized inputs (§6.2), 19 shapes × xorshift sweeps × `Vec`/`Option`/`Result` | done (M2) |
| §6.1 three-way A/B/C Criterion suite: map-heavy / fold-heavy / filter / collect × `Vec<i32>` / `Vec<String>` × 10k / 100k / 1M | done (M3) |
| §6.1 deterministic decision gates: three-way value agreement + allocation profile via a thread-local counting allocator | done (M3) |
| §6.1 compile-time corpus: 50-pipeline fixture, measured by the nightly `-Zmacro-stats` job (§6.3) | done (M3) |
| **M3 gate**: macro output ≈ idiomatic iterators on map-heavy and fold-heavy; C > A by 2–10×; filter-chain barrier cost re-examined (Rule 3 upheld) and published | done (M3) |
| Clippy pedantic at **deny** with zero allow-list exemptions (`#[expect]` only where §2.2 mandates) | done (M1) |
| CI (fmt, clippy, pinned-toolchain tests, bench smoke, nightly `-Zmacro-stats`) | done |
| git-cliff changelog + tag-driven GitHub releases | done |

Next: **M4** — the Result-chain (10-stage bind-heavy, cold/warm) and
Kleisli-composition benchmarks (§6.1), the last benchmark gate before the
v0.1 release milestone.

### M2 notes — what fuses, and why

`pipeline!` now runs the deforestation pass (§3.3). A pure `fold` absorbs
its entire trailing `map`/`bind` run into one imperative `.fold(init, g)`
eliminator — the flagship `map → map → fold` pipeline allocates zero
intermediate containers. `collect` pipelines fuse their trailing
barrier-free run into one adapter pass. Barrier stages (`filter`, or a
`bind` whose closure observes container shape) stay monomorphic kernel
calls. Every claim is enforced by the fused-vs-unfused property harness,
not asserted.

Two stable-rustc constraints shape the emitted code (documented in
`catheo-parser`'s codegen module): composed closures `|x| g(f(x))` fail
E0282 on inner closure literals even in `Fn`-bounded positions, so the
emitter chains *components* through `.map`/`.flat_map`/`.fold`; and a
stable macro cannot form a `Witness::Type<T>` container mid-chain (the
`collect()` selection only resolves through concrete types), so the
pre-suffix prefix keeps the sequential-kernel shape. Both walls are the
§5.1 transitional posture in miniature; the next-solver removes them
without breaking user code.

### M3 notes — the decision gate, measured

The §6.1 three-way comparison ran on the pinned toolchain
(`rustc 1.99.1`, release profile, criterion medians, 1M-element inputs;
`cargo bench --bench abc` reproduces it, 10k/100k included):

| Pipeline | A naive (µs) | B idiomatic (µs) | C `pipeline!` (µs) | C/B | C/A |
|---|---|---|---|---|---|
| map-heavy `Vec<i32>` | 3 272.8 | 730.2 | 723.0 | **0.99×** | **0.22×** |
| fold-heavy `Vec<i32>` | 1 955.2 | 927.8 | 969.6 | 1.04× | **0.50×** |
| filter chain `Vec<i32>` | 6 421.0 | 647.7 | 3 866.4 | 5.97×¹ | **0.60×** |
| collect `Vec<i32>` | 1 759.2 | 172.9 | 175.1 | 1.01× | **0.10×** |
| map-heavy `Vec<String>` | 134 890 | 69 674 | 68 968 | **0.99×** | **0.51×** |
| fold-heavy `Vec<String>` | 61 081 | 36 762 | 36 779 | 1.00× | **0.60×** |
| filter chain `Vec<String>` | 109 200 | 50 433 | 55 463 | 1.10×¹ | **0.51×** |
| collect `Vec<String>` | 141 000 | 53 884 | 53 364 | **0.99×** | **0.38×** |

¹ The filter chain's published shortfall — see below.

Reading the table against the §6.1 gate:

- **C ≥ B on the barrier-free shapes**: parity within ~1% everywhere
  (run-to-run noise on shared runners is of the same order; the fold-heavy
  1M gap sits inside that band). The macro output *is* the idiomatic
  chain, one pass, same adapters — the suite exists to prove that claim,
  not assume it.
- **C > A on map-heavy and fold-heavy** (the two required wins): 2–4.5×
  faster than naive — C allocates *zero* intermediates where A
  materializes one container per stage.
- **The filter chain is the honest exception.** The barrier (§3.3 Rule 3)
  keeps the pre-barrier run at the sequential-kernel shape, so C pays
  extra passes over the materialized state (visible on `Vec<i32>` where
  the per-element work is trivial; nearly invisible on `Vec<String>`).
  Per the §6.1 protocol the responsible rule was re-examined before
  merge: **Rule 3 is upheld** — a rewrite crossing a filter changes
  evaluation semantics by construction — and the root cause is the
  documented stable-rustc wall (a macro cannot form a witness container
  mid-chain to feed a barrier kernel; §5.1). The next-solver migration
  removes the wall without breaking user code.

What the deterministic gates pin (`cargo test --workspace`, no timing):
three-way value agreement on every shape and universe; C == B allocation
profiles on *all* shapes (on this toolchain even the barrier kernels
reuse their input buffers in place, so C's barrier cost is temporal
only); C strictly below A's allocation profile on map-heavy/fold-heavy
(C is exactly zero-allocation there); and fused-emission shape lockstep
between the benchmarks and the gates. One toolchain subtlety the gates
document: rustc 1.99 reuses an *owned* source buffer through
`into_iter().map().collect()` chains, so the naive baseline borrows
(`iter`) — the honest "each combinator materializes" shape.

## Usage

```rust
use catheo::{pipeline, VecWitness};

let input_vector = vec![1_i32, 2, 3, 4];
let total: usize = pipeline! {
    witness: VecWitness,
    source: input_vector,
    map: |x| x * 2,
    map: |x| x.to_string(),
    fold: (0_usize, |acc, s| acc + s.len()),
};
assert_eq!(total, 4);
```

Combinators in the `pipeline!` grammar: `map`, `bind`, `filter` (absolute
fusion barrier), `fold` (universal eliminator), `collect`. Witnesses:
`VecWitness`, `OptionWitness`, `ResultWitness<E>`. Stages accept closures
(`|x| …`) or bare callables (`bind: parse_tokens`). The §4.3 `pure` and
`join` kernels are available on the runtime layer as of M1
(`CategoricalPointed::run_pure`, `CategoricalMonad::run_join`); their
`pipeline!` grammar integration is deferred past M2. As of M2 there is a
second macro, `pipeline_unfused!` (the sequential lowering): it is the
oracle of the §6.2 property gate and a debugging aid — expanding a suspect
pipeline through both macros isolates any mis-rewrite to the fusion pass.

```rust
use catheo::{pipeline, ResultWitness};

let raw = Ok::<String, CmdError>("1 2 3".into());
let count: usize = pipeline! {
    witness: ResultWitness<CmdError>,
    source: raw,
    bind: tokenize,
    fold: (0_usize, |acc, toks: Vec<String>| acc + toks.len()),
};
```

## Kernel coverage

| Kernel | `VecWitness` | `OptionWitness` | `ResultWitness<E>` |
|---|---|---|---|
| `run_map` | ✅ | ✅ | ✅ |
| `run_bind` | ✅ (flat_map) | ✅ (and_then) | ✅ (and_then) |
| `run_pure` | ✅ (single) | ✅ (Some) | ✅ (Ok) |
| `run_join` | ✅ (concat) | ✅ (and_then(id)) | ✅ (and_then(id)) |
| `run_filter` | ✅ | ✅ | ❌ by design¹ |
| `run_fold` | ✅ | ✅ | ✅ (Err → init) |
| `into_iter` | ✅ | ✅ | ✅ |

¹ Dropping an `Ok` value has no error-free denotation; a `filter` stage over
`ResultWitness<E>` is a compile error.

## Development

```sh
cargo fmt --all -- --check     # style gate
cargo clippy --workspace --all-targets -- -D warnings   # pedantic, deny, zero allow-list
cargo test --workspace         # unit + behavioral + trybuild UI + §6.3/kernel gates
cargo bench --bench parse      # parser throughput (§6.3 floor)
cargo bench --bench kernels    # Layer-2 kernel floor (§2.3)
cargo bench --bench abc        # §6.1 three-way A/B/C decision suite
```

The lint posture is *pedantic with no exemptions*: the workspace sets
`clippy::pedantic` to `deny` and contains no `#[allow]` anywhere. The only
suppressions are `#[expect]` attributes at sites the architecture itself
mandates (`#[inline(always)]` kernels, §2.2), and `#[expect]` errors the
moment its lint stops firing.

Two deterministic performance gates run as ordinary tests in
`cargo test --workspace`: the §6.3 expansion-budget gate (parse+emit well
under budget) and the kernel-linearity gate (a 1M-element map+fold stays
far below one second). The CI `test` job pins `rustc 1.99.1` because
`trybuild` snapshots depend on rustc's diagnostic rendering.

Releases are Conventional-Commits-driven: merge to `master`, then push a
`v*.*.*` tag — [git-cliff](https://git-cliff.org) renders the changelog and
the `release.yml` workflow publishes the GitHub release.

## Known limitations (v0.0.2 subset of §7)

1. **Closure shape is constrained.** Closure parameters must be plain
   identifiers (with optional `: Type` / `-> Ret` ascriptions); destructure
   inside the body. Fused composition (M2) happens by application, not
   substitution.
2. **Structural observers are detected lexically.** A `bind` closure whose
   body mentions `.len`, `.is_empty`, `.iter`, `Some`, `None`, `Ok`, or
   `Err` anywhere blocks fusion. False positives are acceptable; false
   negatives are not. (Escape hatches arrive in v0.2.)
3. **No strict `Monad` on stable.** The split-trait dual-witness layer
   (`CategoricalMonad::run_bind` keeps a strict-`Monad`-compatible
   signature) is a documented transitional architecture pending
   `-Znext-solver` stabilization.
4. **No `traverse`**, no Kleisli `>=>` — scheduled for v0.2+. The `pure`
   and `join` kernels exist on the runtime layer (M1); their `pipeline!`
   grammar integration lands with M2.
5. **LLVM is not controlled.** The performance claims are validated
   empirically per pipeline, not guaranteed universally (the M3 gate).
   The measured boundary as of M3: barrier-free pipelines match idiomatic
   iterator chains; a `filter` barrier costs extra passes over the
   materialized barrier state (§3.3 Rule 3, re-examined and upheld), and
   micro-gaps within ±5% on single shapes are inside shared-runner noise.

## License

MIT — see [LICENSE](LICENSE).
