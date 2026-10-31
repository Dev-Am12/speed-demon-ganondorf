# Track B — performance log (v6 verification candidate)

This file distinguishes official board measurements, local measurements, algebraic/property checks, and hypotheses. **Only the organizer board measures official `T`, `M`, `S`, and `K`.** Do not copy a projection or a local measurement into the board columns.

## 1. Current board position

Latest board snapshot supplied by the team (31 Oct 2026):

| Track B commit | Rank | T | M | S lines | K | Machine speed points | Board run ID |
|---|---:|---:|---:|---:|---:|---:|---|
| `4d1311a` | 38 (one place up from 39) | 0.592 s | 66 MB | 1,014 | 13.702564 | 56.840 | Not supplied |

The previous submitted baseline was `a3f64b1` at 0.631 s / 57 MB / 1,294 lines / K 13.162834. The supplied new run is about 6.2% faster in wall time and raises K by about 4.1%. Its reported machine score is `50 × log10(13.702564) = 56.8401`, shown as 56.840. The board's `pts` column is the **machine speed score**, not the final judge-weighted track total.

The ZIP supplied for this task has root commit `080c956`, not `4d1311a`. Therefore the source in this candidate directory must **not** be assumed byte-identical to the source that produced the latest board result. Before transplanting v6, the verifier must inspect `git show 4d1311a:track-b/src/main.rs` and port/reconcile the changes against that exact file. Do not replace a newer board-tested file wholesale with a stale ZIP snapshot.

## 2. Scoring method and what is worth optimizing

Frozen machine coefficient:

```text
K = (Tref / T)^0.50
  × min(Mref / M, 50)^0.20
  × min(Sref / S, 1)^0.15
  × min(Dref / D, 1)^0.15

machine_speed_points = clamp(50 × log10(K), 0, 100)
track_total = 0.65 × machine_speed_points + 0.35 × judge_score
```

Track B reference values recorded by the team / organizer harness notes are `Tref = 56.35 s`, `Mref ≈ 2,200 MB`, `Sref = 91 scc code lines`, and `Dref = 1`. Reconfirm `Mref` from the exact frozen organizer calibration file if it differs in the checkout used for final verification. The dependency count is `D = 1 + direct non-stdlib runtime dependencies`; this implementation has no external runtime dependency, so `D = 1` and gets no dependency penalty.

The board formula implies (the published board K is authoritative; displayed T/M/S axes are rounded, so recomputing K from the table may differ in the last few decimals):

- `T` is the entire run: input read, index construction, query evaluation and output. Track B has no separately scored build/query phases.
- `T` is uncapped. The live board takes five measurements, discards the highest and lowest, and uses the mean of the middle three; timing spread beyond the published tolerance can trigger a rerun.
- `M` is capped at a 50× reference ratio. With `Mref ≈ 2,200 MB`, the cap is reached at approximately 44 MB. At the current 66 MB, memory still contributes to K; reducing 66 MB to 44 MB can improve K by at most about 8.4% if all else is held constant.
- `S` is counted by `scc` after the required formatter, across the whole track folder. The ratio is capped at 1, so source size only costs. Reducing 1,014 lines to 700 lines would improve K by about 5.7%, all else equal.
- `D` cannot improve above 1. One new direct dependency would multiply K by `2^-0.15 ≈ 0.901`, requiring more than 23% runtime improvement just to break even. Keep the standard-library-only design.

### Target arithmetic (not a prediction)

The latest user-supplied B leaderboard showed K=24.997261 at rank 5 and K=35.047382 at rank 1. The current B result is K=13.702564. Holding its M and S factors fixed, a time-only equivalent would require:

| Target from the last supplied leaderboard | Target K | Time equivalent at current M and S | Improvement from 0.592 s |
|---|---:|---:|---:|
| Top 5 | 24.997261 | about 0.178 s | 3.33× faster |
| First | 35.047382 | about 0.0905 s | 6.54× faster |

If memory reached its approximate 44 MB cap **and** source fell to 700 lines, the corresponding time equivalents would still be about 0.234 s for the prior top-five cutoff and 0.119 s for the prior first-place cutoff. This is why v6 prioritises query and index runtime. These cutoffs are the last leaderboard numbers supplied in the conversation, not a live re-check of the current board.

## 3. Dataset facts that constrain the design

The frozen Track B contract and seed-1 generator report:

| Item | Full seed 1 |
|---|---:|
| Documents | 250,000 |
| `docs.txt` bytes | 253,827,587 |
| Tokens | 36,276,657 |
| Distinct tokens | 609,833 |
| Distinct `(term, document)` pairs | 29,798,541 |
| Queries | 50,000 (31,000 BOOL; 19,000 PHRASE) |
| `queries.txt` bytes | 1,763,534 |
| Expected `results.txt` bytes | 1,459,301 |

