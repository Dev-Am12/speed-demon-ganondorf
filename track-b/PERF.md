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
