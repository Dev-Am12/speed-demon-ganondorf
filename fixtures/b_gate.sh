#!/usr/bin/env bash
set -euo pipefail
if [[ $# -lt 1 || $# -gt 2 ]]; then echo 'usage: b_gate.sh <binary> [--no-full]' >&2; exit 2; fi
bin="$(realpath "$1")"
no_full=0
if [[ ${2:-} == --no-full ]]; then no_full=1; elif [[ $# -eq 2 ]]; then echo 'unknown option' >&2; exit 2; fi
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
check="$repo/fixtures/b_check.sh"
hashes="$repo/fixtures/expected_hashes.md"
tiny="$HOME/sd-data/tiny/1/b"
sample="$HOME/sd-data/sample/1/b"
full="$repo/data/data/b/full"
pass=0
fail=0
run_check() {
  local label="$1"
  shift
  if "$@"; then echo "PASS $label"; pass=$((pass+1)); else echo "FAIL $label"; fail=$((fail+1)); fi
}
read -r h_tiny h_sample h_full < <(awk -F'|' '$3 ~ /results.txt/ {for(i=4;i<=6;i++){gsub(/[ `]/,"",$i)}; print $4,$5,$6; exit}' "$hashes")
example="$repo/fixtures/custom/b/spec_example"
example_hash="$(sha256sum "$example/expected_results.txt" | cut -d' ' -f1)"
shopt -s nullglob
sets=()
for entry in "$repo"/fixtures/custom/b/*; do [[ -d "$entry" ]] && sets+=("$entry"); done
large_sets=("$HOME"/sd-data/custom-b/bigids "$HOME"/sd-data/custom-b/vocab70k "$HOME"/sd-data/custom-b/df-ladder)
run_check spec_example "$check" "$bin" "$example" "$example_hash"
for dir in "${sets[@]}"; do
  [[ "${dir##*/}" == spec_example ]] && continue
  if [[ ! -f "$dir/expected.sha256" ]]; then echo "FAIL missing expected.sha256: ${dir%/}"; fail=$((fail+1)); continue; fi
  run_check "hostile ${dir##*/}" "$check" "$bin" "$dir" "$(cat "$dir/expected.sha256")"
done
for dir in "${large_sets[@]}"; do
  if [[ ! -f "$dir/expected.sha256" ]]; then echo "FAIL missing large expected.sha256: $dir"; fail=$((fail+1)); continue; fi
  run_check "large ${dir##*/}" "$check" "$bin" "$dir" "$(cat "$dir/expected.sha256")"
done
run_check tiny "$check" "$bin" "$tiny" "$h_tiny"
run_check sample "$check" "$bin" "$sample" "$h_sample"
if [[ $no_full -eq 0 ]]; then run_check full "$check" "$bin" "$full" "$h_full"; fi
repeat_set() {
  local label="$1" data="$2" expected="$3" baseline='' got
  for _ in 1 2 3 4 5; do
    got="$("$check" "$bin" "$data" "$expected" | awk '/^sha256 /{print $2}')" || return 1
    if [[ -n "$baseline" && "$baseline" != "$got" ]]; then echo "determinism mismatch $label"; return 1; fi
    baseline="$got"
  done
  echo "PASS deterministic $label x5"
}
repeat_set sample "$sample" "$h_sample" || { echo 'FAIL deterministic sample'; fail=$((fail+1)); }
for dir in "${sets[@]}"; do
  [[ "${dir##*/}" == spec_example ]] && continue
  repeat_set "${dir##*/}" "$dir" "$(cat "$dir/expected.sha256")" || { echo "FAIL deterministic ${dir##*/}"; fail=$((fail+1)); }
done
for dir in "${large_sets[@]}"; do
  repeat_set "large ${dir##*/}" "$dir" "$(cat "$dir/expected.sha256")" || { echo "FAIL deterministic large ${dir##*/}"; fail=$((fail+1)); }
done
thread_check() {
  local label="$1" cpus="$2" data="$3" expected="$4"
  if taskset -c "$cpus" "$check" "$bin" "$data" "$expected" >/dev/null; then echo "PASS taskset $label"; pass=$((pass+1)); else echo "FAIL taskset $label"; fail=$((fail+1)); fi
}
for cpus in 0 0-1 0-5 0-11; do
  thread_check "sample $cpus" "$cpus" "$sample" "$h_sample"
  for dir in "${sets[@]}"; do
    [[ "${dir##*/}" == spec_example ]] && continue
    thread_check "${dir##*/} $cpus" "$cpus" "$dir" "$(cat "$dir/expected.sha256")"
  done
  for dir in "${large_sets[@]}"; do
    thread_check "large ${dir##*/} $cpus" "$cpus" "$dir" "$(cat "$dir/expected.sha256")"
  done
done
if [[ $fail -eq 0 ]]; then echo "GATE PASS ($pass checks)"; else echo "GATE FAIL ($fail failures; $pass checks passed)"; exit 1; fi