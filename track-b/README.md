# Track B: Index

The contract and exact output format are in the [Track B specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

The current b/v2 implementation builds on the board-measured v1 baseline. It parses query terms into a deterministic open-addressed vocabulary, maps /data/docs.txt read-only and scans line-aligned chunks in parallel twice. Pass one counts document frequencies and records document boundaries; pass two fills flat posting arenas. The corpus is never copied to a heap buffer.

A shared byte scanner handles both passes. A compile-time 256-entry table maps ASCII letters and digits, while all other bytes are separators. Short terms use packed keys; longer keys are confirmed against the stored term bytes, and a bitmap filters most vocabulary misses before probing. Tokens longer than 32 bytes still occupy a phrase position but cannot match query terms.

Posting sets use sorted document-ID lists below max(1, N/32) and dense bitsets otherwise. BOOL operations are evaluated strictly left to right with reusable list or bitset scratch buffers. PHRASE queries order their terms by document frequency, seed from the rarest one, filter candidates by membership, then verify consecutive tokens using precomputed phrase keys. Workers claim blocks of 256 queries and retain output by block index, so output remains deterministic across thread counts. The completed blocks are written in order; the result file is flushed and closed before the process exits.

Rust's standard library is the only dependency. The only unsafe code is the read-only mmap call and slice construction, guarded by checks for nonzero length, isize bounds and MAP_FAILED.

## Why it is fast

The memory map avoids copying the corpus. Parallel line-aligned scanning shares indexing work across up to 12 workers, and exact-size flat arenas avoid growing posting vectors during the scan. The vocabulary prefilter reduces probes for non-query tokens. Dense-set operations use word-level bit operations; rarest-term phrase seeds reduce verification candidates; parallel query blocks preserve ordered results while spreading evaluation across workers.

## Measurements

The supplied v1 board result is run 832e2c8: 1.408 s, 278 MB, 1,033 source lines, K 6.643748 and 41.121 points.

The kept local V2 result is Stage 3: hyperfine (2 warmups, 10 runs) measured 1.858 s +/- 0.178 s on full. The memory watcher reported RssAnon 87,336 kB, VmHWM 337,308 kB and RssFile 249,972 kB. Calibrated scc --by-file src/main.rs Dockerfile reports 1,172 code lines (1,165 Rust plus 7 Dockerfile). The local-only K estimate is 7.156; no V2 board measurement is claimed.

Stage 4a packed sparse postings and reduced RssAnon to 64,940 kB, but its paired full run was slower than the acceptance limit, so that experiment was reverted.

## Dependencies

None. The solution uses Rust's standard library only.

## With another 24 hours

Profile the remaining full-corpus scan and query costs under stable benchmark conditions. Try a compact posting representation that keeps query iteration fast, then tune memory with the full correctness gate. Reduce source size only where the behavior remains easy to audit.

## AI tools used

OpenAI Codex (GPT-6) assisted with implementation, code review and verification workflow.
