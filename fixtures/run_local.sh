#!/usr/bin/env bash
# Run a compiled candidate the way the board does: it reads /data and writes one file to /out.
# usage: fixtures/run_local.sh <a|b|c|d> <binary> <datadir> [expected_sha256]
# Env: REF_OUT=<reference output file>  -> on a mismatch, show the first differing lines.
# /data and /out are replaced by symlinks (needs sudo). It refuses to touch a real directory.
set -euo pipefail
track="${1:?usage: run_local.sh <a|b|c|d> <binary> <datadir> [expected_sha256]}"
bin="$(readlink -f "${2:?binary}")"
data="$(readlink -f "${3:?datadir}")"
want="${4:-}"
case "$track" in a) name=summary.txt;; b) name=results.txt;; c) name=aggregates.txt;; d) name=distances.txt;;
  *) echo "track must be a, b, c or d" >&2; exit 2;; esac
[ -x "$bin" ] || { echo "not executable: $bin" >&2; exit 2; }
[ -d "$data" ] || { echo "not a directory: $data" >&2; exit 2; }
for l in /data /out; do
  if [ -e "$l" ] && [ ! -L "$l" ]; then echo "$l exists and is not a symlink; refusing to replace it" >&2; exit 2; fi
done
outdir="$(mktemp -d "$HOME/sd-out.XXXXXX")"
sudo ln -sfn "$data" /data
sudo ln -sfn "$outdir" /out
start="$(date +%s%N)"
set +e; "$bin"; code=$?; set -e
end="$(date +%s%N)"
echo "exit $code, wall $(( (end - start) / 1000000 )) ms (includes process start), out dir $outdir"
[ "$code" -eq 0 ] || { echo "FAIL  non-zero exit"; exit 1; }
found="$(ls -A "$outdir" | tr '\n' ' ')"
[ "$found" = "$name " ] || { echo "FAIL  /out must hold exactly $name, found: $found"; exit 1; }
got="$(sha256sum "$outdir/$name" | cut -d' ' -f1)"
echo "sha256 $got"
if [ -n "$want" ]; then
  if [ "$got" = "$(echo "$want" | tr 'A-F' 'a-f')" ]; then echo "PASS"; else
    echo "FAIL  expected $want"
    if [ -n "${REF_OUT:-}" ]; then echo "first differing lines (candidate < > reference):"; diff "$outdir/$name" "$REF_OUT" | head -20 || true; fi
    exit 1
  fi
fi
