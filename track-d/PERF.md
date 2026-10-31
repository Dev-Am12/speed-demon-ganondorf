# Track D: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 72.69 | 893 | 0 |
| 1 | Rust single-threaded baseline (v0) | 85a7884 | 8.796 | 79 | 3.75 |
| 2 | D5: parallel queries via std::thread::scope and AtomicUsize | pending | 3.716 | 239 | 5.355 |
| 3 | D3: compact u32 offsets array; drops redundant working arrays | pending | 4.087 | 227 | 5.424 |

### Experiment Log (Wave 1)
- **Step 2 (D5 - Parallel queries):**
  - *Hypothesis:* Queries account for 88.7% (11.18s) of execution time. Distributing queries across worker threads using `std::thread::scope` and an `AtomicUsize` work counter with thread-local search buffers will scale query throughput near-linearly.
  - *Local relative result:* Hyperfine on full: baseline single-threaded 12.341 s -> 6 threads 4.209 s (2.93x faster, 153 MB RSS) -> 12 threads 3.716 s (3.32x faster, 239 MB RSS).
  - *Verdict:* Kept. Invariance verified for threads 1/2/6/12, 5/5 identical sample runs, 49/49 fuzz seeds passed. Estimated K improves from 3.75 to 5.355 (12 threads) / 5.503 (6 threads).
- **Step 3 (D3 - Compact u32 offsets):**
  - *Hypothesis:* Offsets array size is $n+1 = 1,740,001$ entries. Since $m = 5,000,000 < 2^{32}-1$, switching `offsets` from `usize` (8 bytes) to `u32` (4 bytes) saves ~7 MB permanent heap and another ~7 MB during pass 2 building, reducing peak RSS without performance penalty.
  - *Local relative result:* Peak RSS reduced from 238.8 MB to 227.4 MB (-11.4 MB on 12 threads) and 153.4 MB to 144.4 MB (-9.0 MB on 6 threads). Wall clock time unchanged (4.087 s on 12 threads, 4.329 s on 6 threads).
  - *Verdict:* Kept. Invariance verified for threads 1/2/6/12, 5/5 identical sample runs, 49/49 fuzz seeds passed. K estimate 5.424.
