# Track B — Index (v6 verification candidate)

Track B answers 50,000 Boolean and exact-phrase queries over a 250,000-document corpus. The frozen organizer specification is authoritative: [specs/b-index.md](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

> **Status: verification candidate only.** This source has not been compiled or run through the organizer gate in the artifact environment. The latest user-supplied board result is for commit `4d1311a`; the uploaded ZIP has a different root commit (`080c956`). Reconcile the candidate against the exact `4d1311a` source before replacing any file. Do not push this file until the full correctness gate and same-session benchmark pass.

## Input and output contract

- Read `/data/docs.txt` and `/data/queries.txt`.
- Write only `/out/results.txt`; do not print diagnostics to stdout or stderr.
- There are 250,000 lines in `docs.txt`, each `document_id TAB text LF`. Document IDs are dense and ascending.
- `queries.txt` contains 31,000 BOOL queries and 19,000 PHRASE queries. Duplicate query lines remain separate outputs.
- The token alphabet is ASCII `[a-z0-9]`. Only ASCII uppercase letters are lowercased. Every other byte—including every byte >= `0x80`—is a separator. No Unicode case folding, stemming or stopword removal is performed.
- BOOL uses binary `AND`, `OR`, `NOT` operations **strictly left-to-right**, without precedence. `NOT` means current-result set difference.
- PHRASE matches adjacent token positions in exact order. Separators are invisible, but documents are hard boundaries. Repeated words must repeat.
- Each document counts once per query. Output lines preserve query order and contain exact count, exact `u64` sum of matching IDs, and the ten smallest matching IDs in ascending order (or all IDs when fewer than ten). An empty result is `0 0`.
- Output is ASCII with LF line endings and a final LF.

## Implementation outline

The candidate uses Rust standard library only and a read-only POSIX memory map for the corpus.

1. Parse the queries first and intern the query vocabulary. The source uses `u32` term IDs and a compact open-addressed lookup table. Query vocabulary reduction is allowed by the specification.
2. Split the corpus into line-aligned shards. Local document IDs fit in `u16`; shard count is increased if required by the local-ID limit.
3. Pass 1 records document text spans and per-shard document frequencies for query terms. It does not retain the full document text in the heap.
4. Build adaptive posting representations: sorted sparse `u16` lists for lower-frequency terms and dense bitsets for higher-frequency terms. Terms not needed by query evaluation are skipped.
5. Pass 2 fills the exact postings and selectively records byte offsets for rare phrase-seed occurrences.
6. Plan phrase queries around the rarest term. Candidate occurrences are checked using the same byte tokenizer and exact adjacent-token semantics. Unanchored repeated-word phrases use an exact Shift-And fallback.
7. Evaluate BOOL terms in strict left-to-right order. Reordering is restricted to safe contiguous AND-only runs; it never crosses an OR or NOT boundary. List intersections use merge for moderate size ratios and scalar galloping for highly skewed lists.
8. Fold each shard's exact count, sum and first ten IDs in document order, then write results once.

## v6 changes under review

- Fix the mutable binding for the query vector before the in-place query planner iterates with `&mut queries`.
- Track whether an intermediate BOOL result is empty. Skip binary operations while the accumulator remains empty until the next OR, which resets the accumulator to the right-hand set.
- Apply exact empty-right identities for AND, OR and NOT.
- Handle sparse-left / dense-right NOT as a sparse list difference rather than silently producing an empty list.
- Use a one-multiply initial bucket hash for vocabulary lookup. Full key equality remains mandatory; the hash change affects placement only. It is a performance hypothesis and must be compared against the previous hash on the same full dataset.

These changes do not change query parsing, tokenisation, matching semantics, output format or the permitted dependency set. No result is hard-coded by seed or query.

## Verification gate

Run from the repository root using the event's Rust 1.99.1 toolchain:

```bash
rustc --version
cargo fmt --manifest-path track-b/Cargo.toml -- --check
cargo build --manifest-path track-b/Cargo.toml --release --offline
fixtures/b_gate.sh track-b/target/release/solution --no-full
fixtures/b_fuzz.sh track-b/target/release/solution 2 40 tiny
fixtures/b_fuzz.sh track-b/target/release/solution 2 6 sample
fixtures/b_gate.sh track-b/target/release/solution
```

The full output hash for the official seed-1 full dataset must be:

```text
f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927
```

The committed gate also checks the specification example, hostile cases, large synthetic cases when generated, determinism across repeated runs, and thread masks 1/2/6/12 (CPU lists `0`, `0-1`, `0-5`, `0-11`). Re-run all checks after every source edit and after the final `rustfmt` pass. A single mismatch, failed build, unexpected output, nondeterminism or thread-mask failure is a hard stop.

## Performance and scoring

The latest team-supplied board result for Track B is commit `4d1311a`: `T=0.592 s`, `M=66 MB`, `S=1,014`, `K=13.702564`, machine score `56.840`. The last supplied fifth-place leaderboard K was `24.997261`; at unchanged memory/source factors, this would require about `0.178 s`. The last supplied first-place K was `35.047382`, equivalent to about `0.0905 s` at unchanged other factors. These are score-equation calculations, not forecasts; the candidate has no board result.

Track B's clock includes reading, indexing, querying and writing. `T` dominates the remaining opportunity: memory is capped at a 50× ratio and source/dependency ratios cannot pay. Keep zero runtime dependencies. See [PERF.md](PERF.md) for the formula, source-derived dataset facts, research references, experiment order, current evidence and caveats.
