# Track B performance log

Keep official board measurements separate from local timing and memory measurements. Board values below are supplied run results; local measurements do not predict the board axes.

## Latest all-track board snapshot

Commit `c35a41d` finished at 2026-11-01 09:39 UTC. The board reported:

| Track | Points | K | T | M | S |
|---|---:|---:|---:|---:|---:|
| A | 73.053 | 28.910180 | 0.048 s | 28 MB | 440 |
| B | 59.852 | 15.741032 | 0.435 s | 67 MB | 1,112 |
| C (best) | 70.837 | 26.106129 | 0.060 s | 10 MB | 393 |
| D | 48.206 | 9.207174 | 0.957 s | 135 MB | 369 |

The all-track candidate `c35a41d` used Track A source from a later candidate and replaced the team's earlier A best from `faaeb9d`. The promotion accompanying this update restores Track A from `faaeb9d`; that promotion has not yet received a board run.

## Track B board history

| Candidate / run | Board commit | T | M | S | K | Points |
|---|---|---:|---:|---:|---:|---:|
| B v1 | `832e2c8` | 1.408 s | 278 MB | 1,033 | 6.643748 | 41.121 |
| B v2 | not supplied | 0.970 s | 103 MB | 1,172 | 9.585 | 49.450 |
| B v2.5 | `272dc82` | 0.652 s | 103 MB | 1,059 | 11.872829 | 53.728 |
| Pre-phase candidate | `b22f4e2` | 0.417 s | 67 MB | 1,208 | 15.860 | 60.015 |
| B v9 candidate | `c35a41d` (source `e7c9ea6`) | 0.435 s | 67 MB | 1,112 | 15.741032 | 59.852 |

The latest candidate reduced board source size by 96 lines and held memory at 67 MB, but its board time was 18 ms slower than `b22f4e2`; K consequently decreased slightly. The board score is authoritative.

## Local phase experiments

These measurements were made in WSL2 on the full seed-1 input. The host was noisy. Do not compare the absolute local seconds with board time.

| Experiment | Result | Decision |
|---|---|---|
| B2a-x: compact tagged vocabulary with exact key confirmation | Correctness passed; paired PSET runs were 6-9% slower | Reverted |
| B2b: prune the pass-2 vocabulary table | Correctness passed; paired timing was noisy and order-dependent; no repeatable 3% gain | Reverted |
| B2c: branchless pass-1 sink update | Correctness passed; measured gains stayed below 3% | Reverted |
| B2c: branchless pass-2 sink update | Correctness passed; timing differed by order and all-CPU timing was slower | Reverted |
| B-X1: event-buffer replacement for pass 2 | Abandoned from a memory estimate: 28,716,697 hit events are about 230 MB at 8 bytes each before sorting; 12 x 4 MiB buffers alone add 48 MiB | Abandoned |
| B4: dense kept-term refs | Correctness passed; saved about 1.06 MB RSS, but PSET and all-CPU timing regressed | Reverted |
| B5: remove BOOL emptiness tracking | Gate and fuzz passed; local scc Code fell 1,201 -> 1,155; repeated paired PSET aggregate was about 1.9% faster, RSS increased about 0.5 MB | Kept |
| B6: remove galloping-list intersection | Gate passed; local scc Code fell 1,155 -> 1,129; paired PSET aggregate was about 0.5% slower, RSS fell about 0.19 MB | Kept as source compaction |
| B6: simplify `Acc10::push_word` | Full B gate passed (68 checks); tiny fuzz seeds 2-40 passed. Local scc Code fell 1,129 -> 1,105. Across four 40-run PSET batches, mean was 1.109 s versus 1.176 s for the prior B6 binary (-5.7%); several batches exceeded 5% standard deviation. Candidate RSS and all-CPU comparison were not measured. | Selected for board run by team decision |

The candidate's full seed-1 output hash was `f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927`. The gate also checked the spec example, hostile and large fixtures, determinism, and the configured CPU masks.

Local `scc --by-file track-b/src/main.rs track-b/Dockerfile` reported 1,250 lines and 1,105 Code lines for `e7c9ea6`. The board reports S=1,112. `scripts/static_axes.py` could not be run in this environment because Docker is unavailable; use the board's S value for official scoring.

## Scoring context

The supplied Track B formula is:

```text
K = (56.35 / T)^0.5
  x min(2200 / M, 50)^0.2
  x min(91 / S, 1)^0.15
  x min(Dref / D, 1)^0.15
```

Track B has no third-party runtime dependency, so `D = Dref = 1`. At the current 67 MB board memory, reducing memory toward the 44 MB cap can still help. Correctness remains a hard gate, including on the unseen holdout corpus.

### Earlier supplied board runs

| Candidate | Commit | T | M | S | K | Points |
|---|---|---:|---:|---:|---:|---:|
| Team v3 | f473810 | 0.711 s | 63 MB | 1,212 | 12.304 | 54.503 |
| Submitted baseline | a3f64b1 | 0.631 s | 57 MB | 1,294 | 13.163 | 55.9 (computed) |
| Positional index rebuild | 4d1311a | 0.592 s | 66 MB | 1,014 | 13.703 | 56.840 |
| Shard tuning and phrase planning | 5792950 | 0.505 s | 66 MB | 1,077 | 14.691 | 58.353 |
| Team candidate | 3fe70c9 | 0.515 s | 67 MB | 1,077 | 14.529 | 58.112 |
| Restored with other tracks | 3165259 | 0.534 s | 66 MB | 1,077 | 14.277 | 57.732 |
| Team candidate | a52ad27 | 0.494 s | 67 MB | 1,077 | 14.831 | 58.558 |
| Same B code, later run | 7580e12 | 0.506 s | 67 MB | 1,077 | 14.651 | 58.293 |

These rows are earlier team-supplied board snapshots. The board run at c35a41d is the latest supplied B result.

### B0 diagnostic profile

Three diagnostic runs on full seed 1 showed the scan/index build dominates. The recorded ranges were:

| Phase / counter | Local diagnostic result |
|---|---:|
| Query parse and vocabulary | 10.8-12.0 ms; V=75,747; table slots=262,144 |
| Line cuts | 7-11 ms |
| Pass 1 | 92.5-100.8 ms |
| Pass 2 | 243.8-284.2 ms |
| Decisions / phrase plans | about 2.5 ms; 19,000 phrases |
| Layout | 1.2-1.6 ms; 59,831 kept terms |
| Position event sort | 13-15 ms |
| Assembly | 2.4-3.0 ms |
| Query evaluation | 62-63 ms; 50,000 queries, 196 blocks |
| Output | 0.7-1.1 ms |
| Total diagnostic run | 428-467 ms |
| Tokens / vocabulary hits | 36,276,657 / 28,716,697 |
| Position events | 798,126 |
| Arena / bitsets | 27,134,486 / 5,594,072 bytes |
| VmHWM | about 311 MB |

These are diagnostic-build measurements, not production timings. Passes 1 and 2 account for roughly 79% of diagnostic runtime.

### Other compact source experiments

Reusing pass-1 state as the pass-2 cursor did not reduce RSS and was slightly slower; it was reverted. Removing one unreachable match arm saved one scc Code line, but paired timings were noisy and the all-CPU result regressed, so it was reverted. Neither change is part of e7c9ea6.
