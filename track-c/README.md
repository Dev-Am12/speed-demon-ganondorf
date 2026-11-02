# Track C: Join

Track C reads a hostile CSV of about 200 MB (quoted commas, escaped quotes, embedded line feeds, mixed CRLF and LF line endings, and byte-order marks in the middle of the file), joins each record to a country through a region table, and writes per-country aggregates with exact distinct-name counts. The task, the input files and the exact output format are in the [spec](https://github.com/hackathon-raptors/speed-demon-2026/blob/main/specs/c-join.md). Measured history and open experiments are in [PERF.md](PERF.md).

## Status

| Item | Value |
|---|---|
| Current board commit | `3b7b716` |
| Points | 75.426 |
| K | 32.249688 |
| T | 0.042 s |
| M | 9 MB |
| S | 371 lines |
| Dependencies | None (`dependency_count = 0`; standard library only) |

Track C has 43 board pushes: 39 scored and 4 failed (one build failure and three time-limit failures). The full list, with each commit, result and recorded change, is in [PERF.md](PERF.md), section 1. The best result before 2 Nov was 70.837 points (`c35a41d`, 1 Nov).

Results are identified by commit ID, not run ID, because the board feed did not give a run ID for every run (see the disclosure at the top of `PERF.md`).

## Approach

The program maps both inputs read-only, cuts the record body into chunks at record starts, parses the chunks in parallel, and joins each record to its country through a 1 MB region table. The steps below follow `track-c/src/main.rs`.

1. **Regions** (`load_regions`). The region file is mapped and read once. Each region line gives a region id and a three-letter country code. The three letters are mapped to a 15-bit index in a table of 32,768 entries, and each new country is given a one-byte index (0 is reserved for `_UNMATCHED`). The region map (`rmap`) is a byte array of 1,000,000 entries indexed by region id. Region ids of 1,000,000 or more are mapped to `_UNMATCHED` in the current source. See `PERF.md`, section 6, item 1.

2. **Chunks** (`cut`, `is_record_end`). The body is split into 512 KiB chunks (`body.len() >> 19`), with at least one chunk. Each cut point moves forward to the next line feed that ends a record. A line ends a record when it finishes in `,region_id,amount` (with an optional CR before the LF), and the next record starts with a digit after an optional byte-order mark. Embedded line feeds sit only inside quoted fields, so this check cannot cut inside a record.

3. **Parse** (`parse_chunk`, `qmask`, `field`). Records are parsed in batches of 64. For each record the parser reads the id (digits), the name field, the note field, the region id (digits), and the amount. The amount is read as an optional sign, integer part and two-digit fraction, and converted to integer cents. Quoted fields are scanned 64 bytes at a time with AVX2 compares for the quote byte (`qmask`). A doubled quote inside a quoted field is counted as an escape, and the field's raw and unescaped lengths are both recorded. No floating point is used.

4. **Name hashing** (`hash_name_fast`, `hash_long`). A name of up to 32 bytes is hashed by a fast path that reads up to four 8-byte words, with the first and last words overlapping the name's ends. A longer name is hashed by `hash_long`, which reads every byte. The hash covers the raw bytes of the field, including any doubled quotes. An empty name gets hash 0 and is counted separately. Whether raw-byte hashing gives the same identity as the spec's name comparison is listed as an open item in `PERF.md`, section 6.

5. **Distinct names** (`find_slot`, `mark`). A shared table of 131,072 slots (`NAME_SLOTS = 1 << 17`) stores 64-bit name keys with linear probing and compare-and-swap. Each slot has four mask words of 64 bits, one bit per country index. `mark` tests the bit and then sets it with `fetch_or`. The caller that sees the bit previously clear is the only one that counts the `(name, country)` pair, so each pair is counted exactly once across threads, with no post-pass over the table.

6. **Aggregates and output** (`main`). Each thread keeps one array of nine counters per country: records, id sum, amount sum, amount minimum and maximum, name bytes, note bytes, an empty-name flag, and a distinct-name count. The threads take chunks from a shared atomic counter (dynamic scheduling). The thread arrays are merged by sum, min, max and OR. The output line for each country is:

   `country records id_sum amount_sum min max distinct_names name_bytes note_bytes`

   Amounts are written as decimal cents with two fraction digits. The `distinct_names` column is the number of distinct non-empty names plus one if an empty name occurs. Countries are sorted bytewise by name, with `_UNMATCHED` last. The output is `aggregates.txt`, written to `/out`.

The program uses up to 12 threads. If the body is smaller than one chunk, it runs on one thread.

## Build and verification

Build the way the board does:

```bash
cd track-c
RUSTFLAGS="-C target-cpu=x86-64-v3" cargo build --release --offline
cd ..
```

Produce the organizers' reference output for a dataset, then run the candidate and compare the hash:

```bash
fixtures/run_ref.sh c ~/sd-data/sample/1/c ~/sd-ref/c-sample     # prints the reference sha256
fixtures/run_local.sh c track-c/target/release/solution ~/sd-data/sample/1/c <expected_sha256>
```

Run the scoring-style check with Docker, which the team's machine does not have (WSL2 without Docker). On a machine with Docker, the organizers' `check.py` runs the board's build and run steps:

```bash
python3 check.py c --data ~/sd-data/sample/1/c --expected <expected_sha256>
```

The expected full seed-1 output hash is:

```text
d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536
```

The last recorded local checks, the differential fuzz against the reference (300 seeds), and the CPU-pinning and repeat-run checks are in `PERF.md`, section 3. The check for `3b7b716` is not yet recorded there.

The hostile long-name fixture is described in earlier notes, and its generator (`fixtures/make_c_longname.py`) is not in this repository snapshot. The fixture cannot be regenerated from this repository until the generator is added.

## Where each claim is checked

| Claim | Where in `track-c/src/main.rs` |
|---|---|
| Names over 32 bytes hash every byte | `hash_long` |
| Distinct `(name, country)` pairs are counted once | `mark` (`fetch_or` on the country bit) |
| Name table uses linear probing with compare-and-swap | `find_slot` |
| Quote detection is 64 bytes at a time with AVX2 | `qmask`, `field` |
| Chunk boundaries are record starts | `is_record_end`, `cut` |
| Amounts are integer cents | `parse_chunk` (`cents`), `fmt_cents` |
| Aggregates are merged by sum, min, max and OR | the merge loop in `main` |
| Output is sorted bytewise with `_UNMATCHED` last | `order` and `order.push(0)` in `main` |

## Known limits and open risks

- **Region ids of 1,000,000 or more (resolved).** The current source counts them as `_UNMATCHED`. An earlier ledger entry describes a sparse lookup for longer region keys that matched the reference on 300 seeds, and we could not find that path in the current source. This must be resolved against the spec. It is the most important open item for this track.
- **Raw-byte name hashing (checked).** Names are hashed over the raw field bytes, including doubled quotes. This is correct if the spec's names are canonical in their quoting. We have not confirmed that against the spec's name rules.
- **Prefetch (checked).** The source prefetches the region-map and name-table entries for each batch of 64 records. The history of that change is not fully recorded (see `PERF.md`, section 6).
- **Memory is near the floor for this design (checked).** At 9 MB, M is well under the 367 MB reference. The name table and the per-thread counters are fixed in size, so a denser design would be needed to change this much. A design that shares one table across threads without adding memory is the open problem on this track (`PERF.md`, section 4).

## Dependencies and tooling

No third-party dependencies. The program declares the C functions it needs (`mmap`) itself. `dependency_count = 0` in `.speed-demon.toml`, so D = 1. The board shows the standard library as one dependency for every entry; that is the baseline the formula counts.

Authorship: earlier versions of this track were written by coding agents under the team's loop prompts. Changes in the 1 to 2 Nov phase (the long-name hash, the distinct-count change and the removal of the duplicate cache) were written by Claude in chat and applied by the team. The team's records do not say, change by change, which tool wrote each line of the 3b7b716 source. Every board push was made by a team member, not by an AI tool.

## With another 24 hours

1. **Resolve the region-id range question** against the spec, and either add the sparse path or confirm that it is not needed.
2. **Deduplication under the 10 MB memory cap.** The name path is about half of the run time. Per-thread tables would multiply memory by 12, so the next design must share one table with fewer random accesses.
3. **Smaller region map.** The 1 MB region table is read at random once per record. A denser mapping could save about 9 ms (`PERF.md` estimate).
4. **Source reduction** from 371 lines. The formula gives about 0.3 points per 100 lines removed (estimate).