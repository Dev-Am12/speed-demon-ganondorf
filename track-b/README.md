# Track B: Index

Track B evaluates Boolean and exact-phrase queries over the document corpus. The organizer [specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md) defines the input, tokenizer, query semantics, and output contract.

## Current board result

The all-track submission `c35a41d` finished on 2026-11-01 at 09:39 UTC. Track B scored 59.852 points with K 15.741032, T 0.435 s, M 67 MB, and S 1,112 lines.

That submission used Track B candidate `e7c9ea6`. This promotion restores Track A's best source from `faaeb9d` and updates these Track B notes. The promotion commit has not been scored yet.

| Track | Points | K | Time | Memory | Lines |
|---|---:|---:|---:|---:|---:|
| A | 73.053 | 28.910180 | 0.048 s | 28 MB | 440 |
| B | 59.852 | 15.741032 | 0.435 s | 67 MB | 1,112 |
| C (best) | 70.837 | 26.106129 | 0.060 s | 10 MB | 393 |
| D | 48.206 | 9.207174 | 0.957 s | 135 MB | 369 |

## Implementation

The program uses Rust's standard library only. It parses queries first, interns only query terms, maps `docs.txt` read-only, and scans line-aligned shards in parallel.

The vocabulary table stores a compact tag and term id. A tag match is checked against the full key, so hash collisions do not change term identity. The tokenizer uses an AVX2 block classifier for common input and a scalar fallback for longer tokens.

Two corpus passes build the index. Per-term postings use sparse local-id lists or dense bitsets, chosen from the shard document frequency. Boolean operations are evaluated left to right; intersection operands may be reordered only within an uninterrupted AND run. Phrase queries use rare seed terms and exact token-position checks, with an exact fallback for phrases without an indexed seed. Each shard contributes exact counts, sums, and the ten smallest matching document ids.

The current B code removes BOOL emptiness tracking and the galloping-list intersection path. `Acc10::push_word` adds set bits in increasing order through the common accumulator method.

## Build and verification

```bash
cd track-b
RUSTFLAGS='-C target-cpu=x86-64-v3' cargo build --release --offline
cd ..
fixtures/gate_b.sh track-b/target/release/solution
fixtures/b_fuzz.sh track-b/target/release/solution 2 40 tiny
fixtures/b_fuzz.sh track-b/target/release/solution 2 6 sample
```

The expected full seed-1 output hash is:

```text
f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927
```

## Dependencies and next work

There are no third-party Rust dependencies and no `libc` crate. The next useful work is to profile the two corpus passes on the scoring host, then test one scan or memory-layout change at a time against the full correctness gate.

AI tools used across this Track B work: Codex, Gemini 4 Argon, and Claude.
