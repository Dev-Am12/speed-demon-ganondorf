#!/usr/bin/env python3
"""Wrapper around the organizer's static_axes.py harness tool.

Runs static_axes.py (scc + language formatters like rustfmt/ruff in Docker) on a
given track folder to compute static axes:
  - S: Source lines of code (after canonical formatting)
  - D: Dependency weight (1 + count of direct non-std runtime dependencies)

Usage:
    python scripts/static_axes.py <track_folder_or_name> [--organizer-dir <path>] [--raw]
    python scripts/static_axes.py track-a
    python scripts/static_axes.py track-d --raw

Note on Windows:
    The organizer's static_axes.py references os.getuid() / os.getgid() which are
    not available on native Windows. This wrapper polyfills them (defaulting to
    UID:GID 1000:1000) so Docker can run seamlessly under Docker Desktop / WSL2.
"""
import argparse
import os
import sys

# Windows compatibility polyfill for Unix-only os.getuid / os.getgid
if not hasattr(os, "getuid"):
    os.getuid = lambda: 1000
if not hasattr(os, "getgid"):
    os.getgid = lambda: 1000

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_organizer_harness(organizer_dir=None):
    candidates = []
    if organizer_dir:
        candidates.append(os.path.abspath(organizer_dir))
    if "SPEED_DEMON_2026" in os.environ:
        candidates.append(os.path.abspath(os.environ["SPEED_DEMON_2026"]))
    # Sibling directory check
    candidates.append(os.path.abspath(os.path.join(ROOT, "..", "speed-demon-2026")))

    for cand in candidates:
        harness_path = os.path.join(cand, "harness")
        if os.path.isfile(os.path.join(harness_path, "static_axes.py")):
            return harness_path

    return None


def run_measurement(track_path, harness_dir, raw=False):
    if harness_dir not in sys.path:
        sys.path.insert(0, harness_dir)

    try:
        import static_axes
    except ImportError as e:
        print(f"FAIL: Could not import static_axes from {harness_dir}: {e}")
        return 1

    try:
        # If raw is True, do not exempt published reference files
        exempt = not raw
        res = static_axes.measure(track_path, exempt_published=exempt)
    except Exception as e:
        print(f"FAIL: Error measuring {track_path}: {e}")
        return 1

    s_lines = res.get("S_lines", 0)
    d_val = res.get("D", 1)
    d_basis = res.get("D_basis", "unknown")
    notes = res.get("notes", [])

    print(f"Track: {track_path}")
    print(f"S (source lines): {s_lines}")
    print(f"D (dependency weight): {d_val} ({d_basis})")
    if notes:
        print("Notes:")
        for note in notes:
            print(f"  - {note}")

    return 0


def main():
    parser = argparse.ArgumentParser(description="Run organizer's static_axes.py tool on a track folder.")
    parser.add_argument("track", help="Track directory (e.g. track-a, track-d, or full path)")
    parser.add_argument("--organizer-dir", help="Path to speed-demon-2026 organizer repo")
    parser.add_argument("--raw", action="store_true", help="Count raw lines without exempting starter/published files")

    args = parser.parse_args()

    track_dir = args.track
    if not os.path.isabs(track_dir):
        # Resolve relative to ROOT if not found directly
        if not os.path.isdir(track_dir) and os.path.isdir(os.path.join(ROOT, track_dir)):
            track_dir = os.path.join(ROOT, track_dir)
        elif not os.path.isdir(track_dir) and os.path.isdir(os.path.join(ROOT, f"track-{track_dir}")):
            track_dir = os.path.join(ROOT, f"track-{track_dir}")

    if not os.path.isdir(track_dir):
        print(f"FAIL: Track directory {track_dir} does not exist.")
        sys.exit(1)

    harness_dir = find_organizer_harness(args.organizer_dir)
    if not harness_dir:
        print("FAIL: Could not locate speed-demon-2026 organizer harness directory.")
        print("Set SPEED_DEMON_2026 environment variable or pass --organizer-dir.")
        sys.exit(1)

    code = run_measurement(track_dir, harness_dir, raw=args.raw)
    sys.exit(code)


if __name__ == "__main__":
    main()
