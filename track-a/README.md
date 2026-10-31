# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The implementation (Wave 1) is written in safe Rust using only the standard library:

1. **Parallel streaming parse (A3):** Snaps `/data/requests.log` into line-aligned byte ranges across 6 worker threads spawned with `std::thread::scope`. Each worker stream-parses its range in 64 KiB chunks via `read_at`, maintaining carried slice bytes across buffer boundaries. This completely avoids allocating large file-sized or chunk-sized buffers in memory (keeping input buffer memory under 400 KiB total).
2. **Positional field scanning (A1):** Uses fixed byte offsets from the 27-byte RFC 3339 timestamp to extract date/time digits and compute the dense hour index in `0..72` with overflow-safe arithmetic. Endpoints, status codes, and latencies are parsed directly from byte slices without string allocations.
3. **Compact endpoint interning (A2):** Each worker maintains a local open-addressing hash table with word-wise hashing and byte-level collision comparison. Local endpoint tables are merged and deduplicated once globally, then sorted lexicographically by unsigned byte order.
4. **Compact 6-byte records:** Records pack `u32` latency and a 16-bit tagged endpoint ID/error flag into 6 bytes, keeping the entire 3.1M request record state under 25 MB.
5. **Parallel per-hour quantile formatting & chronological order:** Workers dynamically claim hours via an atomic counter. Group latencies within each hour are sorted to calculate nearest-rank percentiles (`p50`, `p95`, `p99`) alongside 5xx error counts and ppm. Each worker writes its hourly output to an indexed slot in `hour_outputs[0..HOURS]`, which is concatenated in chronological hour order, guaranteeing byte-identical total order across any thread count.
6. **Single atomic output write:** The merged buffer is written in one buffered pass to `/out/summary.txt`, flushed and closed before process exit. Empty files yield an empty 0-byte output.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None beyond the Rust standard library.

## With another 24 hours

1. **SIMD delimiter search:** Accelerate space and newline scanning using AVX2 vector instructions.
2. **Monotone bucket sort:** For large groups, replace sorting with a small-radix or bucket queue for latency percentiles.
3. **Branchless integer output formatting:** Format numbers directly into preallocated page-aligned buffers via two-digit lookup tables.
