# Track D: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 72.69 | 893 | 0 |
| 1 | Rust single-threaded baseline (v0) | 85a7884 | 8.796 | 79 | 3.75 |
| 2 | D5: parallel queries via std::thread::scope and AtomicUsize | pending | 3.716 | 239 | 5.355 |

### Experiment Log (Wave 1)
- **Step 2 (D5 - Parallel queries):**
  - *Hypothesis:* Queries account for 88.7% (11.18s) of execution time. Distributing queries across worker threads using `std::thread::scope` and an `AtomicUsize` work counter with thread-local search buffers will scale query throughput near-linearly.
  - *Local relative result:* Hyperfine on full: baseline single-threaded 12.341 s -> 6 threads 4.209 s (2.93x faster, 153 MB RSS) -> 12 threads 3.716 s (3.32x faster, 239 MB RSS).
  - *Verdict:* Kept. Invariance verified for threads 1/2/6/12, 5/5 identical sample runs, 49/49 fuzz seeds passed. Estimated K improves from 3.75 to 5.355 (12 threads) / 5.503 (6 threads).
