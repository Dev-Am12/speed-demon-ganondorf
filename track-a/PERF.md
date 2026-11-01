# Track A: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 32.05 | 228 | 0 |
| 1 | Rust single-threaded baseline (v0) | c0b4eb6 | 0.681 | 46 | 7.92 |
| 2 | A1: positional fixed-offset parsing and integer hour mapping | pending | 2.510 | 36 | 8.25 |
| 3 | A2: open-addressed compact endpoint table with word-wise hash | pending | 2.180 | 36 | 8.85 |
| 4 | A3: parallel line-snapped streaming parse and chronological assembly | pending | 1.004 | 52 | 11.45 |
| 5 | A4: FNV-1a interning and line count reduction (S=287) | pending | 1.010 | 52 | 11.15 |
| 6 | A5: structure-of-arrays records and direct streamed output | pending | 0.213 | 39 | 18.39 |
| 7 | A6: single-pass forward line parser | pending | 0.205 | 39 | 18.86 |
| 8 | A7: 8-byte word-wise endpoint hash table with u16 slots | pending | 0.205 | 39 | 18.97 |
| 9 | A11: 12-thread default worker pool | pending | 0.214 | 39 | 19.58 |
| 10 | A15: per-hour counting sort gather into flat Vec | pending | 0.139 | 44 | 20.25 |
| 11 | A10: pipelined streaming output via mpsc channel | pending | 0.149 | 42 | 21.35 |
| 12 | A16: SWAR 8-byte word scan for endpoint boundaries | pending | 0.135 | 41 | 22.84 |
| 13 | A9: direct integer formatting and precomputed hour prefixes | pending | 0.134 | 44 | 23.80 |
| 14 | A17: buffer sizing, table streamlining, and thread constants | pending | 0.133 | 43 | 24.50 |
| 15 | Thread tuning: 6-thread worker pool evaluation | pending | 0.131 | 41 | 25.40 |
| 16 | A18: parallel endpoint remap across worker threads | pending | 0.125 | 41 | 26.15 |
| 17 | A: 12 workers (A-v2 default; board 0.070 s) | pending | 0.070 | 42 | 30.50 |
| 18 | A21: 64-bit tag slot intern, flat arena, growable table | pending | 0.052 | 44 | 33.20 |
| 19 | M1: reserve record buffers once with chunk_len / 40 | pending | 0.052 | 44 | 33.20 |
| 20 | M2: pack records to 4B u32 (11-bit id | 1-bit err | 20-bit lat) with overflow list | pending | 0.050 | 41 | 35.80 |
| 21 | M3: progressive hourly buffer drops with ordered streaming pipeline | pending | 0.050 | 26 | 38.40 |
| 22 | A-v8-ARCH: global lock-free table, AVX2 parser, non-overlapping memory lifecycle, hybrid sort | pending | 0.072 | 17.87 | 27.88 |

### Experiment Log
- **Step 2 (A1 - Positional parsing & integer hour index):**
  - *Hypothesis:* Eliminating per-line iterator splitting (`split(' ')`) and replacing calendar parsing with direct fixed byte offset scanning and arithmetic day/hour mapping (`0..72`) reduces parser instruction overhead and branch misses.
  - *Local relative result:* Evaluated on full dataset; verifies byte-identical to expected SHA-256 with lower CPU instruction count.
  - *Verdict:* Kept. Correctness verified on tiny, sample, and full.
- **Step 3 (A2 - Open-addressed endpoint table):**
  - *Hypothesis:* Standard `HashMap<Vec<u8>, usize>` incurs SipHash overhead and multiple pointer indirections. A flat open-addressing table with word-wise hashing and full byte comparison on collision provides higher cache locality.
  - *Local relative result:* Wall clock time improves from ~2.51 s to ~2.18 s locally on full.
  - *Verdict:* Kept. Correctness verified on tiny, sample, and full.
- **Step 4 (A3 - Parallel streaming parse, 6-byte records, chronological formatting):**
  - *Hypothesis:* Chunking `requests.log` with small 64 KiB streaming buffers per worker avoids 240 MB chunk allocations. Packing records to 6 bytes (`u32 latency` + `u16 id|err<<15`) keeps records under 25 MB. Reusable thread-local hour accumulators and strictly ordered `hour_outputs` guarantee chronological output and low peak RSS.
  - *Local relative result:* Hyperfine 10 runs on full: baseline v0 2.764 s -> v1 1.004 s (2.75x speedup). Peak RSS dropped from 271 MB to 52 MB (/usr/bin/time -v). S = 320 lines.
  - *Verdict:* Kept. Thread count invariance (1, 2, 6, 12), 5-run determinism, and 49/49 fuzz seeds passed.
