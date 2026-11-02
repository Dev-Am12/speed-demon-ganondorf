# Track C performance log (Join)

This file is the complete measurement record for Track C. Section 1 lists every board push the scoring board has recorded for this team, in time order: 43 pushes, of which 39 were scored and 4 failed. Later sections cover correctness, the local experiments, the diagnostic ablation, the scoring context, and the discrepancies we have found in our own records.

## Disclosure and conventions

- **Commit IDs replace run IDs.** The raw board feed did not give a run ID for every run. Every board row here is identified by the commit that was scored. Run IDs we did receive are kept in `runs_ledger.md`.
- **Time zone.** Times are UTC, as in the board feed. Team notes sometimes give dates in IST (UTC+5:30).
- **Board values are authoritative.** Rows come from the board feed. Local measurements (a laptop, Intel Core i5-1235U under WSL2, 12 logical CPUs) are in their own tables and do not predict board values.
- **Points** are the board's speed points, `50 × log10 K`. Every scored row was checked: the points match `50 × log10 K` to within rounding.
- **Notes column.** A note comes from the earlier log (step numbers), the ledger, or the root README where one exists. "No change description recorded" means the record does not say what changed in that commit. We have not guessed.
- **Best.** The row marked "Scored (best)" is the highest-scoring Track C push, currently `3b7b716`.

## Current status

| | Commit | Finished (UTC) | Points | K | T | M | S |
|---|---|---|---:|---:|---:|---:|---:|
| Best and current Track C result | `3b7b716` | 2 Nov 11:51 | 75.426 | 32.249688 | 0.042 s | 9 MB | 371 |
| Previous best before 2 Nov | `c35a41d` | 1 Nov 09:39 | 70.837 | 26.106129 | 0.060 s | 10 MB | 393 |

`3b7b716` is also the commit of the uploaded repository snapshot (the archive comment carries its full hash), so the Track C README describes the source that scored 75.426.

Board pushes for this track: 43 in total, 39 scored, 4 failed. The failures are in section 1 and section 5.

## 1. Board history (every push, in time order)

Times are UTC. Step numbers refer to the earlier log's step list (steps 1 to 9).

