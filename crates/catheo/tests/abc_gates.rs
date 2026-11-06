//! The §6.1 M3 decision gate, as real integration tests.
//!
//! ARCHITECTURE.md §6.1 gates M3 on a three-way comparison — **A**: naive
//! per-stage-materializing closures (the baseline the macro must beat),
//! **B**: idiomatic Rust iterator chains (the baseline the macro must
//! match), **C**: the macro-flattened `pipeline!` output — over the
//! representative `Vec` map/filter/fold shapes, on `Vec<i32>` and
//! `Vec<String>`. Wall-clock comparisons cannot be enforced on shared CI
//! runners, so — per the house gate style set by the M1 kernel-linearity
//! and §6.3 budget gates — this file enforces the *deterministic*
//! consequences of the §6.1 claim, while `benches/abc.rs` is the
//! measurement instrument whose criterion output is the published
//! evidence (README, "M3 notes").
//!
//! What is enforced here, and why it is equivalent to the §6.1 gate:
//!
//! 1. **Three-way agreement.** A == B == C, value-for-value, on every
//!    representative shape, both element universes, edge and randomized
//!    inputs. (A correct-but-cheating C — e.g. a fused rewrite that
//!    changes semantics — fails here.)
//! 2. **Allocation gates.** The macro's fused output (C) performs **no
//!    more heap allocations than the idiomatic chain (B)** on every
//!    shape — on the pinned toolchain (`rustc 1.99.1`) C == B == 0 on all
//!    four shapes, because the Rule-4 fold absorbs the whole chain into
//!    one pass and every kernel reuses its input buffer in place (std's
//!    in-place collection) — and **strictly fewer than naive (A)** on the
//!    map-heavy and fold-heavy pipelines, where C is exactly
//!    **zero**-allocation while A pays one container per materialized
//!    stage. The naive baseline borrows (`iter`) rather than consumes
//!    (`into_iter`) on purpose: rustc 1.99 reuses an *owned* source
//!    buffer through `into_iter().map().collect()` chains, which would
//!    hide the per-stage materialization cost A is meant to represent.
//!    Allocation counts are deterministic via the thread-local counting
//!    allocator (`catheo-counting-alloc`).
//! 3. **Filter-chain barrier cost, pinned.** The filter is an absolute
//!    barrier (§3.3 Rule 3) and the pre-barrier run stays at the
//!    sequential-kernel shape (the stable-rustc container-formation wall
//!    documented in `catheo-parser`'s codegen module). Per the §6.1
//!    protocol the responsible rule was re-examined: Rule 3 is upheld —
//!    crossing a filter changes evaluation semantics by construction. On
//!    the pinned toolchain the barrier's cost is *temporal only* (extra
//!    passes over the in-place-materialized state; the criterion suite
//!    measures it), so the allocation gate pins a regression band, not a
//!    point.
//! 4. **Emission lockstep.** The exact benchmark pipelines are re-parsed
//!    through `catheo-parser` and their fused emission shape is pinned,
//!    so `benches/abc.rs` and these gates can never drift apart.
//!
//! Every variant closure below is kept in lockstep with `benches/abc.rs`
//! (the same discipline as `kernel_gates.rs` ↔ `benches/kernels.rs`).

use catheo_counting_alloc::{Counting, counted};

// The counting allocator is installed for this whole test binary; every
// other test in the file therefore also runs under it, harmlessly.
#[global_allocator]
static COUNTING: Counting = Counting;

/// Gate input size: large enough that Vec growth doubling stays inside the
/// pinned filter-cost band, small enough to keep the suite instant.
const GATE_SIZE: usize = 1_024;

/// The allocation gates' shared `Vec<i32>` input.
fn gate_i32s() -> Vec<i32> {
    (0..GATE_SIZE)
        .map(|i| i32::try_from(i).unwrap_or(0))
        .collect()
}

// ─── Universe 1: Vec<i32> (§6.1) ────────────────────────────────────────

/// map-heavy i32: three maps, then a summing fold.
mod map_heavy_i32 {
    use catheo::VecWitness;

