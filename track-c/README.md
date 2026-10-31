# Track C: Join

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

The implementation is written in standard-library Rust (`D = 1`) and combines an L1-hot `AVX2` + `VPCLMULQDQ` structural CSV pipeline with a per-thread 71-bit index-stealing L1 filter cache and lock-free Structure-of-Arrays (SoA) bitmask aggregation:

1. **Direct-Indexed Zero-Allocation `mmap` Region Join (`regions.csv`):** Maps `/data/regions.csv` read-only and maps each 3-letter `[A-Z]{3}` country code directly into a 32 KiB L1 lookup array (`((b0 & 31) << 10) | ((b1 & 31) << 5) | (b2 & 31)`) with zero hashing or collision loops, populating a flat 1 MB `u8` lookup table (`region_map[0..1_000_000]`, with `0` representing `_UNMATCHED`).
2. **Single-Pass Grammar-Verified Chunk Alignment:** Maps `/data/records.csv` read-only via `mmap` and partitions the body across up to 12 worker threads (`std::thread::scope`). Because the schema guarantees that every record terminates with unquoted `,region_id,amount\r?\n` (`\.[0-9]{2}\r?\n`) followed by `[BOM]id,` while embedded newlines occur only inside alphabetic `note` fields, each worker snaps its starting boundary in $O(1)$ time without a separate 202 MB quote-counting pass.
3. **L1-Hot `VPCLMULQDQ` + `AVX2` Structural Scanner:** Marked explicitly with `#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,pclmulqdq")]` so LLVM emits VEX-encoded `vpclmulqdq` inlined directly into the 64-byte loop. Each 64-byte block extracts structural delimiter offsets and cumulative quote popcounts (via BMI2 `_bzhi_u64`) and immediately drains complete 5-delimiter records (`n_delims >= 5`) while the 64-byte CSV cache line is hot in L1d.
4. **Port-Balanced Integer Parsing (`parse_uint`):** Parses `id`, `region_id`, and `amount` dollars using `lea`-based digit accumulation on Execution Ports 0/5/6 so Port 1 remains dedicated to `TZCNT`, `POPCNT`, and `wymix` 128-bit multiplies.
5. **71-Bit Index-Stealing L1 Filter Cache & Lock-Free SoA Country Bitmasks:** Empty unquoted `,` and quoted `""` names set a local `empty_name` flag (`agg[7] = 1`). Non-empty names are hashed via unaligned `u64` loads folded through a 64x64->128-bit multiply (`wymix | 1`). Each worker checks a 2,048-slot L1-resident direct-mapped filter cache (`seen: [u64; 2048]`) using lossless index bit-stealing (`c_idx = (h >> 1) & 2047` holds bits `1..=11` of `h`; `c_tag = (h & !0xFFF) | (country << 1) | 1` holds bits `12..=63` of `h` plus all 8 bits of `country`). Cache hits skip global table traffic completely; misses update the shared 131,072-slot open-addressed `keys` and 256-bit `masks` arrays via lock-free atomic operations.

## Dependencies

None. Uses Rust's standard library only.

## With another 24 hours

1. **Profile-Guided Optimization (PGO):** Generate an LLVM PGO profile on synthetic generator data using the allowlisted `halloumee/speed-demon-rust` toolchain image.
2. **Partitioned Name Aggregation:** Partition distinct name hashes across threads for parallel final popcount reduction.

## AI tools used

Gemini, GPT-6 Luna, and Claude assisted with SIMD structural parsing design, post-mortem bottleneck analysis, and verification workflows.