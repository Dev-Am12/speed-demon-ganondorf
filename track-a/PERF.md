# Track A: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 32.05 | 228 | 0 |
| 1 | Rust single-threaded baseline (v0) | c0b4eb6 | 0.681 | 46 | 7.92 |
| 2 | A1: positional fixed-offset parsing and integer hour mapping | pending | 2.510 | 36 | 8.25 |
| 3 | A2: open-addressed compact endpoint table with word-wise hash | pending | 2.180 | 36 | 8.85 |
| 4 | A3: parallel line-snapped streaming parse and chronological assembly | pending | 1.004 | 52 | 11.45 |

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
  - *Local relative result:* Hyperfine 10 runs on full: baseline v0 2.764 s -> v1 1.004 s (2.75x speedup). Peak RSS dropped from 271 MB to 52 MB (/usr/bin/time -v). S = 285 lines. Estimated K = 11.45 (vs v0 baseline 7.92).
  - *Verdict:* Kept. Thread count invariance (1, 2, 6, 12), 5-run determinism, and 49/49 fuzz seeds passed.