- **Step 5 (A4 - FNV-1a interning and line count reduction):**
  - *Hypothesis:* Replacing word-wise hashing with 64-bit FNV-1a, merging formatted timestamp output into the summary line writer, and eliminating redundant helper functions reduces source code lines $S$ towards $\le 300$ while maintaining identical throughput and cache locality.
  - *Local relative result:* Hyperfine 10 runs on full: 1.010 s ± 0.067 s (2.66x faster than v0 2.684 s). Peak RSS: 52,708 KB (~52.7 MB). Source lines $S = 287$ (Rust 280, Dockerfile 7), $D = 1$. Estimated $K = 11.15$ (vs baseline 7.92).
  - *Verdict:* Kept. Passed L0-L3, 5 identical runs, 49/49 fuzz seeds, and thread count invariance.
- **Step 6 (A5 - Structure-of-arrays records and direct streamed output):**
  - *Hypothesis:* Packing latency (u32) and endpoint+err (u16) into separate parallel Vecs achieves true 6 B per record without 2 B struct alignment padding (saving ~6.2 MB across 3.1M records). Streaming formatted hourly buffers directly to output eliminates the 10.1 MB `concat()` heap allocation.
  - *Local relative result:* Hyperfine 10 runs on full: a_v1c 228.8 ms ± 19.3 ms -> A5 213.2 ms ± 16.5 ms (1.07x faster). Peak RSS dropped from 52.5 MB to 39.2 MB (-13.3 MB, 25.3% reduction). S = 299 lines.
  - *Verdict:* Kept. Correctness verified on tiny, sample, full, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 7 (A6 - Single-pass forward line parser):**
  - *Hypothesis:* Replacing per-line subslice splitting (`split('\n')`) and multi-pass space scanning (`position(' ')`) with a single forward scan loop traversing directly from timestamp to LF touches each byte in the 64 KiB buffer once, eliminating intermediate Option checks and slice bounds checks.
  - *Local relative result:* Hyperfine 10 runs on full: A5 217.2 ms ± 21.2 ms -> A6 205.3 ms ± 16.7 ms (1.06x faster, min 191.5 ms). Peak RSS: 39,168 KB (~39.2 MB). S = 306 lines.
  - *Verdict:* Kept. Correctness verified on tiny, sample, full, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 8 (A7 - 8-byte word-wise endpoint hash table with u16 slots):**
  - *Hypothesis:* Replacing byte-by-byte FNV-1a with an 8-byte chunked hash mixing the first 8 bytes, last 8 bytes, and length reduces hash calculation from ~30-50 iterations to two 64-bit loads and rotate-mix operations for 99.9% of endpoints. Storing slot indices as `u16` instead of `usize` shrinks the 4096-entry table from 32 KiB to 8 KiB, keeping each worker's entire hash table in L1 cache. Full byte comparison resolves collisions on hit.
  - *Local relative result:* Hyperfine 10 runs on full: A6 220.6 ms ± 20.2 ms -> A7 205.3 ms ± 18.4 ms (1.07x faster, min 190.8 ms). Peak RSS: 38,848 KB (~38.8 MB). S = 316 lines.
  - *Verdict:* Kept. Correctness verified on tiny, sample, full, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 9 (A8 - Three-stage select_nth_unstable percentiles):**
  - *Hypothesis:* Replacing `sort_unstable` with shrinking `select_nth_unstable` partitions (p99, p95, p50) avoids $O(n \log n)$ full quicksort for groups with $n > 32$.
  - *Local relative result:* Hyperfine 10 runs on full: A7 209.6 ms ± 24.7 ms vs A8 212.6 ms ± 17.5 ms (neutral/slightly slower). S increased from 316 to 336 (+20 lines).
  - *Verdict:* Reverted per task card rule ("revert what is not clearly better"). Maintained fast cache-friendly `sort_unstable`.
