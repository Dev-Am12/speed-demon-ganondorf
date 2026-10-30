#!/usr/bin/env bash
# usage: fixtures/hashcmp.sh <file> <expected_sha256>   -> prints OK or MISMATCH (exit 1)
set -euo pipefail
f="${1:?file}"; want="$(echo "${2:?expected sha256}" | tr 'A-F' 'a-f')"
got="$(sha256sum "$f" | cut -d' ' -f1)"
if [ "$got" = "$want" ]; then echo "OK        $got"; else echo "MISMATCH"; echo "  got      $got"; echo "  expected $want"; exit 1; fi
