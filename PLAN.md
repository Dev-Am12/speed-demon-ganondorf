# PLAN.md — Speed Demon 2026: team operating plan

**Version P-1 · written Wed 28 Oct 2026 (late night IST) · team of two · Rust, standard library only**
This file is design and process only. It contains no solution code (Rule 5: all solution code is written after kickoff, Fri 30 Oct 20:30 IST). Authority order when sources disagree: (1) the organizers' specs, harness and board code in `hackathon-raptors/speed-demon-2026`; (2) host emails and speeddemon.dev; (3) this file. Update the status checkboxes as you go; log decisions in section 14.

---

## 0. Status board (tick as done)
- [ ] Repo created from `hackathon-raptors/speed-demon-starter` (owner: Role C), teammate added as collaborator with write access, invitation accepted
- [ ] Board GitHub App installed on the repo; registration form submitted (repo URL, tracks A, B, C, D); `team` set in each `.speed-demon.toml`
- [ ] Python 3.11+ ready; organizer repo cloned; `tiny`, `sample`, `full` seed-1 data generated for A, B, C, D; reference outputs hash-match the spec tables
- [ ] Role P: Docker works; `check.py` ran on an unchanged starter track; images pulled; `scripts/` written
- [ ] Day-0 data measurements done (Appendix D)
- [ ] Linux environment decision for Role C (WSL2 or alternative); Rust toolchain decision (1.99.1)
- [ ] Agent availability and limits known; sleep blocks agreed
- [ ] G0 … G8 (section 3)

---

## 1. The game in one page
**Score.** `K = (Tref/T)^0.50 · (Mref/M)^0.20 · (Sref/S)^0.15 · (Dref/D)^0.15`. Speed score = `50·log10(K)` clamped to 0–100 (K = 10 → 50, 20 → 65, 30 → 74, 50 → 85, 100 → 100). Track total = 0.65 × speed + 0.35 × judges (code quality 15%, technique 12%, PERF.md 8% of the total). Overall title = geometric mean of our **best three** entered track totals. Prizes: $800 / $400 / $200 overall, $50 for the highest K on each track, $100 × 2 write-ups (individual, after submission; not a build task).

| Axis | Exponent | Cap | Meaning |
|---|---|---|---|
| T wall time | 0.50 | none | mean of the middle three of five runs, exec to exit (`sd-timer`; container start and teardown never count); re-run if the spread exceeds `max(5%, 50 ms)` |
| M peak memory | 0.20 | ratio ≤ 50× | **highest of the five `memory.peak` readings**; includes tmpfs and the output file's page cache |
| S source size | 0.15 | ratio ≤ 1.0 | `scc` code lines after rustfmt defaults over the whole track folder; Dockerfile and build scripts count; Markdown, TOML, JSON, lockfiles, `.speed-demon.toml`, `.profdata` are free |
| D dependencies | 0.15 | ratio ≤ 1.0 | 1 + number of direct non-std runtime dependencies (we use none) |

**What each lever is worth (multiplier on K).**
| Change | Effect |
|---|---|
| halve runtime | ×1.414 |
| halve memory (while the ratio is below 50×) | ×1.149 |
| double source lines above the reference | ×0.901 |
| add one dependency | ×0.901 |
| +10% lines | ≈ ×0.986, so it must buy about 3% speed |
| double lines | must buy ≥ 23.1% speed |
| one memory halving | worth a 32% runtime change |

**Reference numbers (scoring box):** A 32.05 s / 228 MB / 56 lines; B 56.35 s; C 14.44 s; D 72.69 s. Reference M and S for B, C, D are in the organizer repo (`harness/reference_axes.json` or the board calibration file). Timeouts are 10× these; the board caps a run at 3×.

**What wins:** (1) correct on all five runs of every entered track (one wrong byte or one flaky run = zero); (2) the highest time ratio we can reach; (3) modest source (cheap, ×0.9 per doubling), memory that is both small and stable, zero dependencies; (4) readable code and an honest PERF.md that cites board run IDs.

---

## 2. People, roles, ownership
| | **Role P — Platform & Performance** | **Role C — Correctness & Oracle** |
|---|---|---|
| Tracks | A (Ingest), D (Traverse) | B (Index), then C (Join, fallback) |
| Tooling (`scripts/`, outside `track-*`) | all Docker work: `check.py`, harness runs, `static_axes.py` (S/D), image digests, PGO profile generation, bench logger, K calculator, `board_push.sh`, `runs_ledger.md`, clean-room rehearsal | none |
| Tests (`fixtures/`, outside `track-*`) | none | oracle for all tracks: generate data, run the Python references, store expected hashes, differential/fuzz scripts, determinism ×5, known-bad controls, day-0 measurements |
| Docs | `PERF.md` + `README.md` for A and D | `PERF.md` + `README.md` for B and C |
| Reviews | performance, `unsafe`, determinism in B and C diffs | correctness in A and D diffs |
| Owner-only tasks | collects board results, keeps the slot ledger | **repo owner:** create the repo, install the Board App, submit the registration form, invite the collaborator, make the repo public at G7/G8 |

**Third counted track rule (B-or-C):** the third counted track must be correct on the board by H40. If B is not correct by H24, Role C starts C immediately and B is retried afterwards. The weakest entered track is dropped from the overall score, so a fourth track is insurance and a shot at a $50 prize, never a priority over A, B, D.

**Repository ownership (decided).** A personal private repo created from the template by Role C, with Role P as a collaborator with write access. No GitHub organization is needed: the board polls the head of the `speed-demon` branch with `git ls-remote` and does not care who pushed. Only the owner can install the App, change visibility and settings (a personal repo has one admin). The owner must therefore be reachable at G7–G8 (Mon 14:30–20:30 IST); keep 2FA recovery codes at hand.

**Communication.** One team chat. Post before every push to `speed-demon`: `BOARD: <track> <short-sha> <hypothesis>`. Post a status line every 4 hours during the build (what is correct, what is running, what is blocked). Decisions go in section 14 the same hour.

**Decision rights.** Track owner decides design inside their track folder. Anything that changes scope (dropping or adding a track), a dependency, `unsafe` beyond what section 6 allows, or a board push policy change needs both people.

---

## 3. Schedule (IST) and gates
Kickoff Fri 30 Oct 20:30 (15:00 UTC); freeze Mon 2 Nov 20:30 (15:00 UTC); 10% samples + hashes Fri 30 Oct 11:30. H0 = kickoff.