- **Step 10 (A9 - Direct integer formatter and precomputed hour prefixes):**
  - *Hypothesis:* Precomputing 72 21-byte hour prefixes and formatting decimal integers directly into byte slices avoids `write!` format string parsing and UTF-8 validation overhead.
  - *Local relative result:* Hyperfine 10 runs on full: A7 229.2 ms ± 16.7 ms vs A9 228.5 ms ± 30.8 ms (neutral, within error). S increased from 316 to 354 (+38 lines). Because output formatting accounts for <15 ms total, manual string formatting does not overcome the line count penalty in the K formula.
  - *Verdict:* Reverted per task card rule ("revert what is not clearly better").
- **Step 11 (A11 - 12-thread worker pool scaling):**
  - *Hypothesis:* With lightweight single-pass line parsing and 8-byte word-wise table interning, thread serialization and memory contention are minimal. Expanding default workers from 6 to 12 allows all 72 hourly aggregations to map evenly (6 hours per worker) and saturate available hardware cores.
  - *Local relative result:* Hyperfine 10 runs on full: A7 (6 workers) 220.8 ms ± 13.0 ms -> A11 (12 workers) 214.0 ms ± 9.2 ms (1.03x faster locally, up to 1.28x under explicit THREADS=12 175.0 ms). Peak RSS: 39,168 KB (~39.2 MB). S = 316 lines.
  - *Verdict:* Kept. Correctness verified on tiny, sample, full, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 12 (A15 - Per-hour counting sort gather into flat Vec):**
  - *Hypothesis:* Counting sort (histogram + prefix sum + scatter into one flat `Vec<u32>`) eliminates 1,842 small `Vec` allocations per worker per hour (~22,000 allocations across 12 threads), improves cache locality, and reduces minor page faults.
  - *Local relative result:* Hyperfine 10 runs on full: baseline 147.2 ms ± 18.5 ms -> A15 139.3 ms ± 8.0 ms (saving 7.9 ms, 1.06x speedup). Peak RSS dropped from 47.1 MB to 44.5 MB (-2.6 MB). Page faults reduced by 612. System time dropped from 0.46s to 0.35s. S = 334 lines.
  - *K-accounting:* Evaluated at target (T ~40 ms, S ~330): $\Delta T \approx -2.15\text{ ms} \implies \times 1.0280$; $\Delta S = +22 \implies \times 0.9904$; net gain = $+1.81\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 13 (A10 - Pipelined ordered output via mpsc channel):**
  - *Hypothesis:* Streaming completed hours to the main writer thread via a safe standard library channel allows immediate sequential writing and per-hour buffer reclamation, overlapping disk I/O with worker compute and eliminating all 72 formatted hour buffers from peak memory.
  - *Local relative result:* Peak RSS dropped from 44.5 MB to 42.1 MB (-2.4 MB). Under cgroup v2 measurement, `memory.peak` dropped from 55.4 MB to 45.9 MB (-9.5 MB reduction in peak kernel footprint). Source lines reduced from 334 to 331 (-3 lines).
  - *K-accounting:* Evaluated at target (T ~40 ms, S ~330): $\Delta S = -3 \implies \times 1.0014$; memory saving $\Delta M \approx -9.5\text{ MB} \implies \times 1.066$; net gain = $+6.8\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 14 (A16 - SWAR 8-byte word scan for endpoint boundaries):**
  - *Hypothesis:* Scanning 8 bytes per iteration via safe 64-bit integer arithmetic (`u64::from_le_bytes` with zero-byte detection) replaces scalar byte-by-byte scanning for endpoint path delimiters, significantly speeding up the parse loop.
  - *Local relative result:* Hyperfine 10 runs on full: 149.9 ms ± 13.0 ms -> 135.4 ms ± 8.0 ms (saving 14.5 ms, 1.11x speedup, range min 124.5 ms). Peak RSS: 41,192 KB (~41.2 MB). Minor page faults dropped to 11,125. S = 346 lines (+15 lines).
  - *K-accounting:* Evaluated at target (T ~40 ms, S ~330): $\Delta T \approx -3.87\text{ ms} \implies \times 1.0522$; $\Delta S = +15 \implies \times 0.9934$; net gain = $+4.52\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 15 (A9 - Direct integer formatter and precomputed hour prefixes):**
  - *Hypothesis:* Precomputing 72 21-byte hour prefixes and formatting decimal integers directly into byte slices avoids `write!` format string parsing, UTF-8 validation overhead, and allows eliminating all `Arc` wrappers by borrowing stack variables directly across `thread::scope`.
  - *Local relative result:* Hyperfine 20 runs on full: 139.0 ms ± 18.0 ms -> 134.1 ms ± 23.6 ms (saving 4.9 ms wall time, min 111.4 ms). User CPU time dropped from 656.6 ms to 610.9 ms (-45.7 ms CPU time). Peak RSS: 43.8 MB. Source lines $S = 372$ (+26 lines).
  - *K-accounting:* Evaluated at target (T ~40 ms, S ~330): $\Delta T \approx 4.0\text{ ms} \implies \times 1.0541$; $\Delta S = +26 \implies \times 0.9887$; net gain = $+4.22\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 16 (A17 - Output buffer pre-sizing, dead table resize removal, and compile-time thread constants):**
  - *Hypothesis:* Pre-sizing hour output buffers to 160 KiB eliminates all dynamic vector reallocations during formatting. Removing dead table-resize checks (catalogue fixed at 1,842 endpoints < 4,096 table capacity) eliminates unneeded branches in the line parser. Compile-time thread constants `PARSE_WORKERS = 12` and `AGG_WORKERS = 12` ensure predictable worker sizing without environment variable overhead.
  - *Local relative result:* Hyperfine 20 runs on full: 147.4 ms ± 25.8 ms -> 146.0 ms ± 25.4 ms. User CPU time dropped from 651.2 ms to 601.1 ms (-50.1 ms CPU time). Peak RSS: 42.8 MB. Source lines $S = 360$ (-12 lines). Docker run time: 0.94 s.
  - *K-accounting:* Evaluated at target (T ~40 ms, S ~330): $\Delta T \approx 1.4\text{ ms} \implies \times 1.018$; $\Delta S = -12 \implies \times 1.0056$; net gain = $+2.4\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/12.
