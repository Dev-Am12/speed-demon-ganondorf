# Track C: Join

The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md).

## Approach

The implementation is written in standard-library Rust (`D = 1`) and combines a two-stage `AVX2` + `VPCLMULQDQ` structural CSV pipeline with SWAR integer parsing, single-pass `mmap` region joining, and dense-ID Structure-of-Arrays (SoA) lock-free bitmask aggregation:

1. **Single-Pass Zero-Allocation `mmap` Region Join (`regions.csv`):** Maps `/data/regions.csv` read-only and scans `region_id` and country bytes in a single tight loop without subslice iterators, interning the 240 three-letter country slices in a 512-entry stack hash table and populating a flat 1 MB `u8` lookup array (`region_map[0..1_000_000]`, with `0` representing `_UNMATCHED` via OS zero-page allocation).
2. **Single-Pass Grammar-Verified Chunk Alignment:** Maps `/data/records.csv` read-only via `mmap` and partitions the body across up to 12 worker threads (`std::thread::scope`). Because the schema guarantees that every record terminates with unquoted `,region_id,amount\r?\n` (`\.[0-9]{2}\r?\n`) followed by `[BOM]id,` while embedded newlines occur only inside alphabetic `note` fields, each worker snaps its starting boundary in $O(1)$ time without a separate 202 MB quote-counting pass.
3. **Two-Stage `VPCLMULQDQ` + `AVX2` L1 Structural Tape:** Marked explicitly with `#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,pclmulqdq")]` so LLVM inlines `_mm_clmulepi64_si128` as a single VEX-encoded `vpclmulqdq` instruction (avoiding `-C target-cpu=x86-64-v3`'s missing `+pclmul` attribute and out-of-line SSE transition stalls). Stage 1 scans 64-byte blocks and extracts structural delimiter offsets and cumulative quote popcounts (via BMI2 `_bzhi_u64`) into a 320-entry L1-resident stack buffer (`d_pos`, `d_q`). Stage 2 consumes complete records at a fixed stride of 5 delimiters (`idx += 5`) using unchecked pointer offsets.
4. **Branchless 8-Byte SWAR Integer & Cent Parsing:** Replaces byte-by-byte digit loops for `id`, `region_id`, and `amount` dollars with 8-byte SWAR multiplications (`2561`, `6_553_601`, `42_949_672_960_001`), parsing 1-to-8-digit integers in 6 cycles with zero branches. Unescaped `note` byte lengths and unescaped `name` spans (`q_name <= 2`) are computed branchlessly from cumulative quote popcounts.
5. **Branchless `wymix` Name Hashing & Dense-ID L2-Resident Country Bitmasks:** Empty unquoted `,` and quoted `""` names set a local `empty_name` flag (`agg[7] = 1`) with zero hashing or atomic traffic. Non-empty names are hashed via masked overlapping unaligned `u64` loads folded through a 64x64->128-bit hardware multiply (`wymix`). Each distinct name inserted into the 131,072-slot open-addressed `keys` table claims a dense 17-bit ID from an atomic counter (`next_id`) and packs `(tag | id)` into `keys[idx]`. Indexing the 256-bit country masks (`masks[(id << 2) + w_off]`) by dense ID rather than hash slot keeps all active country masks inside each P-core's 1.25 MiB L2 cache and avoids faulting untouched pages into physical RAM.

## Dependencies

None. Uses Rust's standard library only.

## With another 24 hours

1. **Profile-Guided Optimization (PGO):** Generate an LLVM PGO profile on synthetic generator data using the allowlisted `halloumee/speed-demon-rust` toolchain image to optimize register allocation in `parse_chunk`.
2. **Custom Integer Output Formatting:** Format output integers directly into a stack buffer using a 100-entry two-digit lookup table.

## AI tools used

Gemini, GPT-6 Luna, and Claude assisted with SIMD structural parsing design, post-mortem bottleneck analysis, and verification workflows.