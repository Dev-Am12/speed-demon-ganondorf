# Track B: performance log

Board measurements and local estimates are labeled separately.

| Step | Change | Result | Board run ID | T (s) | M (MB) | Source lines | K | Points | Verdict / note |
|---|---|---|---|---:|---:|---:|---:|---:|---|
| 0 | Reference, unchanged | - | - | 56.35 | 2,200 | - | - | 0 | Baseline from the existing log |
| 1 | B v0: query-term inverted index; BOOL merges; phrase candidate verification | Board measurement | 40ad00c | 13.5 | 400 | 308 | 2.397181 | 18.985 | Supplied board result |
| 2 | B v1: read-only mmap; parallel two-pass scan; hybrid lists/bitsets; parallel ordered query blocks | LOCAL: hyperfine 2 warmups + 10 runs; v0 12.956 +/- 2.472 s, v1 2.845 +/- 0.159 s; RssAnon 273.67 MB, VmHWM 530.43 MB, RssFile 255.99 MB | pending |  |  | 1,113 | 4.638 | - | Local K estimate only; no v1 board result |