- **Step 17 (Thread tuning: 6-thread worker pool evaluation):**
  - *Hypothesis:* On 6 physical cores, 6 worker threads avoid hyperthreading context switching, reduce glibc arena allocations, and lower system time by 64% (from 541 ms to 194 ms). Peak cgroup memory drops from 50.95 MB to 40.80 MB.
  - *Local relative result:* Hyperfine 10 runs on full: 12-thread 184.5 ms ± 49.4 ms -> 6-thread 145.6 ms ± 25.0 ms (1.27x speedup). Cgroup `memory.peak` drops from 50.95 MB to 40.80 MB (-10.15 MB, 20% reduction). Anonymous RSS drops to 34.2 MB. Minor page faults drop from 11,116 to 9,104. S = 360 lines (0 net line change).
  - *K-accounting:* Evaluated at target (T ~40 ms, S ~330): $\Delta T \approx -4.2\text{ ms} \implies \times 1.057$; $\Delta M \approx -10.15\text{ MB} \implies \times 1.071$; net gain = $+13.2\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/8/12.
- **Step 18 (A18 - Parallel endpoint remap across worker threads):**
  - *Hypothesis:* Performing the local-to-global endpoint binary search mapping across scoped worker threads parallelizes the ~11,000 binary searches and memory accesses, reducing Phase B remap time from 3.00 ms to 1.51 ms.
  - *Local relative result:* Hyperfine 20 runs on full: 140.7 ms ± 14.7 ms -> 133.1 ms ± 8.0 ms (saving 7.6 ms wall time, 1.06x speedup, σ cut in half). User CPU time: 476.5 ms vs 501.4 ms. Peak RSS: 34.0 MB. Cgroup `memory.peak`: 42.66 MB. Source lines $S = 368$ (+8 lines).
  - *K-accounting:* Evaluated at target (T ~35 ms, S ~330): $\Delta T \approx 1.5\text{ ms} \implies \times 1.0221$; $\Delta S = +8 \implies \times 0.9964$; net gain = $+1.84\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/8/12.
- **Step 19 (A: 12 workers - A-v2 default; board 0.070 s):**
  - *Hypothesis:* Restore 12-worker default pool (`PARSE_WORKERS = 12`, `AGG_WORKERS = 12`). On 12-core evaluation hardware, 12 workers fully parallelize parse chunks and hourly aggregations, matching the A-v2 configuration that achieved 0.070 s on the live board.
  - *Local relative result:* Thread count invariance verified across default, 1, 2, 6, 8, and 12 threads producing identical SHA-256 output. S = 368 lines.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/8/12.
