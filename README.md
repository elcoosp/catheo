# catheo — Category Theory at the Speed of Rust

A proc-macro-backed categorical pipeline library with a syntax-directed
compilation engine and an imperative execution core. You write morphisms,
functors, and monadic sequences; the `pipeline!` macro lowers them to flat,
`let`-bound imperative code that sits below LLVM's inlining and SROA
thresholds. The categorical algebra lives at the type level and in the
macro's validation — it is **absent from the generated runtime
representation**.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the full design.

## Status — milestone M1 ✅

Per the roadmap in the architecture document:

| Deliverable | State |
|---|---|
| Pipeline AST + syntax-directed parser (`catheo-parser`) | done (M0) |
| Barrier annotation at parse time (§3.2) | done (M0) |
| `pipeline!` macro with naive sequential lowering (deforestation lands in M2) | done (M0) |
| `trybuild` error tests with spanned diagnostics (§6.2) | done (M0) |
| Criterion parse benchmarks + §6.3 expansion-budget gate | done (M0) |
| Split-trait runtime + `Vec` / `Option` / `Result` kernels (M1 gate: kernel unit tests on stable Rust) | done (M1) |
| `pure` / `join` runtime kernels (§4.3 v0.1 combinators) | done (M1) |
| Categorical law proofs: functor identity/composition, monad left/right identity, associativity, `join ∘ map(pure) = id` | done (M1) |
| Kernel-floor criterion benchmark + 1M-element linearity gate | done (M1) |
| Clippy pedantic at **deny** with zero allow-list exemptions (`#[expect]` only where §2.2 mandates) | done (M1) |
| CI (fmt, clippy, pinned-toolchain tests, bench smoke, nightly `-Zmacro-stats`) | done |
| git-cliff changelog + tag-driven GitHub releases | done |

Next: **M2** — the linear-time deforestation pass and SSA codegen (the law
proofs shipped in M1 are its correctness foundation).

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
`pipeline!` grammar integration lands with the M2 codegen pass.

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

## License

MIT — see [LICENSE](LICENSE).
