#!/usr/bin/env bash
# Run the organizers' Python reference on a dataset and print the sha256 of its output file.
# usage: fixtures/run_ref.sh <a|b|c|d> <datadir> <outdir>
# The reference output stays in <outdir> so you can diff a candidate against it.
set -euo pipefail
ORG="${SD_ORG:-$HOME/sd-organizer}"
track="${1:?usage: run_ref.sh <a|b|c|d> <datadir> <outdir>}"
data="${2:?datadir}"
out="${3:?outdir}"
case "$track" in a) mod=a_ingest; name=summary.txt;; b) mod=b_index; name=results.txt;;
  c) mod=c_join; name=aggregates.txt;; d) mod=d_traverse; name=distances.txt;;
  *) echo "track must be a, b, c or d" >&2; exit 2;; esac
mkdir -p "$out"
rm -f "$out/$name"
python3 "$ORG/reference/$mod.py" --data "$data" --out "$out" >&2
sha256sum "$out/$name" | cut -d' ' -f1
