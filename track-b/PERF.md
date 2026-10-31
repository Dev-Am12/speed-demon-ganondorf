# Track B — performance log (v5 candidate)

This log separates organizer-board measurements, same-machine local measurements, projections, and unmeasured candidates. Only organizer-board values are official. Do not put a local or projected value in the board columns.

## Board snapshot and target

The user's 31 Oct board snapshot reports the following Track B result:

| Board commit | Rank | T | M | S lines | K | Speed score | Board run ID |
|---|---:|---:|---:|---:|---:|---:|---|
| `a3f64b1` | 39 | 0.631 s | 57 MB | 1,294 | 13.162834 | 55.967 | Not recorded in the supplied notes |

The current top-five cutoff in the supplied leaderboard is K=24.997261. Holding the current M and S factors fixed, the time-only equivalent is approximately 0.175 s (`0.631 × (13.162834 / 24.997261)^2`). This is a scale-of-the-problem calculation, not a forecast: v5 must change actual measured time and may also change memory and source size. A top-five result is not guaranteed by the current candidate.

Important provenance warning: the uploaded ZIP has root commit `080c956`, while the current Track B board result is shown against `a3f64b1`. The supplied Context Pack says the branch, author and exact source that produced `a3f64b1` have not been recovered. This v5 candidate is based on the `track-b/src/main.rs` contained in the ZIP; it is not certified as a patch on the exact board-tested `a3f64b1` source.

## Historical measurements

| Version / experiment | Change | Local measurement | Board T | Board M | Board S | Board K | Board points | Status |
|---|---|---|---:|---:|---:|---:|---:|---|
| Reference | Organizer Python reference | — | 56.35 s | 2,200 MB | 91 | — | — | Reference axes |
| v0 | Query-term inverted index; BOOL merges; phrase verification | — | 13.5 s | 400 MB | 308 | 2.397181 | 18.985 | Board result; commit `40ad00c` |
| v1 | Read-only mmap; parallel two-pass scan; hybrid sets | — | 1.408 s | 278 MB | 1,033 | 6.643748 | 41.121 | Board result; commit `832e2c8` |
| v2 | Fused scan; compact vocabulary; phrase-key verification | — | 0.970 s | 103 MB | 1,172 | 9.585 | 49.450 | Board result; commit not supplied |
| v2.5 | Previous board baseline | — | 0.652 s | 103 MB | 1,059 | 11.872829 | 53.728 | Board result; commit `272dc82` |
| v3 | Segmented index with per-document replay; 16 segments | — | 0.711 s | 63 MB | 1,212 | 12.304135 | 54.503 | Board result; commit `f473810`; slower than v2.5 |
| a3f64b1 | Board's latest B result | — | 0.631 s | 57 MB | 1,294 | 13.162834 | 55.967 | Best recorded B K and T in the supplied history; exact source identity still open |
| V3 Stage 1 | Fixed query records; shared document-start table; compact vocabulary | median 1.303 s; min 0.999 s; RssAnon 72.404 MB; VmHWM 322.432 MB | — | — | 1,169 | 13.392 projected | — | Local only; historical note says gate and tiny fuzz passed |
| V3 Stage 2a | Segmented index, u32 local IDs, K=4 | median 1.424 s; min 1.322 s; RssAnon 34.384 MB | — | — | 1,214 | 14.265 projected | — | Local only; historical note says gate and tiny fuzz passed |
| V3 Stage 2b | u16 sparse IDs; segment-local dense threshold | median 1.654 s; min 1.303 s; RssAnon 25.532 MB | — | — | 1,214 | 13.575 projected | — | Reverted; memory fell but projected K fell 4.8% |
| V3 K=6 | Segment-count sweep | median 1.460 s; min 1.294 s; RssAnon 29.444 MB | — | — | 1,214 | 14.384 projected | — | Reverted; +0.84% projected K, below the keep threshold |
| V3 K=8 | Segment-count sweep | median 1.923 s; min 1.330 s; RssAnon 26.916 MB | — | — | 1,212 | 12.590 projected | — | Reverted; slower and lower projected K |
| V3 Stage 3 | Per-document replay of query-term hits; 16 segments | median 1.108 s; min 0.622 s; RssAnon 34.060 MB | — | — | 1,212 | 16.193 projected | — | Local only; historical note says gate and tiny fuzz passed |
| V3 Stage 4 | Rare-term phrase anchor; smallest-first leading AND | median 1.115 s; min 1.059 s; RssAnon 32.936 MB | — | — | not remeasured | about 16.2 projected | — | Reverted; did not clear the keep threshold and had no 5% S reduction |
| V3 checked-index experiment | Replace unchecked indexing with checked indexing | median 1.242 s; min 1.048 s; RssAnon 36.156 MB | — | — | 1,212 | 15.168 projected | — | Reverted; projected K fell |
| V3 final | Stage 3 plus safety comments and docs | mean 0.986 ± 0.211 s; median 1.068 s; min 0.622 s; max VmHWM 291.092 MB; RssAnon 41.128 MB; RssFile 250.032 MB | — | — | 1,212 | 16.062 projected | — | Local only; no V3 board run |
| V4 local comparison | Sharded candidate in the prior V4 loop | 30-run quiet-session minima: v2.5 0.745 s; v3 1.066 s; v4 1.148 s; v4 user CPU 12.0 s vs v3 7.8 s | — | — | — | — | — | V4 was not kept; the uploaded source snapshot's exact relationship to this measured candidate is unresolved |

