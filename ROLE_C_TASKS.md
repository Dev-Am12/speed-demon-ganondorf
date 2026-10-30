# ROLE C — setup and tasks (pre-kickoff, then H0–H2)

**Version RC-1 · for the user (Role C) · agents available now: Gemini 3.8 Flash and GPT Luna 6 (about 100% credits each)**
Rules for everything below: tooling only (no solution code before kickoff, Fri 30 Oct 20:30 IST); nothing goes inside `track-*` folders; no dependencies; large data stays outside the repo. Priorities: **P0** = must be done before kickoff, **P1** = should be done, **P2** = nice to have. If kickoff is close, do the P0 items in order and skip the rest.

---

## Part 1 — Install WSL2 and Rust 1.99.1 (about 40 minutes, mostly downloads)

### 1A. WSL2 (Ubuntu)
1. **Check virtualization.** Task Manager → Performance → CPU → "Virtualization" must say *Enabled*. If it says Disabled, restart into the BIOS/UEFI and switch on "Intel Virtualization Technology" (VT-x), then boot Windows again.
2. **Install.** Open PowerShell **as Administrator** and run: `wsl --install -d Ubuntu-24.04` (if the name is rejected, run `wsl --list --online` and use the Ubuntu name it shows). Restart when asked.
3. **First start.** Open "Ubuntu 24.04" from the Start menu and create a Linux username and password (the password does not show while you type).
4. **Check.** In PowerShell: `wsl -l -v` → the distro must show `VERSION 2`. If an install error appears: code `0x80370102` = virtualization is off in the BIOS (step 1); "update the WSL kernel" = run `wsl --update` and repeat step 2.
5. **Optional memory cap** (recommended; WSL may otherwise take up to half your RAM). Create the file `C:\Users\<you>\.wslconfig` containing `[wsl2]` and a line `memory=6GB` (use about half of your RAM), then run `wsl --shutdown` and reopen Ubuntu.
6. **Work in the Linux home folder, never under `/mnt/c`** (that path is slow and breaks line endings). Everything below is typed inside the Ubuntu window.
7. **Base packages:** `sudo apt update && sudo apt -y upgrade`, then `sudo apt install -y build-essential git curl unzip python3 hyperfine`. Check `python3 --version` (3.11 or newer; Ubuntu 24.04 has 3.12).
8. **Git:** `git config --global user.name "<your name>"`, `git config --global user.email "<your email>"`, `git config --global core.autocrlf input`.
9. **SSH key for GitHub:** `ssh-keygen -t ed25519 -C "<your email>"` (press Enter three times), `cat ~/.ssh/id_ed25519.pub`, copy the line, add it at GitHub → Settings → SSH and GPG keys → New SSH key, then test with `ssh -T git@github.com` (type `yes` once; "Hi <name>!" means success).
10. **Clone both repos into the home folder:** `git clone git@github.com:<you>/<your-repo>.git ~/speed-demon` and `git clone https://github.com/hackathon-raptors/speed-demon-2026.git ~/sd-organizer`.
11. **`scc` (line counter):** download `scc_Linux_x86_64.tar.gz` from github.com/boyter/scc/releases, then `tar xzf scc_Linux_x86_64.tar.gz scc && sudo mv scc /usr/local/bin/ && scc --version`.
12. **Disk check:** `df -h ~` (expect to use about 4–6 GB in total, including data later).
13. **Editing:** from the repo folder run `code .` if VS Code with the "WSL" extension is installed, or open `\\wsl$\Ubuntu-24.04\home\<user>\speed-demon` from Windows tools.
Undo, if ever needed: `wsl --unregister Ubuntu-24.04` deletes the distro and its disk.