| Hour | IST | What |
|---|---|---|
| H0 | Fri 20:30 | Kickoff: full datasets, board opens |
| H2 | Fri 22:30 | **G0** conversion done, repo pushed, first board run seen |
| H8 | Sat 04:30 | end of night-1 sprint (first sleep block possible) |
| H12 | Sat 08:30 | **G1** A baseline passes the sample hash locally |
| H20 | Sat 16:30 | **G2** A and B correct on the board; noise measured |
| H24 | Sat 20:30 | B-or-C switch point |
| H30 | Sun 02:30 | **G3** D baseline correct on the board |
| H40 | Sun 12:30 | **G3b** third counted track secured (B or C correct on the board) |
| H48 | Sun 20:30 | **G4** A, D and the best of B/C at a plateau |
| H58 | Mon 06:30 | **G5** experiments finished; S/D audited; PGO profile last |
| H60 | Mon 08:30 | **G6 feature freeze** |
| H66 | Mon 14:30 | **G7** clean-room rehearsal passed; docs complete; repo public |
| H70 | Mon 18:30 | **G8** release candidate on `speed-demon` |
| H72 | Mon 20:30 | freeze |

**Gate contents.**
- **G0:** each built track folder converted (section 5), repo pushed, Board App working (first run recorded), oracle hashes stored, local floor measurement done (trivial binary in a container with the real flags).
- **G1:** A v0 passes `tiny` and `sample` hashes locally; B v0 in progress.
- **G2:** A and B pass the full-data hash on the board, deterministic ×5 locally; baseline T/M recorded with run IDs; board noise measured by re-running identical code once.
- **G3:** D v0 correct on the board; C v0 in progress if the B-or-C rule needs it.
- **G3b:** at least one of B and C correct on the board; the other continues until H48, else cut.
- **G4:** PERF.md rows exist for every experiment with run IDs; gap to the public leaderboard understood; effort rebalanced toward the weakest counted track.
- **G5:** final experiments done; `static_axes.py` run; zero dependencies; determinism ×5 and thread-count independence verified; memory stable across runs; PGO profile generated last, from final source, only if the board shows a clear gain.
- **G6:** no new algorithms after this; only fixes with a test.
- **G7:** fresh clone builds offline with the allowlisted images and passes; README and PERF.md complete; `.speed-demon.toml` correct; licence added; **repo made public**.
- **G8:** release candidate tagged on `speed-demon`; fix-with-test only.

**Sleep.** The night-1 sprint runs H0–H8. Both people must not sleep at the same time; both are awake at G2, G4, G6, G7, G8. Draft: two blocks of 5–6 hours each, staggered; fix the exact blocks on Thu 29.

**Pre-kickoff checklist** (see section 0 for owners): create the repo; accept the invitation; install the App; submit the form; generate data and verify reference hashes; test Docker; write `scripts/` and `fixtures/` tooling (outside track folders); read this file and Appendix C; decide agents; fix sleep blocks.

---

