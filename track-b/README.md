# Track B: Index

The task, input files and exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

The v0 solution reads the queries first and interns their distinct terms into a vocabulary. It then scans `/data/docs.txt` once, tokenizes each document, and stores ascending document IDs only for terms used by queries. Each document's text range is retained so phrase candidates can be checked later.

Indexing and phrase verification share one byte tokenizer. It lowercases ASCII letters, treats maximal `[a-z0-9]` runs as tokens, and treats every other byte as a separator. A token longer than 32 bytes still occupies a phrase position, but cannot match a query term.

BOOL queries run their sorted posting-list operations strictly from left to right. PHRASE queries first intersect the posting lists for their distinct terms, starting with the rarest list, then re-tokenize each candidate document and check for the exact consecutive token sequence. Results are formatted in query order with the count, the sum of all matching IDs, and up to the ten smallest IDs.

This is a single-threaded, safe Rust baseline using only the standard library. It reads each input with `std::fs::read` and writes `/out/results.txt` once.

## Board result

Run `40ad00c`: 13.5 s, 400 MB, 308 source lines, K 2.397181, 18.985 points.

## Dependencies

None. The solution uses Rust's standard library only.

## With another 24 hours

Profile the baseline, then measure focused improvements to document scanning, posting-list representation, and query evaluation while keeping the output hash unchanged.