    /// A: every stage materializes its own container.
    ///
    /// Stages borrow via `iter` and collect fresh containers, so the
    /// source (and every intermediate) stays alive next to its successor:
    /// the honest "the abstraction is the runtime representation" shape.
    /// (`into_iter` chains are *not* naive on rustc 1.99 — std reuses the
    /// source buffer in place, hiding the stage cost.)
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<i32>) -> i64 {
        let step_1: Vec<i32> = input.iter().map(|x| x.wrapping_mul(3)).collect();
        let step_2: Vec<i32> = step_1.iter().map(|x| x.wrapping_add(7)).collect();
        let step_3: Vec<i32> = step_2.iter().map(|x| x.wrapping_mul(5)).collect();
        step_3.into_iter().fold(0_i64, |acc, x| acc + i64::from(x))
    }

    /// B: one iterator pass, no intermediate containers.
    pub fn idiomatic(input: Vec<i32>) -> i64 {
        input
            .into_iter()
            .map(|x| x.wrapping_mul(3))
            .map(|x| x.wrapping_add(7))
            .map(|x| x.wrapping_mul(5))
            .fold(0_i64, |acc, x| acc + i64::from(x))
    }

    /// C: the macro-flattened output.
    pub fn fused(input: Vec<i32>) -> i64 {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |x| x.wrapping_mul(3),
            map: |x| x.wrapping_add(7),
            map: |x| x.wrapping_mul(5),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        }
    }
}

/// fold-heavy i32: one map, then a four-accumulator fold whose per-element
/// work dominates (the §6.1 "fold-heavy" characterization).
mod fold_heavy_i32 {
    use catheo::VecWitness;

    /// The multi-accumulator fold: (sum, min, max, positive count).
    type Stats = (i64, i32, i32, u64);

    /// A: the map stage materializes, then the fold consumes the copy.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<i32>) -> Stats {
        let mapped: Vec<i32> = input.iter().map(|x| x.wrapping_mul(2)).collect();
        mapped
            .into_iter()
            .fold((0_i64, i32::MAX, i32::MIN, 0_u64), |acc, x| {
                fold_step(acc, x)
            })
    }

    /// B: one iterator pass.
    pub fn idiomatic(input: Vec<i32>) -> Stats {
        input
            .into_iter()
            .map(|x| x.wrapping_mul(2))
            .fold((0_i64, i32::MAX, i32::MIN, 0_u64), |acc, x| {
                fold_step(acc, x)
            })
    }

    /// C: the macro-flattened output.
    pub fn fused(input: Vec<i32>) -> Stats {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |x| x.wrapping_mul(2),
            fold: (
                (0_i64, i32::MAX, i32::MIN, 0_u64),
                |acc: Stats, x| fold_step(acc, x)
            ),
        }
    }

    fn fold_step(acc: Stats, x: i32) -> Stats {
        (
            acc.0.wrapping_add(i64::from(x)),
            acc.1.min(x),
            acc.2.max(x),
            acc.3.wrapping_add(u64::from(x.is_positive())),
        )
    }
}

/// filter chain i32: map → filter → map → fold (§1's "map/filter/fold").
mod filter_chain_i32 {
    use catheo::VecWitness;

    /// A: every stage materializes — including both sides of the barrier.
    /// Map stages borrow (fresh container per stage); the filter stage
    /// moves elements, so its container is the only cost.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<i32>) -> i64 {
        let mapped: Vec<i32> = input.iter().map(|x| x.wrapping_mul(3)).collect();
        let filtered: Vec<i32> = mapped.iter().copied().filter(|x| x % 2 == 0).collect();
        let shifted: Vec<i32> = filtered.iter().map(|x| x.wrapping_add(1)).collect();
        shifted.into_iter().fold(0_i64, |acc, x| acc + i64::from(x))
    }

    /// B: one iterator pass.
    pub fn idiomatic(input: Vec<i32>) -> i64 {
        input
            .into_iter()
            .map(|x| x.wrapping_mul(3))
            .filter(|x| x % 2 == 0)
            .map(|x| x.wrapping_add(1))
            .fold(0_i64, |acc, x| acc + i64::from(x))
    }

    /// C: the macro-flattened output — the filter stays a kernel barrier.
    pub fn fused(input: Vec<i32>) -> i64 {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |x| x.wrapping_mul(3),
            filter: |x| x % 2 == 0,
            map: |x| x.wrapping_add(1),
            fold: (0_i64, |acc, x| acc + i64::from(x)),
        }
    }
}