Seed-1 query distribution facts:

- The query vocabulary is much smaller than the corpus vocabulary (about 75.7k distinct query terms versus 609.8k distinct corpus tokens). Indexing only terms referenced by queries is explicitly allowed.
- BOOL term occurrences are drawn across frequency bands: very common, common, medium, uncommon, rare and absent. BOOL results range from empty to 234,103 documents; median result count is 8.
- 18,658 / 19,000 phrase queries (98.2%) contain a rare anchor with `df <= N/1,000`; 250 contain an absent term; the remaining 92 generated repeat phrases can lack a rare anchor.
- Phrase results are small (median 1, 99th percentile at most 5, maximum 38), but candidate verification must remain exact.

Correctness traps that must stay explicit in every review:

1. Tokenisation is byte-based: ASCII A–Z only are lowercased; tokens are maximal `[a-z0-9]+`; every other byte, including every UTF-8 byte >= 0x80, is a separator. No Unicode folding, stemming or stopword removal. `007` differs from `7`.
2. BOOL operators are binary and strictly left-to-right, with no precedence. `a OR b AND c` means `(a OR b) AND c`; `NOT` is set difference from the current accumulator, not unary complement.
3. A document is counted once regardless of repeated term occurrences. Phrase tokens must be adjacent in token order; separators are invisible; phrases can cross punctuation/sentence boundaries but cannot cross documents. Repeated tokens must repeat.
4. Output must have exactly one line per query, in original order, with exact `u64` count/sum and up to the ten smallest document IDs ascending. Empty result is `0 0`; output is ASCII with LF and one trailing LF.
5. Duplicate query lines remain duplicate output lines. Term IDs require at least 32 bits. Lowercase `and`, `or`, `not` in term positions are ordinary words.

Seed-1 hashes to check local data and output:

```text
docs.txt    6cd461670d6c39f9c5211162577532d3648edeab12340c71cc7d2ab5f18e66c8
queries.txt 77ef7ffc59afde051006b02a2ce8e4c090fbbfbf101acd6f862d0f8e298d39f9
results.txt f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927
```

