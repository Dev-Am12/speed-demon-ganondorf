#!/usr/bin/env bash
# Convert track-b from the organizers' Python reference to a Rust package (PLAN.md section 5).
# Run from the repo root, on branch b/v0, with a clean working tree. Makes one commit. Does NOT push.
# The Dockerfile and Cargo.toml are copied from track-a so the digest-pinned FROM lines are identical.
set -euo pipefail
[ -f track-a/Dockerfile ] && [ -f track-a/Cargo.toml ] || { echo "run from the repo root; track-a must be converted first" >&2; exit 2; }
[ -z "$(git status --porcelain)" ] || { echo "working tree not clean; commit or stash first" >&2; exit 2; }
[ "$(git branch --show-current)" = "b/v0" ] || { echo "switch to branch b/v0 first: git checkout -b b/v0" >&2; exit 2; }
git rm -q track-b/b_index.py track-b/run.py
cp track-a/Dockerfile track-b/Dockerfile
cp track-a/Cargo.toml track-b/Cargo.toml
sed -i 's/^language = "Python"/language = "Rust"/' track-b/.speed-demon.toml
mkdir -p track-b/src
git add track-b
git commit -q -m "track-b: convert to a Rust package (no solution code yet)"
echo "--- track-b/.speed-demon.toml"; cat track-b/.speed-demon.toml
echo "--- track-b files"; ls -A track-b