/// collect i32: two maps into a fresh container.
mod collect_i32 {
    use catheo::VecWitness;

    /// A: materialize each map, return the last.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<i32>) -> Vec<i32> {
        let mapped: Vec<i32> = input.iter().map(|x| x.wrapping_mul(2)).collect();
        let shifted: Vec<i32> = mapped.iter().map(|x| x.wrapping_add(3)).collect();
        shifted
    }

    /// B: one adapter chain into `collect`.
    pub fn idiomatic(input: Vec<i32>) -> Vec<i32> {
        input
            .into_iter()
            .map(|x| x.wrapping_mul(2))
            .map(|x| x.wrapping_add(3))
            .collect()
    }

    /// C: the macro-flattened output.
    pub fn fused(input: Vec<i32>) -> Vec<i32> {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |x| x.wrapping_mul(2),
            map: |x| x.wrapping_add(3),
            collect,
        }
    }
}

// ─── Universe 2: Vec<String> (§6.1) ─────────────────────────────────────

/// map-heavy String: two transforming maps, then a length fold.
mod map_heavy_str {
    use catheo::VecWitness;

    /// A: per-stage materialization (borrowing stages, fresh containers).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<String>) -> usize {
        let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
        let marked: Vec<String> = shouted.iter().map(|s| format!("{s}!")).collect();
        marked.into_iter().fold(0_usize, |acc, s| acc + s.len())
    }

    /// B: one iterator pass.
    pub fn idiomatic(input: Vec<String>) -> usize {
        input
            .into_iter()
            .map(|s| s.to_ascii_uppercase())
            .map(|s| format!("{s}!"))
            .fold(0_usize, |acc, s| acc + s.len())
    }

    /// C: the macro-flattened output.
    pub fn fused(input: Vec<String>) -> usize {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |s| s.to_ascii_uppercase(),
            map: |s| format!("{s}!"),
            fold: (0_usize, |acc, s| acc + s.len()),
        }
    }
}

/// fold-heavy String: one map, then a three-accumulator fold.
mod fold_heavy_str {
    use catheo::VecWitness;

    /// The multi-accumulator fold: (total length, longest, count longer
    /// than three bytes).
    type Stats = (usize, usize, u64);

    /// A: the map stage materializes.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<String>) -> Stats {
        let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
        shouted
            .into_iter()
            .fold((0, 0, 0), |acc, s| fold_step(acc, &s))
    }

    /// B: one iterator pass.
    pub fn idiomatic(input: Vec<String>) -> Stats {
        input
            .into_iter()
            .map(|s| s.to_ascii_uppercase())
            .fold((0, 0, 0), |acc, s| fold_step(acc, &s))
    }

    /// C: the macro-flattened output.
    pub fn fused(input: Vec<String>) -> Stats {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |s| s.to_ascii_uppercase(),
            fold: ((0_usize, 0_usize, 0_u64), |acc: Stats, s| fold_step(acc, &s)),
        }
    }

    fn fold_step(acc: Stats, s: &str) -> Stats {
        (
            acc.0.wrapping_add(s.len()),
            acc.1.max(s.len()),
            acc.2.wrapping_add(u64::from(s.len() > 3)),
        )
    }
}

/// filter chain String: map → filter (even length) → map → fold.
mod filter_chain_str {
    use catheo::VecWitness;

    /// A: both sides of the barrier materialize (borrowing map stages).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<String>) -> usize {
        let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
        let filtered: Vec<String> = shouted.into_iter().filter(|s| s.len() % 2 == 0).collect();
        let marked: Vec<String> = filtered.iter().map(|s| format!("{s}?")).collect();
        marked.into_iter().fold(0_usize, |acc, s| acc + s.len())
    }

    /// B: one iterator pass.
    pub fn idiomatic(input: Vec<String>) -> usize {
        input
            .into_iter()
            .map(|s| s.to_ascii_uppercase())
            .filter(|s| s.len() % 2 == 0)
            .map(|s| format!("{s}?"))
            .fold(0_usize, |acc, s| acc + s.len())
    }

    /// C: the macro-flattened output.
    pub fn fused(input: Vec<String>) -> usize {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |s| s.to_ascii_uppercase(),
            filter: |s| s.len() % 2 == 0,
            map: |s| format!("{s}?"),
            fold: (0_usize, |acc, s| acc + s.len()),
        }
    }
}

