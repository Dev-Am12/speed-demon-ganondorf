#!/usr/bin/env bash
# Generate one track's input data with the organizers' generator.
# usage: fixtures/gen_data.sh <a|b|c|d> <seed> <tiny|sample|full>
# Output goes to ~/sd-data/<scale>/<seed>/<track>/ (outside the repo, never committed).
# Env overrides: SD_ORG (organizer repo, default ~/sd-organizer), SD_DATA (default ~/sd-data).
set -euo pipefail
ORG="${SD_ORG:-$HOME/sd-organizer}"
DATA="${SD_DATA:-$HOME/sd-data}"
track="${1:?usage: gen_data.sh <a|b|c|d> <seed> <tiny|sample|full>}"
seed="${2:?seed}"
scale="${3:?scale}"
case "$track" in a) mod=a_ingest;; b) mod=b_index;; c) mod=c_join;; d) mod=d_traverse;;
  *) echo "track must be a, b, c or d" >&2; exit 2;; esac
case "$scale" in tiny|sample|full) ;; *) echo "scale must be tiny, sample or full" >&2; exit 2;; esac
out="$DATA/$scale/$seed/$track"
mkdir -p "$out"
python3 "$ORG/generators/$mod.py" --seed "$seed" --scale "$scale" --out "$out" >&2
echo "$out"
