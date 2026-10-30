#!/usr/bin/env python3
"""Append a board benchmark run entry to runs_ledger.md.

Implements PLAN.md section 9, step 4 & P-PRE-02 requirements:
  - Requires --run-id (official board run ID)
  - Requires -T/--t and -M/--m taken strictly from the board results
  - Never computes K from local numbers (K must come from the board)
  - Never uses Docker image size as M (M must be memory.peak from board)
  - Refuses execution if required board measurements are missing
  - Allows --dry-run with prominent [DRY RUN] label without modifying ledger

Usage:
    python scripts/bench_log.py --run-id 1402 --track a --hypothesis "A1 baseline" -T 0.285 -M 48MB -K 13.20 --verdict kept
    python scripts/bench_log.py --run-id 1403 --track d --hypothesis "parallel CSR" -T 0.720 -M 215MB -K 12.10 --dry-run

Format:
    | run ID | time | track | commit | hypothesis | T | M | K | kept/reverted |
"""
import argparse
import datetime
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_LEDGER = os.path.join(ROOT, "runs_ledger.md")
HEADER_ROW = "| run ID | time | track | commit | hypothesis | T | M | K | kept/reverted |\n"
DIVIDER_ROW = "|---|---|---|---|---|---|---|---|---|\n"


def get_current_git_commit():
    try:
        res = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True, check=True)
        return res.stdout.strip()
    except Exception:
        return "unknown"


def validate_board_memory(m_str):
    """Validate that M looks like a memory reading and not an image size."""
    s = str(m_str).strip()
    if "image" in s.lower():
        raise ValueError("M must be peak memory (memory.peak) from the board run, never Docker image size.")
    # Match number optionally followed by B, KB, MB, GB, MiB, etc.
    if not re.match(r"^[0-9.]+\s*(B|K|KB|KIB|M|MB|MIB|G|GB|GIB)?$", s, re.IGNORECASE):
        raise ValueError(f"Invalid peak memory format from board: {m_str!r}")
    return s


def main():
    parser = argparse.ArgumentParser(description="Log board run results to runs_ledger.md (P-PRE-02).")
    parser.add_argument("--run-id", help="Official board run ID (required for real logging)")
    parser.add_argument("--track", choices=["a", "b", "c", "d"], help="Track (a, b, c, d)")
    parser.add_argument("--hypothesis", help="Hypothesis / experiment description")
    parser.add_argument("-T", "--t", "--time-sec", dest="time_sec", type=float, help="Board wall time in seconds")
    parser.add_argument("-M", "--m", "--memory", dest="memory", help="Board peak memory (e.g. 48MB, 48234496). Never image size.")
    parser.add_argument("-K", "--k", dest="k_score", type=float, help="Official K score from the board report (never computed locally)")
    parser.add_argument("--commit", help="Candidate commit short SHA (defaults to git rev-parse --short HEAD)")
    parser.add_argument("--time", help="Timestamp (defaults to current UTC ISO-8601)")
    parser.add_argument("--verdict", default="kept", choices=["kept", "reverted", "pending"], help="verdict (kept/reverted/pending)")
    parser.add_argument("--ledger", default=DEFAULT_LEDGER, help="Path to runs_ledger.md")
    parser.add_argument("--dry-run", action="store_true", help="Preview the ledger entry without writing to file")

    args = parser.parse_args()

    # In non-dry-run mode, strictly require all board measurements
    missing = []
    if not args.run_id:
        missing.append("--run-id (board run ID)")
    if not args.track:
        missing.append("--track")
    if not args.hypothesis:
        missing.append("--hypothesis")
    if args.time_sec is None:
        missing.append("-T/--t (board wall time)")
    if args.memory is None:
        missing.append("-M/--m (board peak memory)")

    if missing:
        if args.dry_run:
            # Allow dry run with placeholders for quick testing
            print("====================================================================")
            print("[DRY RUN] Warning: Incomplete board arguments provided.")
            print(f"Missing board fields: {', '.join(missing)}")
            print("In non-dry-run mode, bench_log.py strictly requires all board fields.")
            print("====================================================================")
            run_id = args.run_id or "DRY-RUN-ID"
            track = (args.track or "a").upper()
            hypo = args.hypothesis or "dry run placeholder"
            t_str = f"{args.time_sec:.3f}s" if args.time_sec is not None else "0.000s"
            m_str = str(args.memory or "0MB")
        else:
            print(f"ERROR: Missing required board measurements: {', '.join(missing)}", file=sys.stderr)
            print("bench_log.py requires official board run results. Refusing to log incomplete run.", file=sys.stderr)
            sys.exit(1)
    else:
        run_id = args.run_id
        track = args.track.upper()
        hypo = args.hypothesis
        t_str = f"{args.time_sec:.3f}s"
        try:
            m_str = validate_board_memory(args.memory)
        except ValueError as err:
            print(f"ERROR: {err}", file=sys.stderr)
            sys.exit(1)

    commit = args.commit or get_current_git_commit()
    timestamp = args.time or datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    # K is strictly taken from the board (--k / -K). Never computed from local numbers.
    k_str = f"{args.k_score:.2f}" if args.k_score is not None else "N/A"
    safe_hypo = hypo.replace("|", "/")

    row = f"| {run_id} | {timestamp} | {track} | {commit} | {safe_hypo} | {t_str} | {m_str} | {k_str} | {args.verdict} |\n"

    if args.dry_run:
        print("[DRY RUN] Would append to ledger:")
        print(row, end="")
        return

    # Real logging to file
    ledger_path = os.path.abspath(args.ledger)
    if not os.path.exists(ledger_path) or os.path.getsize(ledger_path) == 0:
        with open(ledger_path, "w", encoding="utf-8", newline="\n") as f:
            f.write(HEADER_ROW)
            f.write(DIVIDER_ROW)

    with open(ledger_path, "a", encoding="utf-8", newline="\n") as f:
        f.write(row)

    print(f"Logged run {run_id} to {args.ledger}:")
    print(row.strip())


if __name__ == "__main__":
    main()
