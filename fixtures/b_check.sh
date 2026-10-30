#!/usr/bin/env bash
set -euo pipefail
if [[ $# -lt 2 || $# -gt 3 ]]; then echo 'usage: b_check.sh <binary> <datadir> [expected_sha256|expected_file]' >&2; exit 2; fi
bin="$(realpath "$1")"; data="$(realpath "$2")"; expected="${3:-}"
[[ -x "$bin" ]] || { echo "not executable: $bin" >&2; exit 2; }
[[ -d "$data" && -f "$data/docs.txt" && -f "$data/queries.txt" ]] || { echo "invalid B dataset: $data" >&2; exit 2; }
link_root="$HOME/sd-link"; out="$link_root/out"
mkdir -p "$link_root/logs"
[[ -d "$out" && ! -L "$out" ]] || { echo "expected a real output directory: $out" >&2; exit 2; }
ln -sfn "$data" "$link_root/data"
find "$out" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
start="$(date +%s%N)"
set +e
"$bin" >"$link_root/logs/binary.stdout" 2>"$link_root/logs/binary.stderr"
code=$?
set -e
end="$(date +%s%N)"; wall_ms=$(( (end - start) / 1000000 ))
if [[ $code -ne 0 ]]; then echo "FAIL exit=$code wall_ms=$wall_ms"; cat "$link_root/logs/binary.stderr"; exit 1; fi
if [[ -s "$link_root/logs/binary.stdout" || -s "$link_root/logs/binary.stderr" ]]; then echo 'FAIL binary printed output'; cat "$link_root/logs/binary.stdout" "$link_root/logs/binary.stderr"; exit 1; fi
mapfile -t entries < <(find "$out" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort)
if [[ ${#entries[@]} -ne 1 || "${entries[0]}" != results.txt || ! -f "$out/results.txt" ]]; then echo "FAIL /out must contain exactly results.txt; found: ${entries[*]:-<empty>}"; exit 1; fi
last_byte="$(tail -c 1 "$out/results.txt" | od -An -tu1 | tr -d '[:space:]')"
if [[ "$last_byte" != 10 ]]; then echo 'FAIL results.txt does not end in LF'; exit 1; fi
got="$(sha256sum "$out/results.txt" | cut -d' ' -f1)"; echo "sha256 $got wall_ms=$wall_ms"
if [[ -n "$expected" ]]; then
  if [[ "$expected" =~ ^[[:xdigit:]]{64}$ ]]; then want="${expected,,}"; expected_file=""; else expected_file="$(realpath "$expected")"; want="$(sha256sum "$expected_file" | cut -d' ' -f1)"; fi
  if [[ "$got" != "$want" ]]; then echo "FAIL expected $want"; if [[ -n "$expected_file" ]]; then echo 'first differing lines:'; diff -u "$expected_file" "$out/results.txt" | head -20 || true; fi; exit 1; fi
fi
echo PASS