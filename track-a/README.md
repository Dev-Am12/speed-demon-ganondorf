# Track A: Ingest

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/a-ingest.md).

## Approach

The implementation (A-v11-ON-D715004) uses standard-library Rust with targeted AVX2 intrinsics:

1. **Parallel mapped parse (A3, A11):** Maps `/data/requests.log` with `mmap` (`PROT_READ, MAP_PRIVATE`) and divides byte offsets across 12 worker threads spawned with `std::thread::scope`. Each worker stream-parses its range without intermediate allocation, snapping to newlines and tracking chronological active hour capacities.
2. **AVX2 space scanner & fast line parser:** Locates method and endpoint boundaries using AVX2 SIMD comparison masks (`_mm256_cmpeq_epi8`), parsing dates and timestamps with direct positional arithmetic.
3. **Tag-only L1d global intern table:** Shared 4096-slot `AtomicU64` hash table (32 KiB, resident in L1d cache). Probes test an upper 52-bit tag against a full-string in-bounds hash. Cache hits return arrival IDs in 1 load with zero string comparisons, zero pointer dereferences, and lock-free Acquire ordering.
4. **Packed 4-byte records with overflow list:** Packs records into a single `u32` (11-bit endpoint arrival id | 1-bit 5xx error flag | 20-bit latency). Latencies $\ge 2^{20}$ spill into a compact per-(worker, hour) overflow list.
5. **Non-overlapping memory lifecycle & malloc_trim:** Hourly records are scattered into thread-local flat latency arrays and aggregated into a global preallocated `SUMMARY: [AtomicU32; 72 * 2048 * 5]`. At Barrier 2, before formatting begins, the leader drops all raw hour records, unmaps the input file (`munmap`), and calls `malloc_trim(0)`. This guarantees raw parse buffers and the input file do not overlap with the kernel page cache of the 10.11 MB output file (dropping cgroup v2 `memory.peak` to 5.37 MB).
6. **Zero-remap barrier endpoint sort:** At Barrier 2, the leader sorts the $\le 1,842$ active endpoint names lexicographically in 180 µs, storing the sorted indices into `ORDER: [AtomicU16; 2048]`, avoiding any data remapping during parsing or aggregation.
7. **Parallel Phase 3 format & ordered direct write:** All 12 worker threads participate in Phase 3 formatting from `SUMMARY` into thread-local 180 KiB reusable buffers. Output lines format integers via branchless lookup, utilizing a single-store `0x0a302030` (`"0 0\n"`) for zero-error endpoints. A sequential `WRITE_TURN` counter coordinates lock-free, in-order chunk writes directly to `/out/summary.txt`.

AI coding tools were used in drafting this solution, and human teammates reviewed, validated, and verified every line against the formal specification and test harness.

## Dependencies

None beyond the Rust standard library.

## With another 24 hours

1. **SIMD AVX2 parser:** Accelerate space and newline scanning using hand-tuned 256-bit SIMD registers once unconstrained by std.
2. **Monotone bucket sort:** For large groups, replace sorting with a small-radix or bucket queue for latency percentiles.
3. **Branchless integer output formatting:** Format numbers directly into preallocated page-aligned buffers via two-digit lookup tables.