/// collect String: two maps into a fresh container.
mod collect_str {
    use catheo::VecWitness;

    /// A: materialize each map, return the last.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "A receives the container the way a naive call site hands it over: by value. The source stays alive next to every materialized stage - that residency is the cost the naive baseline represents."
    )]
    pub fn naive(input: Vec<String>) -> Vec<String> {
        let shouted: Vec<String> = input.iter().map(|s| s.to_ascii_uppercase()).collect();
        let marked: Vec<String> = shouted.iter().map(|s| format!("{s}?")).collect();
        marked
    }

    /// B: one adapter chain into `collect`.
    pub fn idiomatic(input: Vec<String>) -> Vec<String> {
        input
            .into_iter()
            .map(|s| s.to_ascii_uppercase())
            .map(|s| format!("{s}?"))
            .collect()
    }

    /// C: the macro-flattened output.
    pub fn fused(input: Vec<String>) -> Vec<String> {
        catheo::pipeline! {
            witness: VecWitness,
            source: input,
            map: |s| s.to_ascii_uppercase(),
            map: |s| format!("{s}?"),
            collect,
        }
    }
}

// ─── Gate 1: three-way value agreement (§6.2 posture, §6.1 shapes) ──────

/// Deterministic xorshift64\* sweep, same idiom as `fusion_properties.rs`.
struct XorShift(u64);