The V3 projections use a historical local-to-board calibration (`r=0.406407`, memory offset `15.560 MB`, S0=1,059). They are retained for historical context only. Do not use that calibration to predict v5 or compare it to the current board run without remeasuring a same-session baseline.

## v5 changes in this candidate

| Change | Reasoning | Risk / correctness guard | Measurement still required |
|---|---|---|---|
| Rare-seed positional events | Replace whole-document phrase replay with per-occurrence anchor verification. The spec says 18,658/19,000 full seed-1 phrases have an anchor with `df <= N/1,000`; 250 contain an absent term; 92 repeat phrases have neither. | Every candidate still receives exact byte-token verification. Repeated unanchored phrases use the old Shift-And whole-document fallback. No expected answer is embedded. | Phase profile, full-gate hash, RssAnon/peak memory, query wall time, board T/M/K. |
| Precomputed seed-position mask | Avoid scanning all phrase positions for every anchor occurrence; iterate only positions in the phrase equal to the chosen seed. | Mask is built from the parsed phrase terms; all occurrences of a repeated seed term remain represented. | Compare query phase before/after on identical data. |
| Galloping lower bound for skewed list intersections | A monotone cursor avoids repeated binary searches from the head of a large sorted posting list. Use it only when the list ratio exceeds 16; moderate ratios still use merge. | Both inputs are sorted local document IDs; gallop targets are nondecreasing. Python property test passed 990,000 generated searches; Rust build and differential gate remain required. | Full-run timing; test ratio thresholds 8/16/32 one at a time. |
| One shard per worker (increase only for `u16` capacity) | Halve the number of per-query shard evaluations relative to the previous `2 × worker` layout while preserving parallel phases. | Line-aligned cuts; shard-local IDs stay within 65,534; shard order is global document order. | Test on full set and with all gate thread masks; compare 1× vs 2× workers locally. |
| Safe AND-chain ordering | Start with the rarest terms in pure conjunctions; later chains retain their first operand after an OR/NOT boundary and may reorder only the remaining AND operands. | No OR or NOT boundary is crossed; Boolean grammar remains left-associative. | Tiny hostile tests and full gate; query-phase profile. |

**v5 verification status: NOT BUILT OR RUN IN THIS ENVIRONMENT.** This runtime has no `rustc`, `cargo`, `rustfmt`, or Docker. The candidate has been statically reviewed and small Python property checks passed for (1) galloping lower-bound searches, (2) walking backwards from an anchor to a phrase start, (3) anchor-centred phrase verification over randomized byte documents, and (4) the Shift-And fallback. These are algorithm-level checks only. They are not a Rust compile, not a reference run, and not a substitute for the organizer gate. `scc` has not been run; the current source's line count is not a source-axis measurement.

Do **not** put a v5 K, speed score, board run ID, or correctness-pass claim in the submission until the exact candidate passes the Rust build and full gate.

## Correctness and performance acceptance gates

Run these commands in the prepared WSL2 Ubuntu environment from the repository root, with `/data` and `/out` symlinks prepared as described in `ROLE_C_TASKS.md` Part 6:

```bash
cargo build --manifest-path track-b/Cargo.toml --release --offline
cargo fmt --manifest-path track-b/Cargo.toml -- --check
fixtures/b_gate.sh track-b/target/release/solution
fixtures/b_fuzz.sh track-b/target/release/solution 2 40 tiny
fixtures/b_fuzz.sh track-b/target/release/solution 2 6 sample
```

The full `b_gate.sh` (without `--no-full`) checks the spec example, the committed hostile sets, generated large sets when available, tiny/sample/full seed-1 hashes, repeated deterministic outputs, and thread-count independence. The expected full result hash is `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`. Re-run the gate yourself on the final candidate SHA; do not rely solely on an agent's report.

