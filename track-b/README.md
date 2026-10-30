# Track B: Index

The contract and exact output format are in the [Track B specification](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/b-index.md).

## Approach

The v1 solution parses queries first and interns their distinct terms in a deterministic open-addressed vocabulary. It maps `/data/docs.txt` read-only and scans line-aligned chunks in parallel twice: the first pass counts each term's document frequency and records document ranges; the second fills exact-sized posting storage. The corpus is never copied into a heap buffer.

Indexing and phrase verification share one byte tokenizer. A 256-entry class table handles ASCII lowercase, uppercase and digits; every other byte is a separator. Tokens longer than 32 bytes still occupy a phrase position but cannot match a query term. Terms use a fixed-width, zero-padded representation in the vocabulary table.

Posting sets use sorted document-ID lists when their document frequency is below `max(1, N/32)`, and dense bitsets otherwise. BOOL operations stay strictly left to right and choose list or bitset scratch storage based on their inputs. PHRASE queries seed candidates from their rarest distinct term, filter by membership in the other terms, then check consecutive tokens in each remaining document.

Workers claim blocks of 256 queries atomically and keep their scratch buffers private. Each completed block is retained by index and written in query order, so scheduling and CPU affinity do not change output bytes. Output is written once, flushed, and the process exits after closing the file.

This version uses Rust's standard library only. The only unsafe code is the read-only `mmap` call and slice construction, guarded by checks for nonzero length, `isize` bounds and `MAP_FAILED`.

## Why it is faster

The memory map avoids copying the full corpus. Parallel, line-aligned scanning shares work across up to 12 workers, while the two-pass index avoids growing posting vectors during the corpus scan. The hybrid representation makes dense-set operations word-based, and parallel query blocks reduce the serial evaluation cost without changing result order.

## Measurements

The supplied board result for v0 was run `40ad00c`: 13.5 s, 400 MB, 308 source lines, K 2.397181, 18.985 points.

On the local machine, hyperfine (2 warmups, 10 runs) measured v0 at 12.956 s +/- 2.472 s and v1 at 2.845 s +/- 0.159 s on full. The v1 memory watcher reported RssAnon 273.67 MB, VmHWM 530.43 MB and RssFile 255.99 MB. The local scc count is 1,113 lines including Dockerfile (1,105 Rust lines). The resulting K estimate is 4.638, marked local; v1 has not been measured on the board.

## Dependencies

None. The solution uses Rust's standard library only.

## With another 24 hours

Measure the dense-set threshold and query block size on the board, keeping a full correctness gate for every change. Then profile the remaining scan and phrase-verification costs and reduce code size only where it does not weaken the tested behavior.

## AI tools used

OpenAI Codex (GPT-6) assisted with implementation, code review and verification workflow.
