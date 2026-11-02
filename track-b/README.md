# Track B performance log (Index)

This file is the complete measurement record for Track B. Section 1 lists every board push the scoring board has recorded for this team, in time order: 53 pushes, of which 47 were scored and 6 failed to build. Later sections cover the local experiments, the diagnostic profile, the scoring context, and the discrepancies we have found in our own records.

## Disclosure and conventions

- **Commit IDs replace run IDs.** The raw board feed did not give a run ID for every run. Every board row here is identified by the commit that was scored. Run IDs we did receive are kept in `runs_ledger.md`.
- **Time zone.** Times are UTC, as in the board feed. Team notes sometimes give dates in IST (UTC+5:30), so a run that the team describes as "2 Nov" may appear here as 1 Nov UTC. Section 6 lists the cases.
- **Board values are authoritative.** Rows come from the board feed. Local measurements (a laptop, Intel Core i5-1235U under WSL2, 12 logical CPUs) are in their own tables and do not predict board values.
- **Points** are the board's speed points, `50 × log10 K`. Every scored row was checked: the points match `50 × log10 K` to within rounding.
- **Notes column.** A note comes from the team's notes, the ledger, or the root README where one exists. "No change description recorded" means the team record does not say what changed in that commit. We have not guessed.
- **Best.** The row marked "Scored (best)" is the highest-scoring Track B push. The overall score uses the best Track B result (61.034 points).

## Current status

| | Commit | Finished (UTC) | Points | K | T | M | S |
|---|---|---|---:|---:|---:|---:|---:|
| Best Track B result | `005c99a` | 2 Nov 04:33 | 61.034 | 16.622063 | 0.297 s | 122 MB | 1,235 |
| Latest Track B push | `3b7b716` | 2 Nov 11:51 | 60.640 | 16.322698 | 0.308 s | 122 MB | 1,225 |

`3b7b716` is also the commit of the uploaded repository snapshot (the archive comment carries its full hash). The Track B README describes the source of that snapshot. The best result, `005c99a`, is a different source (1,235 lines against 1,225).

Board pushes for this track: 53 in total, 47 scored, 6 failed to build. The failures are listed in section 1 and discussed in section 5.

## 1. Board history (every push, in time order)

Times are UTC. Notes are as recorded; see the conventions above.

