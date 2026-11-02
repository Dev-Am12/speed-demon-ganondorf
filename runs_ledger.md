# Runs ledger

This ledger records every board run for all four tracks, taken from the board feed for this team, and the local experiments that came before the board runs. Board results and local measurements are kept in separate sections. Every row traces to the feed or to a named source.

## Conventions

- **Times are UTC**, as the board feed gives them. Team notes sometimes use IST (UTC+5:30). Where a team note and the feed disagree on a date, the feed is used and the difference is listed in section 4.
- **Commit IDs identify every run.** The feed did not give a run ID for every run, so the commit is the identifier. The Run ID column shows an ID only where the team recorded one; a dash means none is on record.
- **Points** are the board's speed points, `50 × log10 K`. Every scored row matches that formula to within rounding.
- **Best** marks the best scored run per track, as the feed labels it.
- **Notes** come from the per-track PERF files, and from the earlier root README where a PERF file does not say. "No change description recorded" means no record says what changed in that commit. Track A and D change descriptions are not in this ledger beyond the milestones in the earlier root README.
- Labels of the form `board-<commit>` in the earlier ledger are not run IDs and are dropped here (section 4).

## Summary

| Track | Board runs | Scored | Failed | Best scored result | Best commit |
|---|---:|---:|---:|---:|---|
| A | 56 | 53 | 3 | 76.902 | `64925b1` |
| B | 53 | 47 | 6 | 61.034 | `005c99a` |
| C | 43 | 39 | 4 | 75.426 | `3b7b716` |
| D | 50 | 50 | 0 | 48.692 | `5792950` |

Run IDs on record: seven. They are `run-8d8dca2d47f6` (A, `248c1df`), `run-c57c7b5fbb81` (B, `248c1df`), `run-71b1f569bb8e` (B, `0c55114`), `run-276183e7f8c9` (B, `25e6277`), `run-a83dcf9081da` (B, `fbe7674`), `run-6ec0c149d625` (B, `5792950`, from the earlier root README) and `run-ff966b474091` (C, `1039e85`, from the earlier root README).

## 1. Board runs

### Track A (Ingest)