impl XorShift {
    fn next_i32(&mut self) -> i32 {
        i32::try_from(self.next_u64() % 2_000).unwrap_or(0) - 1_000
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

const XORSHIFT_SEED: u64 = 0xABCD_EF01_2345_6789;

/// Edge shapes first, then a seeded pseudo-random vector: the same input
/// discipline the §6.2 harness established.
fn edge_i32_vecs() -> Vec<Vec<i32>> {
    let mut rng = XorShift(XORSHIFT_SEED);
    let mut cases = vec![
        Vec::new(),
        vec![0],
        vec![1],
        vec![-1],
        vec![1, 2, 3],
        vec![2, 4, 6, 8],
        vec![1, 3, 5, 7],
        vec![i32::MAX, i32::MIN, 0],
        (0..64).collect(),
        (0..GATE_SIZE).map(|_| rng.next_i32()).collect(),
    ];
    let mut alternating: Vec<i32> = Vec::new();
    for i in 0..GATE_SIZE {
        let value = i32::try_from(i).unwrap_or(0);
        alternating.push(if i % 2 == 0 { value } else { -value });
    }
    cases.push(alternating);
    cases
}

/// Edge strings: empty, parity-mixed lengths, and a seeded sweep.
fn edge_string_vecs() -> Vec<Vec<String>> {
    let mut cases = vec![
        Vec::new(),
        vec![String::new()],
        vec!["ab".to_owned(), "cd".to_owned()],
        vec!["abc".to_owned()],
        vec![
            "a".to_owned(),
            "ab".to_owned(),
            "abc".to_owned(),
            "abcd".to_owned(),
        ],
    ];
    cases.push((0..GATE_SIZE).map(gate_word).collect());
    cases
}

/// Deterministic gate word: length 5..=9, mixed parity.
fn gate_word(i: usize) -> String {
    format!("{}{}", "w".repeat(4 + i % 5), i % 10)
}

#[test]
fn three_way_agreement_on_the_i32_pipelines() {
    // Spot values first: if all three variants shared one wrong rewrite,
    // pairwise equality alone could not catch it.
    assert_eq!(map_heavy_i32::naive(vec![1, 2, 3]), 195);
    assert_eq!(filter_chain_i32::naive(vec![1, 2, 3, 4, 5, 6]), 39);
    assert_eq!(fold_heavy_i32::naive(vec![1, 2, 3]), (12, 2, 6, 3));
    assert_eq!(collect_i32::naive(vec![1, 2, 3]), vec![5, 7, 9]);

    for input in edge_i32_vecs() {
        let (naive, _) = counted({
            let input = input.clone();
            move || map_heavy_i32::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || map_heavy_i32::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || map_heavy_i32::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, map_heavy/i32");
        assert_eq!(naive, fused, "A == C, map_heavy/i32");

        let (naive, _) = counted({
            let input = input.clone();
            move || filter_chain_i32::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || filter_chain_i32::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || filter_chain_i32::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, filter_chain/i32");
        assert_eq!(naive, fused, "A == C, filter_chain/i32");

        let (naive, _) = counted({
            let input = input.clone();
            move || fold_heavy_i32::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || fold_heavy_i32::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || fold_heavy_i32::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, fold_heavy/i32");
        assert_eq!(naive, fused, "A == C, fold_heavy/i32");

        let (naive, _) = counted({
            let input = input.clone();
            move || collect_i32::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || collect_i32::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || collect_i32::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, collect/i32");
        assert_eq!(naive, fused, "A == C, collect/i32");
    }
}

#[test]
fn three_way_agreement_on_the_string_pipelines() {
    // Spot values first: if all three variants shared one wrong rewrite,
    // pairwise equality alone could not catch it.
    assert_eq!(map_heavy_str::naive(vec!["ab".into(), "cd".into()]), 6);
    assert_eq!(
        fold_heavy_str::naive(vec!["abc".into(), "de".into()]),
        (5, 3, 0)
    );
    assert_eq!(
        filter_chain_str::naive(vec!["abcd".into(), "abc".into()]),
        5
    );
    assert_eq!(
        collect_str::naive(vec!["ab".into()]),
        vec!["AB?".to_owned()]
    );

    for input in edge_string_vecs() {
        let (naive, _) = counted({
            let input = input.clone();
            move || map_heavy_str::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || map_heavy_str::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || map_heavy_str::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, map_heavy/String");
        assert_eq!(naive, fused, "A == C, map_heavy/String");

        let (naive, _) = counted({
            let input = input.clone();
            move || fold_heavy_str::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || fold_heavy_str::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || fold_heavy_str::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, fold_heavy/String");
        assert_eq!(naive, fused, "A == C, fold_heavy/String");

        let (naive, _) = counted({
            let input = input.clone();
            move || filter_chain_str::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || filter_chain_str::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || filter_chain_str::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, filter_chain/String");
        assert_eq!(naive, fused, "A == C, filter_chain/String");

        let (naive, _) = counted({
            let input = input.clone();
            move || collect_str::naive(input)
        });
        let (idiomatic, _) = counted({
            let input = input.clone();
            move || collect_str::idiomatic(input)
        });
        let (fused, _) = counted({
            let input = input.clone();
            move || collect_str::fused(input)
        });
        assert_eq!(naive, idiomatic, "A == B, collect/String");
        assert_eq!(naive, fused, "A == C, collect/String");
    }
}

// ─── Gate 2: the allocation profile IS the §6.1 comparison ──────────────

#[test]
fn macro_output_allocates_zero_on_the_map_heavy_pipeline() {
    let input: Vec<i32> = gate_i32s();

    let (_, naive_allocs) = counted({
        let input = input.clone();
        move || map_heavy_i32::naive(input)
    });
    let (_, idiomatic_allocs) = counted({
        let input = input.clone();
        move || map_heavy_i32::idiomatic(input)
    });
    let (_, fused_allocs) = counted({
        let input = input.clone();
        move || map_heavy_i32::fused(input)
    });

    // §6.1 required win #1, the sharp form: the pure fold absorbs the
    // whole chain (§3.3 Rule 4), so C allocates *nothing* — exactly one
    // pass, zero containers — while A pays one container per stage.
    assert_eq!(fused_allocs, 0, "C must be allocation-free on map_heavy");
    assert_eq!(idiomatic_allocs, 0, "B stays allocation-free: C ≥ B");
    assert_eq!(
        naive_allocs, 3,
        "A materializes exactly its three stages: C > A"
    );
}

#[test]
fn macro_output_allocates_zero_on_the_fold_heavy_pipeline() {
    let input: Vec<i32> = gate_i32s();

    let (_, naive_allocs) = counted({
        let input = input.clone();
        move || fold_heavy_i32::naive(input)
    });
    let (_, idiomatic_allocs) = counted({
        let input = input.clone();
        move || fold_heavy_i32::idiomatic(input)
    });
    let (_, fused_allocs) = counted({
        let input = input.clone();
        move || fold_heavy_i32::fused(input)
    });

    // §6.1 required win #2: same Rule-4 absorption as map_heavy.
    assert_eq!(fused_allocs, 0, "C must be allocation-free on fold_heavy");
    assert_eq!(idiomatic_allocs, 0, "B stays allocation-free: C ≥ B");
    assert_eq!(naive_allocs, 1, "A materializes the map stage: C > A");
}

#[test]
fn macro_output_matches_idiomatic_on_the_collect_pipeline() {
    let input: Vec<i32> = gate_i32s();

    let (_, naive_allocs) = counted({
        let input = input.clone();
        move || collect_i32::naive(input)
    });
    let (_, idiomatic_allocs) = counted({
        let input = input.clone();
        move || collect_i32::idiomatic(input)
    });
    let (_, fused_allocs) = counted({
        let input = input.clone();
        move || collect_i32::fused(input)
    });

    // On this toolchain even the terminal container is free: std's
    // in-place collection reuses the source buffer through the adapter
    // chain, so B and C both build the result with zero fresh
    // allocations, while A pays for every materialized stage.
    assert_eq!(
        fused_allocs, 0,
        "C collects in place: zero fresh containers"
    );
    assert_eq!(idiomatic_allocs, 0, "B collects in place too: C ≥ B holds");
    assert_eq!(naive_allocs, 2, "A materializes both stages: C > A");
}

#[test]
fn filter_barrier_cost_is_pinned_not_hidden() {
    let input: Vec<i32> = gate_i32s();

    let (_, naive_allocs) = counted({
        let input = input.clone();
        move || filter_chain_i32::naive(input)
    });
    let (_, idiomatic_allocs) = counted({
        let input = input.clone();
        move || filter_chain_i32::idiomatic(input)
    });
    let (_, fused_allocs) = counted({
        let input = input.clone();
        move || filter_chain_i32::fused(input)
    });

    // Allocation parity holds even across the barrier: on this toolchain
    // every kernel reuses its input buffer in place (std's in-place
    // collection), so C == B == 0 here, and C still beats A. The Rule-3
    // cost on the filter shape is *temporal* — extra passes over the
    // materialized state — and that is what the criterion suite measures
    // (README, "M3 notes"). The band guards the future: if a std change
    // or a kernel regression makes the barrier path allocate, this gate
    // flags it for re-review.
    assert_eq!(idiomatic_allocs, 0, "B stays allocation-free");
    assert!(
        fused_allocs <= 16,
        "C's barrier cost exploded past the pinned band: {fused_allocs}"
    );
    // A's exact count is std-internal (a non-TrustedLen collect grows by
    // doubling), so the gate pins the invariant: at least one container
    // per stage, and C strictly below it.
    assert!(
        naive_allocs >= 3,
        "A materializes all three stages, got {naive_allocs}"
    );
    assert!(
        fused_allocs < naive_allocs,
        "C still beats A on the filter shape: {fused_allocs} vs {naive_allocs}"
    );
}

#[test]
fn string_universe_matches_the_same_allocation_profile() {
    let input: Vec<String> = (0..GATE_SIZE).map(gate_word).collect();

    // Per-element String allocations (uppercase, format!) are identical
    // across variants, so the *container* difference must be exact:
    // C == B everywhere barrier-free, C < A by exactly A's stage
    // containers on the required-win shapes.
    let (_, naive) = counted({
        let input = input.clone();
        move || map_heavy_str::naive(input)
    });
    let (_, idiomatic) = counted({
        let input = input.clone();
        move || map_heavy_str::idiomatic(input)
    });
    let (_, fused) = counted({
        let input = input.clone();
        move || map_heavy_str::fused(input)
    });
    assert_eq!(fused, idiomatic, "map_heavy/String: C == B");
    assert_eq!(naive - fused, 2, "A pays exactly its two stage containers");

    let (_, naive) = counted({
        let input = input.clone();
        move || fold_heavy_str::naive(input)
    });
    let (_, idiomatic) = counted({
        let input = input.clone();
        move || fold_heavy_str::idiomatic(input)
    });
    let (_, fused) = counted({
        let input = input.clone();
        move || fold_heavy_str::fused(input)
    });
    assert_eq!(fused, idiomatic, "fold_heavy/String: C == B");
    assert_eq!(naive - fused, 1, "A pays exactly its map container");

    let (_, naive) = counted({
        let input = input.clone();
        move || collect_str::naive(input)
    });
    let (_, idiomatic) = counted({
        let input = input.clone();
        move || collect_str::idiomatic(input)
    });
    let (_, fused) = counted({
        let input = input.clone();
        move || collect_str::fused(input)
    });
    assert_eq!(fused, idiomatic, "collect/String: C == B");
    assert_eq!(naive - fused, 2, "A pays exactly its two stage containers");

    let (_, naive) = counted({
        let input = input.clone();
        move || filter_chain_str::naive(input)
    });
    let (_, idiomatic) = counted({
        let input = input.clone();
        move || filter_chain_str::idiomatic(input)
    });
    let (_, fused) = counted({
        let input = input.clone();
        move || filter_chain_str::fused(input)
    });
    assert!(
        fused - idiomatic <= 4,
        "C's barrier cost exploded past the pinned band: {fused} vs {idiomatic}"
    );
    assert!(
        fused < naive,
        "filter_chain/String: C still beats A: {fused} vs {naive}"
    );
}

// ─── Gate 3: emission lockstep (why the counts above are what they are) ─

use catheo_parser::{emit, parse_pipeline};
use quote::quote;

#[test]
fn fused_emission_shape_lockstep_with_the_bench_pipelines() {
    // map_heavy: the pure fold absorbs everything — no kernels, no
    // registers, exactly one eliminator. This is why map_heavy's gate
    // count is zero.
    let map_heavy = parse_pipeline(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x.wrapping_mul(3),
        map: |x| x.wrapping_add(7),
        map: |x| x.wrapping_mul(5),
        fold: (0_i64, |acc, x| acc + x),
    })
    .expect("map_heavy parses");
    let out = emit(&map_heavy).expect("map_heavy emits").to_string();
    assert!(!out.contains("run_"), "no kernels survive Rule 4: {out}");
    assert!(!out.contains("__catheo_state"), "no registers: {out}");
    assert_eq!(out.matches(". fold (").count(), 1, "one eliminator: {out}");

    // fold_heavy: same absorption.
    let fold_heavy = parse_pipeline(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x.wrapping_mul(2),
        fold: ((0_i64, i32::MAX), |acc: (i64, i32), x| (acc.0 + i64::from(x), acc.1.max(x))),
    })
    .expect("fold_heavy parses");
    let out = emit(&fold_heavy).expect("fold_heavy emits").to_string();
    assert!(!out.contains("run_"), "no kernels survive Rule 4: {out}");
    assert!(!out.contains("__catheo_state"), "no registers: {out}");

    // filter_chain: the barrier keeps the prefix at the sequential-kernel
    // shape — one run_map, one run_filter — and the trailing map fuses
    // into the fold. This is the exact emission the pinned barrier-cost
    // band measures.
    let filter_chain = parse_pipeline(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x.wrapping_mul(3),
        filter: |x| x % 2 == 0,
        map: |x| x.wrapping_add(1),
        fold: (0_i64, |acc, x| acc + x),
    })
    .expect("filter_chain parses");
    let out = emit(&filter_chain).expect("filter_chain emits").to_string();
    assert_eq!(out.matches("run_map").count(), 1, "prefix kernel: {out}");
    assert_eq!(
        out.matches("run_filter").count(),
        1,
        "barrier kernel: {out}"
    );
    assert!(
        out.contains("into_iter (__catheo_state_1)"),
        "the fold loops over the barrier's output: {out}"
    );

    // collect: the whole run chains into the user-annotated collect —
    // no kernels, exactly the one result container.
    let collect = parse_pipeline(quote! {
        witness: VecWitness,
        source: xs,
        map: |x| x.wrapping_mul(2),
        map: |x| x.wrapping_add(3),
        collect,
    })
    .expect("collect parses");
    let out = emit(&collect).expect("collect emits").to_string();
    assert!(!out.contains("run_"), "no kernels: {out}");
    assert!(out.contains(". collect ()"), "one collect: {out}");
}