| # | Finished (UTC) | Commit | Result | Points | K | T | M (MB) | S (lines) | Notes |
|---:|---|---|---|---:|---:|---:|---:|---:|---|
| 1 | 10-30 19:27 | `40ad00c` | Scored | 18.985 | 2.397181 | 13.5 s | 400 | 308 | Earliest recorded run (about 13 s). Root README calls the 18.985-point figure the single-threaded baseline. No change description recorded. |
| 2 | 10-30 19:59 | `ddcc075` | Scored | 19.402 | 2.443630 | 12.9 s | 400 | 308 | Early version. No change description recorded. |
| 3 | 10-30 20:04 | `db73a8d` | Scored | 19.096 | 2.409497 | 12.3 s | 400 | 406 | Early version. No change description recorded. |
| 4 | 10-30 20:30 | `85a7884` | Scored | 18.903 | 2.388119 | 12.5 s | 400 | 406 | Early version. No change description recorded. |
| 5 | 10-30 21:17 | `832e2c8` | Scored | 41.121 | 6.643748 | 1.408 s | 278 | 1,033 | B v1 (team notes). |
| 6 | 10-30 22:17 | `1a55090` | Scored | 49.080 | 9.585006 | 0.970 s | 103 | 1,172 | B v2 (team notes). Earlier docs listed 49.450; the board's K gives 49.080. |
| 7 | 10-30 22:43 | `1a65ccb` | Scored | 53.595 | 11.800617 | 0.646 s | 108 | 1,059 | No change description recorded. |
| 8 | 10-31 04:26 | `272dc82` | Scored | 53.728 | 11.872829 | 0.652 s | 103 | 1,059 | B v2.5 (team notes: updated with Gemini 4 Argon). |
| 9 | 10-31 05:40 | `adae5b1` | Scored | 53.754 | 11.887014 | 0.651 s | 102 | 1,059 | No change description recorded. |
| 10 | 10-31 06:25 | `f473810` | Scored | 54.503 | 12.304135 | 0.711 s | 63 | 1,212 | Team v3 (team notes). |
| 11 | 10-31 06:55 | `eabc130` | Scored | 54.400 | 12.246072 | 0.708 s | 65 | 1,212 | No change description recorded. |
| 12 | 10-31 11:35 | `a3f64b1` | Scored | 55.967 | 13.162834 | 0.631 s | 57 | 1,294 | Submitted baseline (team notes). |
| 13 | 10-31 12:28 | `080c956` | Scored | 55.710 | 13.007479 | 0.684 s | 69 | 845 | Same commit as Track C's 67.853 run. Track B change not recorded. |
| 14 | 10-31 12:47 | `f9a6ddb` | Scored | 55.872 | 13.104908 | 0.679 s | 67 | 845 | No change description recorded. |
| 15 | 10-31 13:12 | `c6ef5cf` | Failed: build | — | — | — | — | — | Build failed (Track B and Track C; Track A scored 66.971 at this commit). Team account: a Rust build error from a push made before a local build was confirmed. |
| 16 | 10-31 13:19 | `4d1311a` | Scored | 56.840 | 13.702564 | 0.592 s | 66 | 1,014 | Positional index rebuild (team notes). |
| 17 | 10-31 14:49 | `1ce625d` | Scored | 58.232 | 14.609385 | 0.513 s | 68 | 1,014 | No change description recorded. |
| 18 | 10-31 15:02 | `be71488` | Scored | 57.137 | 13.891334 | 0.565 s | 66 | 1,077 | No change description recorded. |
| 19 | 10-31 15:36 | `a49777a` | Scored | 58.058 | 14.493192 | 0.518 s | 67 | 1,077 | No change description recorded. |
| 20 | 10-31 16:09 | `5792950` | Scored | 58.353 | 14.691306 | 0.505 s | 66 | 1,077 | Shard tuning and phrase planning (team notes). Best Track B result before the 31 Oct evening. |
| 21 | 10-31 18:23 | `8c36883` | Scored | 57.901 | 14.388376 | 0.520 s | 68 | 1,077 | No change description recorded. |
| 22 | 10-31 18:37 | `1039e85` | Scored | 58.142 | 14.549503 | 0.514 s | 67 | 1,077 | Same commit ID as Track C's prefetch and structure run (68.983). Track B change not recorded. |
| 23 | 10-31 19:23 | `3fe70c9` | Scored | 58.112 | 14.529197 | 0.515 s | 67 | 1,077 | Team candidate (team notes). |
| 24 | 10-31 20:00 | `a52ad27` | Scored | 58.558 | 14.830771 | 0.494 s | 67 | 1,077 | Team candidate (team notes). |
| 25 | 10-31 21:11 | `3165259` | Scored | 57.732 | 14.276928 | 0.534 s | 66 | 1,077 | Restored with the other tracks (team notes). |
| 26 | 10-31 21:31 | `7580e12` | Scored | 58.293 | 14.650588 | 0.506 s | 67 | 1,077 | Same B code as the 5792950 run, later board run (team notes). |
| 27 | 10-31 22:02 | `b22f4e2` | Scored | 60.015 | 15.859547 | 0.417 s | 67 | 1,208 | Tagged vocabulary and block tokenizer (ledger: source c65b4af). Best Track B result of 31 Oct. |
| 28 | 11-01 06:42 | `167b950` | Scored | 59.625 | 15.577819 | 0.435 s | 66 | 1,208 | No change description recorded. |
| 29 | 11-01 07:49 | `71743e4` | Scored | 60.276 | 16.051306 | 0.408 s | 66 | 1,208 | No change description recorded. |
| 30 | 11-01 08:48 | `faaeb9d` | Scored | 60.371 | 16.122280 | 0.404 s | 67 | 1,208 | The 0.404 s entry the team recorded as rank 53 on 2 Nov (UTC 1 Nov). The leaderboard shows 1 dependency for every entry; that is the standard library, which the formula counts as D = 1 (see section 6, item 2). |
| 31 | 11-01 09:39 | `c35a41d` | Scored | 59.852 | 15.741032 | 0.435 s | 67 | 1,112 | Board run of local change e7c9ea6 (ledger). Board was 18 ms slower than b22f4e2. |
| 32 | 11-01 09:49 | `8e07365` | Scored | 59.835 | 15.729136 | 0.436 s | 67 | 1,112 | No change description recorded. |
| 33 | 11-01 10:00 | `64925b1` | Scored | 59.916 | 15.787408 | 0.428 s | 68 | 1,112 | Same commit ID as the Track A best (76.902). |
| 34 | 11-01 10:32 | `e3c539c` | Scored | 59.911 | 15.784227 | 0.432 s | 67 | 1,112 | No change description recorded. |
| 35 | 11-01 10:49 | `ffb33f1` | Scored | 59.712 | 15.640466 | 0.440 s | 67 | 1,112 | No change description recorded. |
| 36 | 11-01 16:31 | `8a9667d` | Scored | 59.401 | 15.417909 | 0.449 s | 68 | 1,112 | No change description recorded. |
| 37 | 11-01 16:35 | `82dbc51` | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 74.573 at this commit. Team account: a Rust build error from a push made before a local build was confirmed. |
| 38 | 11-01 16:50 | `d715004` | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 72.752 at this commit. Team account: a Rust build error from a push made before a local build was confirmed. |
| 39 | 11-01 18:54 | `e091971` | Scored | 59.006 | 15.139892 | 0.328 s | 154 | 1,207 | No change description recorded. Memory rose to 154 MB. |
| 40 | 11-01 21:17 | `f29eac6` | Scored | 60.391 | 16.136537 | 0.304 s | 133 | 1,234 | First Claude-written source rewrite (team notes, which date it 2 Nov IST; the feed's UTC time is 1 Nov 21:17). |
| 41 | 11-02 03:11 | `bbbb3f1` | Scored | 60.189 | 15.987554 | 0.314 s | 129 | 1,235 | Second Claude-written source rewrite (team notes). |
| 42 | 11-02 03:28 | `2deeaab` | Scored | 60.371 | 16.121716 | 0.305 s | 133 | 1,237 | No change description recorded. |
| 43 | 11-02 03:35 | `21a5643` | Scored | 60.066 | 15.897124 | 0.316 s | 129 | 1,245 | No change description recorded. |
| 44 | 11-02 04:33 | `005c99a` | Scored (best) | 61.034 | 16.622063 | 0.297 s | 122 | 1,235 | Best Track B result to date (team status, 2 Nov). |
| 45 | 11-02 04:46 | `ce0d3c1` | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 74.658 at this commit. Team account: a Rust build error from a push made before a local build was confirmed. |
| 46 | 11-02 05:27 | `3640ce1` | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 75.192 at this commit. The local build passed; a host-side error is suspected (see section 5). |
| 47 | 11-02 06:04 | `6b67bfe` | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 75.302 at this commit. The local build passed; a host-side error is suspected (see section 5). |
| 48 | 11-02 06:45 | `248c1df` | Scored | 59.546 | 15.520774 | 0.341 s | 121 | 1,235 | Repair commit: restore verified sources (ledger: A 64925b1, B 005c99a). |
| 49 | 11-02 06:58 | `0c55114` | Scored | 60.697 | 16.365833 | 0.307 s | 122 | 1,224 | B: rebuild Vocab with kept keys; remap removed. Kept (ledger). |
| 50 | 11-02 07:17 | `25e6277` | Scored | 59.459 | 15.459171 | 0.338 s | 127 | 1,224 | MAX_DENSE 384 to 256 to reduce bitset memory. Reverted (ledger). |
| 51 | 11-02 07:47 | `fbe7674` | Scored | 58.712 | 14.936184 | 0.409 s | 94 | 1,225 | INDEX_THREADS = 6 to reduce peak memory. Reverted (ledger). |
| 52 | 11-02 08:22 | `9d15558` | Scored | 60.589 | 16.284347 | 0.309 s | 123 | 1,225 | No change description recorded. |
| 53 | 11-02 11:51 | `3b7b716` | Scored | 60.640 | 16.322698 | 0.308 s | 122 | 1,225 | Snapshot commit (archive hash of the uploaded repo). Latest Track B result. Not the best (005c99a is). |

Notes on this table:

- **Failures.** Six pushes failed with "The build failed." Track C was scored at five of those six commits, which shows that a Track B build failure did not stop the other tracks. The exception is `c6ef5cf`, where Track C's build failed too. The causes are in section 5. The cause of each build failure is not recorded in the team notes.
- **The early runs (rows 1 to 4)** took 12 to 13 seconds and 400 MB. They predate the first 1-second runs and are included because they are part of the record.
- **Corrections to the earlier version of this file.** The earlier log listed B v2 (commit `1a55090`) at 49.450 points. The board's K of 9.585006 gives 49.080, which is the board-reported value in the feed. The earlier log's a3f64b1 value of 55.9 (computed) is now 55.967, as reported by the board.
- **The two source rewrites.** The team notes describe two rewrites of Track B's `main.rs`, written by Claude in chat. In the feed they are `f29eac6` (1 Nov 21:17 UTC) and `bbbb3f1` (2 Nov 03:11 UTC). Team notes date them 2 Nov, which is IST for the first one.
- **Time versus memory.** Between `faaeb9d` (0.404 s, 67 MB) and `005c99a` (0.297 s, 122 MB), time fell by about 0.11 s and memory rose by about 55 MB. The points gain between those two rows is 0.66.
- **Commits shared with other tracks.** Several Track B rows share a commit ID with a Track A or Track C row. Where the record says what changed, the note says so. Where it does not, the row is recorded as shared and we have not assumed a change to Track B.

## 2. Local experiments

Local results were measured on the laptop, full seed 1, mostly with paired runs using `hyperfine -N` (both run orders when the result mattered). RSS and time are local figures and are not comparable with board M and T. A change was kept only if the full correctness gate passed and the paired gain was clearly above the noise.

| Date (UTC) | Experiment | Source | Local result | Decision |
|---|---|---|---|---|
| 1 Nov 07:48 | B2a-x: exact 64-bit tagged slots (full key check) | reverted | Gate and fuzz passed. Paired PSET runs 6 to 9% slower, RSS 310 MB | Reverted |
| 1 Nov 08:00 | B2b: pruned pass-2 vocabulary table | reverted | Correctness passed. PSET results noisy and order-dependent; RSS −0.35 MB; no repeatable 3% gain | Reverted |
| 1 Nov 08:10 | B2c pass-1 sink update, branchless | reverted | Gate and fuzz passed. Gains below 3%; RSS +0.2 MB | Reverted |
| 1 Nov 08:21 | B2c pass-2 sink entries | reverted | Gate passed. Order-dependent PSET with large outliers; all-CPU 4.6% slower; RSS +0.33 MB | Reverted |
| 1 Nov 08:21 | B-X1: event-buffer replacement for pass 2 | abandoned | Memory estimate: 28,716,697 hit events are about 230 MB at 8 bytes each before sorting, and 12 × 4 MiB buffers add 48 MiB. Projected peak above 108 MB | Abandoned before coding |
| 1 Nov 08:35 | B4: dense kept-term references | reverted | Gate passed (68 checks). RSS −1.06 MB, but PSET timing +1.1% and +3.8% slower; all-CPU +8.6% slower | Reverted |
| 1 Nov 08:45 | B5: remove BOOL emptiness tracking | `69e0ec3` | Gate and fuzz passed. Local scc Code 1,201 → 1,155. Repeated paired PSET aggregate −1.9%; RSS +0.5 MB | Kept |
| 1 Nov 08:52 | B4: reuse pass-1 worker state as pass-2 cursors | reverted | Gate and fuzz passed. RSS +0.05 MB; timing slightly slower | Reverted |
| 1 Nov 09:10 | B5: dead match-arm removal | reverted | Gates passed and scc Code −1. Corrected paired PSET noisy and order-sensitive; all-CPU +15.7%; RSS +0.03 MB | Reverted |
| 1 Nov 09:17 | B6: remove galloping-list intersection | `196ab00` | Gate passed. Local scc Code 1,155 → 1,129. Paired PSET aggregate +0.5% time; RSS −0.19 MB | Kept as source compaction |
| 1 Nov 09:39 | Acc10 set-bit loop (ledger name `push_word`) | `e7c9ea6` | Gate (68 checks) and tiny fuzz seeds 2 to 40 passed. Four 40-run PSET batches averaged 5.7% faster than B6 (1.109 s vs 1.176 s); several batches had standard deviation above 5%. Candidate RSS and all-CPU not measured | Selected for board run by team decision. Board gave 0.435 s (`c35a41d`) |

Other local figures recorded for this track:

- **Full seed-1 output hash of `e7c9ea6`:** `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`, which matches the organisers' published value.
- **Local line count of `e7c9ea6`:** `scc --by-file` reported 1,250 lines and 1,105 Code lines. The board reported S = 1,112 for the same source. `scripts/static_axes.py` could not run in WSL2 because Docker is not available, so the board's S is used for scoring.
- **B2a (first tagged-vocabulary attempt)** is the basis of `b22f4e2` (row 27 above). Its local result is not in the ledger and is not reproduced here.

## 3. Diagnostic profile (B0)

Three diagnostic runs on full seed 1 with a timing-instrumented build. These measure the diagnostic build, not the production binary. The commit for this profile is not recorded.

| Phase / counter | Local diagnostic result |
|---|---:|
| Query parse and vocabulary | 10.8 to 12.0 ms; V = 75,747; table slots = 262,144 |
| Line cuts | 7 to 11 ms |
| Pass 1 | 92.5 to 100.8 ms |
| Pass 2 | 243.8 to 284.2 ms |
| Decisions and phrase plans | about 2.5 ms; 19,000 phrases |
| Layout | 1.2 to 1.6 ms; 59,831 kept terms |
| Position event sort | 13 to 15 ms |
| Assembly | 2.4 to 3.0 ms |
| Query evaluation | 62 to 63 ms; 50,000 queries; 196 blocks |
| Output | 0.7 to 1.1 ms |
| Total diagnostic run | 428 to 467 ms |
| Tokens and vocabulary hits | 36,276,657 tokens; 28,716,697 vocabulary hits |
| Position events | 798,126 |
| Arena and bitsets | 27,134,486 bytes; 5,594,072 bytes |
| Peak RSS (VmHWM) | about 311 MB |

Passes 1 and 2 account for roughly 79% of the diagnostic run time. The scan and index build dominates, which is why the later work concentrated there.

## 4. Scoring context

```text
K = (56.35 / T)^0.5
  x min(2200 / M, 50)^0.2
  x min(91 / S, 1)^0.15
  x min(Dref / D, 1)^0.15
```

Track B has no third-party runtime dependency, so D = Dref = 1 in the intended build. The board displays the standard library as one dependency for every entry; the formula's D = 1 + direct dependencies gives D = 1 here. The memory factor reaches its cap of 50 at M = 44 MB. At 122 MB the memory factor is about 1.78, and moving from 122 MB to 44 MB would add only about a factor of 1.23 to K.

Leaderboard context on 2 Nov, as the team recorded it:

| Entry | T (s) | M (MB) | S | K | Points |
|---|---:|---:|---:|---:|---:|
| Rank 1 (another team) | 0.064 | 47 | 1,024 | 44.485581 | 82.411 |
| Rank 5 (another team) | 0.094 | — | — | 37.29 | — |
| Our entry `faaeb9d` (rank 53, 60.371 points) | 0.404 | 67 | 1,208 | 16.122280 | 60.371 |
| Our best `005c99a` | 0.297 | 122 | 1,235 | 16.622063 | 61.034 |

The gap to rank 1 is mostly in time (0.064 s against 0.297 s). Halving our time would multiply K by about 1.41. Memory alone cannot close the gap.

## 5. Failed runs and rejected work

**Build failures (6 pushes).** The team attributes four of the failures (`c6ef5cf`, `82dbc51`, `d715004`, `ce0d3c1`) mostly to Rust build errors in pushes made before a local build was confirmed. These were accidental or hasty pushes. The two latest Track B failures, `3640ce1` (2 Nov 05:27 UTC) and `6b67bfe` (2 Nov 06:04 UTC), built successfully locally. Other teams' Track B builds were also failing, so the team suspects a host-side error for these two rather than a source error. Track C was scored at five of the six commits; `c6ef5cf` was a failure for Track C too.

**Local experiments that did not pass.** The memory and table-layout experiments (B2a-x, B2b, B2c, B4) did not pass the 3% paired keep rule and were reverted. The event-buffer design (B-X1) was rejected on memory arithmetic before any code was written. The board run of `e7c9ea6` (`c35a41d`, 0.435 s) was slower than `b22f4e2` (0.417 s); the board decided.

**Board experiments that did not help.** `25e6277` (MAX_DENSE 384 → 256) and `fbe7674` (INDEX_THREADS = 6) were reverted. Compared with `0c55114`, `25e6277` raised both time (0.338 s against 0.307 s) and memory (127 MB against 122 MB). `fbe7674` cut memory to 94 MB but raised time to 0.409 s and lowered K.

**The rewrites on 1 and 2 Nov.** `f29eac6`, `bbbb3f1`, `2deeaab` and `21a5643` raised memory to 129–133 MB and did not move K much. `e091971` (1 Nov 18:54, 154 MB) is not a rewrite; the record does not say what changed in it. `005c99a` (`61.034`) is the best Track B result. The team notes say the rewrites "did not help". The board numbers show a net gain over `faaeb9d` of 0.66 points, so both statements are in the record.

## 6. Open items and discrepancies

The status of every item, including those resolved since this list was first written, is in `runs_ledger.md` section 3. Items marked resolved are settled.

These are problems we found while building this record. Each stays open until someone checks it against the source or the board.

1. **Vocabulary lookup is tag-based, not exact (checked).** The current source (`track-b/src/main.rs`, table `Vocab`) stores a 46-bit tag and a term id in each slot, and a lookup compares tags. It does not compare the full key on a hit. A false match is possible in principle; a comment in the source estimates about 1 in 10^7 per dataset, and the build keeps a seed only if every present term looks itself up. Earlier documentation, including an earlier version of this file, said that a tag match is checked against the full key. That was wrong for the current source. The fuzz and gate results are evidence, not proof, and the holdout corpus has not been seen. This is the main correctness risk on Track B.
2. **"1 dep" (resolved).** The board displays one dependency for every entry, including ours. That is the standard library, which the formula counts as D = 1. Our zero external dependencies therefore give D = 1, the value this file's formula uses, and no change to the scoring analysis is needed.
3. **Function name `push_word` (checked).** The ledger names the Acc10 change `push_word`, but the source has no function of that name. The source has `Acc10::push` and `Acc10::add`. This file uses the ledger name for the experiment and the source name for the code.
4. **Gemini model name.** The team records name both "Gemini 4 Argon" and "Gemini 3.8 Flash". See `track-b/README.md`.
5. **Controls (resolved).** The earlier root README said the gate includes three deliberately wrong controls (`NOT` read as `OR`, bytes at or above 0x80 treated as letters, and an inverted phrase result). We could not find those controls in the repository snapshot, so they are not claimed here.
6. **Local gate for `005c99a`.** Not recorded. It should be run and recorded here.
7. **Build failure causes (resolved).** The team's account is recorded in section 5. The local build passed for `3640ce1` and `6b67bfe`, and a host-side error is suspected. This is not confirmed; the team should confirm it, for example by re-running the failed commits on the scoring host.
8. **Change descriptions.** 18 of the 53 rows have no recorded description of what changed. The team should add them where it can, and must not add them from guesswork.
9. **Dates in team notes (resolved).** The feed is UTC and is used throughout. Team notes that use IST are marked where they differ.