| # | Finished (UTC) | Commit | Result | Points | K | T | M (MB) | S (lines) | Notes |
|---:|---|---|---|---:|---:|---:|---:|---:|---|
| 1 | 10-31 11:35 | `a3f64b1` | Scored | 53.646 | 11.828112 | 0.168 s | 29 | 622 | Step 1: two-pass SIMD quote-parity scanner, region lookup table. First recorded Track C board run. |
| 2 | 10-31 12:28 | `080c956` | Scored | 67.853 | 22.754156 | 0.074 s | 10 | 509 | Step 2: single-pass record-boundary snapping, mmap region loader, 64-bit name table. |
| 3 | 10-31 12:47 | `f9a6ddb` | Scored | 67.654 | 22.546806 | 0.075 s | 10 | 509 | No change description recorded. |
| 4 | 10-31 13:12 | `c6ef5cf` | Failed: build | — | — | — | — | — | Build failed (Track C and Track B; Track A scored 66.971 at this commit). Team account: a Rust build error from a push made before a local build was confirmed. |
| 5 | 10-31 13:20 | `4d1311a` | Scored | 67.477 | 22.363358 | 0.065 s | 18 | 400 | No change description recorded. |
| 6 | 10-31 14:49 | `1ce625d` | Scored | 66.806 | 21.683157 | 0.064 s | 22 | 400 | No change description recorded. |
| 7 | 10-31 15:03 | `be71488` | Failed: too slow | — | — | — | — | — | Failed: too slow, hit the time limit. |
| 8 | 10-31 15:37 | `a49777a` | Failed: too slow | — | — | — | — | — | Failed: too slow, hit the time limit. |
| 9 | 10-31 16:10 | `5792950` | Failed: too slow | — | — | — | — | — | Failed: too slow, hit the time limit. Track D scored its best (48.692) at this commit. |
| 10 | 10-31 18:23 | `8c36883` | Scored | 66.746 | 21.623679 | 0.067 s | 20 | 395 | No change description recorded. |
| 11 | 10-31 18:37 | `1039e85` | Scored | 68.983 | 23.969214 | 0.068 s | 12 | 389 | Prefetch and structure work (root README). Metrics here are from the board feed; this run was not in the earlier log. |
| 12 | 10-31 19:23 | `3fe70c9` | Scored | 70.109 | 25.245704 | 0.067 s | 10 | 389 | Step 3: team C version on the speed-demon branch. |
| 13 | 10-31 20:00 | `a52ad27` | Scored | 68.932 | 23.913717 | 0.073 s | 10 | 389 | Step 4: same code as step 3, scored about 40 minutes later (run-to-run noise of about 1.2 points). |
| 14 | 10-31 21:03 | `15d48f6` | Scored | 67.243 | 22.124088 | 0.085 s | 10 | 399 | Step 5: C0 full-coverage long-name hash, plus C2a distinct count from the fetch_or result. Other tracks were reset in this run. |
| 15 | 10-31 21:11 | `3165259` | Scored | 70.237 | 25.394298 | 0.063 s | 10 | 399 | Step 6: same code as step 5, restored with the other tracks. |
| 16 | 10-31 21:31 | `7580e12` | Scored | 70.508 | 25.713786 | 0.063 s | 10 | 393 | Step 7: E1 duplicate (name, country) cache removed. Best measured result at the time. |
| 17 | 10-31 22:02 | `b22f4e2` | Scored | 70.118 | 25.255831 | 0.065 s | 10 | 393 | Step 8: same E1 code, scored in the run with the Track B change. |
| 18 | 11-01 06:42 | `167b950` | Scored | 67.933 | 22.838238 | 0.078 s | 11 | 393 | No change description recorded. |
| 19 | 11-01 07:49 | `71743e4` | Scored | 70.406 | 25.592651 | 0.061 s | 11 | 393 | No change description recorded. |
| 20 | 11-01 08:48 | `faaeb9d` | Scored | 70.513 | 25.719560 | 0.061 s | 11 | 393 | No change description recorded. |
| 21 | 11-01 09:39 | `c35a41d` | Scored | 70.837 | 26.106129 | 0.060 s | 10 | 393 | Step 9: all-track run. Last result before 2 Nov. |
| 22 | 11-01 09:49 | `8e07365` | Scored | 70.670 | 25.905722 | 0.061 s | 10 | 393 | No change description recorded. |
| 23 | 11-01 10:00 | `64925b1` | Scored | 70.868 | 26.143867 | 0.060 s | 10 | 393 | Same commit ID as the Track A best (76.902). |
| 24 | 11-01 10:32 | `e3c539c` | Scored | 74.357 | 30.700043 | 0.043 s | 9 | 450 | No change description recorded. Source 450 lines. |
| 25 | 11-01 10:49 | `ffb33f1` | Scored | 74.721 | 31.218452 | 0.042 s | 10 | 411 | No change description recorded. |
| 26 | 11-01 16:31 | `8a9667d` | Scored | 74.599 | 31.044012 | 0.042 s | 10 | 411 | No change description recorded. |
| 27 | 11-01 16:35 | `82dbc51` | Scored | 74.573 | 31.007227 | 0.042 s | 10 | 411 | No change description recorded. Track B failed at this commit. |
| 28 | 11-01 16:51 | `d715004` | Scored | 72.752 | 28.513011 | 0.049 s | 10 | 411 | No change description recorded. Track B failed at this commit. |
| 29 | 11-01 18:54 | `e091971` | Scored | 74.274 | 30.582671 | 0.043 s | 11 | 411 | No change description recorded. |
| 30 | 11-01 21:17 | `f29eac6` | Scored | 74.690 | 31.174792 | 0.042 s | 10 | 411 | No change description recorded. Track B's first Claude-written rewrite was scored at this commit. |
| 31 | 11-02 03:11 | `bbbb3f1` | Scored | 74.494 | 30.894735 | 0.043 s | 10 | 411 | No change description recorded. |
| 32 | 11-02 03:28 | `2deeaab` | Scored | 74.498 | 30.899927 | 0.042 s | 10 | 411 | No change description recorded. |
| 33 | 11-02 03:35 | `21a5643` | Scored | 74.588 | 31.028434 | 0.043 s | 10 | 411 | No change description recorded. |
| 34 | 11-02 04:34 | `005c99a` | Scored | 74.491 | 30.890247 | 0.042 s | 10 | 411 | Same commit ID as the Track B best (61.034). |
| 35 | 11-02 04:47 | `ce0d3c1` | Scored | 74.658 | 31.128925 | 0.043 s | 10 | 411 | No change description recorded. Track B failed at this commit. |
| 36 | 11-02 05:28 | `3640ce1` | Scored | 75.192 | 31.904207 | 0.042 s | 10 | 371 | No change description recorded. Source down to 371 lines. Track B failed at this commit. |
| 37 | 11-02 06:04 | `6b67bfe` | Scored | 75.302 | 32.066009 | 0.041 s | 10 | 371 | No change description recorded. Track B failed at this commit. |
| 38 | 11-02 06:45 | `248c1df` | Scored | 75.214 | 31.935814 | 0.041 s | 10 | 371 | Repair commit: restore verified sources (ledger: A 64925b1, B 005c99a). |
| 39 | 11-02 06:58 | `0c55114` | Scored | 75.376 | 32.175409 | 0.041 s | 10 | 371 | Same commit ID as Track B's kept change (rebuild Vocab, remove remap). The root README previously listed this as the Track C current result; it is a Track B commit. |
| 40 | 11-02 07:17 | `25e6277` | Scored | 75.257 | 31.999246 | 0.041 s | 10 | 371 | No change description recorded. Track B reverted MAX_DENSE change at this commit. |
| 41 | 11-02 07:47 | `fbe7674` | Scored | 75.240 | 31.973869 | 0.042 s | 10 | 371 | No change description recorded. Track B reverted INDEX_THREADS change at this commit. |
| 42 | 11-02 08:22 | `9d15558` | Scored | 75.017 | 31.647174 | 0.042 s | 10 | 371 | No change description recorded. |
| 43 | 11-02 11:51 | `3b7b716` | Scored (best) | 75.426 | 32.249688 | 0.042 s | 9 | 371 | Snapshot commit (archive hash). Best Track C result to date; current submission. |

