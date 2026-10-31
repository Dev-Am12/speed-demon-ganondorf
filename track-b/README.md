# Track B — Index (v5 candidate)

The frozen contract and byte-exact output format are defined by the [organizer's Track B specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md). The specification is authoritative over this README.

## Contract-critical behaviour

- The program reads `/data/docs.txt` and `/data/queries.txt`, then writes only `/out/results.txt`.
- Tokens are ASCII letters and digits, case-folded to lowercase. Every other byte is a separator; stopwords are retained.
- BOOL operators are applied strictly left-to-right. `NOT` means set difference from the current result; it is not unary negation.
- PHRASE means adjacent tokens in exact order. Punctuation, whitespace, sentence boundaries and non-ASCII bytes do not create token positions; phrases never cross document boundaries.
- Each document contributes at most once per query. Output is one line per query, in query-file order: exact match count, exact sum of document IDs, then up to the ten smallest IDs in ascending order.

## v5 candidate design

The implementation is standard-library-only Rust. It maps the corpus read-only, interns the terms used by the query file in a compact open-addressed vocabulary, and creates line-aligned shards. Shard count starts at the worker count and increases only when a shard would exceed the `u16` local-document-ID limit.

Index construction has two corpus passes:

1. **Pass 1:** each worker records each document's text span and computes per-shard document frequencies for query terms. The global step selects the terms that need postings and derives query plans once.
2. **Pass 2:** each shard is owned by one worker. Sparse postings are filled into exact-size `u16` arenas; dense terms use local bitsets. Terms unused by any query path are skipped.

### Boolean queries

BOOL evaluation retains the specification's left-to-right semantics. The planner sorts operands only within a contiguous AND chain where that reordering is algebraically safe. At the start of a query, the whole initial AND chain can be sorted; after an OR or NOT boundary, its first operand is kept in place and only later operands in the AND chain may be reordered. OR/NOT boundaries are never crossed.

Set representation is adaptive per shard: sparse sorted `u16` lists, dense bitsets, or reusable scratch lists/bitsets. List intersections use a merge for moderately sized ratios and a monotone galloping lower-bound search for highly skewed lists. The latter is a scalar implementation, not the SIMD algorithm from the cited literature.

### Phrase queries

The frozen seed-1 query set has 19,000 phrase queries. The spec records that 18,658 contain an anchor with document frequency at most `N/1,000`, 250 contain an absent term, and 92 repeat phrases have neither. v5 uses that workload property without hard-coding any expected answers:

- For a phrase whose rarest term has `df <= N/1,000`, Pass 2 stores the byte offsets of that term's token occurrences in packed `(term, shard-document, byte-offset)` events.
- At query time, the rare seed's events enumerate candidate documents. The next-rarest distinct term's document posting list is a second filter when one exists.
- The verifier starts at each seed occurrence, walks back by the seed's position in the phrase, and checks the phrase forward using the same byte tokenizer. It examines only candidate occurrences, not the entire document from byte zero.
- If the selected seed does not meet the rare-anchor threshold, v5 retains its document posting list and uses the exact whole-document Shift-And verifier. This covers the generated repeat phrases without an anchor, as well as scales/seeds where the threshold does not select a rare term.
- If the selected seed is absent, its positional range is empty and the query correctly returns no matches.

The positional event index is selective rather than a full positional index: only terms selected as rare phrase seeds have occurrence offsets stored. This trades a compact extra index for less phrase-query replay.

### Results and determinism

Each query is evaluated on shards in document order. Shard-local IDs are translated to global IDs while accumulating count, sum, and the first ten hits. Shards are contiguous and ordered, so each shard's hits append in ascending global-ID order. Output formatting is integer-only and ends each query line with LF.

The only unsafe operations are the read-only POSIX `mmap` boundary and two locally documented unchecked reads (vocabulary slot and byte map). No runtime crates are added; the dependency count remains zero.

## Verification status — do not skip

**This file is a v5 candidate, not a verified release.** The provided environment did not contain `rustc`, `cargo`, `rustfmt`, or Docker, so this exact Rust source could not be compiled, run against the reference, or measured here. Python property checks were run for the galloping lower-bound, anchor backtracking, phrase verifier, and Shift-And fallback, but those checks do not replace compiling the Rust binary or running the organizer's reference gate.

Before merging or pushing, run at minimum:

```bash
cd ~/speed-demon
cargo build --manifest-path track-b/Cargo.toml --release --offline
cargo fmt --manifest-path track-b/Cargo.toml -- --check
fixtures/b_gate.sh track-b/target/release/solution
fixtures/b_fuzz.sh track-b/target/release/solution 2 40 tiny
fixtures/b_fuzz.sh track-b/target/release/solution 2 6 sample
```

`b_gate.sh` covers the spec example, committed hostile cases, generated large cases (when present), tiny/sample/full hashes, repeated deterministic runs, and thread-count independence. Do not push a failing candidate. The full gate and your own final rerun must pass before any live-board run.

## Performance status

The latest user-supplied board result is commit `a3f64b1`: `T=0.631 s`, `M=57 MB`, `S=1,294`, `K=13.162834`, speed score `55.967`, rank 39. The uploaded repository snapshot has root commit `080c956`; the context notes explicitly say the exact source that produced B commit `a3f64b1` has not been identified. Therefore, do not assume the source in this ZIP is byte-for-byte the board-tested implementation.

v5 has no local Rust timing, memory measurement, `scc` count, or board result yet. No speedup is claimed. See [PERF.md](PERF.md) for the historical measurements, research rationale, and acceptance gates.

## Research that shaped the candidate

- Sorted inverted-index postings are a standard representation; intersection cost is sensitive to list-length skew. Sanders and Transier compare intersection algorithms across widely varying posting-list ratios. Lemire, Boytsov and Kurz describe SIMD galloping for this operation and report up to a 2× improvement for their conjunctive-query benchmarks. v5 adopts the algorithmic idea of galloping for high-skew scalar intersections; it does **not** claim their SIMD result.
- Adaptive bitmap representations are established for set-heavy workloads. Roaring bitmaps are a prominent compressed alternative. This implementation keeps its dependency-free local choice of sorted `u16` arrays for sparse sets and direct bitsets for dense sets; a Roaring dependency is not automatically a win here because the direct-dependency axis is penalized.
- Positional inverted indexes answer phrase queries by retaining term positions and checking offsets. The KIT thesis by Transier describes document-level postings and positional structures, as well as phrase-query planning and alternatives for short documents. v5 stores positions only for rare phrase seeds rather than building a complete positional index for every query term.
- WAND/Block-Max WAND is not used: this task does not ask for top-k ranked retrieval. It requires exact counts, exact sums and the first ten document IDs for **every** query, so a score-bound pruning algorithm cannot discard potentially matching documents.

References are linked in [PERF.md](PERF.md).
