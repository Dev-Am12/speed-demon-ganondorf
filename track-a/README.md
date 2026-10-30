# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The baseline implementation (v0) is written in safe, single-threaded Rust using only the standard library.

1. **Ingest & I/O:** Reads `/data/requests.log` through a 256 KiB `BufReader`, reusing a single line buffer across the 3.1M lines to avoid per-line heap allocation.
2. **Zero-allocation timestamp parsing:** Timestamp digits are parsed directly from the guaranteed 27-byte RFC 3339 string. The 72-hour contest span (`2026-10-30T18:00:00Z` to `2026-11-02T17:59:59Z`) is mapped directly via integer arithmetic to a dense hour index in `0..72`.
3. **Dense endpoint mapping:** Unique endpoints are mapped to contiguous integer IDs via a standard `HashMap<Vec<u8>, usize>`. Lookups borrow `&[u8]` without allocating; allocations occur only on the first encounter of each catalogue endpoint (at most 1,842 times).
4. **Group aggregation:** Aggregated in a 2D table `groups[hour_idx][endpoint_id]`, giving O(1) direct slice indexing for each line to append latencies and increment error counts.
5. **Deterministic total ordering:** Unique endpoints are sorted once by unsigned byte order (proper prefix first). The output is emitted hour-major: an outer loop over `0..72` hours and an inner loop over sorted endpoint IDs. This guarantees total lexicographical order without sorting the output file or iterating a `HashMap`.
6. **Integer arithmetic:** Percentiles use the exact integer nearest-rank formula `(p * n + 99) / 100`, keeping duplicate latencies in sorted order. Error ppm is calculated via `(errors * 1_000_000) / count` with integer math; status 499 is explicitly treated as non-error.
7. **Buffered output:** The entire output is formatted into an in-memory buffer and written in a single pass to `/out/summary.txt`, then flushed and closed before process exit. Empty input files produce an empty 0-byte output.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None.

## With another 24 hours

1. **Parallel chunking:** Partition `/data/requests.log` into chunks snapped to line boundaries across worker threads using `read_at`, with thread-local endpoint interning and lock-free append buffers.
2. **Custom hash table:** Replace standard `HashMap` with a compact, cache-friendly open-addressing table and word-wise integer hashing.
3. **SIMD delimiter scanning:** Use AVX2 vector instructions to find spaces and newlines rapidly.
4. **Fast formatting:** Replace `write!` with branchless two-digit lookup table formatting directly into page-aligned output buffers.
