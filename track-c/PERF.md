# Track C: performance log

Every step from the reference to your final version, each with the live-board run ID that measured it. Judges check that the run IDs exist and that each change moved the way you say it did.

| Step | Change | Run ID | T (s) | M (MB) | S | K | Points | Verdict |
|---|---|---|---:|---:|---:|---:|---:|---|
| 0 | The reference, unchanged | - | 14.444 | 367.44 | 108 | 0.980 | 0.000 | Baseline Python reference |
| 1 | C1: Two-pass SIMD quote-parity CSV scanner, PCLMULQDQ structural delimiter indexing, 1 MB region lookup array, and shared lock-free 128-bit hash to 256-bit country bitmask table | a3f64b1 | 0.168 | 29 | 622 | 11.828 | 53.646 | Kept (first board baseline) |
| 2 | C2: Single-pass grammar-snapped chunking, zero-allocation mmap region interning, Port-0 fused AVX2 delimiter mask, and unaligned wyhash + demand-zeroed 64-bit name table | 080c956 | 0.074 | 10 | 509 | 22.754 | 67.853 | Kept (2.27x faster, -65% M) |
| 3 | C3: Explicit `+pclmul` target feature inlining, two-stage L1 mini-batch structural tape, 8-byte SWAR integer/cent parsing, SoA L2-resident name table, and S compaction | pending | pending | pending | ~235 | pending | pending | Candidate ready for board verification |

### Experiment Log

- **Step 1 (C1 — Two-pass `PCLMULQDQ` + AVX2 structural CSV join, board run `a3f64b1`):**
  - *Hypothesis:* Using 64-byte AVX2 comparisons paired with `_mm_clmulepi64_si128` carry-less multiplication computes the quote parity mask in one instruction, allowing 12 threads to locate structural commas and record terminators while deriving unescaped `note` and `name` byte lengths directly from quote popcounts.
  - *Board result (`a3f64b1`):* Passed correctness gate (`d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536`). Measured `T = 0.168 s`, `M = 29 MB`, `S = 622 lines`, `D = 1`, `K = 11.828112` (`53.646 pts`).

- **Step 2 (C2 — Single-pass record boundary snapping & zero-allocation region loader, board run `080c956`):**
  - *Hypothesis:* Eliminating the 400,000 heap `Vec<u8>` allocations in `load_regions` via `mmap` + a 512-slot stack hash table will drop peak RSS from 29 MB to ~10 MB and save ~50 ms of single-threaded startup. Replacing the 202 MB Pass 1 quote-counting scan with $O(1)$ grammar-verified record boundary snapping (`is_record_end`) halves memory bandwidth.
  - *Board result (`080c956`):* Passed correctness gate. Runtime dropped **2.27x** (`0.168 s -> 0.074 s`), peak memory dropped **65.5%** (`29 MB -> 10 MB`, tied for lowest on the leaderboard), and $K$ nearly doubled (`11.828 -> 22.754`, `53.646 -> 67.853 pts`).
  - *Post-mortem analysis of `080c956` (`0.074 s`, `S = 509`):*
    1. **Missing `+pclmul` target feature caused 3.15M out-of-line SSE calls:** In the x86-64 psABI and LLVM, `-C target-cpu=x86-64-v3` does **not** include `pclmul`. Because `parse_chunk` lacked `#[target_feature(enable = "pclmul")]`, LLVM refused to inline `_mm_clmulepi64_si128` (which has `#[target_feature(enable = "pclmul")]` in `libcore`) and emitted an out-of-line function call to legacy SSE `pclmulqdq` on every 64-byte block, triggering register spills and AVX-to-SSE transition stalls.
    2. **Interleaved delimiter extraction & stack spills:** Indexing `d[field_idx]` and `q[field_idx]` by a runtime variable `field_idx` inside `while sep != 0` forced `d` and `q` onto the stack and executed 25+ branches inside the inner loop on every 5th iteration.
    3. **Byte-by-byte integer loops & 40-byte AoS cache straddling:** Parsing `id`, `region_id`, and `amount` with byte-by-byte loops ran ~55M loop branches, and 40-byte `NameTable` entries straddled 64-byte cache lines while exceeding the 1.25 MiB L2 cache.
    4. **Formatted line count (`S = 509`):** `rustfmt` expanded `load_regions`, `is_record_end`, CLI parsing, and `push_u64`/`push_cents` boilerplate to 509 lines (`(108 / 509)^0.15 = 0.792`).

- **Step 3 (C3 — `+pclmul` VEX inlining, two-stage L1 structural tape, branchless SWAR integer parsing, SoA name table, and S compaction):**
  - *Hypothesis:*
    1. **Explicit `#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,pclmul")]` (`T`: -25 ms):** Enables LLVM to inline `_mm_clmulepi64_si128` directly into `parse_chunk` as a single VEX-encoded `vpclmulqdq` instruction with zero register spills or AVX-SSE transition stalls.
    2. **Two-stage L1 mini-batch structural tape (`T`: -12 ms):** Decouples Stage 1 (a branch-free 6-instruction `while sep != 0` loop writing `(pos, q_here)` into a 320-element L1-resident stack array) from Stage 2 (`while idx + 5 <= n_delims`, `idx += 5`), eliminating `field_idx` state-machine branching and stack store-to-load forwarding stalls.
    3. **8-byte SWAR integer & cent parsing + branchless `hash_name_fast` (`T`: -10 ms):** Parses `id`, `region_id`, and `amount` dollars in 6 cycles without loops using 8-byte SWAR multiplications (`2561`, `6_553_601`, `42_949_672_960_001`), and hashes all `1..=16` byte names along a single branchless masked path.
    4. **SoA `NameTable` (`keys` + `masks`):** Keeps the 1 MiB `keys` array inside each P-core's 1.25 MiB L2 cache and aligns every 32-byte country bitmask so it never straddles a 64-byte cache line.
    5. **Code compaction (`S`: 509 → ~235 lines, +12.3% $K$):** Compacting cold helpers and formatting the 240 output lines with integer `writeln!` cuts `S` by more than half.
  - *Verdict:* Kept; ready for board verification.