## 4. Environment
- **Role C (Windows, Intel 12th-gen i5, no Docker):** data generation, the Python references and tests need only Python 3.11+. Rust work should be on Linux because the plan uses Linux-only std APIs (`std::os::unix::fs::FileExt`, `mmap` via `extern "C"`); WSL2 is the cheapest Linux environment (about 4–6 GB in total, removable with `wsl --unregister`). **Open decision;** the fallback is Windows-portable baselines plus Role P's checks. Never write outputs with PowerShell `>` (UTF-16).
- **Role P (Windows, Ryzen 7 7445HS with AVX-512, Docker):** runs everything that needs Docker. Never build with `target-cpu=native`, `x86-64-v4` or AVX-512 intrinsics: the scoring box (i5-13500) has no AVX-512.
- **Toolchain:** the board builds with rustc 1.99.1 (allowlisted images). Matching it locally avoids "compiles here, fails there" and matches rustfmt. **Open decision for Role C** (installing 1.99.1 and removing the old 1.97.1 and duplicate `stable`). Never drift to a newer `stable`.
- **Dev tools (never dependencies, never in a track folder):** `hyperfine` (repeat-run timing for local A/B comparisons), `scc` (code-line counter, the organizers' S tool), the harness's `static_axes.py` (runs scc and rustfmt inside Docker).
- **Local measurement caveat:** both machines are laptops with hybrid or mobile cores; local timings are noisy and not comparable across machines. Use them only for relative A/B on one machine. The board is the truth for T and M. `harness/run_submission.py` needs root and Docker's cgroupfs driver on cgroup v2 and will not run on native Windows; try it inside the Docker VM or WSL2, and otherwise rely on `check.py` (correctness) and the board.
- **Line endings:** `.gitattributes` from the first commit: `* text=auto eol=lf`, `*.sh text eol=lf`, test data `-text`.

---

## 5. Repository layout and templates
```
PLAN.md                  this file
.gitattributes
LICENSE                  MIT (add by G7)
runs_ledger.md           board run ledger (outside track folders)
scripts/                 Role P tooling (never copied into a track folder)
fixtures/                Role C tooling and expected hashes (never copied into a track folder)
track-a/ track-b/ track-c/ track-d/   one folder per built track
   Cargo.toml  Dockerfile  .speed-demon.toml  PERF.md  README.md  src/main.rs
```
Delete the folders of tracks we do not build. A track folder that is byte-identical to the starter (apart from Markdown and `.speed-demon.toml`) is reported `unchanged` and never run. Each folder is self-contained (the Docker context is the track folder); anything shared between tracks is copied and counted in each.

**Cargo.toml (all tracks):** package name `solution`, edition 2021, no dependencies, and a `[profile.release]` section with `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`. Build settings live here because `Cargo.toml` is free in S.

**Dockerfile (about 7 lines, counted in S, no `build.sh`):** `FROM` the allowlisted Rust image pinned by digest (`rust:1-slim-trixie@sha256:1a939748a7cbab172de91a014e3804a97af1394a8c844faa67fce9401c4e40fb`; verify against `speeddemon.dev/allowlist.json`) `AS build`; `WORKDIR /src`; `COPY . .`; `RUN RUSTFLAGS="-C target-cpu=x86-64-v3" cargo build --release --offline` (the flag inline adds no line); then `FROM debian:13-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad`; `COPY --from=build /src/target/release/solution /app/solution`; `ENTRYPOINT ["/app/solution"]`. For PGO the allowlist also has `halloumee/speed-demon-rust:1.99.1-trixie-llvm-tools@sha256:13e4ee7135d8eac6c56476c7d1aee201f36479f4745fd5d0e6b4529f4e13251a` (rustc 1.99.1 plus llvm-tools). Rule: every `FROM` is an allowlisted image pinned by digest; nothing else enters the image; builds are offline, at most 10 minutes and 2 GB.

**Target CPU.** Default `x86-64-v3`. An explicit `-C target-cpu=raptorlake` is a measured late experiment (it names the box's CPU without asking for the build host's CPU, so it still builds on the off-box builder and cannot emit AVX-512 for the i5-13500). `native` is not used unless a measured gain justifies the box-built path.

**PERF.md (per track, required, 8% of the total).** A table, one row per experiment, newest last: `# | hypothesis | change | local result | board run ID | T | M | S lines | verdict (kept/reverted) | note`. Start with the naive reference run. Cite a board run ID for every claim. Failed experiments stay in. Do not claim an improvement that the cited runs do not show.

**README.md (per track).** The approach in plain prose, why the fast version is fast, the dependency list ("none"), what we would do with another 24 hours, and which AI tools were used (allowed; humans must understand every line).

**Code style for judges.** One cohesive `main.rs` per track (or a few small modules) with a comment wherever the trick is not self-evident (comments are free in S); no dead code; no commented-out experiments; honest error handling (but no panics on valid input); `unsafe` only as allowed in section 6.

---

## 6. Engineering conventions (all tracks)
1. **Baseline first.** v0 is single-threaded, safe, simple, and must pass `tiny`, `sample` and full-data hashes before any optimisation.
2. **I/O.** Default: positional reads (`std::os::unix::fs::FileExt::read_at`) from worker threads into small per-thread buffers (128–256 KiB), never reading the whole input into one buffer. Input pages are pre-warmed by the host and not charged to our container; reading them is free in M. `mmap` through a hand-declared `extern "C"` is allowed where random access is needed (Track B's corpus) or if the board shows it faster. **Never OR madvise advice values** (they are enumerations, not flags: `MADV_WILLNEED | MADV_HUGEPAGE` equals `MADV_NOHUGEPAGE`). Never map `MAP_PRIVATE` read-write (copy-on-write charges memory).
3. **Output.** Exactly one file in `/out` with the contract name; build it in memory with hand-written integer formatting (a two-digit lookup table avoids division) and write it once; flush and close before exiting.
4. **No scratch files.** `/tmp` is tmpfs and every byte counts in M. Keep all state in memory with bounded sizes.
5. **Threads.** `std::thread::scope`, no pool crates, at most 12 threads. Test 6 vs 12 (hyperthreads). Do not spawn more. Default thread stacks are fine (stack pages count only when touched); do not add `stack_size` code. Work distribution: static chunks for uniform work, an atomic counter for uneven work (Track D).
6. **Determinism.** Never iterate a `HashMap` to produce output. Merge per-thread results in a fixed order. No behaviour that depends on thread timing, thread count, addresses or time. The output must be identical for any thread count.
7. **Hashing.** The default `HashMap` (SipHash) is slow and randomised; use dense integer keys, or a small hand-written hasher that mixes 8 bytes at a time (a byte-at-a-time multiply loop is slow). Distinctness counting must not allow collisions to change the answer (see Track C).
8. **Integers.** All outputs are integers; no floating point anywhere. Use `u64`/`i64` wherever a value can exceed 32 bits (sums, ids, distances, rank arithmetic).
9. **Panics and exits.** `panic = "abort"` makes any panic a failed run. Parse code must be correct for every byte sequence the spec allows. After flushing and closing the output file, `std::process::exit(0)` skips destructors of big structures (a measured experiment; the kernel still tears the address space down). Never call anything from the `libc` crate (that is a dependency); declare the few C functions needed by hand.
10. **`unsafe`.** Allowed only in small, commented blocks for `mmap` and for unchecked indexing in a proven-hot loop after profiling shows it matters; every block states its invariant in a comment. Not in v0.
11. **SIMD.** Only after the scalar version is correct and profiling shows the loop dominates. Stable `std::arch` AVX2 only; no AVX-512.
12. **Memory discipline.** Pre-size buffers; no per-record allocation; no unbounded growth (M is the highest of five runs, so a rare spike hurts). Target the per-track budgets in section 7 but trade memory against time with the break-even table.
13. **Seed independence.** Never hard-code data-derived constants (endpoint names, counts, vocabulary, dates beyond what the spec guarantees, the sample hashes). The holdout seed is private.

---

## 7. Track playbooks
Targets are **hypotheses**; replace them with board numbers. Ranges assume 12 threads on the box.

### 7.1 Track A — Ingest (Role P)
**Contract.** `/data/requests.log` → `/out/summary.txt`. 3.1M lines, 239 MB, ASCII, LF: `ts SP method SP endpoint SP status SP latency LF`; `ts` is always 27 bytes (`YYYY-MM-DDTHH:MM:SS.uuuuuuZ`) inside 2026-10-30T18:00:00 … 2026-11-02T17:59:59 (72 hours); status three digits; latency u32 microseconds; lines are not time-sorted. Group = (UTC hour, endpoint bytes); per group: `count`, `p50`, `p95`, `p99` by **nearest rank** `(p·n + 99) div 100` (1-indexed, ascending, duplicates kept), `errors` = status 500–599, `error_ppm = floor(errors·1e6 / count)`. Output line: `hour endpoint count p50 p95 p99 errors error_ppm`, hour as `YYYY-MM-DDTHH:00:00Z`, sorted by hour then endpoint bytes (prefix first), empty groups omitted, LF after every line. Full: 126,908 groups, 10,113,024 B. 1,842 endpoints per seed (seed-dependent, Zipf).

**Traps.** 499 is not an error; integer-only rank and ppm (a floating-point rank is wrong for some n); byte order (`/v2/rank100` < `/v2/rank20`); lines out of hour order; duplicates count; an empty or tiny input; latency up to 4,294,967,295; n = 1 gives all percentiles equal; guard the case where two percentile indices coincide in a small group.

**v0.** Read with large buffered reads; per line: hour index from the date digits (Oct 30 → 0, Oct 31 → 1, Nov 1 → 2, Nov 2 → 3; index = day·24 + hour − 18), endpoint → dense id via a `HashMap`, status first digit, latency parse; per-group `Vec<u32>`; sort and index per group; output in (hour, endpoint-sorted) order.

**v1 architecture (HYPOTHESIS).**
1. Chunks snapped to line boundaries, one per thread, read with `read_at` into small buffers.
2. Each thread interns endpoints in its own small open-addressed table (local ids) and appends `(hour_idx, local_endpoint_id, latency)` records to its own buffer. No locks.
3. After the parallel pass, merge the per-thread endpoint tables once (at most 1,842 strings each), sort the unique endpoints by bytes once, and build `remap[thread][local_id] → sorted_id`. Then `group_id = hour_idx·E + sorted_id` is already in output order.
4. Partition the records by group-id range so each worker owns a disjoint range; per partition do a counting sort by group id, then per group: sizes 1–3 by direct index, up to about 32 by `sort_unstable`, larger by `select_nth_unstable` (three selections on shrinking sub-slices).
5. Each worker formats its groups' lines into its own buffer in output order (hand-written integer formatting); concatenate in partition order and write once.
**Memory plan:** records 8 B × 3.1M ≈ 25 MB, buffers 12 × 256 KiB, output ≈ 10 MB (output page cache counts), so about 40–55 MB (factor ≈ 1.3–1.6). Ideas to trade: pre-size by counting pass (costs T), smaller record packing.
**Targets:** T 0.1–0.3 s (ratio 107–320×), M 40–60 MB, S 200–300 lines.

**Experiment queue (one change each).** A1 allocation-free parse, fixed offsets; A2 flat endpoint table + word-wise hash; A3 hour index from digits; A4 parallel `read_at` chunks + remap + partition; A5 6 vs 12 threads, chunk size; A6 memory vs time arena trade (a memory halving must cost less than ×1.32 in T); A7 page-fault and startup costs; A8 explicit exit after flush; A9 AVX2 scanning if profiling supports it; A10 `target-cpu=raptorlake`; A11 PGO (last).

**Verification.** `tiny`, `sample`, full hashes (full `summary.txt` sha256 begins `495ca096`); fuzz seeds 1–50 on `tiny`; determinism ×5; thread counts 1/2/6/12 give the same hash.

### 7.2 Track B — Index (Role C)
**Contract.** `/data/docs.txt` (250,000 lines `id TAB text LF`, 253.8 MB, ids 0…N-1) and `/data/queries.txt` (50,000 lines: 31,000 `BOOL t (op t)*` with 1–6 terms and op ∈ AND OR NOT; 19,000 `PHRASE "t t …"` with 2–8 terms) → `/out/results.txt`: per query in order `count sum id_1 … id_m`, m = min(10, count) smallest ids ascending, sum exact (u64), no match prints `0 0`. Tokens: ASCII-lowercase, maximal runs of `[a-z0-9]`; every other byte (including all bytes ≥ 0x80) is a separator; no stemming, no stopwords; `007` ≠ `7`. `S(t)` = ids of documents containing `t`. BOOL: strictly left to right, no precedence; `AND` ∩, `OR` ∪, `NOT` difference. PHRASE: consecutive token positions, separators invisible (a phrase can cross a sentence), never across documents, repeated terms must repeat. Indexing only the query terms is explicitly allowed. 36.3M tokens; 609,833 distinct; 98% of phrases contain an anchor term with df ≤ N/1000.

**Traps.** Term ids need more than 16 bits (the query vocabulary is likely tens of thousands of terms; use `u32`); lowercase `and`/`or`/`not` in term positions are ordinary terms; NOT is binary; empty documents; sum above 2^32; duplicate query lines still get a line each; no parentheses exist in the grammar.

**v0.** Parse queries; collect the query vocabulary; tokenise exactly per the contract; build per-term sorted document lists for query terms; evaluate BOOL on the lists left to right; evaluate PHRASE by intersecting all phrase-term lists starting from the rarest, then verifying each candidate document by tokenising it and looking for the consecutive sequence; output.

**v1 architecture (HYPOTHESIS).**
1. Parse queries first (ids as `u32`), build a hash set of query terms.
2. Parallel scan of documents (mmap or `read_at`), each thread covering a contiguous range of document ids so per-thread posting lists concatenate in ascending order; record `(term_id, doc_id)` only for query terms; keep a line-offset table (250,001 offsets).
3. Hybrid sets: a sorted `u32` list when the document frequency is below about N/32 (≈ 7.8k, the break-even against a 31 KB bitset), otherwise a 250,000-bit bitset (do not use a low threshold like 1,024: it costs more memory than lists).
4. BOOL: iterate over the operators on an accumulator bitset; extract count (popcount), sum (iterate set bits) and the first 10 ids (lowest set bits).
5. PHRASE: candidates = intersection of all phrase terms' sets from the rarest term; verify each candidate by tokenising that document slice (mmap) and checking the consecutive sequence with correct token boundaries. Do not search raw substrings: `cat` inside `category` must not match.
6. Run the query phase in parallel (12 contiguous query chunks), each thread writing its own output buffer; concatenate; write once.
**Memory plan:** corpus via mmap (file pages are not charged to us), sets tens of MB, output 1.5 MB; reading the whole corpus into the heap would cost about 29% of K.
**Targets:** T 0.2–0.6 s, M 50–100 MB, S 250–350 lines.

**Experiment queue.** B1 vocabulary set + `u32` ids; B2 parallel document-snapped scan with in-order merge; B3 hybrid set threshold sweep; B4 phrase verification from the rarest term (day-0 measurement: total candidate documents after intersecting all phrase terms); B5 left-to-right evaluation with word-wise bitset operations; B6 startup and query-parse cost.

**Verification.** `tiny`/`sample`/full hashes (full `results.txt` sha256 begins `f86973c2`); fuzz seeds on `tiny`; determinism ×5; different thread counts.

### 7.3 Track D — Traverse (Role P)
**Contract.** `/data/graph.txt` (header `1740512 5000000`, then 5M lines `u v w`, 91,950,247 B, unordered; weights u32 including 0; directed; parallel edges, duplicate lines, self-loops, isolated nodes, nodes without in- or out-edges, weights up to 4,294,967,295) and `/data/queries.txt` (2,000 lines `s t`) → `/out/distances.txt`: one line per query, the shortest directed distance (u64, up to < 2^63) or `-1`; `d(s,s) = 0` always. Road-network structure (lattice, arterials, highways), node ids randomly relabelled. Queries target the node at Dijkstra rank r from s (octave uniform 1…16, median 512, mean ≈ 12,300, max 131,071), plus injected kinds (`s == t`, zero-length paths, unreachable, distances above 2^32, duplicates).

**Where the time goes (derived from the distribution).** The octaves 15 and 16 are 12.5% of the queries but about 75% of the settled nodes; total ≈ 25M node settlements over 2,000 queries. At roughly 100 ns per settlement that is about 2.5 s of CPU, so about 0.3–0.5 s on the box: **search time, not parsing, dominates.** Cache behaviour matters because ids are randomly relabelled (each touched node is a different cache line).

**Traps.** Distances must be `u64`: do **not** pack cost and node into one `u64` (`cost << 32`), distances above 2^32 exist; do not use `u32` distance arrays. The file is `graph.txt`. **Bidirectional stopping rule:** maintain `best` (initially infinity); whenever relaxing an edge improves a node already reached by the other direction, update `best` with the sum through that node; stop only when `min_forward_key + min_backward_key >= best` (or when either queue is empty); advance the side whose minimum key is smaller. Stopping when a node is first settled by both sides is wrong on weighted graphs. `s == t` → 0 even for isolated nodes; targets with no in-edges and sources with no out-edges → `-1` immediately (unless `s == t`); lightest parallel edge wins; zero weights are legal.

**v0.** Single-threaded parse; forward CSR; unidirectional Dijkstra with a binary heap and early exit at the target; per-query state reset through a touched list; `u64` distances.

**v1 architecture (HYPOTHESIS).**
1. Parallel parse in line-snapped chunks with a hand-written integer parser.
2. Build forward and reverse CSR by counting sort (atomic degree counters, or partition by node id to avoid atomics); keep offsets, targets and weights as three flat `u32` arrays.
3. Queries on 12 threads with an atomic counter for work distribution (query costs span four orders of magnitude), each thread with its own reusable state and heap.
4. Bidirectional Dijkstra with the stopping rule above (bidirectional search on a road-like graph typically settles about half as many nodes as one-directional search, not the square root; it also kills the unreachable cases cheaply); 4-ary flat heap of `(u64 distance, u32 node)` pairs.
5. Results written to a preallocated array by query index; format once; write once.
**Memory plan:** forward + reverse CSR ≈ 95 MB; per-thread distance state is the big lever (dense `u64` arrays cost about 28 MB per thread per direction, 330 MB for 12 threads): see experiments.
**Targets:** T 0.3–0.9 s, M 150–450 MB (unknown reference M), S 200–300 lines.

**Experiment queue.** D1 fast integer parsing + parallel chunks; D2 parallel CSR build (atomics vs partition); D3 reusable per-thread state reset via touched list; D4 heap type (binary vs 4-ary) and early exit; D5 dynamic scheduling; D6 bidirectional vs unidirectional (board decides; costs +47 MB and about 40 lines); D7 **locality**: renumber nodes (BFS order from a seed) before building the CSR so searches touch contiguous memory (the organizers' hint: memory layout beats asymptotics); D8 per-thread visited state as a small hash table instead of dense arrays (smaller and more cache-friendly, much less memory); D9 bit-packing edges (only with a general fallback, weights are u32); ALT and contraction hierarchies are out (preprocessing counts inside T and the 2,000 local queries cannot repay it).

**Verification.** `tiny`/`sample`/full hashes (full `distances.txt` sha256 begins `a23dbdf0`); fuzz seeds; check answers above 2^32 and the 10 `-1` answers; determinism ×5.

### 7.4 Track C — Join (Role C, fallback)
**Contract.** `/data/records.csv` (3,320,000 records, 201.9 MB) and `/data/regions.csv` (200,000 regions, 240 three-letter countries; ids 1…999,999) → `/out/aggregates.txt`. Rules: BOM `EF BB BF` only at file start or directly after a record terminator (skip it there, data elsewhere); quoted fields end at the next `"` that is not part of `""`; commas, LF and U+FEFF are literal inside quotes; terminators CRLF or bare LF outside quotes; empty unquoted and `""` are the same value. Guaranteed by the generator for every seed: no CR except as the first byte of a CRLF terminator, only printable ASCII plus LF/CR/BOM, `id` unquoted unique increasing, `region_id` unquoted ≤ 9,999,999, `amount` unquoted `-?(0|[1-9][0-9]*)\.[0-9]{2}` (never `-0.00`), non-empty names always quoted, notes lowercase words with at most 3 LFs, the file ends with CRLF. Join on integer `region_id`; unknown → `_UNMATCHED`. Per country: `count`, `id_sum`, `amount_sum`, `amount_min`, `amount_max` (cents), `names_distinct` (exact bytes after unescaping), `name_bytes`, `note_bytes` (lengths after unescaping; each `""` counts 1, an embedded LF counts 1). Output line `country count id_sum amount_sum amount_min amount_max names_distinct name_bytes note_bytes`, sorted bytewise, `_UNMATCHED` last, money as `-?I.dd`, LF after every line. 240 lines (17,054 B).

**Traps.** Region ids and country codes are **not** 16-bit / two-letter: use a direct array of 1,000,000 entries mapping `region_id` to a country index (ids above 999,999 are unmatched). Names differing in trailing spaces are distinct. Check on the reference whether the empty name counts as one distinct name. Sums exceed 2^32. A BOM mistake shows up in `id_sum`.

**v0.** Byte-stream state machine (start of field / unquoted / quoted / after closing quote), working on the stream and not on lines; join through the direct array; integer cents; aggregates per country; distinct names through a hash of the name bytes; output per contract.

**v1 architecture (HYPOTHESIS).**
1. Pass 1 (parallel): count `"` bytes per chunk; prefix parity gives each chunk's in-quote state at its start (every quote, opening, closing or half of `""`, flips parity).
2. Pass 2 (parallel): each thread finds its first record start (first LF at even parity, then skip a BOM) and parses its records until the next chunk's start.
3. Per record: skip `id`; for `name` compute the unescaped length (raw length − 2 − inner quote count / 2 for quoted fields) and hash the raw name bytes for distinctness (non-empty names are always quoted, so raw bytes are canonical; treat empty separately); for `note` only the unescaped length (same formula) while locating the field end by quote parity; parse `region_id` and `amount`.
4. Distinct names: per-thread table from a 128-bit hash (two independent 64-bit lanes) to a 256-bit country mask (241 groups fit), then OR-merge the masks; or one shared lock-free table. Count per country by scanning the table once.
5. Aggregates per thread in cache-line-aligned structs, merged in a fixed order; format once.
**Memory plan:** lookup 1 MB; name tables a few MB each; verify the distinct-name count on the data before choosing a design (day-0 measurement).
**Targets:** T 0.12–0.25 s, M 20–60 MB, S 200–300 lines.

**Experiment queue.** C1 allocation-free state machine; C2 intern names by hash once; C3 quote-parity parallel parse; C4 name table design (per-thread masks vs shared table); C5 SIMD quote scanning; C6 explicit exit.

**Verification.** `tiny`/`sample`/full hashes (full `aggregates.txt` sha256 begins `d8088b1c`); hostile fixtures from `tiny`; determinism ×5.

---

## 8. Verification protocol
Every change passes these levels in order; the owner runs L0–L4, the reviewer re-runs L1–L4 independently before merge.
- **L0** builds with the pinned toolchain, no warnings that hide bugs.
- **L1** `tiny` hash equals the reference's (every edge case from the spec appears at every scale).
- **L2** `sample` hash equals the reference's.
- **L3** full-data hash equals the spec's published sha256 (seed 1).
- **L4** determinism: five consecutive runs give identical output; thread counts 1, 2, 6, 12 give identical output; fuzz seeds 2–50 on `tiny`/`sample` match the Python reference byte for byte.
- **L5 (Role P)** `check.py` in the allowlisted image with the board's Docker settings; build under 10 minutes and under 2 GB.
- **L6** board run: hash passes, T and M recorded in `runs_ledger.md`.
**Known-bad controls:** keep a deliberately broken variant per track (for example integer ceil replaced by float ceil, or a tokenizer that treats bytes ≥ 0x80 as letters) and confirm the checks fail on it, so we know the checks can fail.
**Debug ladder when the board hash fails but local passes:** (1) same data size? the board uses full seed 1; (2) thread-count dependence; (3) uninitialised or reused buffers across queries/threads; (4) output not fully flushed before exit; (5) panic on an input shape that only appears in the full data; (6) a different build flag.
**Reviewer checklist:** contract read against the spec; integer widths; ordering; empty and tiny inputs; every `unwrap`/index justified; no HashMap iteration in output; no hard-coded constants; comments explain the tricks; no dependencies; no files outside `/out`.

---

## 9. Board and experiment workflow
**Facts.** The board polls `speed-demon` every 60 s; one dispatch per 15 minutes per team across all tracks (at most about 288 runs in 72 hours), one job in flight, a newer push replaces a queued one. One push builds and runs **every** `track-*` folder that differs from the starter, serially, one timed run each (cap 3× reference). Builds run on an off-box builder; a track that asks for the native CPU is built on the scoring box. Every attempt, including failures, is recorded with run ID, gates, T, M, S, D, K and a failure tail; judges see our run history.
**Procedure.** (1) Local screen with `hyperfine` and the hash. (2) Post `BOARD: <track> <sha> <hypothesis>`. (3) Role P pushes the candidate to `speed-demon` (to run only one track, `scripts/board_push.sh` resets the other track folders to the starter on a throwaway commit). (4) Record the run in `runs_ledger.md` (`run ID | time | track | commit | hypothesis | T | M | K | kept/reverted`). (5) Decide with the break-even table. (6) PERF.md row.
**Noise rule.** The board is a single run; with a 50 ms spread floor, gains under about 10% can reverse on re-run. Claim a gain only if it is clear on the board and consistent with local timing, or cite two runs. Measure board noise once by re-running identical code (an empty commit; check that it queues).
**Slot budget.** Reserve about 25% of slots for emergencies and the final verification; about one run per hour per active track.
**Final push.** The head of `speed-demon` at freeze is what is scored (assumed; FAQ: "whatever is at your recorded commit"); it must contain every entered track in its final state.

---

## 10. Working with coding agents
Rule 6 allows and expects AI tools; judges check that a human understood what happened.
- **Never assume an agent is available.** Before each coding session state which agents and limits are available, then choose; no task is tied to a particular agent or model. Task cards name a *suggested* agent, model and effort that you may replace.
- **Classes:** a strong-reasoning agent for design, state machines, graph and index logic, adversarial review and performance analysis; a fast, cheap agent for boilerplate, tests, scripts and formatting. Spend quota on hard parts.
- **Four roles:** spec analyst (no code; cites the spec), implementer (one scoped change), adversarial reviewer (reads the diff, proposes concrete failing inputs, never edits), performance analyst (reads measured numbers, proposes at most three ranked experiments). No agent grades its own work.
- **Task card:** `Suggested agent / model / effort (confirm availability)` · `Track / owner / working tree` · `Goal in one sentence` · `Inputs: the spec section, the files, the failing case` · `Constraints: the relevant guardrail ids` · `Deliverable: files changed, tests run, output of the hash commands` · `Out of scope` · `Verification commands`.
- **Human responsibilities:** re-run the hash yourself, read the diff, be able to explain every line and every arithmetic choice, run the measurements (agents do not run the board).
- **Quota discipline:** feed agents only the spec section, the diff, the failing case and the profile; do not paste whole logs or the whole repo; one agent per working tree.

---

## 11. GUARDRAILS (hard rules; a violation blocks the merge or the push)
### 11.1 Event rules (disqualification or zero)
- **GR-E1** No solution code before kickoff, inside or outside track folders (including "test helpers" that are really solution logic). Nothing in a `track-*` folder changes before Fri 30 Oct 20:30 IST.
- **GR-E2** No precomputed answers, no cached outputs, no code that recognises the input and short-circuits, no sample hashes or data-derived constants in code (Rule 4: disqualification from the event).
- **GR-E3** Read only from `/data`; write exactly one file to `/out` with the contract name; nothing else on disk, not even in `/tmp`; no network; no spawning processes; exit code 0.
- **GR-E4** Every `FROM` is an allowlisted image pinned by digest; nothing else enters the image; no `ADD` of URLs, no `# syntax=`; builds offline, ≤ 10 minutes, ≤ 2 GB.
- **GR-E5** Zero dependencies and no vendored code (any `libc::` call, `rayon`, `memchr`, `ahash`, … is a dependency, ×0.901 each). Approval needs both people and a measured gain of ≥ 23.1% in T.
- **GR-E6** No text addressed to an AI or a judge inside the repo (no prompt injection); the repo is public under an OSI licence at freeze; PERF.md is truthful.
- **GR-E7** The first commit touching a `track-*` folder is after kickoff; the repo contains no organizer secrets or other teams' code.

### 11.2 Correctness
- **GR-C1** The spec is the contract. When this plan, an AI draft or a "popular trick" disagrees with the spec, the spec wins.
- **GR-C2** Integers only. No `f32`/`f64` in any output path. Rank, ppm, money and distances use integer arithmetic with explicit widths (`u64`/`i64` wherever 32 bits could overflow).
- **GR-C3** Sort orders are bytewise and total, exactly as specified; never rely on locale, `HashMap` order, thread order or unstable sort order for equal keys that appear in the output.
- **GR-C4** Determinism: five identical runs, any thread count, identical bytes. No timing- or address-dependent behaviour; no uninitialised or reused state across queries or threads.
- **GR-C5** No panics on valid input: no unchecked arithmetic assumptions, no `unwrap()` on a parse path without a proof, no index out of range on empty or tiny inputs. A panic aborts the process and fails the gate.
- **GR-C6** No merge and no board push without L0–L4 on the exact commit. Agent-claimed passes count for nothing until a human re-runs the hash.
- **GR-C7** Any change that can alter output (parsing, ordering, arithmetic, formatting) re-runs `tiny`, `sample`, full and the fuzz seeds.
- **GR-C8** Flush and close the output before exiting; the file must end with the required LF.
- **GR-C9** Never hard-code seed-dependent data (endpoint names, vocabulary, counts); only constants the spec guarantees for every seed.

### 11.3 Performance and measurement
- **GR-P1** Measure before optimising. One change per experiment. Revert anything that does not clearly help (see the break-even table).
- **GR-P2** Baseline correct before threads, `unsafe`, SIMD, `mmap`, custom hashers or PGO.
- **GR-P3** No `native`, `x86-64-v4` or AVX-512; default target `x86-64-v3`; `raptorlake` only as a measured experiment.
- **GR-P4** At most 12 threads; no thread oversubscription.
- **GR-P5** No scratch files in `/tmp`; no unbounded buffers; no whole-input buffers in the heap; M must be stable across runs.
- **GR-P6** Never OR madvise advice constants; never `MAP_PRIVATE` with write access.
- **GR-P7** Never pack a distance and a node into one `u64`; never use `u32` distances (Track D).
- **GR-P8** Local timings are only comparable on the same machine; the board is the truth. No benchmarking on battery, no heavy background work during a measurement.
- **GR-P9** Do not tune to the board's seed-1 data (holdout is a private seed); prefer contract-level properties.
- **GR-P10** No timing or measurement code inside the submitted binary.

### 11.4 Git, board and process
- **GR-G1** `speed-demon` is the board branch: no WIP and no experiments pushed to it; only deliberate candidates, announced in chat first (judges read the run history).
- **GR-G2** Work happens on branches `a/*`, `b/*`, `c/*`, `d/*`; merge to `speed-demon` by pull request reviewed by the other person, by reading the diff.
- **GR-G3** Never force-push `speed-demon`; never rewrite published history.
- **GR-G4** One owner per track folder; write only in your own folders; `scripts/` is Role P's, `fixtures/` is Role C's; tooling is never copied into a track folder; tests never live inside `track-*` (they would count in S).
- **GR-G5** Every board run goes into `runs_ledger.md` the minute it finishes; every kept or reverted experiment goes into PERF.md with its run ID.
- **GR-G6** `.gitattributes` forces LF; test data is binary. Never commit generated datasets or outputs.
- **GR-G7** The repo is private until G7; the owner flips it public at G7 and checks it from a logged-out browser.
- **GR-G8** After G6 (feature freeze) only fixes with a test; after G8 nothing without a second review.

### 11.5 Agents
- **GR-A1** Agents never push to `speed-demon` and never touch another person's track folder.
- **GR-A2** Agents never add dependencies, `build.sh`, extra files in track folders, or `unsafe` outside the section 6 allowance.
- **GR-A3** Every agent change is one scoped task with a task card; unscoped "make it faster" requests are not allowed without a profile.
- **GR-A4** No secrets, tokens or personal data in prompts or the repo.
- **GR-A5** Every claim an agent makes about the spec is checked against the spec text; every claimed speedup against a measurement.

### 11.6 Stop rules
- Baseline not correct at its gate → stop optimising that track until it is.
- Behind schedule → follow the cut order in section 12; never cut correctness checks, determinism, the clean-room rehearsal or PERF.md/README.
- Anything uncertain about the rules that could disqualify us → ask the hosts only if it cannot be settled from the specs, the public repos or a safe assumption (none pending).

---

## 12. Risks and cut order
| Risk | Warning sign | Response |
|---|---|---|
| Hash mismatch on B or C | one edge case diverges | `tiny` contains every edge case; row-by-row diff against the reference; scalar first |
| Plausible-but-wrong output | tests pass, hash fails or flickers | differential fuzz against the Python reference; known-bad controls; ×5 determinism |
| M spike in one of five runs | peak varies | bounded buffers; stable allocation pattern; watch the board's M across runs |
| Build fails offline | `cargo` needs network, build > 10 minutes | std only, `--offline`; clean-room rehearsal at H18 and H66 |
| Illegal instruction | AVX-512 or `native` | `x86-64-v3` only |
| Board slot wasted | multi-track pushes, WIP on `speed-demon` | push policy, `board_push.sh`, reserved slots |
| Noise misleads PERF.md | gain under 10% reverses | noise rule, two runs |
| Stale PGO profile | profile from old source | generate last, from the final source |
| Windows artefacts | CRLF, wrong flags | `.gitattributes`; Linux for Rust work |
| Owner unreachable at freeze | repo still private | flip public at G7 |
| Agent code wrong | claims without logs | reviewer role; human re-runs hashes |
| Third counted track slips | neither B nor C correct by H40 | B-or-C rule; all hands on the closer one |
| Quota exhausted | limits hit | know limits first; cheap agents for boilerplate |
**Cut order when behind:** (1) late C optimisation (C is the likely dropped track) → (2) B if not yet correct → (3) D tuning → (4) late A experiments (SIMD, PGO, raptorlake) → (5) extra sweeps. Emergency: revert to the last passing tag and get a second person to review.

---

## 13. Release checklists
**G5 (Mon 06:30):** all experiments logged; `static_axes.py` run (S lines per track, D = 1); five-run determinism and memory stability seen on the board; PGO profile (if used) generated from the final source with the `halloumee` image and committed as data.
**G6 (Mon 08:30):** feature freeze announced in chat.
**G7 (Mon 14:30):** fresh clone in an empty directory; for each track: `docker build --network=none` with the allowlisted images, run, hash; README and PERF.md complete (every row has a run ID); `.speed-demon.toml` has `team`, language and `dependency_count = 0`; LICENSE (MIT) present; no tests or scripts inside `track-*`; `git status` clean; **repo public** and visible when logged out.
**G8 (Mon 18:30):** tag `rc-1` on `speed-demon`; verify the remote head equals the tag; board run of the final head passes for every entered track.
**Freeze (Mon 20:30):** nothing is pushed in the last 30 minutes; both people verify the head SHA and the public repo.

---

## 14. Decisions and open items
| ID | Decision | Status |
|---|---|---|
| D-01 | Team of two; Rust std only | decided |
| D-04 | Counted trio A, B, D; C fallback; B-or-C rule | decided |
| D-05 | Roles P and C as in section 2 | decided |
| D-11 | Personal repo (Role C) + collaborator (Role P); no organization | decided |
| D-12 | Board branch `speed-demon`, announced pushes | decided |
| D-15 | Build image `rust:1-slim-trixie` (rustc 1.99.1); PGO image `halloumee/speed-demon-rust` | decided |
| D-19 | Register all four tracks; delete folders we do not build | proposed |
| D-20 | Default `x86-64-v3`; `raptorlake` only as an experiment | proposed |
| D-06 | Linux environment for Role C (WSL2 or alternative) | **open** |
| D-22 | Rust 1.99.1 on Role C's machine | **open** |
| D-23 | Agents available and limits; sleep blocks | **open (Thu 29)** |
**Assumptions to confirm at the first board runs:** the scored commit is the head of `speed-demon` at freeze; a failed-hash run is recorded (T and M may not show); an empty commit queues a run; the registration `tracks` list does not block later folder changes (hence all four registered).

---

## Appendix A — Rules recap
Rule 1 any language that builds offline in a Linux container and runs on x86-64. Rule 2 dependencies allowed and priced; banned libraries per track (A and C: analytics/dataframe/SQL engines; B: search engines/IR libraries/full-text indexes; D: shortest-path libraries and CH/hub-label builders). Rule 3 correct output or no score. Rule 4 no precomputed answers. Rule 5 new code only: all solution code written in the 72-hour window. Rule 6 AI tools expected; PERF.md must show a human understood. Rule 7 team size 1–4. Rule 8 source public at freeze under an OSI licence (private until then; the Board App needs read access). Rule 9 the harness is the referee (48-hour re-run request window). Rule 10 the formula does not change after kickoff. Gates (every run): correctness (sha256), timeout (10× reference), sandbox (no network, read-only dataset and root, 1 GB tmpfs `/tmp`, filesystem diff; precomputed answers or touching the dataset mount disqualify from the event). Machine: Hetzner, Falkenstein, i5-13500, turbo off, 12 threads on 6 P-cores (`--cpuset-cpus=0-11`), 8 GB, no swap, no network, `--read-only`, `--pids-limit=512`, `io_uring` blocked (`mmap` works).

## Appendix B — Output contract quick reference
| Track | Input files | Output file | One line per | Sorted by |
|---|---|---|---|---|
| A | `requests.log` | `summary.txt` | non-empty (hour, endpoint) group | hour, endpoint bytes |
| B | `docs.txt`, `queries.txt` | `results.txt` | query | query order |
| C | `records.csv`, `regions.csv` | `aggregates.txt` | country with ≥ 1 record (`_UNMATCHED` last) | country bytes |
| D | `graph.txt`, `queries.txt` | `distances.txt` | query | query order |
Common: UTF-8, LF after every line, no BOM, no trailing spaces, integers only, single-space field separators.

## Appendix C — Claims in AI-generated drafts that are wrong or unsafe (do not follow)
- **Track D drafts:** the input file is `graph.txt` (not `edges.txt`); distances are `u64`, so `cost << 32 | node` packing and `u32` distance arrays are wrong; bidirectional search on this road-like graph saves about half of the settlements, not a square root; "queries cost ~22 ms in total" ignores that the tail octaves hold about 75% of the work; stopping bidirectional Dijkstra at the first node settled by both sides is wrong.
- **Track C drafts:** `region_id` is up to 999,999 in the lookup and 9,999,999 in records, and countries are three-letter codes (a 65,536-entry table of two-letter codes is wrong); per-thread, per-country `u128` sets can cost tens of MB; hashing the raw non-empty name bytes avoids a streaming unescaper; 64-bit hashes are acceptable only if the distinct-name pool is small (verify), 128-bit is cheap insurance.
- **Track A drafts:** `MADV_WILLNEED | MADV_HUGEPAGE` equals `MADV_NOHUGEPAGE`; hashing a 13-byte hour string per row is slower than an integer hour index; per-thread tables with 40-byte entries and per-thread node arenas can reach 70–90 MB; memory estimates assumed a 420 MB reference (the box's is 228 MB); projected times of 40 ms are optimistic.
- **Track B drafts:** query-term ids do not fit in 16 bits; a bitset threshold of 1,024 documents costs more memory than lists (break-even ≈ 7.8k); "5–15 ms exit teardown" and "7 ns per token" are guesses; searching raw substrings in a candidate document must respect token boundaries and lowercasing.
- **Build drafts:** "committed `.profdata` is the only PGO route" is outdated (the `halloumee` image has llvm-tools); a byte-at-a-time multiply hasher is not 10 GB/s; `libc::_exit` needs the `libc` crate; 256 KiB thread stacks save nothing; `target-cpu=native` should not be used; `harness/run_submission.py` does not run on native Windows and `/mnt/c` I/O under WSL2 is slow.
- **Always check:** projected K and T values in drafts (for example K ≈ 30–38) are guesses; replace them with board numbers.

## Appendix D — Day-0 measurements (Role C, analysis scripts outside track folders)
- **A:** group-size distribution (counts of groups with n ≤ 3, 4–32, > 32; maximum), endpoint count and maximum length, latency range, per-hour line counts, confirm the 27-byte timestamps and the 72-hour span.
- **B:** document count and length distribution, distinct query terms (BOOL vs PHRASE), document-frequency distribution of query terms and the number above N/32, total phrase candidates after intersecting all phrase terms, empty documents, maximum result sizes.
- **C:** distinct names (and whether the empty name appears), maximum name and note lengths, share of quoted notes, records with embedded LF, BOM count, region id range, whether `names_distinct` counts the empty name (check the reference).
- **D:** in/out-degree distribution, nodes without in- or out-edges, zero-weight edges, maximum weight, query kinds (`s == t`, `-1`, answers above 2^32), and the total number of node settlements for a forward Dijkstra over the 2,000 queries (instrument the reference or a throwaway script).

## Appendix E — Useful commands (placeholders; see the organizer repo README for exact names)
Generate data: `python3 generators/<track>.py --seed 1 --scale {tiny,sample,full} --out <dir>`. Run the reference: `python3 reference/<track>.py --data <dir> --out <outdir>`. Hash: `sha256sum <file>`. Tests: `python3 -m unittest discover -s tests`. Build and run a track like the board does: `python3 check.py <track> --data <dir> [--expected <sha256>]` (Docker). Local timing: `hyperfine --warmup 3 '<command>'`. Source lines: run `scc` on a track folder after `rustfmt` (or `harness/static_axes.py`).
