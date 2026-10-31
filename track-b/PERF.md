# Track B: performance log

Board measurements, local measurements, and projected scores are labeled separately. Local memory is process RssAnon; mapped input pages are reported separately. Projected K uses the V3 calibration described below and is not a board result.

| Step | Change | Local result | Board run ID | T (s) | M (MB) | S | K | Points | Verdict |
|---|---|---|---|---:|---:|---:|---:|---:|---|
| 0 | Reference axes | Board reference | - | 56.35 | 2,200 | 91 | - | - | Organizer reference |
| v0 | Query-term inverted index; BOOL merges; phrase verification | Board | 40ad00c | 13.5 | 400 | 308 | 2.397181 | 18.985 | Supplied board result |
| v1 | Read-only mmap; parallel two-pass scan; hybrid sets | Board | 832e2c8 | 1.408 | 278 | 1,033 | 6.643748 | 41.121 | Supplied board result |
| v2 | Fused scan; compact vocabulary; phrase-key verification | Board | not supplied | 0.970 | 103 | 1,172 | 9.585 | 49.450 | Supplied board result |
| v2.5 | Board-verified baseline | Board | 272dc82 | 0.652 | 103 | 1,059 | 11.872829 | 53.728 | Supplied baseline for V3 |
| v3 | Segmented index with per-document replay; 16 segments | Board | f473810 | 0.711 | 63 | 1,212 | 12.304135 | 54.503 | Supplied board result; slower than v2.5 |
| V3 Stage 1 | Fixed query records; shared document-start table; compact vocabulary | local: median 1.303, min 0.999; RssAnon 72.404; VmHWM 322.432 | pending | 1.303 | 72.404 | 1,169 | 13.392 projected | - | Kept; gate and tiny fuzz passed |
| V3 Stage 2a | Segmented index, u32 local IDs, K=4 | local: median 1.424, min 1.322; RssAnon 34.384 | pending | 1.424 | 34.384 | 1,214 | 14.265 projected | - | Kept; gate and tiny fuzz passed |
| V3 Stage 2b | u16 sparse IDs; segment-local dense threshold | local: median 1.654, min 1.303; RssAnon 25.532 | pending | 1.654 | 25.532 | 1,214 | 13.575 projected | - | Reverted; memory fell, projected K fell 4.8% |
| V3 K=6 | Segment-count sweep | local: median 1.460, min 1.294; RssAnon 29.444 | pending | 1.460 | 29.444 | 1,214 | 14.384 projected | - | Reverted; +0.84% K, below 1.5% threshold |
| V3 K=8 | Segment-count sweep | local: median 1.923, min 1.330; RssAnon 26.916 | pending | 1.923 | 26.916 | 1,214 | 12.590 projected | - | Reverted; slower and lower projected K |
| V3 Stage 3 | Per-document replay of query-term hits; 16 segments | local: median 1.108, min 0.622; RssAnon 34.060 | pending | 1.108 | 34.060 | 1,212 | 16.193 projected | - | Kept; gate and tiny fuzz passed |
| V3 Stage 4 | Rare-term phrase anchor; smallest-first leading AND | local: median 1.115, min 1.059; RssAnon 32.936 | pending | 1.115 | 32.936 | not remeasured | about 16.2 projected | - | Reverted; below threshold and no 5% S reduction |
| V3 checked-index experiment | Replace unchecked indexing with checked indexing | local: median 1.242, min 1.048; RssAnon 36.156 | pending | 1.242 | 36.156 | 1,212 | 15.168 projected | - | Reverted; projected K fell |
| V3 final | Stage 3 plus safety comments and documentation | local: mean 0.986 +/- 0.211; median 1.068, min 0.622; max VmHWM 291.092, RssAnon 41.128, RssFile 250.032 | pending | 1.068 | 41.128 | 1,212 | 16.062 projected | - | Local only; no V3 board run |
| V4 draft | Byte-chunk shards (2 x threads, each <= 65,534 documents); fused pass 1 (line end found by the tokenizer, TAB by a bounded scan); per-shard text spans; pass 2 with no `last` array (list dedupe by the previous arena entry, bitsets idempotent); exact per-shard arena and bitsets, one scope per phase, no atomics except work counters; one generic `par_map`; phrase candidates = seed list filtered by `second`, verified on stored spans; `write!` output; about 820 code lines before rustfmt | Not compiled or measured: written without a Rust toolchain | pending | - | - | about 830 (estimate, not scc) | - | - | Unverified draft: run `cargo build`, `fixtures/b_gate.sh` and fuzz before trusting it |
| V4 Stage 2 | C in {12, 24, 48}; dense factor; extra phrase filter terms; query block size | Not run | - | - | - | - | - | - | Not run |
| V4 Stage 3 | Phrase verification (seed-first compare, seed occurrence offsets) | Not run | - | - | - | - | - | - | Not run; needs the Stage 1 profile first |

## V3 calibration and Stage 0 profile (local)

The v2.5 HEAD reference used a 20-run hyperfine median of 1.604305 s, minimum 0.816548 s, and RssAnon 87.440 MB. Thus the local-to-board time factor is `r = 0.406407`, the memory offset is `off = 15.560 MB`, and calibrated S0 is 1,059 lines. The projected values use `T' = r x T_local`, `M' = max(44, RssAnon + off)`, and the organizer's K formula.

The five-run scratch profile found V=75,747 query terms, 250,000 documents, 36,276,657 corpus tokens, and 22,240,530 posting pairs (12,147,372 sparse and 10,093,158 dense across 370 dense terms). The document table occupied 2,000,000 bytes. The query phase took 348 ms wall time; summed worker CPU was 0.849 s for BOOL and 3.124 s for PHRASE. Pass-one and pass-two wall medians were approximately 245 ms and 561 ms. These measurements triggered scan replay and the query experiment.

The final full gate reports `GATE PASS (68 checks)`; final fuzz reports `FUZZ PASS` for tiny seeds 2-40 and sample seeds 2-6. Temporary segment-count variants 1, 3, 7, and 64, all-list, all-bitset, and force-keep variants matched tiny and sample hashes. Both known-bad controls fail the gate. No V3 board timing or memory has been measured.

## V4 notes

Why v3 lost time on the board (checked against its code): at least 16 segments with three thread scopes each; serial vocabulary and chunk loops inside every segment; the phrase keep-set rebuilt and about 19,000 phrases re-sorted per segment; the dense threshold taken from the total document count; events pushed before the per-document dedupe; a two-scan document-start table; 50,000 queries evaluated 16 times.

V4 replaces that with one shard layer and five parallel phases (newline count, pass 1, layout, pass 2, queries). The global keep set, df and phrase plans are computed once, serially, between pass 1 and layout. Output is independent of shard and thread count because shards are concatenated in document order and BOOL/PHRASE results are merged by count, sum and the first ten ids.

The draft was written without a Rust compiler or the data, so no V4 number exists yet. To judge it, use the Section 5 rule of the V4 prompt: the minimum of 20 `hyperfine` runs on the full dataset in one session against `b_v25` and `b_v3`, then the board.