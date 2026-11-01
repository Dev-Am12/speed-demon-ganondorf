# Track C: performance log

This is a living log. Every row in the board tables is an official measurement from the live board, identified by the scored commit and the time the board finished the run (UTC). The board page shows no run IDs, so these identifiers are the commit and the finish time. Local timings are kept in their own table and are never used as board results.

## Current submission

| | |
|---|---|
| Latest all-track board commit | c35a41d |
| Board result (finished 2026-11-01 09:39 UTC) | 70.837 points; K 26.106129; T 0.060 s; M 10 MB; S 393 lines |
| Best measured board result | 70.837 points; K 26.106129 (commit c35a41d) |
| Correctness | Full seed 1: `d8088b1c…f536` (matches the expected hash). Tiny `e4e43bc3…`, sample `9035fed3…` match. Five repeat runs identical. Four CPU-pinning configurations (0, 0-1, 0-5, 0-11) identical. Hostile long-name fixture matches the reference. |

## 1. Board log

Points are the board's machine speed points, 50 × log10(K).

| Step | Change | Board commit | Finished (UTC) | T (s) | M (MB) | S (lines) | K | Points | Status |
|---|---|---|---|---:|---:|---:|---:|---:|---|
| 0 | Organiser reference, unchanged | — | — | 14.44 | 367 | 108 | 1.000 | 0.000 | reference |
| 1 | Two-pass SIMD quote-parity scanner, region lookup table | a3f64b1 | not recorded | 0.168 | 29 | 622 | 11.828 | 53.646 | superseded |
| 2 | Single-pass record-boundary snapping, mmap region loader, 64-bit name table | 080c956 | not recorded | 0.074 | 10 | 509 | 22.754 | 67.853 | superseded |
| 3 | Team C version on `speed-demon` at 3fe70c9 | 3fe70c9 | 19:23 | 0.067 | 10 | 389 | 25.246 | 70.109 | superseded |
| 4 | Same C code as step 3, scored in a later run | a52ad27 | 20:00 | 0.073 | 10 | 389 | 23.914 | 68.932 | same code |
| 5 | C0 full-coverage long-name hash + C2a distinct from the fetch_or result | 15d48f6 | 21:03 | 0.085 | 10 | 399 | 22.124 | 67.243 | superseded (other tracks were reset in that run) |
| 6 | Same as step 5, restored with the other tracks | 3165259 | 21:11 | 0.063 | 10 | 399 | 25.394 | 70.237 | superseded |
| 7 | **E1: remove the duplicate `(name, country)` cache** | **7580e12** | **21:31** | **0.063** | **10** | **393** | **25.714** | **70.508** | **best measured** |
| 8 | Same E1 code, scored with the Track B change | b22f4e2 | 22:02 | 0.065 | 10 | 393 | 25.256 | 70.118 | superseded |
| 9 | Latest all-track run | c35a41d | 09:39 (Nov 1) | 0.060 | 10 | 393 | 26.106 | 70.837 | current |

Step 4 scored the same code as step 3 about 40 minutes later; the 1.0 point difference is run-to-run noise. Steps 6 and 7 differ by about 0.3 points, which is also within that noise. Points differences under about 3 are not evidence of a change.

## 2. Local experiments (laptop, not board results)

Method: same session, full seed 1, paired runs with `hyperfine -N`, both run orders when the result matters. A change is kept only if the correctness gate passes and the paired gain is at least 3% in every order tested.

| Experiment | Change | Local result | Decision |
|---|---|---|---|
| C0 | Full-coverage hash for names over 32 bytes (`hash_long`) | Correctness fix: the old hash ignored bytes 24 to len-8 of long names, so two different names could count as one. Hostile fixture now matches. | Kept (correctness) |
| C2a | Count distinct names from the `fetch_or` result; remove the post-pass over the table | 1.07× (61.0 vs 65.6 ms) | Kept |
| C2b | Skip quote counts for delimiter phases the record never uses | No gain (67.2 vs 66.6 ms) | Rejected |
| E1 | Remove the 2,048-entry duplicate `(name, country)` cache (measured hit rate 0.015%) | 1.09× (60.3 vs 65.7 ms), tighter spread | Kept |
| E2 | Dynamic scheduling: four line-aligned chunks per thread from a shared counter | 1% (61.0 vs 61.8 ms) | Rejected |
| E3 | Key and mask words packed into one 40-byte slot | Slower (65.6 vs 63.8 ms) | Rejected |
| E4 | Prefetch name slots, delay inserts by four records | Result reversed with run order (9% and 28%) on a loaded machine; not proven | Rejected |

### Where the time goes (diagnostic builds, timing only)

Field ablation on E1 (removing one piece of work at a time, 12 threads):

| Removed | Time (ms) | Saved |
|---|---:|---:|
| Nothing (E1) | 61.3 | — |
| Id parsing | 57.5 | about 4 ms |
| Amount parsing | 57.6 | about 4 ms |
| Region lookup | 52.0 (noisy) | about 9 ms |
| Name hashing and table update | 32.9 | about 28 ms |
| Memory read only (floor) | 10.2 | — |

On one core the name path is 54% of run time; on 12 threads it is 50%. The cost is per core, not contention between threads. A hash-only split was measured in a different session and was not repeated on E1, so the division between hashing and table update is not final.

## 3. Open experiments

| Idea | Expected effect | Status |
|---|---|---|
| Cheaper deduplication under the 10 MB memory cap | Name path is about half of run time | No design found that stays inside memory: per-thread tables multiply memory by 12 |
| Smaller region map (1 MB random reads per record) | About 9 ms | Not started |
| Source reduction from 393 lines | About 0.3 points per 100 lines removed | Planned before freeze |

## 4. Scoring context

K = (Tref/T)^0.50 × min(Mref/M, 50)^0.20 × min(Sref/S, 1)^0.15 × min(Dref/D, 1)^0.15, with Tref 14.44 s, Mref 367 MB, Sref 108 lines, and D = 1 (no runtime dependency). Points = 50 × log10(K).

Top five on the latest board snapshot needs K 36.16 at the current memory and size: about 30 ms against 63 ms now. The measured levers together reach roughly 1.1 to 1.3× K. Top five is not expected on this track.