Notes on this table:

- **Failures.** Four pushes failed: one build failure (`c6ef5cf`, which also failed Track B) and three "too slow, hit the time limit" failures (`be71488`, `a49777a`, `5792950`) on 31 Oct. Track C's score at `5792950` was a failure while Track D scored its best at the same commit.
- **Steps 3 and 4** are the same code. The board scored them 1.2 points apart, about 40 minutes apart. The difference is run-to-run noise.
- **Corrections to the earlier version of this file.** (a) The earlier log listed `1039e85` as "not recorded" for T, M and S. The feed gives 0.068 s, 12 MB, 389 lines and 68.983 points. (b) The root README listed `0c55114` as the Track C current result at 75.376. It is a Track B commit. The Track C result at 75.376 came from the all-track push at 06:58 UTC, which also scored Tracks A, B and D. (c) The root README called `080c956` the first Track C board result. The first recorded Track C board run is `a3f64b1` (53.646 points, 31 Oct 11:35 UTC). `080c956` is the first run above 67 points.
- **Shared commits.** Several Track C rows share a commit ID with a Track A or Track B row. Where the record says what changed, the note says so. Where it does not, the row is recorded as shared.

## 2. Correctness status

| Check | Last recorded | Result |
|---|---|---|
| Full seed 1 output (`aggregates.txt`) | `c35a41d` | SHA-256 `d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536`, which matches the expected value |
| Tiny seed 1 | `c35a41d` | `e4e43bc39920e9d446c3fcd700fdd35b3959e9495c6aaa738491df38a1aed733`, matches |
| Sample seed 1 | `c35a41d` | `9035fed36823232dd41ae2d1b9d6d21aa7c73f49d8e95647e953a89d5ebe9a66`, matches |
| Repeat runs | `c35a41d` | Five identical outputs |
| CPU pinning (0, 0-1, 0-5, 0-11) | `c35a41d` | Identical outputs |
| Hostile long-name fixture | `c35a41d` | Matches the reference |
| Differential fuzz, 300 seeds | `09d07cc` (1 Nov, local) | Matches the reference on all seeds |
| Full seed 1, `3b7b716` | not recorded | **To run before freeze** |

