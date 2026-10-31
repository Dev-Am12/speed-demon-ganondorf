# Track C: Join

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

The implementation is written in standard-library Rust (`D = 1`) and combines single-pass vectorized structural CSV indexing with zero-allocation region joining and lock-free bitmask aggregation:

1. **Zero-Allocation `mmap` Region Join (`regions.csv`):** Maps `/data/regions.csv` read-only and interns the 240 three-letter country slices in a 512-entry stack hash table during a single scan, populating a flat 1 MB `u8` lookup array (`region_map[0..1_000_000]`, with `0` representing `_UNMATCHED` via OS zero-page allocation). Only the 241 unique country slices are sorted lexicographically when formatting the final output.
2. **Single-Pass Grammar-Verified Chunk Alignment:** Maps `/data/records.csv` read-only via `mmap` and partitions the body across up to 12 worker threads (`std::thread::scope`). Because the schema guarantees that every record terminates with unquoted `,region_id,amount\r?\n` (`\.[0-9]{2}\r?\n`) followed by `[BOM]id,` while embedded newlines occur only inside alphabetic `note` fields, each worker snaps its starting boundary in $O(1)$ time without a separate 202 MB quote-counting pass. Inputs under 512 KiB execute on a single thread from byte 0.
3. **64-Byte `PCLMULQDQ` + AVX2 Structural Scanner:** Each worker scans its record-aligned slice in 64-byte blocks. Two 256-bit AVX2 loads identify `"` and fused `, | \n` masks (`_mm256_or_si256` before `_mm256_movemask_epi8` to reduce Port 0 pressure), and `_mm_clmulepi64_si128` computes the prefix-XOR `in_quote` mask across all 64 bytes in a single instruction. Masking `cn_mask & !in_quote` isolates the 5 structural delimiters of every record into a 4-element register array without multi-way jump-table branches.
4. **Zero-Copy `O(1)` Unescaped Field Lengths:** Because embedded newlines inside `note` are bare LF (`1` byte) and every internal quote belongs to an escaped `""` pair, the unescaped UTF-8 length of `note` is computed directly from the cumulative quote popcount between its bounding commas (`raw_len - 1 - (q_count / 2)`) without scanning or copying `note`.
5. **Unaligned `wymix` Name Hashing & Demand-Zeroed 256-Bit Country Bitmask:** Empty unquoted `,` and quoted `""` names are tracked directly in a per-country flag (`empty_name`) with zero hashing or atomic traffic. Non-empty names are hashed via overlapping unaligned `u64`/`u32` loads folded through a 64×64→128-bit hardware multiply (`wymix`). A demand-zeroed open-addressed table (`vec![0u64; NAME_SLOTS * 5]` cast to `&[AtomicU64]`) maps each 64-bit key to a 256-bit country bitmask (`[AtomicU64; 4]`), checking if the country bit is already set via a relaxed load before issuing `fetch_or`.
6. **Branchless Integer Cent Arithmetic:** Mixed CRLF/LF terminators are stripped branchlessly (`pos - (bytes[pos - 1] == b'\r') as usize`), amounts are parsed as signed 64-bit integer cents (`i64`), and mid-file UTF-8 BOMs (`\xEF\xBB\xBF`) at record starts are stripped prior to `id` parsing.

## Dependencies

None. Uses Rust's standard library only.

## With another 24 hours

1. **Profile-Guided Optimization (PGO):** Generate an LLVM PGO profile on synthetic generator data using the allowlisted `halloumee/speed-demon-rust` toolchain image to optimize register allocation and loop alignment in `parse_chunk`.
2. **Two-Digit Lookup Table Formatting:** Use a 200-byte two-digit ASCII lookup table for integer and cent formatting on the final 240 output rows.

## AI tools used

Gemini, GPT-6 Luna, and Claude assisted with SIMD structural parsing design, post-mortem bottleneck analysis, and verification workflows.