#!/usr/bin/env python3
"""Append a benchmark / board run entry to runs_ledger.md.

Usage:
    python scripts/bench_log.py --run-id 1234 --track a --hypothesis "v0 baseline" -T 0.35 -M 48MB -K 12.5 --verdict kept
    python scripts/bench_log.py --run-id 1235 --track d --hypothesis "parallel CSR" -T 0.85 -M 210MB --verdict pending

Format:
    | run ID | time | track | commit | hypothesis | T | M | K | kept/reverted |
"""
import argparse
import datetime
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_LEDGER = os.path.join(ROOT, "runs_ledger.md")
HEADER_ROW = "| run ID | time | track | commit | hypothesis | T | M | K | kept/reverted |\n"
DIVIDER_ROW = "|---|---|---|---|---|---|---|---|---|\n"

# Import k_calc if K calculation is needed
sys.path.insert(0, os.path.join(ROOT, "scripts"))
try:
    import k_calc
except ImportError:
    k_calc = None


def get_current_git_commit():
    try:
        res = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True, check=True)
        return res.stdout.strip()
    except Exception:
        return "unknown"


def main():
    parser = argparse.ArgumentParser(description="Log benchmark or board run results to runs_ledger.md")
    parser.add_argument("--run-id", required=True, help="Board run ID or local benchmark ID")
    parser.add_argument("--track", required=True, choices=["a", "b", "c", "d"], help="Track (a, b, c, d)")
    parser.add_argument("--hypothesis", required=True, help="Hypothesis / experiment description")
    parser.add_argument("-T", "--time-sec", type=float, required=True, help="Execution wall time in seconds")
    parser.add_argument("-M", "--memory", required=True, help="Peak memory (e.g. 48MB, 50331648)")
    parser.add_argument("-K", type=float, help="K score (if omitted and k_calc available, computed automatically)")
    parser.add_argument("-S", "--source", type=int, help="Source lines if computing K automatically")
    parser.add_argument("-D", "--deps", type=int, default=1, help="Direct dependency weight if computing K")
    parser.add_argument("--commit", help="Commit short SHA (defaults to git rev-parse --short HEAD)")
    parser.add_argument("--time", help="Timestamp (defaults to current UTC ISO-8601)")
    parser.add_argument("--verdict", default="kept", choices=["kept", "reverted", "pending"], help="verdict (kept/reverted/pending)")
    parser.add_argument("--ledger", default=DEFAULT_LEDGER, help="Path to runs_ledger.md")
    parser.add_argument("--dry-run", action="store_true", help="Print the row without modifying the file")

    args = parser.parse_args()

    commit = args.commit or get_current_git_commit()
    timestamp = args.time or datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    k_val = args.K
    if k_val is None and k_calc is not None:
        try:
            m_bytes = k_calc.parse_memory_bytes(args.memory)
            ref = k_calc.REFERENCE_AXES.get(args.track, {})
            t_ref = ref.get("T_ref")
            m_ref = ref.get("M_ref")
            s_ref = ref.get("S_ref")
            d_ref = ref.get("D_ref", 1)
            s_val = args.source if args.source is not None else s_ref
            d_val = args.deps
            k_val, _, _, _, _ = k_calc.calc_k(args.time_sec, m_bytes, s_val, d_val, t_ref, m_ref, s_ref, d_ref)
        except Exception:
            k_val = None

    k_str = f"{k_val:.2f}" if k_val is not None else "N/A"
    t_str = f"{args.time_sec:.3f}s"
    m_str = str(args.memory)

    # Format row with escaped pipe symbols if present in hypothesis
    safe_hypo = args.hypothesis.replace("|", "/")
    row = f"| {args.run_id} | {timestamp} | {args.track.upper()} | {commit} | {safe_hypo} | {t_str} | {m_str} | {k_str} | {args.verdict} |\n"

    if args.dry_run:
        print(f"DRY RUN (would append to {args.ledger}):")
        print(row, end="")
        return

    # Ensure ledger exists with header
    ledger_path = os.path.abspath(args.ledger)
    if not os.path.exists(ledger_path) or os.path.getsize(ledger_path) == 0:
        with open(ledger_path, "w", encoding="utf-8", newline="\n") as f:
            f.write(HEADER_ROW)
            f.write(DIVIDER_ROW)

    with open(ledger_path, "a", encoding="utf-8", newline="\n") as f:
        f.write(row)

    print(f"Logged to {args.ledger}:")
    print(row.strip())


if __name__ == "__main__":
    main()
