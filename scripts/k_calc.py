#!/usr/bin/env python3
"""Calculate efficiency coefficient K and speed score for Speed Demon 2026.

Formula:
    K = (T_ref / T)^0.50 * min(M_ref / M, 50)^0.20 * min(S_ref / S, 1)^0.15 * min(D_ref / D, 1)^0.15
    Speed Score = clamp(50 * log10(K), 0, 100)

Usage:
    python scripts/k_calc.py --track a -T 0.25 -M 50MB -S 150 -D 1
    python scripts/k_calc.py --track d -T 0.50 -M 200MB -S 220
    python scripts/k_calc.py --table
    python scripts/k_calc.py --selftest
"""
import argparse
import math
import re
import sys

# Reference axes from organizer calibration-seed1.json and reference_axes.json
REFERENCE_AXES = {
    "a": {
        "T_ref": 32.047276,
        "M_ref": 227983360,     # ~227.98 MB (217.42 MiB)
        "S_ref": 56,
        "D_ref": 1,
    },
    "b": {
        "T_ref": 56.352376,
        "M_ref": 2199961600,    # ~2199.96 MB
        "S_ref": 91,
        "D_ref": 1,
    },
    "c": {
        "T_ref": 14.444059,
        "M_ref": 367435776,     # ~367.44 MB
        "S_ref": 108,
        "D_ref": 1,
    },
    "d": {
        "T_ref": 72.692655,
        "M_ref": 892915712,     # ~892.92 MB (851.55 MiB)
        "S_ref": 43,
        "D_ref": 1,
    },
}

EXPONENTS = {"T": 0.50, "M": 0.20, "S": 0.15, "D": 0.15}
CAPS = {"T": None, "M": 50.0, "S": 1.0, "D": 1.0}


def parse_memory_bytes(val_str):
    """Parse string representation of memory (e.g. '50MB', '200M', '52428800') to bytes."""
    if isinstance(val_str, (int, float)):
        return float(val_str)
    s = str(val_str).strip().upper()
    match = re.match(r"^([0-9.]+)\s*([A-Z]*)$", s)
    if not match:
        raise ValueError(f"Cannot parse memory value: {val_str}")
    num = float(match.group(1))
    unit = match.group(2)
    if unit in ("", "B"):
        return num
    elif unit in ("K", "KB", "KIB"):
        return num * 1024
    elif unit in ("M", "MB"):
        return num * 1e6
    elif unit in ("MIB",):
        return num * (1024 ** 2)
    elif unit in ("G", "GB"):
        return num * 1e9
    elif unit in ("GIB",):
        return num * (1024 ** 3)
    else:
        raise ValueError(f"Unknown memory unit: {unit}")


def calc_k(t, m, s, d, t_ref, m_ref, s_ref, d_ref):
    """Compute K with caps applied."""
    if t <= 0 or m <= 0:
        raise ValueError("T and M must be strictly positive.")

    ratio_t = t_ref / t
    ratio_m = min(m_ref / m, CAPS["M"])
    # S can be 0 (e.g. unchanged starter files exempt); when <= S_ref, capped at 1.0
    ratio_s = CAPS["S"] if s <= 0 else min(s_ref / s, CAPS["S"])
    # D: direct non-std runtime dependencies (1 + count)
    ratio_d = min(d_ref / d, CAPS["D"]) if d > 0 else CAPS["D"]

    k = (ratio_t ** EXPONENTS["T"]) * \
        (ratio_m ** EXPONENTS["M"]) * \
        (ratio_s ** EXPONENTS["S"]) * \
        (ratio_d ** EXPONENTS["D"])
    return k, ratio_t, ratio_m, ratio_s, ratio_d


def speed_score(k):
    """Compute speed score: clamp(50 * log10(K), 0, 100)."""
    if k <= 0:
        return 0.0
    return min(100.0, max(0.0, 50.0 * math.log10(k)))


def print_break_even_table():
    print("Break-even table: What each lever is worth (multiplier on K)")
    print("=" * 68)
    print(f"{'Change':<44} | {'Effect on K':<20}")
    print("-" * 68)
    print(f"{'halve runtime':<44} | {'x1.414':<20}")
    print(f"{'halve memory (while the ratio is below 50x)':<44} | {'x1.149':<20}")
    print(f"{'double source lines above the reference':<44} | {'x0.901':<20}")
    print(f"{'add one dependency':<44} | {'x0.901':<20}")
    print(f"{'+10% lines':<44} | {'~ x0.986 (must buy ~3% speed)':<20}")
    print(f"{'double lines':<44} | {'must buy >= 23.1% speed':<20}")
    print(f"{'one memory halving':<44} | {'worth a 32% runtime change':<20}")
    print("=" * 68)


