| run ID | time | track | commit | hypothesis | T | M | K | kept/reverted |
|---|---|---|---|---|---|---|---|---|
| board-15d48f6 | 2026-10-31T21:16:09Z | C | 15d48f6 | C0+C2a (first board sample) | 0.085s | 10MB | 22.12 | kept |
| board-3165259 | 2026-10-31T21:16:09Z | C | 3165259 | C0+C2a (restore sample) | 0.063s | 10MB | 25.39 | kept |
| board-7580e12 | 2026-10-31T21:33:23Z | C | 7580e12 | E1: seen cache removed (board 70.508, tie with C2a within noise) | 0.063s | 10MB | 25.71 | kept |
| board-b22f4e2-B | 2026-10-31T22:04:29Z | B | c65b4af | B2a+B2b tagged vocab + block tokenizer (board 60.015) | 0.417s | 67MB | 15.86 | kept |
| board-b22f4e2-C | 2026-10-31T22:04:29Z | C | c65b4af | E1 on board with B2a+B2b (board 70.118) | 0.065s | 10MB | 25.26 | kept |
| local-1e11ee6 | 2026-11-01T07:10:18Z | C | 1e11ee6 | Correctness: final record at EOF; candidate matches reference on 300 fuzz seeds (local) | 0.070s local | 202MB local | n/a | kept |
| local-09d07cc | 2026-11-01T07:11:19Z | C | 09d07cc | Correctness: sparse lookup for >7-digit region keys; candidate matches reference on all 300 fuzz seeds (local) | 0.070s local | 202MB local | n/a | kept |
| local-11aeecc | 2026-11-01T07:12:00Z | C | reverted | C-S1 PF=16 prefetch ring: noisy order-dependent local PSET result (local) | 137.6ms local | 202MB local | n/a | reverted |
| local-bd50279 | 2026-11-01T07:19:00Z | C | reverted | C-S1 PF=8: PSET results inconsistent and one order gained only 2.7% (local) | 110.0ms local | 202MB local | n/a | reverted |
| local-9a8a751 | 2026-11-01T07:21:49Z | C | reverted | C-S1 PF=32: noisy order-dependent local PSET result; one order was 6.8% slower (local) | 124.9ms local | 202MB local | n/a | reverted |
| local-3642552 | 2026-11-01T07:48:00Z | B | reverted | B2a-x exact 64-bit tagged slots; full gate and fuzz pass, PSET paired runs noisy and candidate mean 6-9% slower; reverted (local) | 1.21s local | 310MB local | n/a | reverted |
| local-8cdd91f | 2026-11-01T08:00:49Z | B | reverted | B2b pruned pass-2 table (kept and positional terms): correctness passed, but PSET results remained noisy and disagreed between orders; RSS -0.35 MB (local) | 1.13s local (noisy) | 309.6MB local | n/a | reverted |
| local-c0f7210 | 2026-11-01T08:10:58Z | B | reverted | B2c pass-1 sink update: full gate and fuzz pass; PSET gains below 3% consistently once noisy outliers are excluded; RSS +0.2 MB (local) | 1.01s local | 310.0MB local | n/a | reverted |
| local-3b70c24 | 2026-11-01T08:21:44Z | B | reverted | B2c pass-2 sink entries: gate passed; PSET results were order-dependent with large outliers, all-CPU was 4.6% slower and RSS +0.33 MB (local) | 1.08s local (noisy) | 310.1MB local | n/a | reverted |
| local-9f4b00f | 2026-11-01T08:21:44Z | B | abandoned | B-X1 analysis: 61,682/91,168 rarest positions (67.66%, ties included); 28,716,697 events = 229,733,576 B; 4 MiB buffers x12 = 48 MiB before sort scratch, peak >55 MB. (local; rejected: peak memory exceeds 55 MB) | n/a | projected >108MB anon | n/a | reverted |
| local-e450436 | 2026-11-01T08:35:00Z | B | reverted | B4 dense kept-term refs: GATE B PASS (68 checks), tiny fuzz PASS; RSS -1.06 MB, but PSET timing +1.1% / +3.8% slower and all-CPU +8.6% slower (local) | 1.07s local | 308.7MB local | n/a | reverted |
| local-69e0ec3 | 2026-11-01T08:45:44Z | B | 69e0ec3 | B5 BOOL emptiness tracking removed; gate/fuzz PASS, scc code 1201 -> 1155, repeated paired PSET aggregate -1.9%, RSS +0.5 MB (local) | 1.095s local | 310.2MB local | n/a | kept |
| local-783f13a | 2026-11-01T08:52:32Z | B | reverted | B4 reuses pass-1 worker state as pass-2 cursors; gate/fuzz PASS, RSS +0.05 MB and timing slightly slower (local) | 1.11s local | 309.9MB local | n/a | reverted |
| local-714eb19 | 2026-11-01T09:10:00Z | B | reverted | B5 dead match arm removal: gates PASS and scc code -1; corrected repeated PSET pair noisy/order-sensitive, all-CPU +15.7%, RSS +0.03 MB (local) | 1.02s local (noisy) | 310.0MB local | n/a | reverted |
| local-196ab00 | 2026-11-01T09:17:17Z | B | 196ab00 | B6 gallop path removed; GATE B PASS, scc code 1155 -> 1129, paired PSET aggregate +0.5% time, RSS -0.19 MB (local) | 1.035s local | 309.8MB local | n/a | kept |
| local-e7c9ea6 | 2026-11-01T09:39:00Z | B | e7c9ea6 | Acc10 push_word set-bit loop; GATE B PASS (68 checks), tiny fuzz 2-40 PASS; four 40-run PSET batches averaged 5.7% faster than B6 but several were noisy; selected by team; board run c35a41d | 1.109s local; 0.435s board | n/a local; 67MB board | 15.741032 board | kept (board selected) |
| run-8d8dca2d47f6 | 2026-11-02T06:45:04Z | A | 248c1df | repair: restore verified sources (A 64925b1, B 005c99a) | 0.0415s | 24MB | 32.17 | baseline |
| run-c57c7b5fbb81 | 2026-11-02T06:45:10Z | B | 248c1df | repair: restore verified sources (A 64925b1, B 005c99a) | 0.3411s | 121MB | 15.52 | baseline |