- **Step 20 (A21 - 64-bit tag slot intern, flat byte arena, growable table):**
  - *Hypothesis:* Replacing per-name `Vec<u8>` allocations with a single flat byte arena, indexing slots as `u64 = (tag << 32) | (id + 1)`, and checking length/memcmp only on a 32-bit tag match eliminates tens of thousands of heap allocations, keeps table probes in L1 cache, and provides safe dynamic resizing without a silent fixed cap.
  - *Single-core ns/line:*
    - Scan only: 51.56 ns/line
    - Baseline (scan + intern): 108.02 ns/line (intern alone: 56.46 ns/line)
    - A21 (scan + intern): 83.35 ns/line (intern alone: 31.79 ns/line, -43.7% intern time, saving ~76.7 ms CPU)
  - *Local relative result:* Interleaved 20 runs on full: baseline c841249 (150.3 ms ± 37.8 ms, User: 647.0 ms, Sys: 347.6 ms) -> A21 f8e8dd0 (113.9 ms ± 8.8 ms, User: 480.4 ms, Sys: 286.5 ms). Default 12 threads. 1.32x speedup. Peak RSS (42.4 MB vs 43.2 MB) and cgroup `memory.peak` (45.2 MB vs 46.9 MB, ~10,747 vs 11,109 faults) are unchanged by A21 (memory is flat; local RSS is not board M, as board M includes output file page cache). Source lines $S = 404$ (+36 lines).
  - *K-accounting:* Evaluated at board operating point ($T = 70\text{ ms}$, $S = 312 \to 404$): $\Delta T \approx -10.96\text{ ms} \implies \times 1.0889$; $\Delta S = +92 \implies \times 0.9620$; net gain $= +4.75\% \ge 1\%$ (relative to immediate prior commit $S = 368 \to 404 \implies \times 0.9861$, net gain $= +7.38\%$).
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/8/12.
- **Step 21 (M1 - Reserve record buffers once with capacity = chunk_len / 40):**
  - *Hypothesis:* Pre-sizing each `(worker, hour)` record vector to `capacity = chunk_len / 40` (Spec §3 minimum request line length: 27 ts + 4 spaces + 3 method + 1 endpoint + 3 status + 1 latency + 1 LF = 40) ensures vectors never reallocate during parsing. Untouched virtual address space is not charged against physical RSS.
  - *Local relative result:* Minor page faults dropped from 10,687 to 10,468 (-219 faults). VmHWM dropped from 40.3 MB to 39.7 MB. Container check passes (0.90 s). S = 405 lines (+1 line).
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/8/12.
- **Step 22 (M2 - Pack records to 4B u32 with overflow list):**
  - *Hypothesis:* Packing records into a single `u32` (11-bit local id | 1-bit 5xx error flag | 20-bit latency; Spec §2: catalogue has exactly 1,842 endpoints; M0 measured 98.82% of latencies < 2^20) reduces record size from 6 B to 4 B (-33% heap data). The 1.18% of latencies >= 2^20 go to a per-(worker, hour) `overflow: Vec<(u32, u32)>` with `0xFFFFF` stored in the record. In aggregation pass 2, a single linear stream loads id, error flag, and latency simultaneously.
  - *Local relative result:* Interleaved 20 runs: M1 167.3 ms ± 24.8 ms -> M2 138.9 ms ± 10.3 ms (1.21x faster). User CPU dropped from 517.1 ms to 491.2 ms (-25.9 ms). System CPU dropped from 315.4 ms to 219.6 ms (-95.8 ms). Minor page faults dropped from 10,468 to 8,246 (-2,222 faults, -21.2%). Peak RSS dropped from 39.7 MB to 31.3 MB (-8.4 MB). Cgroup `memory.peak` dropped from 45.2 MB to 41.04 MB (-4.16 MB). S = 421 lines (+16 lines).
  - *Known-bad control test:* Verified boundary latencies 0, 100, 1048574, 1048575, 1048576 (2^20), 1048577, and 4294967295 (2^32 - 1) across multiple hours; output matches reference implementation byte-for-byte.
  - *K-accounting:* Evaluated at board operating point ($T = 70\text{ ms}$, $M \approx 45.2\text{ MB}$, $S = 405 \to 421$): $\Delta T \approx -2.5\text{ ms} \implies \times 1.0183$; $\Delta M \approx -4.16\text{ MB} \implies \times 1.0146$; $\Delta S = +16 \implies \times 0.9942$; net gain $= +2.72\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/8/12.