### 1B. Rust 1.99.1 inside WSL (clean, minimal)
1. In the Ubuntu window: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.99.1` (the `minimal` profile installs only the compiler, cargo and the standard library, about 300–400 MB, and pins 1.99.1 instead of "stable").
2. Load it into this session: `. "$HOME/.cargo/env"` (new Ubuntu windows load it automatically).
3. Add the formatter (needed to estimate source size the way the harness does): `rustup component add rustfmt`.
4. Verify: `rustc --version` must print `1.99.1`; `rustup toolchain list` must list only `1.99.1`.
5. Smoke test (throwaway, outside the repo): `cargo new ~/scratch/hello && cd ~/scratch/hello && cargo run --release`, then `cd ~ && rm -rf ~/scratch`.
6. Never run `rustup update` or install `stable` during the event (the board compiles with 1.99.1; a newer compiler could build here and fail there). Skip `llvm-tools`: your teammate generates PGO profiles in Docker.

### 1C. Clean up the old Windows Rust toolchains (optional, saves 1–2 GB each)
WSL's Rust is a separate install, so removing the Windows ones does not affect it.
1. PowerShell (normal): `rustup toolchain list`. Your two entries will look like `1.97.1-x86_64-pc-windows-msvc` and `stable-x86_64-pc-windows-msvc` (use the exact names shown).
2. If you need no Rust on Windows for other projects: `rustup toolchain uninstall <exact name>` for each, or remove everything with `rustup self uninstall`.
3. If you want to keep Windows Rust for other projects: keep only `stable-...` (uninstall the duplicate 1.97.1) and run `rustup update` after the event.

---

## Part 2 — Task list (in order)
| # | Task | Who | Agent (suggested; any equivalent works) | Time | Priority |
|---|---|---|---|---|---|
| R0 | Commit `PLAN.md` and `.gitattributes` to the repo (nothing under `track-*`); push; confirm the teammate has write access and `team` is set in each `.speed-demon.toml` | you | none | 15 min | P0 |
| R1 | Part 1A + 1B above | you | none | 40 min | P0 |
| R2 | Read the organizer repo `README.md`, `CONVENTIONS.md`, `harness/README.md`, and specs B and C in full (A and D skim): write 5 lines per track on what could break you | you | none | 90 min | P0 |
| R3 | Generate `tiny` and `sample` seed-1 data for A, B, C, D into `~/sd-data/` and run the Python references; compare the output hashes with the tables in the specs | you + agent | Flash, low–medium | 45 min | P0 |
| R4 | Fixtures tooling: generate, run reference, compare, benchmark wrapper | agent | Flash, low–medium | 30 min | P0 |
| R5 | Generate `full` data; run references (about 5 s to 60 s each plus the reference run); confirm the full hashes (A `495ca096…`, B `f86973c2…`, C `d8088b1c…`, D `a23dbdf0…`) | you + agent | Flash, low | 30 min | P0 |
| R6 | Differential fuzz runner (compares a binary against the references over many seeds) | agent | Flash, medium | 30 min | P1 |
| R7 | Edge-case inventory per track (where each spec edge case appears in `tiny`) | agent | GPT Luna 6, medium | 40 min | P1 |
| R8 | Day-0 measurements (PLAN.md Appendix D) | agent | Flash for A, C, D; Luna for B | 60 min | P1 |
| R9 | Hostile micro-datasets for B and C (hand-made inputs; expected outputs come from the reference) | agent | GPT Luna 6, medium–high | 45 min | P1 |
| R10 | Meet your teammate: gates, board slot ledger, sleep blocks, how each person pings the other | both | none | 30 min | P0 |
| R11 | When the 10% samples are published (Fri 11:30 IST): confirm they match your generated `sample` data/hashes | you | none | 10 min | P1 |
| R12 | Readiness check (Part 4) | you | none | 15 min | P0 |

---

## Part 3 — Paste-ready agent task cards
**Start every agent session with this context block, then paste one card.**
```
CONTEXT (read first). I am Role C in a two-person team in the Speed Demon 2026 hackathon (public
organizer repo cloned at ~/sd-organizer; our repo at ~/speed-demon). We will write Rust (std only,
no dependencies) solutions for four tracks (A ingest, B index, C join, D traverse); correctness is a
SHA-256 match against Python references. TODAY IS TOOLING ONLY: no solution code, nothing inside any
track-* folder, nothing that ports an algorithm into Rust. Put tooling in ~/speed-demon/fixtures/
(small text/scripts only) and large data in ~/sd-data/ (outside the repo, never committed). Use
bash and Python 3 standard library only, no pip packages, no network. Before writing anything read:
~/sd-organizer/README.md, CONVENTIONS.md, specs/*.md, harness/README.md. Do not guess CLI names or
file names; read them. Output rules from CONVENTIONS apply (LF, no BOM, integers only). When done,
list exactly which commands I should run to verify, and what output I should see. I will run them
myself; do not claim anything passes unless you ran it.
```

**Card R3 + R4 + R5 — fixtures tooling and oracle hashes**
```
Suggested agent: Gemini 3.8 Flash | Effort: low-medium (confirm availability; GPT Luna 6 also fine)
Task: write ~/speed-demon/fixtures/ scripts:
 1. gen_data.sh <track a|b|c|d> <seed> <scale tiny|sample|full> -> generates into ~/sd-data/<scale>/<seed>/<track>/
    using the organizer generators exactly as their README documents.
 2. run_ref.sh <track> <datadir> <outdir> -> runs the Python reference, prints the sha256 of the single output file.
 3. hashcmp.sh <file> <expected_sha256> -> prints OK or MISMATCH with both hashes.
 4. expected_hashes.md -> a table of the published sha256 values from the specs (seed 1, full) and, once
    generated, the hashes of our tiny/sample outputs (leave blank rows to fill).
 5. bench.sh <binary_a> <binary_b> <datadir> -> hyperfine comparison with --warmup 3, and a sha256
    check of each binary's output (binaries read /data and write /out; document how to point them
    at a dataset with sudo ln -sfn; do not require Docker).
Then run gen_data.sh for all four tracks at tiny and sample (seed 1) and run_ref.sh, and report the hashes.
Verification I will do: re-run gen+ref for track A sample and compare with your table.
```

**Card R6 — differential fuzz runner**
```
Suggested agent: Gemini 3.8 Flash | Effort: medium
Task: write ~/speed-demon/fixtures/fuzz.sh <track> <candidate_binary> <first_seed> <last_seed> <scale tiny|sample>.
For each seed: generate data (reuse gen_data.sh) into a temp dir under ~/sd-data/fuzz/, run the Python
reference to get the expected output hash, point /data at the dataset and /out at a fresh empty dir
(sudo ln -sfn; the candidate binary reads /data and writes one file in /out), run the candidate, compare
sha256. Stop at the first failing seed and print: seed, both hashes, and the first 20 differing lines.
Also support --repeat N to run the candidate N times per seed and require identical hashes (determinism),
and --threads-env to pass a thread-count environment variable if the candidate honours one (optional).
Test the script with a trivial stand-in "candidate" that just copies the reference's own output
(a shell script that runs the reference) to prove the pass path, and with one that corrupts a byte to
prove the fail path. Do NOT implement any track algorithm.
```

**Card R7 — edge-case inventory**
```
Suggested agent: GPT Luna 6 | Effort: medium (confirm availability)
Task: for each track, read its spec's edge-case list. Using the tiny seed-1 data (and sample if needed),
write ~/speed-demon/fixtures/edge_cases/<track>.md listing each spec edge case, the exact line number or
record id in the input where it appears, what the correct output must contain for it (taken from the
REFERENCE output, not from your own reasoning), and why a naive implementation might get it wrong.
Also write fixtures/edge_cases/show.sh <track> <n> that prints the n-th edge case's input lines.
Do not write any solution logic; use only grep/sed/awk/python to locate lines.
I will verify three random entries per track against the spec.
```

**Card R8 — day-0 measurements (analysis only)**
```
Suggested agent: Gemini 3.8 Flash for tracks A, C, D; GPT Luna 6 for track B | Effort: low-medium
Task: write ~/speed-demon/fixtures/analysis/<track>.py (Python stdlib only) that prints a markdown report of
the measurements listed in PLAN.md Appendix D for that track on the FULL seed-1 data:
 A: group-size distribution (groups with n<=3, 4-32, >32; max), endpoint count and max length, latency range, lines per hour, check that all timestamps are 27 bytes and inside the 72-hour span.
 B: document count/length distribution, distinct query terms (BOOL vs PHRASE), df distribution of query terms (count above N/32=7812), total phrase candidates after intersecting all terms of each phrase, empty docs, max result sizes.
 C: distinct names (does the empty name appear?), max name/note length, share of quoted notes, records with embedded LF, BOM count, region id range.
 D: in/out-degree distribution, nodes without in or out edges, zero-weight edges, max weight, query kinds (s==t, -1 answers, answers above 2^32), and the total number of settled nodes a forward Dijkstra needs over the 2,000 queries (instrument a throwaway copy of the reference).
These are MEASUREMENT scripts: keep them in fixtures/analysis, never copy their logic into Rust. Save each report to fixtures/analysis/out/<track>.md (small). Keep each run under about 5 minutes.
```

**Card R9 — hostile micro-datasets**
```
Suggested agent: GPT Luna 6 | Effort: medium-high
Task: create ~/speed-demon/fixtures/custom/b/ and fixtures/custom/c/ with 8-12 tiny hand-written datasets each
(docs.txt+queries.txt for B; records.csv+regions.csv for C) that probe the spec's tricky rules:
 B: bytes >= 0x80 as separators, 007 vs 7, uppercase letters, empty docs, phrases across sentence boundaries, repeated terms in a phrase, lowercase and/or/not as terms, NOT chains, results with more than 10 matches, results with 0 matches, duplicate queries.
 C: BOM at file start / after a record / inside quotes, CRLF and LF mixed, LF inside quotes, "" escapes, empty vs "" names, names differing by trailing space, region ids at 999999 and above, unmatched regions, negative amounts and -0.05 style formatting, a final record with CRLF.
For each dataset write a short README line saying what it probes. Run the Python reference on each (via run_ref.sh) and store the expected output hash next to it. Do not write any solution logic.
```

---

## Part 4 — Definition of ready (R12; all must be true before kickoff)
- [ ] `rustc --version` prints 1.99.1 inside WSL; `rustup toolchain list` shows only 1.99.1; `hyperfine --version` and `scc --version` work
- [ ] `git push` works from WSL to your repo; teammate can push too; `speed-demon` is the default branch
- [ ] `PLAN.md` and `.gitattributes` are committed; no `track-*` file changed
- [ ] `~/sd-data/` has tiny, sample and full seed-1 data for A, B, C, D; reference hashes match the spec tables
- [ ] `fixtures/` scripts run (gen, ref, compare, bench; fuzz if done); you have read each script, not only the agent's summary
- [ ] You can state, from memory, the output file name and the first three traps of tracks B and C (PLAN.md section 7, Appendix C)
- [ ] You and your teammate agreed on sleep blocks and the `BOARD:` message format
- [ ] Phone/laptop charged, plugged in, power plan set to high performance for measurements

## Part 5 — H0–H2 checklist (Fri 30 Oct 20:30–22:30 IST)
1. `git pull`; create branch `b/v0` from `speed-demon`.
2. Convert `track-b/` per PLAN.md section 5: remove the Python reference files, add `Cargo.toml`, the ≈ 7-line Dockerfile, set `team` in `.speed-demon.toml`. Commit on the branch; merge only when Role P and you agree (G0).
3. Run `gen_data.sh` for the kickoff full datasets if they differ from your seed-1 copies; verify the hashes again.
4. Hand the B contract (PLAN.md section 7.2 and spec B) to a spec-analyst agent (no code) and read its summary against the spec yourself.
5. Start B v0 with an implementer card (next message: ready-made cards for B v0), one scoped task at a time; run `tiny` then `sample` hashes after each.
6. Post `BOARD:` messages only for deliberate candidates; log every run in `runs_ledger.md`.
