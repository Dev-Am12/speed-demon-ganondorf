#!/usr/bin/env bash
# board_push.sh — Push a candidate to the speed-demon branch for board measurement.
#
# Implements PLAN.md section 9, step 3:
#   1. DRY RUN by default; executes real git actions only when --yes is passed.
#   2. Refuses dirty trees.
#   3. Refuses any force flag (-f, --force).
#   4. Prints "BOARD: <track> <short-sha> <hypothesis>" before doing work.
#   5. When running a single track, resets all other track folders to starter
#      content from commit cef6720 on a throwaway commit, so only the target
#      track is run by the board.
#   6. Never rewrites published history.
#
# Usage:
#   ./scripts/board_push.sh --track <a|b|c|d|all> --hypothesis "<text>" [--yes]
#   bash scripts/board_push.sh -t a -m "A4 parallel read_at chunks"
#   bash scripts/board_push.sh -t a -m "A4 parallel read_at chunks" --yes

set -euo pipefail

STARTER_COMMIT="cef67201033553d3511d82c9905938ad1703ef7d"
ALL_TRACKS=("a" "b" "c" "d")

TRACK=""
HYPOTHESIS=""
EXECUTE=false

# Check for forbidden force flags
for arg in "$@"; do
  case "$arg" in
    -f|--force|--force-with-lease)
      echo "ERROR: Force push is strictly prohibited (GR-G3)." >&2
      exit 1
      ;;
  esac
done

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    -t|--track)
      TRACK="${2:-}"
      shift 2
      ;;
    -m|--hypothesis)
      HYPOTHESIS="${2:-}"
      shift 2
      ;;
    --yes)
      EXECUTE=true
      shift
      ;;
    -h|--help)
      sed -n '2,17p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$TRACK" ]]; then
  echo "ERROR: --track <a|b|c|d|all> is required." >&2
  exit 1
fi

TRACK=$(echo "$TRACK" | tr '[:upper:]' '[:lower:]')
if [[ "$TRACK" != "a" && "$TRACK" != "b" && "$TRACK" != "c" && "$TRACK" != "d" && "$TRACK" != "all" ]]; then
  echo "ERROR: Track must be one of: a, b, c, d, all (got: $TRACK)" >&2
  exit 1
fi

if [[ -z "$HYPOTHESIS" ]]; then
  echo "ERROR: --hypothesis <description> is required." >&2
  exit 1
fi

# Check for dirty tree
DIRTY=$(git status --porcelain --untracked-files=no)
if [[ -n "$DIRTY" ]]; then
  echo "ERROR: Working tree has uncommitted modifications. Refusing to push dirty tree." >&2
  git status --short
  exit 1
fi

SHORT_SHA=$(git rev-parse --short HEAD)

# Announce format: BOARD: <track> <short-sha> <hypothesis>
echo "BOARD: $TRACK $SHORT_SHA $HYPOTHESIS"

OTHER_TRACKS=()
if [[ "$TRACK" != "all" ]]; then
  for t in "${ALL_TRACKS[@]}"; do
    if [[ "$t" != "$TRACK" ]]; then
      OTHER_TRACKS+=("track-$t")
    fi
  done
fi

if [[ "$EXECUTE" != true ]]; then
  echo ""
  echo "=== DRY RUN (No changes made, nothing pushed) ==="
  echo "To execute for real, re-run with --yes"
  echo ""
  echo "Planned actions:"
  if [[ ${#OTHER_TRACKS[@]} -gt 0 ]]; then
    echo "  1. Reset other track folders to starter ($STARTER_COMMIT):"
    echo "     git checkout $STARTER_COMMIT -- ${OTHER_TRACKS[*]}"
    echo "  2. If any other track changed, create throwaway isolation commit:"
    echo "     git commit -m \"board($TRACK): isolate track-$TRACK at $SHORT_SHA ($HYPOTHESIS)\""
  fi
  echo "  3. Push to board branch:"
  echo "     git push origin HEAD:speed-demon"
  exit 0
fi

# Real execution mode (--yes passed)
echo "Executing board push workflow..."

if [[ ${#OTHER_TRACKS[@]} -gt 0 ]]; then
  echo "Resetting other tracks to starter content (${OTHER_TRACKS[*]})..."
  git checkout "$STARTER_COMMIT" -- "${OTHER_TRACKS[@]}"
  
  DIFF_CHECK=$(git status --porcelain -- "${OTHER_TRACKS[@]}")
  if [[ -n "$DIFF_CHECK" ]]; then
    echo "Creating throwaway isolation commit for track-$TRACK..."
    git commit -m "board($TRACK): isolate track-$TRACK at $SHORT_SHA ($HYPOTHESIS)"
  else
    echo "Other track folders are already byte-identical to starter."
  fi
fi

echo "Pushing candidate to origin speed-demon..."
git push origin HEAD:speed-demon
echo "SUCCESS: Pushed to speed-demon. Board will pick up the run."