| # | Finished (UTC) | Commit | Run ID | Result | Points | K | T | M (MB) | S (lines) | Notes |
|---:|---|---|---|---|---:|---:|---:|---:|---:|---|
| 1 | 10-30 18:43 | `c600c66` | — | Failed: output mismatch | — | — | — | — | — | The output did not match the expected answer. |
| 2 | 10-30 18:59 | `7bf5e26` | — | Failed: output mismatch | — | — | — | — | — | The output did not match the expected answer. |
| 3 | 10-30 19:27 | `40ad00c` | — | Failed: output mismatch | — | — | — | — | — | The output did not match the expected answer. |
| 4 | 10-30 19:42 | `c0b4eb6` | — | Scored | 44.815 | 7.875765 | 0.683 s | 47 | 183 | — |
| 5 | 10-30 19:58 | `ddcc075` | — | Scored | 44.739 | 7.848229 | 0.684 s | 47 | 183 | — |
| 6 | 10-30 20:03 | `db73a8d` | — | Scored | 44.883 | 7.900417 | 0.681 s | 46 | 183 | — |
| 7 | 10-30 20:30 | `85a7884` | — | Scored | 44.857 | 7.891145 | 0.683 s | 46 | 183 | — |
| 8 | 10-30 21:17 | `832e2c8` | — | Scored | 44.938 | 7.920669 | 0.680 s | 46 | 183 | Single-threaded Rust baseline (earlier root README). |
| 9 | 10-30 22:17 | `1a55090` | — | Scored | 44.818 | 7.877080 | 0.679 s | 47 | 183 | — |
| 10 | 10-30 22:43 | `1a65ccb` | — | Scored | 44.914 | 7.912025 | 0.678 s | 46 | 183 | — |
| 11 | 10-31 04:26 | `272dc82` | — | Scored | 44.752 | 7.852954 | 0.682 s | 48 | 183 | — |
| 12 | 10-31 05:40 | `adae5b1` | — | Scored | 59.863 | 15.749058 | 0.132 s | 64 | 287 | — |
| 13 | 10-31 06:25 | `f473810` | — | Scored | 59.903 | 15.778085 | 0.131 s | 63 | 287 | Parallel, packed records (earlier root README). |
| 14 | 10-31 06:55 | `eabc130` | — | Scored | 59.891 | 15.769320 | 0.131 s | 64 | 287 | — |
| 15 | 10-31 11:35 | `a3f64b1` | — | Scored | 56.365 | 13.405848 | 0.182 s | 64 | 287 | — |
| 16 | 10-31 12:28 | `080c956` | — | Scored | 59.733 | 15.654909 | 0.133 s | 64 | 287 | — |
| 17 | 10-31 12:47 | `f9a6ddb` | — | Scored | 66.968 | 21.844937 | 0.070 s | 57 | 312 | — |
| 18 | 10-31 13:12 | `c6ef5cf` | — | Scored | 66.971 | 21.848717 | 0.070 s | 57 | 312 | Mid-event plateau (earlier root README). Track B and C failed to build at this commit. |
| 19 | 10-31 13:19 | `4d1311a` | — | Scored | 66.969 | 21.846284 | 0.070 s | 57 | 312 | — |
| 20 | 10-31 14:49 | `1ce625d` | — | Scored | 67.018 | 21.895828 | 0.070 s | 57 | 312 | — |
| 21 | 10-31 15:02 | `be71488` | — | Scored | 66.913 | 21.790624 | 0.070 s | 57 | 312 | — |
| 22 | 10-31 15:36 | `a49777a` | — | Scored | 67.352 | 22.235357 | 0.068 s | 50 | 368 | — |
| 23 | 10-31 16:09 | `5792950` | — | Scored | 69.736 | 24.815505 | 0.053 s | 49 | 404 | — |
| 24 | 10-31 18:23 | `8c36883` | — | Scored | 70.488 | 25.689246 | 0.052 s | 45 | 404 | — |
| 25 | 10-31 18:37 | `1039e85` | — | Scored | 70.534 | 25.743648 | 0.051 s | 46 | 404 | — |
| 26 | 10-31 19:23 | `3fe70c9` | — | Scored | 48.859 | 9.488174 | 0.137 s | 529 | 440 | Memory spike to 529 MB at this commit; no change description recorded. |
| 27 | 10-31 20:00 | `a52ad27` | — | Scored | 71.287 | 26.653124 | 0.056 s | 28 | 440 | — |
| 28 | 10-31 21:11 | `3165259` | — | Scored | 72.832 | 28.618285 | 0.049 s | 28 | 440 | — |
| 29 | 10-31 21:31 | `7580e12` | — | Scored | 72.827 | 28.611343 | 0.048 s | 29 | 440 | — |
| 30 | 10-31 22:02 | `b22f4e2` | — | Scored | 72.959 | 28.786167 | 0.049 s | 27 | 440 | — |
| 31 | 11-01 06:41 | `167b950` | — | Scored | 73.027 | 28.876105 | 0.048 s | 28 | 440 | — |
| 32 | 11-01 07:49 | `71743e4` | — | Scored | 75.667 | 32.609734 | 0.037 s | 29 | 424 | — |
| 33 | 11-01 08:48 | `faaeb9d` | — | Scored | 76.498 | 33.881980 | 0.037 s | 25 | 423 | — |
| 34 | 11-01 09:39 | `c35a41d` | — | Scored | 73.053 | 28.910180 | 0.048 s | 28 | 440 | All-track run (earlier root README). |
| 35 | 11-01 09:49 | `8e07365` | — | Scored | 76.747 | 34.271944 | 0.037 s | 23 | 423 | — |
| 36 | 11-01 10:00 | `64925b1` | — | Scored (best) | 76.902 | 34.516892 | 0.037 s | 23 | 423 | Current best Track A result (earlier root README). |
| 37 | 11-01 10:32 | `e3c539c` | — | Scored | 76.752 | 34.279265 | 0.036 s | 24 | 423 | — |
| 38 | 11-01 10:49 | `ffb33f1` | — | Scored | 74.865 | 31.426095 | 0.043 s | 25 | 423 | — |
| 39 | 11-01 16:31 | `8a9667d` | — | Scored | 68.414 | 23.349716 | 0.102 s | 18 | 257 | — |
| 40 | 11-01 16:35 | `82dbc51` | — | Scored | 69.804 | 24.892883 | 0.090 s | 18 | 257 | — |
| 41 | 11-01 16:51 | `d715004` | — | Scored | 69.670 | 24.739804 | 0.091 s | 18 | 257 | — |
| 42 | 11-01 18:54 | `e091971` | — | Scored | 69.864 | 24.961690 | 0.089 s | 19 | 257 | — |
| 43 | 11-01 21:17 | `f29eac6` | — | Scored | 69.672 | 24.742831 | 0.090 s | 19 | 257 | — |
| 44 | 11-02 03:11 | `bbbb3f1` | — | Scored | 69.627 | 24.690870 | 0.092 s | 18 | 257 | — |
| 45 | 11-02 03:28 | `2deeaab` | — | Scored | 69.482 | 24.526892 | 0.091 s | 19 | 257 | — |
| 46 | 11-02 03:35 | `21a5643` | — | Scored | 69.807 | 24.896873 | 0.090 s | 18 | 257 | — |
| 47 | 11-02 04:33 | `005c99a` | — | Scored | 69.726 | 24.803993 | 0.091 s | 18 | 257 | — |
| 48 | 11-02 04:47 | `ce0d3c1` | — | Scored | 69.823 | 24.914606 | 0.090 s | 18 | 257 | — |
| 49 | 11-02 05:28 | `3640ce1` | — | Scored | 69.672 | 24.742039 | 0.090 s | 19 | 257 | Track B failed to build at this commit (local build passed; see Track B). |
| 50 | 11-02 06:04 | `6b67bfe` | — | Scored | 70.313 | 25.483769 | 0.080 s | 18 | 337 | Track B failed to build at this commit. |
| 51 | 11-02 06:45 | `248c1df` | run-8d8dca2d47f6 | Scored | 75.372 | 32.169039 | 0.042 s | 24 | 423 | — |
| 52 | 11-02 06:58 | `0c55114` | — | Scored | 75.916 | 32.985305 | 0.041 s | 22 | 423 | — |
| 53 | 11-02 07:17 | `25e6277` | — | Scored | 76.751 | 34.278317 | 0.038 s | 21 | 423 | — |
| 54 | 11-02 07:47 | `fbe7674` | — | Scored | 76.195 | 33.412464 | 0.039 s | 24 | 423 | — |
| 55 | 11-02 08:22 | `9d15558` | — | Scored | 76.591 | 34.026731 | 0.038 s | 23 | 423 | — |
| 56 | 11-02 11:51 | `3b7b716` | — | Scored | 75.921 | 32.992169 | 0.041 s | 22 | 423 | — |

### Track B (Index)

