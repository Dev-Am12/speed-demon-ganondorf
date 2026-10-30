#!/usr/bin/env bash
set -euo pipefail
if [[ $# -ne 2 ]]; then echo 'usage: memwatch.sh <binary> <datadir>' >&2; exit 2; fi
bin="$(realpath "$1")"; data="$(realpath "$2")"; root="$HOME/sd-link"; out="$root/out"
[[ -d "$out" && ! -L "$out" ]] || { echo "expected a real output directory: $out" >&2; exit 2; }
ln -sfn "$data" "$root/data"
find "$out" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
"$bin" >/dev/null 2>&1 & pid=$!
max_hwm=0; max_anon=0; max_file=0
while kill -0 "$pid" 2>/dev/null; do
  read -r hwm anon file < <(awk '/^VmHWM:/{h=$2} /^RssAnon:/{a=$2} /^RssFile:/{f=$2} END{print h+0,a+0,f+0}' "/proc/$pid/status" 2>/dev/null || echo '0 0 0')
  (( hwm > max_hwm )) && max_hwm=$hwm || true
  (( anon > max_anon )) && max_anon=$anon || true
  (( file > max_file )) && max_file=$file || true
  sleep 0.002
done
wait "$pid"
echo "VmHWM_kB=$max_hwm RssAnon_kB=$max_anon RssFile_kB=$max_file"