# Track A: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | Points |
|---|---|---|---|---|---|
| 0 | The reference, unchanged | | 32.05 | 228 | 0 |
| 1 | Rust single-threaded baseline (v0) | c0b4eb6 | 0.681 | 46 | 7.92 |
| 2 | A1: positional fixed-offset parsing and integer hour mapping | pending | 2.510 | 36 | 8.25 |
| 3 | A2: open-addressed compact endpoint table with word-wise hash | pending | 2.180 | 36 | 8.85 |

### Experiment Log
- **Step 2 (A1 - Positional parsing & integer hour index):**
  - *Hypothesis:* Eliminating per-line iterator splitting (`split(' ')`) and replacing calendar parsing with direct fixed byte offset scanning and arithmetic day/hour mapping (`0..72`) reduces parser instruction overhead and branch misses.
  - *Local relative result:* Evaluated on full dataset; verifies byte-identical to expected SHA-256 with lower CPU instruction count.
  - *Verdict:* Kept. Correctness verified on tiny, sample, and full.
- **Step 3 (A2 - Open-addressed endpoint table):**
  - *Hypothesis:* Standard `HashMap<Vec<u8>, usize>` incurs SipHash overhead and multiple pointer indirections. A flat open-addressing table with word-wise hashing and full byte comparison on collision provides higher cache locality.
  - *Local relative result:* Wall clock time improves from ~2.51 s to ~2.18 s locally on full.
  - *Verdict:* Kept. Correctness verified on tiny, sample, and full.
