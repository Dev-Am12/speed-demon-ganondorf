#!/usr/bin/env bash
# board_push.sh — Push a candidate to the speed-demon branch via detached worktree.
#
# Implements PLAN.md section 9, step 3 & P-PRE-02 requirements:
#   1. Never modifies or commits on caller's branch or working tree.
#   2. Creates a detached git worktree from origin/speed-demon in .out/wt-<id>.
#   3. Overlays requested track folder(s) from candidate commit (<sha>).
#   4. Commits in the worktree: "board(<track>): candidate <short-sha> (<hypothesis>)".
#   5. Pushes using plain `git push origin <wt-sha>:refs/heads/speed-demon`
#      (no force, no +refspec; aborts cleanly if not fast-forward).
#   6. Cleans up the worktree completely via trap on exit or error.
#   7. DRY RUN by default; only performs real actions when --yes is passed.
#   8. Refuses dirty tree and refuses any force flag (-f, --force).
#   9. Prints "BOARD: <track> <short-sha> <hypothesis>" chat line first.
#  10. Supports --all-tracks for the final push.
#
# Usage:
#   ./scripts/board_push.sh --track <a|b|c|d|all> --hypothesis "<text>" [--sha <commit>] [--yes]
#   ./scripts/board_push.sh --all-tracks --hypothesis "<text>" [--yes]
#   bash scripts/board_push.sh -t a -m "A4 parallel read_at"
#   bash scripts/board_push.sh --all-tracks -m "G8 release candidate" --yes

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
ALL_TRACKS=("a" "b" "c" "d")

TRACK=""
HYPOTHESIS=""
CANDIDATE_REF="HEAD"
EXECUTE=false

# Check for forbidden force flags in arguments
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
    --all-tracks)
      TRACK="all"
      shift
      ;;
    -m|--hypothesis)
      HYPOTHESIS="${2:-}"
      shift 2
      ;;
    --sha)
      CANDIDATE_REF="${2:-}"
      shift 2
      ;;
    --yes)
      EXECUTE=true
      shift
      ;;
    -h|--help)
      sed -n '2,19p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$TRACK" ]]; then
  echo "ERROR: Must specify --track <a|b|c|d|all> or --all-tracks" >&2
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

# Check for dirty tree on caller's working tree
DIRTY=$(git status --porcelain --untracked-files=no)
if [[ -n "$DIRTY" ]]; then
  echo "ERROR: Caller working tree has uncommitted modifications. Refusing to push dirty tree." >&2
  git status --short
  exit 1
fi

# Resolve candidate commit
CANDIDATE_SHA=$(git rev-parse "$CANDIDATE_REF")
SHORT_SHA=$(git rev-parse --short "$CANDIDATE_SHA")

# Always print chat line first: BOARD: <track> <short-sha> <hypothesis>
echo "BOARD: $TRACK $SHORT_SHA $HYPOTHESIS"

# Determine target folders to overlay
TARGET_FOLDERS=()
if [[ "$TRACK" == "all" ]]; then
  for t in "${ALL_TRACKS[@]}"; do
    TARGET_FOLDERS+=("track-$t")
  done
else
  TARGET_FOLDERS+=("track-$TRACK")
fi

if [[ "$EXECUTE" != true ]]; then
  echo ""
  echo "=== DRY RUN (No changes made, nothing pushed) ==="
  echo "Caller branch: $(git rev-parse --abbrev-ref HEAD) (will NOT be modified)"
  echo "To execute for real, re-run with --yes"
  echo ""
  echo "Planned actions in isolated git worktree:"
  echo "  1. Fetch origin/speed-demon"
  echo "  2. Create detached worktree from origin/speed-demon at temporary path"
  echo "  3. Overlay target track folder(s) from $SHORT_SHA:"
  for f in "${TARGET_FOLDERS[@]}"; do
    echo "     git checkout $CANDIDATE_SHA -- $f"
  done
  echo "  4. Commit changes in worktree:"
  echo "     git commit -m \"board($TRACK): candidate $SHORT_SHA ($HYPOTHESIS)\""
  echo "  5. Push via fast-forward only:"
  echo "     git push origin <wt-sha>:refs/heads/speed-demon"
  echo "  6. Clean up worktree (caller working tree remains untouched)"
  exit 0
fi

# Real execution mode (--yes passed)
echo "Executing isolated worktree push workflow..."

mkdir -p "$REPO_ROOT/.out"
WT_DIR="$REPO_ROOT/.out/wt-push-$$"

cleanup() {
  if git worktree list | grep -q "$WT_DIR"; then
    echo "Cleaning up worktree $WT_DIR..."
    git worktree remove -f "$WT_DIR" >/dev/null 2>&1 || true
  fi
  rm -rf "$WT_DIR" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

echo "1. Fetching latest origin/speed-demon..."
git fetch origin speed-demon

echo "2. Creating detached worktree from origin/speed-demon..."
git worktree add --detach "$WT_DIR" origin/speed-demon

echo "3. Overlaying candidate folder(s) from $SHORT_SHA into worktree..."
for f in "${TARGET_FOLDERS[@]}"; do
  if git rev-parse "$CANDIDATE_SHA:$f" >/dev/null 2>&1; then
    echo "   Overlaying $f..."
    git -C "$WT_DIR" checkout "$CANDIDATE_SHA" -- "$f"
  else
    echo "   Note: $f not found in candidate commit $SHORT_SHA; skipping"
  fi
done

WT_STATUS=$(git -C "$WT_DIR" status --porcelain)
if [[ -z "$WT_STATUS" ]]; then
  echo "No differences between candidate folder(s) and origin/speed-demon."
  echo "Nothing new to push."
  exit 0
fi

echo "4. Committing changes in worktree..."
git -C "$WT_DIR" commit -m "board($TRACK): candidate $SHORT_SHA ($HYPOTHESIS)"
WT_SHA=$(git -C "$WT_DIR" rev-parse HEAD)
echo "   Worktree commit: $WT_SHA"

echo "5. Pushing to origin/speed-demon (fast-forward only, no force)..."
# Plain push without force or +refspec; aborts if not fast-forward
git push origin "${WT_SHA}:refs/heads/speed-demon"

echo "SUCCESS: Pushed $WT_SHA to origin/speed-demon. Caller branch remains at $(git rev-parse --short HEAD)."
