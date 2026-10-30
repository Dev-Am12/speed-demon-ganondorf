# Track B: performance log

Board measurements and local estimates are labeled separately. Local memory is peak process RssAnon; mapped input pages are reported separately.

| Step | Change | Result | Board run ID | T (s) | M (MB) | Source lines | K | Points | Verdict / note |
|---|---|---|---|---:|---:|---:|---:|---:|---|
| 0 | Reference, unchanged | Reference axes | - | 56.35 | 2,200 | 91 | - | - | Organizer reference |
| v0 | Query-term inverted index; BOOL merges; phrase verification | Board | 40ad00c | 13.5 | 400 | 308 | 2.397181 | 18.985 | Supplied board result |
| v1 | Read-only mmap; parallel two-pass scan; hybrid lists/bitsets; ordered parallel query blocks | Board | 832e2c8 | 1.408 | 278 | 1033 | 6.643748 | 41.121 | Supplied board result |
| v2 | Fused scan; compact vocabulary; phrase-key verification and rarest-term seeding | Board | not supplied | 0.970 | 103 | 1172 | 9.585 | 49.450 | Supplied board result |
| V2 M1 | Flat sparse and dense arenas; no per-term posting vectors | LOCAL: 2.693 +/- 0.155 s; RssAnon 87.65 MB; VmHWM 337.63 MB | pending | 2.693 | 87.65 | 1068 | - | - | Kept; full gate and tiny fuzz passed |
| V2 T1 | Fused byte scan; compact vocabulary; key prefilter | LOCAL: 1.993 +/- 0.028 s; RssAnon 86.60 MB; VmHWM 336.58 MB | pending | 1.993 | 86.60 | 1167 | - | - | Kept; full gate and tiny fuzz passed |
| V2 Stage 3 | Phrase key verifier; rarest seed; candidate iteration without copies | LOCAL: 1.858 +/- 0.178 s; RssAnon 87.34 MB; VmHWM 337.31 MB | pending | 1.858 | 87.34 | 1172 | 7.156 | - | Kept; local K estimate only |
| V2 Stage 4a | Packed sparse ids by 65,536-document blocks | LOCAL: 1.886 +/- 0.022 s; RssAnon 64.94 MB; VmHWM 314.92 MB | pending | 1.886 | 64.94 | - | - | - | Reverted: paired Stage 3 run was 1.453 +/- 0.361 s; runtime acceptance limit exceeded despite lower memory |

Local K uses Stage 3's measured 1.858 s, 87.336 MB RssAnon and calibrated 1,172 source lines. It is a local estimate, not a board result. Stage 4a passed the correctness gate and tiny fuzz, but was reverted because its paired 10-run timing exceeded the permitted 5% slowdown.
