# Track B: Index

The contract and exact output format are in the [Track B specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

The current V3 local candidate parses all queries into fixed-size records and interns query terms in a compact open-addressed vocabulary. It maps `/data/docs.txt` read-only, builds a document-start table once, and scans each document's text after its first TAB.

The corpus is split into 16 ordered segments (or more when required by the 16-bit local document-ID limit). In pass one, each worker counts per-term document frequency and records vocabulary-hit term IDs grouped by document. Pass two replays those IDs to fill flat posting arenas, avoiding a second tokenization pass. The largest measured replay buffer, including document offsets, was 7,371,264 bytes per segment on full data.

For each segment, sparse posting lists retain 32-bit local IDs and frequent terms use bitsets. BOOL queries use reusable list/bitset scratch storage and preserve strict left-to-right operator semantics. PHRASE queries seed from the rarest retained term, filter candidates by posting membership, and verify consecutive tokens with the same tokenizer used by indexing. Segment results are accumulated in document order, then serialized once in query order. Fixed contiguous query slices give each worker disjoint scratch and accumulator ownership.

The binary uses Rust's standard library only. Its read-only mmap block is the only raw-pointer operation; remaining unchecked accesses have one-line safety invariants beside them.

## Why it is fast

The memory map avoids a corpus copy. Parallel line-table construction and scanning use up to 12 workers. Replaying term IDs replaces the second full tokenizer pass, while flat posting arenas avoid per-term allocation and resizing. Dense sets use word operations, and rare phrase seeds limit document verification.

## Measurements

The supplied v2.5 board baseline is T 0.652 s, M 103 MB, S 1,059 lines, K 11.872829 and 53.728 points (commit/run ID 272dc82). The current V3 candidate has only local measurements: median 1.068 s (minimum 0.622 s over 20 runs), peak RssAnon 41.128 MB in the conservative repeated memwatch sample, S 1,212 calibrated scc code lines, and projected K 16.062. No V3 board run has been supplied.

Earlier supplied board results remain in [PERF.md](PERF.md). All local and projected values are clearly labeled there.

## Dependencies

None. The solution uses Rust's standard library only.

## With another 24 hours

Profile the remaining query and segment costs under quieter benchmark conditions. Revisit compact 16-bit sparse postings with a query path that avoids its measured speed loss, then test segment sizes and dense cutoffs against the full gate. Reduce code size only where it remains easy to audit.

## AI tools used

GPT-6 Luna, Gemini 4 Argon, and Claude assisted across implementation, review, and verification.