The hostile long-name fixture generator (`fixtures/make_c_longname.py`) is named in earlier notes but is not in this repository snapshot.

## 3. Local experiments

Local results are from the laptop, full seed 1, paired runs with `hyperfine -N`, in both run orders when the result mattered. A change was kept only if the correctness gate passed and the paired gain was at least 3% in every order tested. Local RSS values include mapped input and are not comparable with board M.

| Experiment | Change | Local result | Decision |
|---|---|---|---|
| C0 | Full-coverage hash for names over 32 bytes (`hash_long`) | Correctness fix. The old hash ignored bytes 24 to len − 8 of long names, so two different names could count as one. The hostile fixture now matches | Kept (correctness) |
| C2a | Count distinct names from the `fetch_or` result; remove the post-pass over the table | 1.07× (61.0 vs 65.6 ms) | Kept |
| C2b | Skip quote counts for delimiter phases the record never uses | No gain (67.2 vs 66.6 ms) | Rejected |
| E1 | Remove the 2,048-entry duplicate `(name, country)` cache (measured hit rate 0.015%) | 1.09× (60.3 vs 65.7 ms), tighter spread | Kept |
| E2 | Dynamic scheduling: four line-aligned chunks per thread from a shared counter | 1% (61.0 vs 61.8 ms) | Rejected |
| E3 | Key and mask words packed into one 40-byte slot | Slower (65.6 vs 63.8 ms) | Rejected |
| E4 | Prefetch name slots, delay inserts by four records | Result reversed with run order (9% and 28%) on a loaded machine; not proven | Rejected |
| C-S1, PF = 16 | Prefetch ring of 16 | Noisy, order-dependent PSET result (137.6 ms local) | Reverted |
| C-S1, PF = 8 | Prefetch ring of 8 | Inconsistent; one order gained 2.7% (110.0 ms local) | Reverted |
| C-S1, PF = 32 | Prefetch ring of 32 | Noisy; one order 6.8% slower (124.9 ms local) | Reverted |
| 1 Nov 07:10 (`1e11ee6`) | Correctness: final record at EOF | Candidate matches reference on 300 fuzz seeds (local) | Kept |
| 1 Nov 07:11 (`09d07cc`) | Correctness: sparse lookup for region keys with more than 7 digits | Candidate matches reference on all 300 fuzz seeds (local). See section 6, item 1 | Kept |

Local timings from the ledger (for example 70 ms at `1e11ee6`) are not board results and are not comparable with the board T column.

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

## 4. Open experiments

| Idea | Expected effect | Status |
|---|---|---|
| Cheaper deduplication under the 10 MB memory cap | Name path is about half of run time | No design found that stays inside memory: per-thread tables would multiply memory by 12 |
| Smaller region map (1 MB random reads per record) | About 9 ms | Not started |
| Source reduction from 371 lines | About 0.3 points per 100 lines removed (estimate) | Planned before freeze |

## 5. Failed runs

- **Build failure (`c6ef5cf`, 31 Oct 13:12 UTC).** Track A scored 66.971 at this commit; Track B also failed. Team account: a Rust build error from a push made before a local build was confirmed.
- **Time-limit failures (`be71488` 15:03, `a49777a` 15:37, `5792950` 16:10 UTC, 31 Oct).** These were earlier versions of the Track C code that exceeded the board's time limit. The record does not say which change caused the slowdown. The later versions from `3fe70c9` onward were all scored.

## 6. Scoring context

```text
K = (Tref / T)^0.50 × min(Mref / M, 50)^0.20 × min(Sref / S, 1)^0.15 × min(Dref / D, 1)^0.15
Tref = 14.44 s, Mref = 367 MB, Sref = 108 lines, D = 1 (no runtime dependency)
Points = 50 × log10(K)
```

Check of the current row: with T = 0.042, M = 9, S = 371, the formula gives K ≈ 32.3, against the board's 32.25. The difference is within the rounding of T, which the board reports to three decimal places.

The earlier log estimated that top five needed about K 36 at Track C's memory and size. On the 31 Oct board, Track C was not in the top 30. The current leaderboard position is not recorded in this file.