After correctness:

1. Build the existing board-tested baseline from its exact commit if the object exists locally (`git cat-file -e a3f64b1^{commit}`). If it does not, find the preserved baseline binary in `~/sd-bin/`; do not substitute v2.5 and call it a3f64b1.
2. Compare v5 and the actual baseline in the same quiet session on the same full data, using at least 20 runs per binary (or the team's stricter 30-run protocol), alternating runs if possible. Record median/minimum and CPU time; use `RssAnon` only as a local proxy, not as board memory.
3. Keep a change only if the full gate and fuzz pass and the same-session wall time improves by at least 5%, or memory improves by at least 10% without a material time regression. Revert a slower candidate. For a board claim, cite the real board commit and run ID; small gains need repeat board evidence because a single board timing is noisy.
4. Push only a deliberate candidate to `speed-demon`, following the slot ledger and the 15-minute team-wide dispatch rule. Never use a board push as a substitute for local correctness.

## Profiling facts retained from the previous B-V3 session

The historical scratch profile measured 75,747 distinct query terms, 250,000 documents, 36,276,657 corpus tokens and 22,240,530 `(term, document)` posting pairs (12,147,372 sparse and 10,093,158 dense across 370 dense terms). It measured about 245 ms pass-one wall time, 561 ms pass-two wall time and 348 ms query-phase wall time in that particular local session; summed worker CPU during query work was 0.849 s for BOOL and 3.124 s for PHRASE. These are old local measurements, not current board breakdowns. Re-profile v5 rather than assuming the same phase split.

## Research notes and references

1. **Posting-list intersection.** Peter Sanders and Frederik Transier, [“Intersection in Integer Inverted Indices” (ALENEX 2007)](https://doi.org/10.1137/1.9781611972870.7), compare algorithms for sorted document-ID lists with widely varying lengths. The design implication is to select the intersection method based on relative list lengths rather than blindly merge every pair.
2. **Galloping / SIMD intersection.** Daniel Lemire, Leonid Boytsov and Nathan Kurz, [“SIMD Compression and the Intersection of Sorted Integers” (Software: Practice and Experience, 2016)](https://doi.org/10.1002/spe.2326), describe SIMD galloping and report up to 2× faster conjunctive-query processing in their TREC experiments. v5 borrows the skew-aware galloping idea but uses a scalar lower-bound routine; their reported factor is not a prediction for this code or dataset.
3. **Adaptive bitmaps.** Samy Chambi, Daniel Lemire, Owen Kaser and Robert Godin, [“Better Bitmap Performance with Roaring Bitmaps” (Software: Practice and Experience, 2016)](https://doi.org/10.1002/spe.2325), show that hybrid packed-array/bitmap containers can improve space and intersection speed. This supports adaptive sparse/dense representations; the current no-dependency implementation keeps its own simpler `u16`-list/bitset split.
4. **Phrase processing.** Frederik Transier, [“Algorithms and Data Structures for In-Memory Text Search Engines” (KIT dissertation, 2009)](https://publikationen.bibliothek.kit.edu/1000015824/1202502), describes positional indexes, phrase-query evaluation by position offsets, two-term phrase indexes and short-document alternatives. v5 applies a partial positional index only to rare phrase seeds because this workload provides those anchors explicitly.
5. **Not selected: WAND / Block-Max WAND.** These algorithms are intended to prune top-k ranked retrieval using score upper bounds. Track B instead requires exact count, exact ID sum and the ten smallest IDs for every query; score-bound pruning would not preserve that contract.

## Next experiments (only after v5 passes)

Prioritize by measured phase share:

- If phrase query evaluation remains dominant, compare the current selective positional index with a multi-pattern phrase automaton integrated into the corpus scan. This is a separate, higher-risk experiment: it must deduplicate a phrase hit per document, handle duplicate query lines, and preserve exact aggregates.
- If positional-event sorting dominates, test a two-pass per-term bucket fill that writes `(doc, offset)` ranges in naturally sorted order and removes the global `sort_unstable`; keep only if gate and full timing improve.
- If BOOL evaluation dominates, sweep the gallop threshold (8/16/32), query block size (128/256/512), and shard multiplier (1×/2× workers) one variable at a time. Then consider AVX2 only with a portable scalar fallback and a fully tested path.
- Do not add a dependency for a Roaring/SIMD package without computing its direct-dependency penalty and checking the track's allowed-library rules. The current project is zero-dependency.

---

*All local and board numbers above are historical evidence supplied in the team's notes or current board snapshot. v5 has no measured result yet.*