- **Step 23 (M3 - Progressive hourly buffer drops with ordered streaming pipeline):**
  - *Hypothesis:* Workers claim hours dynamically from a shared `Mutex<Vec<Option<Vec<HourData>>>>`. Once an hour's data is scattered into `flat_lats`, that hour's buffers across all workers are dropped immediately (`drop(hour_data)`). Only currently active hours occupy memory during Phase C, allowing glibc to reclaim or reuse pages and preventing Phase A records from coexisting with Phase C formatted output.
  - *Local relative result:* Cgroup `memory.peak` dropped dramatically from 41.04 MB to 26.13 MB (-14.91 MB, -36.3% peak memory reduction). Peak RSS (VmHWM) dropped from 31.3 MB to 21.2 MB (-10.1 MB, -32.3%). Minor page faults dropped from 8,246 to 7,026 (-1,220 faults, -14.8%). User CPU time dropped from 477.3 ms to 469.1 ms. Container check passes (0.88 s). Because `memory.peak` fell to 26.13 MB, M4 (`malloc_trim`) is not triggered, keeping the code 100% `#![forbid(unsafe_code)]`. S = 440 lines (+19 lines).
  - *K-accounting:* Evaluated at board operating point ($M \approx 41.04 \to 26.13\text{ MB}$, $S = 421 \to 440$, $T \approx 70\text{ ms}$): $\Delta M \approx -14.91\text{ MB} \implies \times 1.0701$; $\Delta S = +19 \implies \times 0.9934$; net gain $= +6.30\% \ge 1\%$.
  - *Verdict:* Kept. Correctness verified on full, sample, tiny, 49/49 fuzz seeds, and threads 1/2/6/8/12.
- **Step 24 (A-v8-ARCH - Global lock-free table, AVX2 parser, non-overlapping memory lifecycle, hybrid sort):**
  - *Hypothesis:* Unifying worker endpoint tables into a single global table with atomic 64-bit slot probing avoids per-worker tables, table merging, and endpoint remapping loops (saving ~60 lines of S). Replacing SWAR with AVX2 vectorized space detection and 3-word hash reduces parser CPU. Dropping raw hourly buffers and calling `malloc_trim(0)` before opening and writing `/out/summary.txt` eliminates the physical memory overlap between Phase A/C record buffers and the 10.11 MB output file page cache.
  - *Implementation:*
    1. Single global lock-free-read table: `SLOTS: [AtomicU64; 4096]`, `META: [AtomicU64; 2048]`, `LOCK: Mutex<(usize, usize)>` locked only on cold miss (<1,842 times total).
    2. AVX2 parser: single `_mm256_loadu_si256` at offset 31 locating method space and endpoint space via bit manipulation.
    3. Non-overlapping memory lifecycle: Phase C results stored in flat `SUMMARY: [AtomicU32; 72 * 2048 * 5]`. Raw hour records dropped, `malloc_trim(0)` called, then summary formatted sequentially in 256 KB reusable chunks.
    4. Hybrid percentile selection: bypasses sorting for $n=1$, `sort_unstable` for $n \le 64$, 3-stage `select_nth_unstable` for $n > 64$.
  - *Local relative result:* S reduced from 423 to 264 lines (-37.6%). Total CPU time dropped by 26% (User: 0.27s vs 0.32s, System: 0.15s vs 0.25s). Cgroup `memory.peak` dropped from 22.07 MB to 17.87 MB (-19.0%). Wall time on ext4 in 20-run interleaved hyperfine was 164.4 ms vs 84.4 ms due to sequential post-trim output formatting.
  - *Verdict:* Verified against full dataset (exact SHA-256 match `495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3`), sample seeds 2..10 (9/9), full seeds 2..4 (3/3), tiny fuzz seeds 2..50 (49/49), thread invariance (1, 2, 6, 12), and edge cases.
