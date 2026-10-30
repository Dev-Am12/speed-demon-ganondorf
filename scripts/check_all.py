#!/usr/bin/env python3
"""Wrapper around check.py for a track and data directory.

Usage:
    python scripts/check_all.py <track> --data <data_dir> [--expected <sha256>] [--skip-build]
    python scripts/check_all.py --track <track> --data <data_dir> [--expected <sha256>] [--skip-build]
    python scripts/check_all.py all --scale {tiny,sample,full} [--data-root <dir>]

Outputs:
    Prints PASS/FAIL, output sha256, and runtime. Exits non-zero on failure.
"""
import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHECK_PY = os.path.join(ROOT, "check.py")

KNOWN_SEED1_HASHES = {
    ("a", "tiny"): "c24fa507c963eaeac9224fd8435019b4f4e8c002ece7e32f316681a3957eb92f",
    ("a", "sample"): "ca6311bbeb536fa55d85265bf6fd3d24cc9586f13de1ac73bd2a23ac3893aac3",
    ("a", "full"): "495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3",
    ("b", "sample"): "52dd86efce839e55a3064d2719d9bca1b585bf2961d5635f67b57bb97be982fa",
    ("b", "full"): "f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927",
    ("c", "sample"): "f8174411136c1e5ca29431e5f039bb303e970b8893d56a3375b699c27429188d",
    ("c", "full"): "d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536",
    ("d", "tiny"): "ecaca77d3221fe3928109da36480cfdb39f288f2e57ecbc61d3d7706d5cb4bd5",
    ("d", "sample"): "a4c76ed030bc7e365db33191f6cf74708fba7f25eb48d64c8c4279f2e57046a0",
    ("d", "full"): "a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e",
}


def run_track_check(track, data_dir, expected=None, skip_build=False):
    """Run check.py for a single track and return (passed, sha256, runtime_sec, message)."""
    if not os.path.isfile(CHECK_PY):
        return False, None, None, f"check.py not found at {CHECK_PY}"

    cmd = [sys.executable, CHECK_PY, track, "--data", data_dir]
    if expected:
        cmd += ["--expected", expected]
    if skip_build:
        cmd.append("--skip-build")

    proc = subprocess.run(cmd, capture_output=True, text=True)
    stdout = proc.stdout
    stderr = proc.stderr

    sha_match = re.search(r"sha256\s+([0-9a-fA-F]{64})", stdout)
    sha = sha_match.group(1).lower() if sha_match else None

    time_match = re.search(r"time\s+([0-9.]+)\s+s", stdout)
    runtime = float(time_match.group(1)) if time_match else None

    if proc.returncode != 0:
        fail_msg = "check.py failed"
        for line in stdout.splitlines():
            if line.startswith("FAIL"):
                fail_msg = line.strip()
                break
        if not fail_msg and stderr:
            fail_msg = stderr.strip()
        return False, sha, runtime, fail_msg

    if expected and sha and sha != expected.strip().lower():
        return False, sha, runtime, f"SHA mismatch: got {sha}, expected {expected}"

    return True, sha, runtime, "OK"


def main():
    parser = argparse.ArgumentParser(description="Wrap check.py and report PASS/FAIL, sha256, and timing.")
    parser.add_argument("track_pos", nargs="?", choices=["a", "b", "c", "d", "all"], help="Track to check")
    parser.add_argument("--track", choices=["a", "b", "c", "d", "all"], help="Track to check (flag form)")
    parser.add_argument("--data", help="Path to track input data folder")
    parser.add_argument("--scale", choices=["tiny", "sample", "full"], default="sample", help="Scale when running all")
    parser.add_argument("--data-root", default=os.path.join(ROOT, "data"), help="Root data folder when running all")
    parser.add_argument("--expected", help="Expected output sha256 hash")
    parser.add_argument("--skip-build", action="store_true", help="Skip Docker build and reuse existing image")

    args = parser.parse_args()
    track = args.track or args.track_pos
    if not track:
        parser.error("Must specify track (e.g. 'a', 'd', or 'all')")

    if track == "all":
        tracks_to_check = ["a", "d"]
        all_passed = True
        for t in tracks_to_check:
            data_dir = args.data or os.path.join(args.data_root, t, args.scale)
            if not os.path.isdir(data_dir):
                print(f"FAIL  track-{t}: data dir {data_dir} does not exist")
                all_passed = False
                continue
            exp = args.expected or KNOWN_SEED1_HASHES.get((t, args.scale))
            passed, sha, runtime, msg = run_track_check(t, data_dir, expected=exp, skip_build=args.skip_build)
            time_str = f"{runtime:.2f}s" if runtime is not None else "unknown"
            if passed:
                print(f"PASS  track-{t} [{args.scale}]: sha256={sha} time={time_str}")
            else:
                print(f"FAIL  track-{t} [{args.scale}]: {msg} (sha={sha}, time={time_str})")
                all_passed = False
        sys.exit(0 if all_passed else 1)
    else:
        if not args.data:
            parser.error("--data is required when checking a specific track")
        passed, sha, runtime, msg = run_track_check(track, args.data, expected=args.expected, skip_build=args.skip_build)
        time_str = f"{runtime:.2f}s" if runtime is not None else "unknown"
        if passed:
            print(f"PASS  track-{track}: sha256={sha} time={time_str}")
            sys.exit(0)
        else:
            print(f"FAIL  track-{track}: {msg} (sha={sha}, time={time_str})")
            sys.exit(1)


if __name__ == "__main__":
    main()
