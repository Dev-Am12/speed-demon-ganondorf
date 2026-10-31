# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The implementation (Wave 2) is written in safe Rust using only the standard library:

1. **Parallel streaming parse (A3, A11):** Snaps `/data/requests.log` into line-aligned byte ranges across 12 worker threads spawned with `std::thread::scope`. Each worker stream-parses its range in 64 KiB chunks via `read_at`, maintaining carried slice bytes across buffer boundaries. This completely avoids allocating large file-sized buffers in memory (keeping input buffer memory under 800 KiB total).
2. **Single-pass forward line parsing (A6):** Traverses each 64 KiB buffer directly in a single forward loop from the 27-byte RFC 3339 timestamp (inlined integer hour mapping) past the HTTP method, endpoint, status code, and latency digits to the trailing LF. Eliminates all intermediate subslice splits, Option checks, and redundant scans.
3. **Compact 8-byte word-wise endpoint interning (A7):** Each worker maintains a local open-addressing hash table with word-wise hashing (mixing the first 8 bytes, last 8 bytes, and length in ~3 CPU cycles). Storing slot indices as `u16` keeps the entire 4096-slot table in 8 KiB (L1 cache). Local endpoint tables are merged and deduplicated once globally, sorted lexicographically by unsigned byte order.
4. **Structure-of-arrays 6-byte records (A5):** Stores latencies (`Vec<u32>`) and tagged endpoint IDs with error flags (`Vec<u16>`) in separate parallel vectors, eliminating struct alignment padding and achieving true 6 B per record across all 3.1M requests.
5. **Parallel per-hour quantile aggregation & chronological order:** Workers dynamically claim hours via an atomic counter. Group latencies within each hour are sorted to calculate nearest-rank percentiles (`p50`, `p95`, `p99`) alongside 5xx error counts and ppm. Each worker writes its hourly output to an indexed slot in `hour_outputs[0..HOURS]`.
6. **Direct streamed output write (A5):** Hourly output buffers are streamed sequentially to `/out/summary.txt` via a buffered writer and freed immediately, avoiding any duplicate 10 MB buffer allocation. Empty files yield an empty 0-byte output.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None beyond the Rust standard library.

## With another 24 hours

1. **SIMD delimiter search:** Accelerate space and newline scanning using AVX2 vector instructions.
2. **Monotone bucket sort:** For large groups, replace sorting with a small-radix or bucket queue for latency percentiles.
3. **Branchless integer output formatting:** Format numbers directly into preallocated page-aligned buffers via two-digit lookup tables.