| # | Finished (UTC) | Commit | Run ID | Result | Points | K | T | M (MB) | S (lines) | Notes |
|---:|---|---|---|---|---:|---:|---:|---:|---:|---|
| 1 | 10-30 19:27 | `40ad00c` | — | Scored | 18.985 | 2.397181 | 13.5 s | 400 | 308 | Earliest recorded run (about 13 s). Root README calls the 18.985-point figure the single-threaded baseline. No change description recorded. |
| 2 | 10-30 19:59 | `ddcc075` | — | Scored | 19.402 | 2.443630 | 12.9 s | 400 | 308 | Early version. No change description recorded. |
| 3 | 10-30 20:04 | `db73a8d` | — | Scored | 19.096 | 2.409497 | 12.3 s | 400 | 406 | Early version. No change description recorded. |
| 4 | 10-30 20:30 | `85a7884` | — | Scored | 18.903 | 2.388119 | 12.5 s | 400 | 406 | Early version. No change description recorded. |
| 5 | 10-30 21:17 | `832e2c8` | — | Scored | 41.121 | 6.643748 | 1.408 s | 278 | 1,033 | B v1 (team notes). |
| 6 | 10-30 22:17 | `1a55090` | — | Scored | 49.080 | 9.585006 | 0.970 s | 103 | 1,172 | B v2 (team notes). Earlier docs listed 49.450; the board's K gives 49.080. |
| 7 | 10-30 22:43 | `1a65ccb` | — | Scored | 53.595 | 11.800617 | 0.646 s | 108 | 1,059 | No change description recorded. |
| 8 | 10-31 04:26 | `272dc82` | — | Scored | 53.728 | 11.872829 | 0.652 s | 103 | 1,059 | B v2.5 (team notes: updated with Gemini 4 Argon). |
| 9 | 10-31 05:40 | `adae5b1` | — | Scored | 53.754 | 11.887014 | 0.651 s | 102 | 1,059 | No change description recorded. |
| 10 | 10-31 06:25 | `f473810` | — | Scored | 54.503 | 12.304135 | 0.711 s | 63 | 1,212 | Team v3 (team notes). |
| 11 | 10-31 06:55 | `eabc130` | — | Scored | 54.400 | 12.246072 | 0.708 s | 65 | 1,212 | No change description recorded. |
| 12 | 10-31 11:35 | `a3f64b1` | — | Scored | 55.967 | 13.162834 | 0.631 s | 57 | 1,294 | Submitted baseline (team notes). |
| 13 | 10-31 12:28 | `080c956` | — | Scored | 55.710 | 13.007479 | 0.684 s | 69 | 845 | Same commit as Track C's 67.853 run. Track B change not recorded. |
| 14 | 10-31 12:47 | `f9a6ddb` | — | Scored | 55.872 | 13.104908 | 0.679 s | 67 | 845 | No change description recorded. |
| 15 | 10-31 13:12 | `c6ef5cf` | — | Failed: build | — | — | — | — | — | Build failed (Track B and Track C). Team account: a Rust build error from a push made before a local build was confirmed. |
| 16 | 10-31 13:19 | `4d1311a` | — | Scored | 56.840 | 13.702564 | 0.592 s | 66 | 1,014 | Positional index rebuild (team notes). |
| 17 | 10-31 14:49 | `1ce625d` | — | Scored | 58.232 | 14.609385 | 0.513 s | 68 | 1,014 | No change description recorded. |
| 18 | 10-31 15:02 | `be71488` | — | Scored | 57.137 | 13.891334 | 0.565 s | 66 | 1,077 | No change description recorded. |
| 19 | 10-31 15:36 | `a49777a` | — | Scored | 58.058 | 14.493192 | 0.518 s | 67 | 1,077 | No change description recorded. |
| 20 | 10-31 16:09 | `5792950` | run-6ec0c149d625 | Scored | 58.353 | 14.691306 | 0.505 s | 66 | 1,077 | Shard tuning and phrase planning (team notes). Best Track B result before the 31 Oct evening. |
| 21 | 10-31 18:23 | `8c36883` | — | Scored | 57.901 | 14.388376 | 0.520 s | 68 | 1,077 | No change description recorded. |
| 22 | 10-31 18:37 | `1039e85` | — | Scored | 58.142 | 14.549503 | 0.514 s | 67 | 1,077 | Same commit ID as Track C's prefetch and structure run (68.983). Track B change not recorded. |
| 23 | 10-31 19:23 | `3fe70c9` | — | Scored | 58.112 | 14.529197 | 0.515 s | 67 | 1,077 | Team candidate (team notes). |
| 24 | 10-31 20:00 | `a52ad27` | — | Scored | 58.558 | 14.830771 | 0.494 s | 67 | 1,077 | Team candidate (team notes). |
| 25 | 10-31 21:11 | `3165259` | — | Scored | 57.732 | 14.276928 | 0.534 s | 66 | 1,077 | Restored with the other tracks (team notes). |
| 26 | 10-31 21:31 | `7580e12` | — | Scored | 58.293 | 14.650588 | 0.506 s | 67 | 1,077 | Same B code as the 5792950 run, later board run (team notes). |
| 27 | 10-31 22:02 | `b22f4e2` | — | Scored | 60.015 | 15.859547 | 0.417 s | 67 | 1,208 | Tagged vocabulary and block tokenizer (ledger: source c65b4af). Best Track B result of 31 Oct. |
| 28 | 11-01 06:42 | `167b950` | — | Scored | 59.625 | 15.577819 | 0.435 s | 66 | 1,208 | No change description recorded. |
| 29 | 11-01 07:49 | `71743e4` | — | Scored | 60.276 | 16.051306 | 0.408 s | 66 | 1,208 | No change description recorded. |
| 30 | 11-01 08:48 | `faaeb9d` | — | Scored | 60.371 | 16.122280 | 0.404 s | 67 | 1,208 | The 0.404 s entry the team recorded as rank 53 on 2 Nov (UTC 1 Nov). The leaderboard shows 1 dependency for every entry: the standard library, which the formula counts as D = 1. |
| 31 | 11-01 09:39 | `c35a41d` | — | Scored | 59.852 | 15.741032 | 0.435 s | 67 | 1,112 | Board run of local change e7c9ea6 (ledger). Board was 18 ms slower than b22f4e2. |
| 32 | 11-01 09:49 | `8e07365` | — | Scored | 59.835 | 15.729136 | 0.436 s | 67 | 1,112 | No change description recorded. |
| 33 | 11-01 10:00 | `64925b1` | — | Scored | 59.916 | 15.787408 | 0.428 s | 68 | 1,112 | Same commit ID as the Track A best (76.902). |
| 34 | 11-01 10:32 | `e3c539c` | — | Scored | 59.911 | 15.784227 | 0.432 s | 67 | 1,112 | No change description recorded. |
| 35 | 11-01 10:49 | `ffb33f1` | — | Scored | 59.712 | 15.640466 | 0.440 s | 67 | 1,112 | No change description recorded. |
| 36 | 11-01 16:31 | `8a9667d` | — | Scored | 59.401 | 15.417909 | 0.449 s | 68 | 1,112 | No change description recorded. |
| 37 | 11-01 16:35 | `82dbc51` | — | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 74.573 at this commit. Team account: a Rust build error from a push made before a local build was confirmed. |
| 38 | 11-01 16:50 | `d715004` | — | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 72.752 at this commit. Team account: a Rust build error from a push made before a local build was confirmed. |
| 39 | 11-01 18:54 | `e091971` | — | Scored | 59.006 | 15.139892 | 0.328 s | 154 | 1,207 | No change description recorded. Memory rose to 154 MB. |
| 40 | 11-01 21:17 | `f29eac6` | — | Scored | 60.391 | 16.136537 | 0.304 s | 133 | 1,234 | First Claude-written source rewrite (team notes, which date it 2 Nov IST; the feed's UTC time is 1 Nov 21:17). |
| 41 | 11-02 03:11 | `bbbb3f1` | — | Scored | 60.189 | 15.987554 | 0.314 s | 129 | 1,235 | Second Claude-written source rewrite (team notes). |
| 42 | 11-02 03:28 | `2deeaab` | — | Scored | 60.371 | 16.121716 | 0.305 s | 133 | 1,237 | No change description recorded. |
| 43 | 11-02 03:35 | `21a5643` | — | Scored | 60.066 | 15.897124 | 0.316 s | 129 | 1,245 | No change description recorded. |
| 44 | 11-02 04:33 | `005c99a` | — | Scored (best) | 61.034 | 16.622063 | 0.297 s | 122 | 1,235 | Best Track B result to date (team status, 2 Nov). |
| 45 | 11-02 04:46 | `ce0d3c1` | — | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 74.658 at this commit. Team account: a Rust build error from a push made before a local build was confirmed. |
| 46 | 11-02 05:27 | `3640ce1` | — | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 75.192 at this commit. Local build passed; host-side error suspected, not confirmed. |
| 47 | 11-02 06:04 | `6b67bfe` | — | Failed: build | — | — | — | — | — | Build failed (Track B). Track C scored 75.302 at this commit. Local build passed; host-side error suspected, not confirmed. |
| 48 | 11-02 06:45 | `248c1df` | run-c57c7b5fbb81 | Scored | 59.546 | 15.520774 | 0.341 s | 121 | 1,235 | Repair commit: restore verified sources (ledger: A 64925b1, B 005c99a). |
| 49 | 11-02 06:58 | `0c55114` | run-71b1f569bb8e | Scored | 60.697 | 16.365833 | 0.307 s | 122 | 1,224 | B: rebuild Vocab with kept keys; remap removed. Kept (ledger). |
| 50 | 11-02 07:17 | `25e6277` | run-276183e7f8c9 | Scored | 59.459 | 15.459171 | 0.338 s | 127 | 1,224 | MAX_DENSE 384 to 256 to reduce bitset memory. Reverted (ledger). |
| 51 | 11-02 07:47 | `fbe7674` | run-a83dcf9081da | Scored | 58.712 | 14.936184 | 0.409 s | 94 | 1,225 | INDEX_THREADS = 6 to reduce peak memory. Reverted (ledger). |
| 52 | 11-02 08:22 | `9d15558` | — | Scored | 60.589 | 16.284347 | 0.309 s | 123 | 1,225 | No change description recorded. |
| 53 | 11-02 11:51 | `3b7b716` | — | Scored | 60.640 | 16.322698 | 0.308 s | 122 | 1,225 | Snapshot commit (archive hash of the uploaded repo). Latest Track B result. Not the best (005c99a is). |

### Track C (Join)

| # | Finished (UTC) | Commit | Run ID | Result | Points | K | T | M (MB) | S (lines) | Notes |
|---:|---|---|---|---|---:|---:|---:|---:|---:|---|
| 1 | 10-31 11:35 | `a3f64b1` | — | Scored | 53.646 | 11.828112 | 0.168 s | 29 | 622 | Step 1: two-pass SIMD quote-parity scanner, region lookup table. First recorded Track C board run. |
| 2 | 10-31 12:28 | `080c956` | — | Scored | 67.853 | 22.754156 | 0.074 s | 10 | 509 | Step 2: single-pass record-boundary snapping, mmap region loader, 64-bit name table. |
| 3 | 10-31 12:47 | `f9a6ddb` | — | Scored | 67.654 | 22.546806 | 0.075 s | 10 | 509 | No change description recorded. |
| 4 | 10-31 13:12 | `c6ef5cf` | — | Failed: build | — | — | — | — | — | Build failed (Track C and Track B; Track A scored 66.971 at this commit). Team account: a Rust build error from a push made before a local build was confirmed. |
| 5 | 10-31 13:20 | `4d1311a` | — | Scored | 67.477 | 22.363358 | 0.065 s | 18 | 400 | No change description recorded. |
| 6 | 10-31 14:49 | `1ce625d` | — | Scored | 66.806 | 21.683157 | 0.064 s | 22 | 400 | No change description recorded. |
| 7 | 10-31 15:03 | `be71488` | — | Failed: too slow | — | — | — | — | — | Failed: too slow, hit the time limit. |
| 8 | 10-31 15:37 | `a49777a` | — | Failed: too slow | — | — | — | — | — | Failed: too slow, hit the time limit. |
| 9 | 10-31 16:10 | `5792950` | — | Failed: too slow | — | — | — | — | — | Failed: too slow, hit the time limit. Track D scored its best (48.692) at this commit. |
| 10 | 10-31 18:23 | `8c36883` | — | Scored | 66.746 | 21.623679 | 0.067 s | 20 | 395 | No change description recorded. |
| 11 | 10-31 18:37 | `1039e85` | run-ff966b474091 | Scored | 68.983 | 23.969214 | 0.068 s | 12 | 389 | Prefetch and structure work (root README). Metrics here are from the board feed; this run was not in the earlier log. |
| 12 | 10-31 19:23 | `3fe70c9` | — | Scored | 70.109 | 25.245704 | 0.067 s | 10 | 389 | Step 3: team C version on the speed-demon branch. |
| 13 | 10-31 20:00 | `a52ad27` | — | Scored | 68.932 | 23.913717 | 0.073 s | 10 | 389 | Step 4: same code as step 3, scored about 40 minutes later (run-to-run noise of about 1.2 points). |
| 14 | 10-31 21:03 | `15d48f6` | — | Scored | 67.243 | 22.124088 | 0.085 s | 10 | 399 | Step 5: C0 full-coverage long-name hash, plus C2a distinct count from the fetch_or result. Other tracks were reset in this run. |
| 15 | 10-31 21:11 | `3165259` | — | Scored | 70.237 | 25.394298 | 0.063 s | 10 | 399 | Step 6: same code as step 5, restored with the other tracks. |
| 16 | 10-31 21:31 | `7580e12` | — | Scored | 70.508 | 25.713786 | 0.063 s | 10 | 393 | Step 7: E1 duplicate (name, country) cache removed. Best measured result at the time. |
| 17 | 10-31 22:02 | `b22f4e2` | — | Scored | 70.118 | 25.255831 | 0.065 s | 10 | 393 | Step 8: same E1 code, scored in the run with the Track B change. |
| 18 | 11-01 06:42 | `167b950` | — | Scored | 67.933 | 22.838238 | 0.078 s | 11 | 393 | No change description recorded. |
| 19 | 11-01 07:49 | `71743e4` | — | Scored | 70.406 | 25.592651 | 0.061 s | 11 | 393 | No change description recorded. |
| 20 | 11-01 08:48 | `faaeb9d` | — | Scored | 70.513 | 25.719560 | 0.061 s | 11 | 393 | No change description recorded. |
| 21 | 11-01 09:39 | `c35a41d` | — | Scored | 70.837 | 26.106129 | 0.060 s | 10 | 393 | Step 9: all-track run. Last result before 2 Nov. |
| 22 | 11-01 09:49 | `8e07365` | — | Scored | 70.670 | 25.905722 | 0.061 s | 10 | 393 | No change description recorded. |
| 23 | 11-01 10:00 | `64925b1` | — | Scored | 70.868 | 26.143867 | 0.060 s | 10 | 393 | Same commit ID as the Track A best (76.902). |
| 24 | 11-01 10:32 | `e3c539c` | — | Scored | 74.357 | 30.700043 | 0.043 s | 9 | 450 | No change description recorded. Source 450 lines. |
| 25 | 11-01 10:49 | `ffb33f1` | — | Scored | 74.721 | 31.218452 | 0.042 s | 10 | 411 | No change description recorded. |
| 26 | 11-01 16:31 | `8a9667d` | — | Scored | 74.599 | 31.044012 | 0.042 s | 10 | 411 | No change description recorded. |
| 27 | 11-01 16:35 | `82dbc51` | — | Scored | 74.573 | 31.007227 | 0.042 s | 10 | 411 | No change description recorded. Track B failed at this commit. |
| 28 | 11-01 16:51 | `d715004` | — | Scored | 72.752 | 28.513011 | 0.049 s | 10 | 411 | No change description recorded. Track B failed at this commit. |
| 29 | 11-01 18:54 | `e091971` | — | Scored | 74.274 | 30.582671 | 0.043 s | 11 | 411 | No change description recorded. |
| 30 | 11-01 21:17 | `f29eac6` | — | Scored | 74.690 | 31.174792 | 0.042 s | 10 | 411 | No change description recorded. Track B's first Claude-written rewrite was scored at this commit. |
| 31 | 11-02 03:11 | `bbbb3f1` | — | Scored | 74.494 | 30.894735 | 0.043 s | 10 | 411 | No change description recorded. |
| 32 | 11-02 03:28 | `2deeaab` | — | Scored | 74.498 | 30.899927 | 0.042 s | 10 | 411 | No change description recorded. |
| 33 | 11-02 03:35 | `21a5643` | — | Scored | 74.588 | 31.028434 | 0.043 s | 10 | 411 | No change description recorded. |
| 34 | 11-02 04:34 | `005c99a` | — | Scored | 74.491 | 30.890247 | 0.042 s | 10 | 411 | Same commit ID as the Track B best (61.034). |
| 35 | 11-02 04:47 | `ce0d3c1` | — | Scored | 74.658 | 31.128925 | 0.043 s | 10 | 411 | No change description recorded. Track B failed at this commit. |
| 36 | 11-02 05:28 | `3640ce1` | — | Scored | 75.192 | 31.904207 | 0.042 s | 10 | 371 | No change description recorded. Source down to 371 lines. Track B failed at this commit. |
| 37 | 11-02 06:04 | `6b67bfe` | — | Scored | 75.302 | 32.066009 | 0.041 s | 10 | 371 | No change description recorded. Track B failed at this commit. |
| 38 | 11-02 06:45 | `248c1df` | — | Scored | 75.214 | 31.935814 | 0.041 s | 10 | 371 | Repair commit: restore verified sources (ledger: A 64925b1, B 005c99a). |
| 39 | 11-02 06:58 | `0c55114` | — | Scored | 75.376 | 32.175409 | 0.041 s | 10 | 371 | Same commit ID as Track B's kept change (rebuild Vocab, remove remap). The root README previously listed this as the Track C current result; it is a Track B commit. |
| 40 | 11-02 07:17 | `25e6277` | — | Scored | 75.257 | 31.999246 | 0.041 s | 10 | 371 | No change description recorded. Track B reverted MAX_DENSE change at this commit. |
| 41 | 11-02 07:47 | `fbe7674` | — | Scored | 75.240 | 31.973869 | 0.042 s | 10 | 371 | No change description recorded. Track B reverted INDEX_THREADS change at this commit. |
| 42 | 11-02 08:22 | `9d15558` | — | Scored | 75.017 | 31.647174 | 0.042 s | 10 | 371 | No change description recorded. |
| 43 | 11-02 11:51 | `3b7b716` | — | Scored (best) | 75.426 | 32.249688 | 0.042 s | 9 | 371 | Snapshot commit (archive hash). Best Track C result to date; current submission. |

### Track D (Traverse, dropped from the overall)

| # | Finished (UTC) | Commit | Run ID | Result | Points | K | T | M (MB) | S (lines) | Notes |
|---:|---|---|---|---|---:|---:|---:|---:|---:|---|
| 1 | 10-30 20:30 | `85a7884` | — | Scored | 28.681 | 3.746410 | 8.796 s | 79 | 188 | — |
| 2 | 10-30 21:17 | `832e2c8` | — | Scored | 29.459 | 3.883138 | 8.177 s | 79 | 188 | — |
| 3 | 10-30 22:17 | `1a55090` | — | Scored | 29.633 | 3.914415 | 8.049 s | 79 | 188 | — |
| 4 | 10-30 22:43 | `1a65ccb` | — | Scored | 29.442 | 3.879999 | 8.135 s | 80 | 188 | — |
| 5 | 10-31 04:26 | `272dc82` | — | Scored | 38.365 | 5.851829 | 2.048 s | 236 | 285 | First parallel version (earlier root README). |
| 6 | 10-31 05:40 | `adae5b1` | — | Scored | 38.862 | 5.987482 | 1.957 s | 236 | 285 | — |
| 7 | 10-31 06:25 | `f473810` | — | Scored | 39.078 | 6.047191 | 1.918 s | 236 | 285 | — |
| 8 | 10-31 06:55 | `eabc130` | — | Scored | 48.039 | 9.136653 | 0.908 s | 134 | 469 | Step change: memory and parse work (earlier root README). |
| 9 | 10-31 11:35 | `a3f64b1` | — | Scored | 47.707 | 8.997789 | 0.936 s | 134 | 469 | — |
| 10 | 10-31 12:28 | `080c956` | — | Scored | 47.445 | 8.890053 | 0.959 s | 134 | 469 | — |
| 11 | 10-31 12:47 | `f9a6ddb` | — | Scored | 47.699 | 8.994447 | 0.937 s | 134 | 469 | — |
| 12 | 10-31 13:13 | `c6ef5cf` | — | Scored | 46.657 | 8.573199 | 1.032 s | 134 | 469 | — |
| 13 | 10-31 13:20 | `4d1311a` | — | Scored | 47.327 | 8.841630 | 0.969 s | 134 | 469 | — |
| 14 | 10-31 14:49 | `1ce625d` | — | Scored | 47.929 | 9.090194 | 0.983 s | 135 | 369 | — |
| 15 | 10-31 15:03 | `be71488` | — | Scored | 48.018 | 9.127760 | 0.974 s | 135 | 369 | — |
| 16 | 10-31 15:37 | `a49777a` | — | Scored | 48.545 | 9.351802 | 0.929 s | 135 | 369 | — |
| 17 | 10-31 16:10 | `5792950` | — | Scored (best) | 48.692 | 9.415483 | 0.917 s | 135 | 369 | Current best Track D result (earlier root README). Dropped from the overall as the weakest track. |
| 18 | 10-31 18:23 | `8c36883` | — | Scored | 46.781 | 8.622380 | 1.093 s | 135 | 369 | — |
| 19 | 10-31 18:37 | `1039e85` | — | Scored | 47.859 | 9.061022 | 0.987 s | 136 | 369 | — |
| 20 | 10-31 19:24 | `3fe70c9` | — | Scored | 47.557 | 8.936081 | 1.018 s | 135 | 369 | — |
| 21 | 10-31 20:00 | `a52ad27` | — | Scored | 48.022 | 9.129548 | 0.974 s | 135 | 369 | — |
| 22 | 10-31 21:11 | `3165259` | — | Scored | 48.584 | 9.368846 | 0.926 s | 135 | 369 | — |
| 23 | 10-31 21:31 | `7580e12` | — | Scored | 47.059 | 8.733147 | 1.066 s | 135 | 369 | — |
| 24 | 10-31 22:02 | `b22f4e2` | — | Scored | 46.683 | 8.583421 | 1.103 s | 135 | 369 | — |
| 25 | 11-01 06:42 | `167b950` | — | Scored | 45.555 | 8.149017 | 1.225 s | 135 | 369 | — |
| 26 | 11-01 07:49 | `71743e4` | — | Scored | 48.588 | 9.370510 | 0.926 s | 134 | 369 | — |
| 27 | 11-01 08:48 | `faaeb9d` | — | Scored | 48.353 | 9.269591 | 0.947 s | 134 | 369 | — |
| 28 | 11-01 09:39 | `c35a41d` | — | Scored | 48.206 | 9.207174 | 0.957 s | 135 | 369 | — |
| 29 | 11-01 09:49 | `8e07365` | — | Scored | 47.218 | 8.797338 | 1.049 s | 135 | 369 | — |
| 30 | 11-01 10:01 | `64925b1` | — | Scored | 48.134 | 9.176433 | 0.964 s | 135 | 369 | — |
| 31 | 11-01 10:32 | `e3c539c` | — | Scored | 46.812 | 8.634572 | 1.091 s | 135 | 369 | — |
| 32 | 11-01 10:49 | `ffb33f1` | — | Scored | 47.489 | 8.908161 | 1.026 s | 134 | 369 | — |
| 33 | 11-01 16:31 | `8a9667d` | — | Scored | 46.919 | 8.677324 | 1.078 s | 135 | 369 | — |
| 34 | 11-01 16:35 | `82dbc51` | — | Scored | 48.398 | 9.288771 | 0.942 s | 135 | 369 | — |
| 35 | 11-01 16:51 | `d715004` | — | Scored | 45.910 | 8.283176 | 1.185 s | 134 | 369 | — |
| 36 | 11-01 18:54 | `e091971` | — | Scored | 47.576 | 8.943712 | 1.015 s | 135 | 369 | — |
| 37 | 11-01 21:17 | `f29eac6` | — | Scored | 48.346 | 9.266615 | 0.945 s | 135 | 369 | — |
| 38 | 11-02 03:11 | `bbbb3f1` | — | Scored | 48.149 | 9.182889 | 0.963 s | 135 | 369 | — |
| 39 | 11-02 03:28 | `2deeaab` | — | Scored | 46.722 | 8.598866 | 1.101 s | 134 | 369 | — |
| 40 | 11-02 03:35 | `21a5643` | — | Scored | 47.955 | 9.101085 | 0.982 s | 135 | 369 | — |
| 41 | 11-02 04:34 | `005c99a` | — | Scored | 47.320 | 8.838920 | 1.041 s | 135 | 369 | — |
| 42 | 11-02 04:47 | `ce0d3c1` | — | Scored | 48.085 | 9.156073 | 0.968 s | 135 | 369 | — |
| 43 | 11-02 05:28 | `3640ce1` | — | Scored | 47.666 | 8.981058 | 1.007 s | 135 | 369 | — |
| 44 | 11-02 06:04 | `6b67bfe` | — | Scored | 46.751 | 8.610268 | 1.098 s | 134 | 369 | — |
| 45 | 11-02 06:45 | `248c1df` | — | Scored | 45.912 | 8.283845 | 1.184 s | 135 | 369 | — |
| 46 | 11-02 06:58 | `0c55114` | — | Scored | 45.621 | 8.173613 | 1.215 s | 135 | 369 | — |
| 47 | 11-02 07:17 | `25e6277` | — | Scored | 45.925 | 8.288914 | 1.183 s | 135 | 369 | — |
| 48 | 11-02 07:47 | `fbe7674` | — | Scored | 46.686 | 8.584566 | 1.103 s | 135 | 369 | — |
| 49 | 11-02 08:22 | `9d15558` | — | Scored | 44.930 | 7.917555 | 1.297 s | 135 | 369 | — |
| 50 | 11-02 11:51 | `3b7b716` | — | Scored | 47.553 | 8.934363 | 1.018 s | 135 | 369 | — |

## 2. Local experiments (not board results)

Local results were measured on the laptop (Intel Core i5-1235U, WSL2, 12 logical CPUs). They do not predict board times or memory. A change was kept only when the full correctness gate passed and the paired gain was clearly above noise.

| Logged (UTC) | Track | Commit | Change | Local result | Decision | Source |
|---|---|---|---|---|---|---|
| 1 Nov 07:10 | C | `1e11ee6` | Correctness: final record at EOF | Matches reference on 300 fuzz seeds; 0.070 s, 202 MB (local) | Kept | Ledger |
| 1 Nov 07:11 | C | `09d07cc` | Correctness: sparse lookup for region keys with more than 7 digits | Matches reference on 300 fuzz seeds; 0.070 s, 202 MB (local). The sparse path is not visible in the `3b7b716` source (section 3) | Kept | Ledger |
| 1 Nov 07:12 | C | reverted | C-S1 prefetch ring, PF = 16 | 137.6 ms; order-dependent noise | Reverted | Ledger |
| 1 Nov 07:19 | C | reverted | C-S1 prefetch ring, PF = 8 | 110.0 ms; inconsistent; one order 2.7% faster | Reverted | Ledger |
| 1 Nov 07:21 | C | reverted | C-S1 prefetch ring, PF = 32 | 124.9 ms; one order 6.8% slower | Reverted | Ledger |
| 1 Nov 07:48 | B | reverted | B2a-x: exact 64-bit tagged slots (full key check) | Gate and fuzz passed; paired PSET 6–9% slower; 310 MB | Reverted | Ledger |
| 1 Nov 08:00 | B | reverted | B2b: pruned pass-2 vocabulary table | Correctness passed; noisy and order-dependent; RSS −0.35 MB | Reverted | Ledger |
| 1 Nov 08:10 | B | reverted | B2c: branchless pass-1 sink update | Gate and fuzz passed; gains under 3%; RSS +0.2 MB | Reverted | Ledger |
| 1 Nov 08:21 | B | reverted | B2c: pass-2 sink entries | Gate passed; order-dependent; all-CPU 4.6% slower | Reverted | Ledger |
| 1 Nov 08:21 | B | abandoned | B-X1: event-buffer replacement (analysis only) | Projected peak above 108 MB | Abandoned | Ledger |
| 1 Nov 08:35 | B | reverted | B4: dense kept-term references | Gate passed; RSS −1.06 MB; PSET +1.1% and +3.8%; all-CPU +8.6% | Reverted | Ledger |
| 1 Nov 08:45 | B | `69e0ec3` | B5: BOOL emptiness tracking removed | Gate and fuzz passed; scc Code 1,201 → 1,155; paired PSET −1.9%; RSS +0.5 MB | Kept | Ledger |
| 1 Nov 08:52 | B | reverted | B4: reuse pass-1 worker state as pass-2 cursors | Gate and fuzz passed; RSS +0.05 MB; slightly slower | Reverted | Ledger |
| 1 Nov 09:10 | B | reverted | B5: dead match-arm removal | Gates passed; scc Code −1; all-CPU +15.7%; RSS +0.03 MB | Reverted | Ledger |
| 1 Nov 09:17 | B | `196ab00` | B6: galloping-list intersection removed | Gate passed; scc Code 1,155 → 1,129; PSET +0.5%; RSS −0.19 MB | Kept (source compaction) | Ledger |
| 1 Nov 09:39 | B | `e7c9ea6` | Acc10 set-bit loop (ledger name `push_word`) | Gate (68 checks) and tiny fuzz 2–40 passed; four 40-run batches averaged 5.7% faster than B6 (several noisy); board gave 0.435 s (`c35a41d`) | Kept; team selected it for the board | Ledger |
| before ledger | C | — | C0: full-coverage hash for names over 32 bytes | Correctness fix; hostile fixture now matches | Kept | Earlier C PERF |
| before ledger | C | — | C2a: distinct count from the `fetch_or` result | 1.07× | Kept | Earlier C PERF |
| before ledger | C | — | C2b: skip unused quote counts | No gain | Rejected | Earlier C PERF |
| before ledger | C | — | E1: remove duplicate `(name, country)` cache | 1.09× | Kept | Earlier C PERF |
| before ledger | C | — | E2: dynamic scheduling, four chunks per thread | About 1% | Rejected | Earlier C PERF |
| before ledger | C | — | E3: key and mask packed into one 40-byte slot | Slower | Rejected | Earlier C PERF |
| before ledger | C | — | E4: prefetch, delayed inserts by four records | Result reversed with run order | Rejected | Earlier C PERF |

## 3. Open items and how they stand

"Resolved" means the record now answers the question. "Partly resolved" means the record has a team account but no confirmation. "Open" means the answer needs a check, a run, or a decision from the team.

| Item | Where in PERF | Status | Detail |
|---|---|---|---|
| Track B vocabulary lookup is tag-based, not exact | B §6.1 | Open | A code question. The ledger cannot settle it. Needs a full-key check, or evidence that the 46-bit tag holds on the holdout corpus. |
| "1 dep" on the leaderboard | B §6.2 | Resolved | The board shows the standard library as one dependency on every entry. Zero external dependencies, so D = 1 on every run. |
| `push_word` naming | B §6.3 | Open | The ledger's name is not in the source. The source has `Acc10::push` and `Acc10::add`. The team needs to say which function the ledger means. |
| Gemini model name | B §6.4 | Open | Team records name both "Gemini 4 Argon" and "Gemini 3.8 Flash". The team needs to confirm which was used. |
| Controls not found in the repository | B §6.5 | Open | The earlier root README described three controls that are not in the snapshot. The team needs to point to them, or drop the claim. |
| Local gate for `005c99a` | B §6.6 | Open | Not run. Needs running and recording. |
| Build failure causes | B §6.7, C §5 | Partly resolved | Four Track B failures (`c6ef5cf`, `82dbc51`, `d715004`, `ce0d3c1`) and the Track C failure at `c6ef5cf`: team account of Rust build errors in pushes made before a local build was confirmed. The latest two Track B failures, `3640ce1` and `6b67bfe`: local build passed, host-side error suspected, not confirmed. |
| Change descriptions missing | B §6.8, C §7.8 | Open | 18 Track B rows and 24 Track C rows have no recorded change. The team should add them from its own records; they should not be guessed. |
| Dates: IST in team notes vs UTC in the feed | B §6.9 | Resolved | The feed is UTC and is used throughout. The two Track B rewrites are 1 Nov 21:17 UTC (`f29eac6`) and 2 Nov 03:11 UTC (`bbbb3f1`); team notes date the first one 2 Nov IST. |
| Order of the 2 Nov Track B rows | B §1 | Resolved | Every run has a finish time in the feed, so the order is fixed. |
| Commit identity of the pre-rewrite entry | B §1 | Resolved | It is `faaeb9d` (1 Nov 08:48 UTC, 60.371). |
| Track B v2 points | B §1 | Resolved | Board K 9.585006 gives 49.080, not 49.450. |
| Track C first board result | Root README | Resolved | `a3f64b1`, 53.646 points (31 Oct 11:35 UTC). `080c956` is the first run above 67. |
| Track C `1039e85` metrics | C §1 | Resolved | Feed: 0.068 s, 12 MB, 389 lines, 68.983 points. |
| Track C `0c55114` labelled as Track C current | Root README | Resolved | `0c55114` is a Track B commit; the Track C row there is a shared all-track push. |
| Track C `005c99a` finish time | C §1 | Resolved | 04:34 UTC in the feed; the PERF table said 04:33 and is corrected. |
| Region IDs of 1,000,000 or more | C §7.1 | Open | A spec and code question. The sparse path in ledger entry `09d07cc` is not in the `3b7b716` source. |
| Origin of the 64-record prefetch batch | C §7.2 | Open | The ledger has no row for the change that introduced `BATCH = 64`. The team needs to identify it. |
| Chunk scheduling | C §7.3 | Resolved | The source uses 512 KiB chunks with a shared counter. E2 was a separate, rejected experiment. |
| Quote handling description | C §7.4 | Resolved | The README was corrected to match the source. |
| Raw-byte name hashing vs the spec's name rules | C §7.5 | Open | A spec question. |
| Correctness of `3b7b716` | C §2, §7.6 | Open | Full seed-1 check not yet run. |
| Hostile fixture generator missing | C §7.7 | Open | `fixtures/make_c_longname.py` is named in earlier notes but is not in the repository. The team needs to commit it. |

## 4. Discrepancies found while building this ledger

| Item | Feed (used) | Earlier record | Note |
|---|---|---|---|
| Track C `15d48f6` finish | 21:03 UTC | ledger label logged 21:16:09 | The ledger timestamp was logged later than the finish. |
| Track C `3165259` finish | 21:11 UTC | ledger label logged 21:16:09 | Same pattern. |
| Track C `7580e12` finish | 21:31 UTC | ledger label logged 21:33:23 | Same pattern. |
| Track B and C `b22f4e2` finish | 22:02 UTC | ledger label logged 22:04:29 | Same pattern. |
| Track B rewrites | 1 Nov 21:17 UTC (`f29eac6`) | Team notes: 2 Nov | The team notes use IST. |
| Track B v2 points | 49.080 | Earlier PERF: 49.450 | Corrected in PERF. |
| Track C `005c99a` finish | 04:34 UTC | Earlier PERF: 04:33 | Corrected in PERF. |
| Track A `3fe70c9` memory | 529 MB at 0.137 s | Not recorded earlier | Recorded as the feed gives it. No change description on record. |
| Labels `board-<commit>` in earlier ledger | Not run IDs | Shown as run IDs | Dropped. |