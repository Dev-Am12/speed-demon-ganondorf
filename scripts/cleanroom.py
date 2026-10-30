#!/usr/bin/env python3
"""Clean-room build and run rehearsal tool for Speed Demon 2026.

Implements the Gate G7 clean-room rehearsal verification:
  1. Creates a fresh clone in an isolated temporary directory.
  2. Verifies repo integrity (LICENSE, .gitattributes, no tests/scripts inside track-*).
  3. Validates each track's .speed-demon.toml.
  4. Builds each track image with --network=none (must work completely offline).
  5. Runs the container with sandbox constraints (--network=none, --read-only, etc.).
  6. Hashes output file and verifies byte-for-byte correctness against reference sha256.

Usage:
    python scripts/cleanroom.py [--tracks a,d] [--scale sample] [--data-root <path>]
    python scripts/cleanroom.py --track a --data data/a/sample
    python scripts/cleanroom.py --help
"""
import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUTPUT_NAMES = {"a": "summary.txt", "b": "results.txt", "c": "aggregates.txt", "d": "distances.txt"}
IMAGE_MAX_BYTES = 2 * 1024 ** 3

KNOWN_HASHES = {
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


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def audit_clone(clone_dir, tracks):
    problems = []
    # 1. License check
    if not os.path.isfile(os.path.join(clone_dir, "LICENSE")):
        problems.append("Missing LICENSE file at repo root")

    # 2. .gitattributes check
    if not os.path.isfile(os.path.join(clone_dir, ".gitattributes")):
        problems.append("Missing .gitattributes file at repo root")

    # 3. Track structure
    for t in tracks:
        tdir = os.path.join(clone_dir, f"track-{t}")
        if not os.path.isdir(tdir):
            problems.append(f"track-{t} folder is missing")
            continue
        # Check forbidden files inside track
        for root, dirs, files in os.walk(tdir):
            rel = os.path.relpath(root, tdir)
            if "tests" in rel.lower() or "fixtures" in rel.lower() or "scripts" in rel.lower():
                problems.append(f"track-{t} contains forbidden directory '{rel}' (counts against S)")
            for fname in files:
                if fname.endswith(".pyc") or fname.endswith(".o") or fname.endswith(".so"):
                    problems.append(f"track-{t} contains compiled artifact {fname}")

        cfg = os.path.join(tdir, ".speed-demon.toml")
        if not os.path.isfile(cfg):
            problems.append(f"track-{t}/.speed-demon.toml is missing")

    return problems


def build_and_run_track(clone_dir, track, data_dir, expected_sha=None):
    tag = f"cleanroom-test/track-{track}:{int(time.time())}"
    track_dir = os.path.join(clone_dir, f"track-{track}")
    
    # 1. Docker build --network=none
    print(f"[{track.upper()}] Building offline (--network=none)...")
    build_cmd = ["docker", "build", "--network=none", "-t", tag, track_dir]
    b_res = subprocess.run(build_cmd, capture_output=True, text=True)
    if b_res.returncode != 0:
        return False, None, None, f"Docker build failed:\n{b_res.stderr or b_res.stdout}"

    # Check image size
    size_res = subprocess.run(["docker", "image", "inspect", "-f", "{{.Size}}", tag], capture_output=True, text=True)
    size_bytes = int(size_res.stdout.strip())
    if size_bytes > IMAGE_MAX_BYTES:
        return False, None, None, f"Image size {size_bytes / 1e9:.2f} GB exceeds 2 GB limit"

    # 2. Docker run under sandbox
    out_dir = os.path.join(clone_dir, ".out", f"track-{track}")
    shutil.rmtree(out_dir, ignore_errors=True)
    os.makedirs(out_dir, exist_ok=True)
    os.chmod(out_dir, 0o777)

    run_cmd = [
        "docker", "run", "--rm", "--network=none", "--read-only",
        "--tmpfs", "/tmp:rw,size=1g,exec",
        "--memory=8g", "--memory-swap=8g", "--pids-limit=512",
        "--cap-drop=ALL", "--security-opt=no-new-privileges",
        "--user", "65534:65534", "--ulimit", "nofile=8192:8192",
        "-e", f"TRACK={track}",
        "-v", f"{os.path.abspath(data_dir)}:/data:ro",
        "-v", f"{os.path.abspath(out_dir)}:/out:rw",
        tag
    ]

    print(f"[{track.upper()}] Running under sandbox constraints...")
    t0 = time.monotonic()
    r_res = subprocess.run(run_cmd, capture_output=True, text=True)
    took = time.monotonic() - t0

    if r_res.returncode != 0:
        return False, None, took, f"Container run exited {r_res.returncode}:\n{r_res.stderr or r_res.stdout}"

    expected_name = OUTPUT_NAMES[track]
    out_file = os.path.join(out_dir, expected_name)
    if not os.path.isfile(out_file):
        found = os.listdir(out_dir) if os.path.isdir(out_dir) else []
        return False, None, took, f"Output file {expected_name} not found; /out contains {found}"

    digest = sha256_file(out_file)

    if expected_sha and digest.lower() != expected_sha.lower():
        return False, digest, took, f"SHA mismatch: got {digest}, expected {expected_sha}"

    return True, digest, took, "PASS"


def main():
    parser = argparse.ArgumentParser(description="Cleanroom rehearsal for Speed Demon G7 gate verification.")
    parser.add_argument("--tracks", default="a,d", help="Comma-separated tracks to test (e.g. a,d)")
    parser.add_argument("--scale", default="sample", choices=["tiny", "sample", "full"], help="Data scale for verification")
    parser.add_argument("--data-root", default=os.path.join(ROOT, "data"), help="Root folder containing data/a/..., data/d/...")
    parser.add_argument("--repo-source", default=ROOT, help="Source repo path to clone from")
    parser.add_argument("--keep", action="store_true", help="Keep temporary clone directory")

    args = parser.parse_args()
    tracks = [t.strip().lower() for t in args.tracks.split(",") if t.strip()]

    print("=== SPEED DEMON CLEANROOM REHEARSAL (Gate G7) ===")
    print(f"Source repo: {args.repo_source}")
    print(f"Tracks:      {', '.join(tracks)}")
    print(f"Scale:       {args.scale}")

    temp_dir = tempfile.mkdtemp(prefix="speeddemon-cleanroom-")
    clone_dir = os.path.join(temp_dir, "repo")

    try:
        print(f"\n1. Cloning repo to temporary location ({clone_dir})...")
        c_res = subprocess.run(["git", "clone", "--depth", "1", f"file://{os.path.abspath(args.repo_source)}", clone_dir],
                               capture_output=True, text=True)
        if c_res.returncode != 0:
            # Fallback if local protocol fails on Windows
            c_res = subprocess.run(["git", "clone", "--depth", "1", os.path.abspath(args.repo_source), clone_dir],
                                   capture_output=True, text=True)
        if c_res.returncode != 0:
            print(f"FAIL: git clone failed: {c_res.stderr or c_res.stdout}")
            sys.exit(1)

        print("\n2. Auditing repository structure and guardrails...")
        problems = audit_clone(clone_dir, tracks)
        if problems:
            print("Guardrail warnings / problems:")
            for p in problems:
                print(f"  - {p}")
        else:
            print("OK: Repo structure, LICENSE, .gitattributes, and track folders clean.")

        print("\n3. Testing offline Docker build and sandboxed run per track...")
        overall_ok = True
        for t in tracks:
            data_dir = os.path.join(args.data_root, t, args.scale)
            if not os.path.isdir(data_dir):
                print(f"SKIP track-{t}: data directory {data_dir} not found.")
                continue

            expected_sha = KNOWN_HASHES.get((t, args.scale))
            passed, digest, took, msg = build_and_run_track(clone_dir, t, data_dir, expected_sha)
            if passed:
                print(f"PASS track-{t}: output sha256={digest} took={took:.2f}s")
            else:
                print(f"FAIL track-{t}: {msg}")
                overall_ok = False

        if overall_ok:
            print("\n=== CLEANROOM REHEARSAL RESULT: PASS ===")
            sys.exit(0)
        else:
            print("\n=== CLEANROOM REHEARSAL RESULT: FAIL ===")
            sys.exit(1)

    finally:
        if not args.keep:
            shutil.rmtree(temp_dir, ignore_errors=True)
        else:
            print(f"Retained cleanroom directory at: {temp_dir}")


if __name__ == "__main__":
    main()