def run_selftest(tolerance=0.5):
    """Self-test against known K to Speed Score conversions."""
    test_cases = [
        (10.0, 50.0),
        (20.0, 65.0),
        (30.0, 74.0),
        (50.0, 85.0),
        (100.0, 100.0),
    ]
    all_passed = True
    print(f"Running k_calc selftest (tolerance = {tolerance})...")
    for k_val, expected_score in test_cases:
        score = speed_score(k_val)
        diff = abs(score - expected_score)
        passed = diff <= tolerance
        status = "PASS" if passed else "FAIL"
        print(f"  [{status}] K = {k_val:>3}: got {score:>6.2f}, expected {expected_score:>5.1f} (diff = {diff:.3f})")
        if not passed:
            all_passed = False

    if all_passed:
        print("PASS  k_calc self-test passed successfully.")
        return 0
    else:
        print("FAIL  k_calc self-test encountered failures.")
        return 1


def main():
    parser = argparse.ArgumentParser(description="Calculate K and speed score for Speed Demon 2026.")
    parser.add_argument("--track", choices=["a", "b", "c", "d"], help="Track identifier")
    parser.add_argument("-T", "--time", type=float, help="Wall execution time in seconds")
    parser.add_argument("-M", "--memory", help="Peak memory in bytes or string (e.g. 50MB, 200MiB)")
    parser.add_argument("-S", "--source", type=int, help="Source lines of code (scc)")
    parser.add_argument("-D", "--deps", type=int, default=1, help="1 + direct runtime dependencies (default: 1)")
    parser.add_argument("--Tref", type=float, help="Override reference T in seconds")
    parser.add_argument("--Mref", help="Override reference M in bytes or string")
    parser.add_argument("--Sref", type=int, help="Override reference S lines")
    parser.add_argument("--Dref", type=int, default=1, help="Override reference D (default: 1)")
    parser.add_argument("--table", action="store_true", help="Print break-even trade-off table")
    parser.add_argument("--selftest", action="store_true", help="Run self-test on K -> score curve")

    args = parser.parse_args()

    if args.selftest:
        sys.exit(run_selftest())

    if args.table:
        print_break_even_table()
        if not args.time and not args.track:
            return

    if not args.track and not (args.Tref and args.Mref and args.Sref):
        parser.error("Must specify --track (or explicit --Tref, --Mref, --Sref) to calculate score, or use --selftest / --table.")

    ref = REFERENCE_AXES.get(args.track, {})
    t_ref = args.Tref if args.Tref is not None else ref.get("T_ref")
    m_ref_raw = args.Mref if args.Mref is not None else ref.get("M_ref")
    s_ref = args.Sref if args.Sref is not None else ref.get("S_ref")
    d_ref = args.Dref if args.Dref is not None else ref.get("D_ref", 1)

    if t_ref is None or m_ref_raw is None or s_ref is None:
        parser.error(f"Incomplete reference axes for track {args.track}.")

    m_ref = parse_memory_bytes(m_ref_raw)

    if args.time is None or args.memory is None or args.source is None:
        parser.error("Must specify -T (time), -M (memory), and -S (source lines) for measurement calculation.")

    m_val = parse_memory_bytes(args.memory)
    k, r_t, r_m, r_s, r_d = calc_k(args.time, m_val, args.source, args.deps, t_ref, m_ref, s_ref, d_ref)
    score = speed_score(k)

    print(f"Track: {args.track.upper() if args.track else 'CUSTOM'}")
    print(f"Inputs:    T = {args.time:.3f} s, M = {m_val / 1e6:.2f} MB, S = {args.source} lines, D = {args.deps}")
    print(f"Reference: T = {t_ref:.3f} s, M = {m_ref / 1e6:.2f} MB, S = {s_ref} lines, D = {d_ref}")
    print(f"Ratios:    T_ratio = {r_t:.2f}x, M_ratio = {r_m:.2f}x (cap 50x), S_ratio = {r_s:.3f} (cap 1.0), D_ratio = {r_d:.3f} (cap 1.0)")
    print(f"Result:    K = {k:.3f}")
    print(f"Score:     Speed Score = {score:.2f} / 100.0")


if __name__ == "__main__":
    main()
