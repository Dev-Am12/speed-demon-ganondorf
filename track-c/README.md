# Track C: Join

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

The implementation is written in standard-library Rust (`D = 1`) and combines vectorized structural CSV indexing with lock-free bitmask aggregation:

1. **Direct L2-Resident Join Lookup (`regions.csv`):** Collects all 3-letter country codes plus `_UNMATCHED`, sorts them lexicographically by unsigned byte order, and populates a flat `u8` lookup array indexed directly by integer `region_id` (`0..=999,999`, ~1 MB total). Joining a record's `region_id` takes a single bounds-checked byte load instead of a hash table lookup.
2. **Parallel Quote-Parity Chunk Alignment:** Maps `/data/records.csv` read-only via `mmap` and partitions the body across up to 12 worker threads (`std::thread::scope`). In pass one, workers count double-quote (`"`) characters in their slices using 64-byte AVX2 `movemask` + `popcnt`. Because every `"` in RFC-4180 (opening quote, closing quote, or half of an escaped `""` pair) flips quote parity, the prefix XOR of chunk quote counts determines the exact `in_quote` state at each chunk boundary, allowing each worker to snap to its first true record terminator (`\n` outside quotes).
3. **64-Byte `PCLMULQDQ` + AVX2 Structural Scanner:** Each worker scans its record-aligned slice in 64-byte blocks. Two 256-bit AVX2 loads identify `"`, `,`, and `\n` masks, and `_mm_clmulepi64_si128` computes the prefix-XOR `in_quote` mask across all 64 bytes in a single instruction. Masking `(c_mask | n_mask) & !in_quote` isolates the 5 structural delimiters of every record (`c1`, `c2`, `c3`, `c4`, `nl`).
4. **Zero-Copy `O(1)` Unescaped Field Lengths:** Because embedded newlines inside `note` are bare LF (`1` byte) and every internal quote belongs to an escaped `""` pair, the unescaped UTF-8 length of a quoted field is computed directly from the cumulative quote popcount between its bounding commas (`raw_len - 1 - (q_count / 2)`) without scanning or copying `note`.
5. **Canonical 128-Bit Name Hashing & Lock-Free 256-Bit Country Bitmask:** For `name`, unquoted empty `,` and quoted empty `""` both map to `&[]`. Normal quoted names without internal `""` escapes are hashed directly in place (`&bytes[c1 + 2 .. c2 - 1]`), while names with `q_name > 2` unescape `"" -> "` into a reusable buffer before hashing into a 128-bit key `(h1, h2)`. A single shared lock-free open-addressed table maps `(h1, h2)` to a 256-bit country bitmask (`[AtomicU64; 4]`). Workers check if the country bit is already set via a relaxed load before issuing `fetch_or`, avoiding cross-core cache invalidations on repeated names and saving 12x memory over per-thread tables.
6. **Exact Integer Cent Arithmetic:** Amounts are parsed directly as signed 64-bit integer cents (`i64`) and formatted without floating-point operations. Mid-file UTF-8 BOMs (`\xEF\xBB\xBF`) at record starts are stripped prior to `id` parsing while BOM bytes inside `name` or `note` remain untouched as data.

## Dependencies

None. Uses Rust's standard library only.

## With another 24 hours

1. **Profile-Guided Optimization (PGO):** Generate an LLVM PGO profile on synthetic generator data using the allowlisted `halloumee/speed-demon-rust` toolchain image to optimize branch layout in the delimiter dispatch loop.
2. **Table sizing tuning:** Dynamically scale the shared name hash table capacity for tiny inputs vs full inputs to minimize L2 cache footprint.

## AI tools used

Gemini, GPT-6 Luna, and Claude assisted with SIMD structural parsing design, code review, and verification workflows.