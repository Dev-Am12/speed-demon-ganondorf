# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The implementation (Wave 3) is written in safe Rust (`#![forbid(unsafe_code)]`) using only the standard library:

1. **Parallel streaming parse (A3, A11, A17):** Snaps `/data/requests.log` into line-aligned byte ranges across 6 worker threads (`PARSE_WORKERS = 6` default for cgroup safety) spawned with `std::thread::scope`. Each worker stream-parses its range in 64 KiB chunks via `read_at`, maintaining carried slice bytes across chunk boundaries. This avoids allocating large file-sized buffers in memory, keeping input buffer memory under 400 KiB across all workers.
2. **SWAR delimiter search & single-pass parsing (A6, A16):** Employs safe 8-byte SWAR word scanning (`u64::from_le_bytes` with zero-byte detection) to locate endpoint space boundaries at word granularity instead of byte-by-byte loops. Traverses each line from the RFC 3339 timestamp (inlined integer hour mapping) through method, endpoint, status, and latency digits to the trailing newline.
3. **Compact 8-byte word-wise endpoint interning & parallel remap (A7, A17, A18):** Each worker maintains a local open-addressing hash table with word-wise hashing (mixing the first 8 bytes, last 8 bytes, and length). Storing slot indices as `u16` keeps the entire 4096-slot table in 8 KiB (L1 cache) without dynamic resizing overhead. Local endpoint tables are merged, sorted lexicographically, and remapped in parallel across worker threads via `std::thread::scope`.
4. **Structure-of-arrays 6-byte records (A5):** Stores latencies (`Vec<u32>`) and tagged endpoint IDs with error flags (`Vec<u16>`) in separate parallel vectors, eliminating struct alignment padding and achieving true 6 B per record across all 3.1M requests.
5. **Counting sort per-hour gather (A15):** Aggregates hourly latencies across workers using a flat counting sort (histogram + prefix sum + scatter into one flat `Vec<u32>` reused per worker). Eliminates 22,000 intermediate small `Vec` allocations, dramatically improving CPU cache locality and eliminating minor page faults.
6. **Direct integer formatter & precomputed hour prefixes (A9):** Formats output lines directly into byte slices using precomputed 21-byte RFC 3339 timestamp prefixes and inlined decimal integer-to-ASCII serialization. Bypasses all `write!` format string parsing, UTF-8 validation, and dynamic argument dispatch.
7. **Pipelined streaming output writer (A10, A17):** Workers send completed 160 KiB hour buffers over an `std::sync::mpsc::channel` directly to the main writer thread, which streams them sequentially to `/out/summary.txt` via `BufWriter` and drops each buffer immediately. Peak kernel cgroup memory drops by ~9.5 MB by preventing buffer accumulation.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None beyond the Rust standard library.

## With another 24 hours

1. **SIMD AVX2 parser:** Accelerate space and newline scanning using hand-tuned 256-bit SIMD registers once unconstrained by std.
2. **Monotone bucket sort:** For large groups, replace sorting with a small-radix or bucket queue for latency percentiles.
3. **Branchless integer output formatting:** Format numbers directly into preallocated page-aligned buffers via two-digit lookup tables.
