# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The implementation (Wave 4, A-v8-ARCH) uses standard-library Rust with targeted AVX2 intrinsics:

1. **Parallel mapped parse (A3, A11):** Maps `/data/requests.log` with `mmap` (`PROT_READ, MAP_PRIVATE`) and divides byte offsets across 12 worker threads spawned with `std::thread::scope`. Each worker stream-parses its range without intermediate allocation.
2. **AVX2 dual-space line parser (A6, A16):** Employs 256-bit AVX2 SIMD (`_mm256_loadu_si256` with space comparison mask) to locate the method-space delimiter and endpoint boundary in a single instruction. Computes the day and hour indices with direct arithmetic.
3. **Single global lock-free-read endpoint table (A7, A21):** Replaces per-worker hash tables and post-merge remap passes with a single shared table: `SLOTS: [AtomicU64; 4096]` and `META: [AtomicU64; 2048]`. Workers probe atomically with Acquire ordering on the 99.94% hit path; insertion locks a mutex only on the <1,842 cold misses in the entire dataset.
4. **Packed 4-byte records with overflow list (A5, M2):** Packs records into a single `u32` (11-bit endpoint id | 1-bit 5xx error flag | 20-bit latency). Latencies $\ge 2^{20}$ spill into a compact per-(worker, hour) overflow list.
5. **Non-overlapping memory lifecycle & malloc_trim (M3, M4):** Hourly records are scattered into thread-local flat latency arrays and aggregated into a global preallocated `SUMMARY: [AtomicU32; 72 * 2048 * 5]`. Once Phase C aggregation completes, all raw record buffers are dropped and `malloc_trim(0)` returns heap pages to the OS before `/out/summary.txt` is opened or written. This strictly prevents Phase A/C working vectors (12.4 MB) from overlapping with the kernel output page cache (10.1 MB).
6. **Hybrid percentile selection:** Avoids sorting when $n = 1$, uses `sort_unstable` when $n \le 64$, and executes a 3-stage `select_nth_unstable` partitioning when $n > 64$.
7. **Sequential chunked output formatting:** Formats summary statistics from `SUMMARY` into `/out/summary.txt` in 256 KiB reusable buffers, completely decoupling heap memory from output cache.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None beyond the Rust standard library.

## With another 24 hours

1. **SIMD AVX2 parser:** Accelerate space and newline scanning using hand-tuned 256-bit SIMD registers once unconstrained by std.
2. **Monotone bucket sort:** For large groups, replace sorting with a small-radix or bucket queue for latency percentiles.
3. **Branchless integer output formatting:** Format numbers directly into preallocated page-aligned buffers via two-digit lookup tables.
