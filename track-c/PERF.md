# Track C: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | S | K | Points | Verdict |
|---|---|---|---:|---:|---:|---:|---:|---|
| 0 | The reference, unchanged | - | 14.444 | 367.44 | 108 | 0.980 | 0.000 | Baseline Python reference |
| 1 | C1: SIMD PCLMULQDQ quote-parity CSV scanner, O(1) popcnt field lengths, direct 1 MB region lookup array, and shared lock-free 128-bit hash to 256-bit country bitmask table | pending | pending | pending | pending | pending | pending | Candidate ready for board verification |

### Experiment Log
- **Step 1 (C1 — Branchless `PCLMULQDQ` + AVX2 structural CSV join):**
  - *Hypothesis:* Parsing 201.9 MB of hostile RFC-4180 CSV via a scalar state machine branches on every byte. Using 64-byte AVX2 comparisons paired with `_mm_clmulepi64_si128` carry-less multiplication computes the exact quote parity mask in one instruction, allowing 12 threads to locate structural commas and record terminators while deriving unescaped `note` and `name` byte lengths directly from quote popcounts (`raw_len - 1 - q_count / 2`) without copying bytes. A shared lock-free 128-bit hash table with 256-bit country masks eliminates 12x per-thread hash table duplication.
  - *Verification:* Passed `tiny`, `sample`, and `full` SHA-256 verification (`d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536`), thread-count invariance (`1`, `2`, `6`, `12` threads), and 5-run determinism checks.