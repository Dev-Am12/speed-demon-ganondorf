# Track B: Index

The contract and exact output format are in the [Track B specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

The binary maps `/data/docs.txt` read-only and parses all queries into fixed-size records, interning every query term in a compact open-addressed vocabulary (terms up to 16 bytes are stored verbatim, 17 to 32 bytes are hashed the same way the tokenizer hashes document tokens).

The corpus is cut into byte chunks that start at line starts. A parallel newline count gives each chunk's document count; the chunk count (2 x threads, at most 12 threads) is doubled until every chunk holds at most 65,534 documents, so a document id inside a shard fits in 16 bits. One chunk is one shard, and a shard's base id is the number of documents before it.

Pass 1 runs over shards in parallel. A single fused loop per line finds the TAB with a bounded scan, tokenizes up to the line feed, stores the text span, and counts each vocabulary term once per document. A short serial step then sums document frequencies, marks the terms that any BOOL query uses, and builds one phrase plan per PHRASE query: the rarest term is the seed and the next rarest distinct term is a second filter, both keyed by (df, id). Layout then gives each shard an exact-size arena of u16 lists for sparse terms and a bitset for dense ones (list bytes at least equal to bitset bytes); unused terms are skipped. Pass 2 fills those structures; each shard is owned by one thread, so no atomics or locks are needed.

Queries run in blocks of 256 through one atomic counter. Each query is evaluated on every shard in document order with shard-local ids and merged by count, id sum and the first ten ids. BOOL queries keep strict left-to-right semantics over reusable list and bitset scratch. PHRASE queries take the seed's list, filter by the second term, and verify the survivors on the stored text span with a shift-and state machine over token keys.

The binary uses Rust's standard library only. Its read-only mmap block is the only raw-pointer operation; the two unchecked indexing sites (vocabulary slots and the byte map) carry one-line safety invariants.

## Why it is fast

One shard layer replaces v3's segments, replay buffers and per-segment serial loops. The global decisions run once, there is no per-document or per-term heap allocation, and every phase is one parallel scope with dynamic scheduling over many more work items than threads. Phrase verification touches only documents that contain the two rarest terms. The code is also much smaller than v3, which helps the S term of the score.

## Measurements

The fastest Track B board time recorded so far is commit a3f64b1 (T 0.631 s, M 57 MB, S 1,294, K 13.162834, 55.967 points), ahead of v2.5 (0.652 s) and v3 (0.711 s). Board results are in [PERF.md](PERF.md): v2.5 (272dc82) T 0.652 s, M 103 MB, S 1,059, K 11.872829; v3 (f473810) T 0.711 s, M 63 MB, S 1,212, K 12.304135. The V4 version in this folder was written without a Rust toolchain and has not been built, gated or timed, so it has no measurements and no board run yet.

## Dependencies

None. The solution uses Rust's standard library only.

## With another 24 hours

Build and gate the V4 draft, then measure it against v2.5 and v3 in one session (minimum of 20 runs). Tune the chunk count, the dense factor, the number of extra phrase filter terms and the query block size one at a time. If phrase verification dominates the query phase, store seed occurrence offsets so verification reads only the tokens around each occurrence. Reduce code size further only where it stays easy to audit.

## AI tools used

Gemini, GPT-6 Luna, and Claude assisted with SIMD structural parsing design, post-mortem bottleneck analysis, and verification workflows.