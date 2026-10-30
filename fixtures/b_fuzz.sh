#!/usr/bin/env bash
set -euo pipefail
if [[ $# -ne 4 ]]; then echo 'usage: b_fuzz.sh <binary> <first_seed> <last_seed> <tiny|sample>' >&2; exit 2; fi
bin="$(realpath "$1")"; first="$2"; last="$3"; scale="$4"
[[ "$scale" == tiny || "$scale" == sample ]] || { echo 'scale must be tiny or sample' >&2; exit 2; }
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"; org="$HOME/sd-organizer"
for seed in $(seq "$first" "$last"); do
  data="$HOME/sd-data/fuzz/$scale/$seed/b"; refout="$HOME/sd-data/fuzz/$scale/$seed/ref"
  mkdir -p "$data" "$refout"
  python3 "$org/generators/b_index.py" --seed "$seed" --scale "$scale" --out "$data"
  python3 "$org/reference/b_index.py" --data "$data" --out "$refout"
  expected="$(sha256sum "$refout/results.txt" | cut -d' ' -f1)"
  if ! "$repo/fixtures/b_check.sh" "$bin" "$data" "$expected"; then
    echo "FAIL seed=$seed expected=$expected"
    diff -u "$refout/results.txt" "$HOME/sd-link/out/results.txt" | head -20 || true
    exit 1
  fi
  echo "PASS fuzz seed=$seed"
done
echo 'FUZZ PASS'