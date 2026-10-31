# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The implementation (Wave 3) is written in safe Rust (`#![forbid(unsafe_code)]`) using only the standard library:

1. **Parallel streaming parse (A3, A11, A17):** Snaps `/data/requests.log` into line-aligned byte ranges across 12 worker threads (`PARSE_WORKERS = 12`) spawned with `std::thread::scope`. Each worker stream-parses its range in 64 KiB chunks via `read_at`, maintaining carried slice bytes across chunk boundaries. This avoids allocating large file-sized buffers in memory, keeping input buffer memory under 800 KiB across all workers.
2. **SWAR delimiter search & single-pass parsing (A6, A16):** Employs safe 8-byte SWAR word scanning (`u64::from_le_bytes` with zero-byte detection) to locate endpoint space boundaries at word granularity instead of byte-by-byte loops. Traverses each line from the RFC 3339 timestamp (inlined integer hour mapping) through method, endpoint, status, and latency digits to the trailing newline.
3. **64-bit tag slot intern & flat byte arena (A7, A17, A18, A21):** Each worker maintains an open-addressing hash table storing 64-bit slots (`(tag << 32) | (id + 1)`) and string bytes in a single contiguous flat arena (`Vec<u8>`). Length and full string equality are tested only on a 32-bit hash tag match, keeping probe misses in L1 cache and eliminating all per-string heap allocations. Tables dynamically grow on demand. Local endpoint tables are sorted and remapped in parallel across worker threads via `std::thread::scope`.
4. **Compact 4-byte packed records with overflow list (A5, M2):** Packs records into a single `u32` (11-bit local id | 1-bit 5xx error flag | 20-bit latency; Spec §2: catalogue has exactly 1,842 endpoints; 98.82% of latencies fit within 20 bits). The 1.18% tail latencies >= 2^20 go to a compact per-(worker, hour) overflow list (`Vec<(u32, u32)>`). This cuts record memory from 6 B to 4 B (-33%), eliminating 6.2 MB of heap allocations and reducing minor page faults by 21%.
5. **Counting sort per-hour gather (A15):** Aggregates hourly latencies across workers using a flat counting sort (histogram + prefix sum + scatter into one flat `Vec<u32>` reused per worker). Eliminates 22,000 intermediate small `Vec` allocations, dramatically improving CPU cache locality and eliminating minor page faults.
6. **Direct integer formatter & precomputed hour prefixes (A9):** Formats output lines directly into byte slices using precomputed 21-byte RFC 3339 timestamp prefixes and inlined decimal integer-to-ASCII serialization. Bypasses all `write!` format string parsing, UTF-8 validation, and dynamic argument dispatch.
7. **Progressive memory deallocation & pipelined streaming output (A10, A17, M3):** Hourly records are moved into a shared thread-safe queue. Each worker drops an hour's raw records across all workers immediately after scattering into `flat_lats` (`drop(hour_data)`). Completed 160 KiB formatted hour buffers are sent over an `mpsc::channel` directly to the writer thread, which streams them sequentially to `/out/summary.txt` via `BufWriter` and drops each buffer upon write. Peak cgroup memory drops to 26.13 MB (-42% from baseline).

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None beyond the Rust standard library.

## With another 24 hours

1. **SIMD AVX2 parser:** Accelerate space and newline scanning using hand-tuned 256-bit SIMD registers once unconstrained by std.
2. **Monotone bucket sort:** For large groups, replace sorting with a small-radix or bucket queue for latency percentiles.
3. **Branchless integer output formatting:** Format numbers directly into preallocated page-aligned buffers via two-digit lookup tables.