The full corpus is about 254 MB and must be read as an input file, not embedded. The user-provided raw data links are [docs.txt](https://s3.amazonaws.com/raptors.dev/speeddemon.dev/2026/data/track-b/full-seed1/docs.txt) and [queries.txt](https://s3.amazonaws.com/raptors.dev/speeddemon.dev/2026/data/track-b/full-seed1/queries.txt). The full raw files were not materialised in this environment's local benchmark workspace, so no new byte-level corpus histogram or query-frequency profile was measured here; distribution facts above are the frozen spec's published generator/seed-1 statistics. The live board uses the public full seed-1 data; final judging uses a full-sized holdout seed, so code must generalise and must not special-case seed-1 answers or query strings.

## 4. Research and synthesis

No publicly accessible competitor source implementation was verified in the research pass. The board's scores and line counts are useful targets, but do not reveal the competitors' data structures. Do not infer that a leaderboard team uses a particular library or algorithm.

Relevant established techniques:

1. **Inverted indexes and positional postings.** A document-frequency-sorted postings intersection reduces candidate work; positional indexes permit phrase verification using token offsets. For this workload, the query file is known upfront and the spec explicitly permits indexing only its terms. A selective positional index for rare phrase seeds is a plausible compromise between full positions for every token and rescanning every candidate document.
2. **Galloping / exponential search for skewed sorted lists.** D. Lemire, O. Boytsov and N. Kurz, *SIMD Compression and the Intersection of Sorted Integers* (2016), study SIMD intersection and galloping for sorted postings. Their reported gains belong to their tested SIMD implementation and collections; they are not a speedup prediction for this scalar Rust implementation. The candidate uses a scalar monotone gallop only for highly skewed list intersections.
3. **Adaptive sparse/dense sets.** A sorted `u16` posting list is compact for sparse local document frequencies; a per-shard bitset makes membership and word-wise Boolean operations cheap for denser terms. Roaring-style containers are a useful research direction, but a third-party crate is not automatically beneficial: the dependency axis penalises it and build/runtime constraints are strict.
4. **Conjunction planning.** Sorting a pure AND segment by increasing document frequency is safe because intersection is associative and commutative. The optimisation must not cross an OR or NOT operator; doing so changes strict left-to-right semantics.
5. **Why WAND / Block-Max WAND is rejected.** Those algorithms prune ranked top-k retrieval using score upper bounds. Track B requires exact count, exact sum and the ten smallest IDs for every query; ranked top-k pruning cannot discard arbitrary matching documents.

References:

- Frozen contract: https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md
- Frozen scoring/reference axes: https://github.com/hackathon-raptors/speed-demon-2026/tree/main/harness
- Lemire, Boytsov, Kurz, *SIMD Compression and the Intersection of Sorted Integers*: https://arxiv.org/abs/1401.6399
- Transier, *Algorithms and Data Structures for In-Memory Text Search Engines* (KIT dissertation): https://publikationen.bibliothek.kit.edu/1000015824/1202502
- Manning, Raghavan, Schütze, *Introduction to Information Retrieval*, inverted and positional indexes: https://nlp.stanford.edu/IR-book/
- Roaring bitmap paper: https://arxiv.org/abs/1709.07821

## 5. v6 candidate changes

This source is a **verification candidate**, not a board submission and not a claimed improvement. It is based on the prior v5 candidate file; before use, the verifier must reconcile it against the exact `4d1311a` source as described in §1.

| Change | Intended benefit | Correctness argument / risk | Status |
|---|---|---|---|
| Fix query vector mutability | Make the later `for q in &mut queries` compile | `parse_queries` returns an owned `Vec<Query>`; bind it as `mut` before in-place planning. This is the direct E0596 fix reported by the team. | Source patched; Rust compile still required |
| Empty-right set identities | Avoid scanning a large accumulator when RHS posting is empty | `x AND ∅ = ∅`; `x OR ∅ = x`; `x NOT ∅ = x`. Empty accumulator can skip binary AND/NOT until the next OR, where `∅ OR S(t) = S(t)`. Does not change left-to-right parsing. | Python randomized property checks passed; Rust gate still required |
| Sparse-left / dense-right NOT | Correctly calculate sparse list difference against a dense bitset | Filter each local ID in the left sparse list using exact bitset membership. This representation pairing is possible under the adaptive layout; returning an empty list would be wrong. | Added after adversarial review; Rust gate still required |
| One-multiply vocabulary home hash | Reduce hash arithmetic for the per-token query-vocabulary lookup on both corpus passes | Full `(lo, hi)` equality is checked on every probe; only the initial bucket changes. Synthetic query-vocabulary probe simulation found mean probes of about 1.21 for both old and new hashes, but is not a real-corpus or CPU benchmark. | Hypothesis; must be A/B benchmarked against old `mix64` and reverted if slower |
| Keep rare-seed phrase index, exact fallback, adaptive postings and galloping | Preserve previous candidate's workload-specific algorithmic choices | Anchor event search only narrows candidates; final phrase verification is byte-token exact. Unanchored repeat phrases use exact fallback. Galloping searches sorted arrays and does not truncate answers. | Carried forward; full Rust gate not run here |

No code changes may introduce expected-output tables, query-specific answer branches, seed checks, top-k pruning, Unicode normalisation, stopword deletion, dependencies, or a change to the output format.

## 6. Verification status

**NOT BUILT OR RUN IN THIS ENVIRONMENT.** The available runtime has no `rustc`, `cargo`, `rustfmt`, `scc` or Docker executable. Consequently:

- No Rust compile result is claimed.
- No organizer reference comparison or full output SHA-256 result is claimed for v6.
- No local runtime / peak-memory result or source-line count is claimed for v6.
- No v6 K or leaderboard position is claimed.

Algorithm-level Python checks were run after the last edit:

- 30,000 randomly generated set pairs, each exercised across all 3 Boolean operators and all 4 sparse/dense representation pairings;
- 50,000 random 1–6-term left-to-right BOOL expressions comparing the proposed empty-set shortcuts with direct evaluation;
- 20,000 randomized phrase documents testing rare-anchor backtracking and the Shift-And fallback across spaces, punctuation, UTF-8 separator bytes and repeated terms (Python model).

All three checks passed. They exercise the algebraic changes, not the Rust implementation, parser, tokenizer, phrase path, shard boundaries or file I/O. They do not substitute for the organizer gate.

### Mandatory local verification in WSL2

From the exact repo worktree, with Rust 1.99.1 and the team's `/data` and `/out` symlinks prepared:

```bash
rustc --version                         # must be 1.99.1
cargo fmt --manifest-path track-b/Cargo.toml -- --check
cargo build --manifest-path track-b/Cargo.toml --release --offline
fixtures/b_gate.sh track-b/target/release/solution --no-full
fixtures/b_fuzz.sh track-b/target/release/solution 2 40 tiny
fixtures/b_fuzz.sh track-b/target/release/solution 2 6 sample
fixtures/b_gate.sh track-b/target/release/solution
```

The full gate in the supplied repo checks the spec example, all committed hostile sets, large custom sets (when generated), tiny/sample/full hashes, repeated determinism, and thread masks `0`, `0-1`, `0-5`, `0-11`. The final full gate must pass, including the full output hash above. Run the gate again against the exact final source SHA after any formatting or optimisation edit.

### Performance acceptance

Only after correctness passes:

1. Build the exact `4d1311a` baseline and v6 candidate with the same Rust 1.99.1 compiler, flags and data. If the git object or baseline binary is unavailable, recover the exact baseline first; do not label v2.5 or a different commit as `4d1311a`.
2. Run A/B in the same quiet WSL2 session on the full seed-1 data, pinning the same allowed P-cores and alternating versions. Use at least 20–30 completed runs per version; compare medians and retain raw logs. Local `RssAnon` is a proxy only and is not the board's `memory.peak` value.
3. Measure the hash-function change separately from the empty-set changes. Keep the hash change only if it produces a repeatable >=3–5% wall-time gain without memory regression. Keep any other performance change only if it beats the exact baseline and all hashes remain byte-identical.
4. Do not push just because the local timing is lower. Reserve a board slot, announce the hypothesis, push only the fully gated commit to `speed-demon`, and record the actual board run ID and all four axes.

## 7. Ordered v6/v7 optimisation plan

| Priority | Experiment | Why it is worth testing | Gate / keep rule |
|---|---|---|---|
| 0 | Reconcile to exact `4d1311a`; compile fix; full correctness and fuzz | The last failed build was a compile-time regression; no speed work matters if the source does not build or hash-match. | Stop on any compile/hash/determinism/thread-mask failure. |
| 1 | Profile full run by phase: query parsing, pass 1, layout, pass 2, phrase-index sort, query evaluation, output | Distinguishes index-build cost from query cost under the one-total-time contract. Instrument only a local diagnostic copy; do not leave logging on stdout/stderr in submission. | Record time and allocation/RssAnon per phase; remove instrumentation before candidate build. |
| 2 | A/B empty-right short-circuit and mixed NOT path | The dataset explicitly contains absent and `NOT`-empty BOOL cases. Avoiding full set scans may help; exact mixed-representation subtraction closes a correctness hole. | Full gate and fuzz first; >=3% same-session median improvement to retain the performance path. |
| 3 | A/B one-multiply vocabulary hash | The tokenizer performs many vocabulary lookups across 36.3M token occurrences and two scans. | Benchmark on exact full corpus; retain only if repeatable and collision/probe distribution is acceptable. |
| 4 | Sweep shard counts 4, 6, 8, 12 while keeping max 65,534 local docs | Fewer shards reduce per-query shard dispatch and accumulator folds; more shards increase scan parallelism and reduce local bitset size. This is a genuine time/memory trade-off. | One parameter per build; full hash, same-host timing and memory each time. No assumption that fewer shards win. |
| 5 | Sweep query block size 128 / 256 / 512 and workers 6 / 8 / 12 | Query-phase scheduling and scratch-buffer footprint may dominate after index build. Scoring runs on 12 pinned P-cores; `available_parallelism()` and actual container affinity must be inspected. | Test all gate thread masks and full query order; keep only measured improvement. |
| 6 | Profile phrase candidates and positional event sorting; test bucketed positional fill to avoid global sort | The seed-1 generator has rare phrase anchors but 19k phrases. Sorting all positional events may be significant; bucket fill is a possible build-time alternative. | Compare phase profile; every phrase must still be token-position verified, including repeated seeds and unanchored fallback. |
| 7 | AVX2 intersection / vectorised aggregate only if profiling supports it | Scoring CPU is Intel Core i5-13500; AVX2 is the portable relevant feature from the supplied hardware notes. | Runtime dispatch or guaranteed scoring ISA only after host confirmation; scalar equivalence path, all hashes, no `target-cpu=native` assumption. |
| 8 | PGO last, using the published generator output in the allowlisted Rust 1.99.1 + LLVM tools image | PGO is allowed, but `/data` is not mounted at build time; generated public/representative data is allowed for profile training. | Same compiler/LLVM version, offline build, allowlisted digest, final gate and board evidence. Never train on hidden holdout answers. |

The board machine is the authority: the live board uses the public full seed-1 data on the scoring machine; final scoring uses a full-size holdout seed. The current notes identify the scoring CPU as Intel Core i5-13500 with six P-cores (12 logical threads) used for timed execution and E-cores reserved for build/board support. Do not use `target-cpu=native` on a teammate's Ryzen or on the local laptop and assume it transfers. `mmap` is allowed; `io_uring` is blocked by the sandbox seccomp profile. Builds must remain offline, under 10 minutes and within the 2 GB image limit; every Docker `FROM` must be pinned to an allowlisted digest.

---

*Last updated for candidate v6 after the team's `4d1311a` board result. The next trustworthy update is after the exact-source WSL2 build, full gate, paired benchmark and (only then) a deliberate board run.*
