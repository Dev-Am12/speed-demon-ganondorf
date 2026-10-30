#!/usr/bin/env bash
# board_push.sh — Push a candidate to the speed-demon branch via detached worktree.
#
# Implements PLAN.md section 9, step 3 & P-PRE-02 / P-PRE-03 requirements:
#   1. Never modifies or commits on caller's branch or working tree.
#   2. Creates a detached git worktree from origin/speed-demon in .out/wt-<id>.
#   3. Candidate guard: refuses candidate SHA unless contained in a remote branch
#      (git branch -r --contains).
#   4. Delete-aware overlay: runs git rm -r -q --ignore-unmatch track-X then
#      git checkout <sha> -- track-X so deleted candidate files disappear on speed-demon.
#   5. Single-track isolation: overlays target track; resets every other track-* to
#      STARTER content (commit cef6720) while preserving that track's .speed-demon.toml
#      and *.md from origin/speed-demon so the board marks them as "unchanged".
#   6. --all-tracks: overlays every track from the candidate and resets nothing.
#   7. --empty: allows an empty commit for noise re-run (PLAN §9). Dry run by default.
#   8. Commits in worktree: "board(<track>): candidate <short-sha> (<hypothesis>)".
#   9. Pushes via plain `git push origin <wt-sha>:refs/heads/speed-demon`
#      (no force, no +refspec; aborts cleanly if not fast-forward).
#  10. Cleanup: matches worktree with `git worktree list --porcelain`, removes it,
#      and runs `git worktree prune`.
#  11. DRY RUN by default; requires --yes for real execution.
#  12. Refuses dirty caller tree and refuses any force flag (-f, --force).
#  13. Prints "BOARD: <track> <short-sha> <hypothesis>" chat line first.
#
# Usage:
#   ./scripts/board_push.sh --track <a|b|c|d|all> --hypothesis "<text>" [--sha <commit>] [--empty] [--yes]
#   ./scripts/board_push.sh --all-tracks --hypothesis "<text>" [--yes]
#   bash scripts/board_push.sh -t a -m "A4 parallel read_at"
#   bash scripts/board_push.sh --all-tracks -m "G8 release candidate" --yes
#   bash scripts/board_push.sh -t a --empty -m "noise test 1" --yes

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
ALL_TRACKS=("a" "b" "c" "d")
STARTER_COMMIT="cef67201033553d3511d82c9905938ad1703ef7d"

TRACK=""
HYPOTHESIS=""
CANDIDATE_REF="HEAD"
ALLOW_EMPTY=false
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
    --empty)
      ALLOW_EMPTY=true
      shift
      ;;
    --yes)
      EXECUTE=true
      shift
      ;;
    -h|--help)
      sed -n '2,26p' "$0" | sed 's/^# \?//'
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
CANDIDATE_SHA=$(git rev-parse "$CANDIDATE_REF" 2>/dev/null || true)
if [[ -z "$CANDIDATE_SHA" ]]; then
  echo "ERROR: Cannot resolve candidate ref '$CANDIDATE_REF'." >&2
  exit 1
fi
SHORT_SHA=$(git rev-parse --short "$CANDIDATE_SHA")

# Check identity for real execution (dry-run is unaffected)
if [[ "$EXECUTE" == true ]]; then
  USER_NAME_SET=false
  if git config user.name >/dev/null 2>&1 || [[ -n "${GIT_AUTHOR_NAME:-}" ]]; then
    USER_NAME_SET=true
  fi
  USER_EMAIL_SET=false
  if git config user.email >/dev/null 2>&1 || [[ -n "${GIT_AUTHOR_EMAIL:-}" ]]; then
    USER_EMAIL_SET=true
  fi

  if [[ "$USER_NAME_SET" != true || "$USER_EMAIL_SET" != true ]]; then
    echo "ERROR: Git committer identity is not configured." >&2
    if [[ "$USER_NAME_SET" != true ]]; then
      echo "       git user.name is missing. Run: git config user.name \"Your Name\" (or set GIT_AUTHOR_NAME)" >&2
    fi
    if [[ "$USER_EMAIL_SET" != true ]]; then
      echo "       git user.email is missing. Run: git config user.email \"you@example.com\" (or set GIT_AUTHOR_EMAIL)" >&2
    fi
    exit 1
  fi
fi

# Fetch remote tracking branches before candidate guard (warn if fails, do not abort)
if ! git fetch origin --quiet 2>/dev/null; then
  echo "WARNING: 'git fetch origin --quiet' failed; proceeding with existing remote tracking branches." >&2
fi

# Candidate guard: refuse unless candidate sha is contained in a remote branch
REMOTE_BRANCHES=$(git branch -r --contains "$CANDIDATE_SHA" 2>/dev/null || true)
if [[ -z "${REMOTE_BRANCHES//[[:space:]]/}" ]]; then
  echo "ERROR: Candidate commit $SHORT_SHA is not contained in any remote branch." >&2
  echo "       Candidates must be pushed to a remote branch first (GR-C6/GR-G2/PLAN §9)." >&2
  exit 1
fi

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
  echo "Candidate commit: $SHORT_SHA ($CANDIDATE_SHA)"
  echo "Allow empty commit: $ALLOW_EMPTY"
  echo "To execute for real, re-run with --yes"
  echo ""
  echo "Planned actions in isolated git worktree:"
  echo "  1. Fetch origin/speed-demon"
  echo "  2. Create detached worktree from origin/speed-demon at temporary path"
  if [[ "$TRACK" == "all" ]]; then
    echo "  3. Overlay ALL tracks from candidate $SHORT_SHA (delete-aware):"
    for f in "${TARGET_FOLDERS[@]}"; do
      echo "     git rm -r -q --ignore-unmatch $f && git checkout $CANDIDATE_SHA -- $f"
    done
  else
    echo "  3. Overlay target track-$TRACK from candidate $SHORT_SHA (delete-aware):"
    echo "     git rm -r -q --ignore-unmatch track-$TRACK && git checkout $CANDIDATE_SHA -- track-$TRACK"
    echo "  4. Reset non-target tracks to starter ($STARTER_COMMIT), preserving their .speed-demon.toml and *.md:"
    for t in "${ALL_TRACKS[@]}"; do
      if [[ "$t" != "$TRACK" ]]; then
        echo "     track-$t: backup .speed-demon.toml/*.md -> rm -r track-$t -> checkout $STARTER_COMMIT -- track-$t -> restore .speed-demon.toml/*.md"
      fi
    done
  fi
  echo "  5. Commit in worktree:"
  if [[ "$ALLOW_EMPTY" == true ]]; then
    echo "     git commit --allow-empty -m \"board($TRACK): candidate $SHORT_SHA ($HYPOTHESIS)\""
  else
    echo "     git commit -m \"board($TRACK): candidate $SHORT_SHA ($HYPOTHESIS)\""
  fi
  echo "  6. Push via fast-forward only:"
  echo "     git push origin <wt-sha>:refs/heads/speed-demon"
  echo "  7. Clean up worktree (matching porcelain list) and prune (caller tree remains untouched)"
  exit 0
fi

# Real execution mode (--yes passed)
echo "Executing isolated worktree push workflow..."

mkdir -p "$REPO_ROOT/.out"
WT_DIR="$REPO_ROOT/.out/wt-push-$$"
TEMP_PRESERVE="$REPO_ROOT/.out/preserve-$$"

cleanup() {
  local exit_code=$?
  local found=false
  local line
  while IFS= read -r line; do
    if [[ "$line" =~ ^worktree[[:space:]]+(.*)$ ]]; then
      local wt_p="${BASH_REMATCH[1]}"
      local norm_wt="${wt_p//\\//}"
      local norm_target="${WT_DIR//\\//}"
      if [[ "$norm_wt" == "$norm_target" ]]; then
        found=true
        break
      fi
    fi
  done < <(git worktree list --porcelain 2>/dev/null || true)

  if [[ "$found" == true ]]; then
    echo "Cleaning up worktree $WT_DIR..."
    git worktree remove -f "$WT_DIR" >/dev/null 2>&1 || true
    git worktree prune >/dev/null 2>&1 || true
  fi
  rm -rf "$WT_DIR" >/dev/null 2>&1 || true
  rm -rf "$TEMP_PRESERVE" >/dev/null 2>&1 || true
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

echo "1. Fetching latest origin/speed-demon..."
git fetch origin speed-demon

echo "2. Creating detached worktree from origin/speed-demon..."
git worktree add --detach "$WT_DIR" origin/speed-demon

if [[ "$TRACK" == "all" ]]; then
  echo "3. Overlaying all candidate tracks from $SHORT_SHA (delete-aware)..."
  for f in "${TARGET_FOLDERS[@]}"; do
    if git rev-parse "$CANDIDATE_SHA:$f" >/dev/null 2>&1; then
      echo "   Overlaying $f..."
      git -C "$WT_DIR" rm -r -q --ignore-unmatch "$f"
      git -C "$WT_DIR" checkout "$CANDIDATE_SHA" -- "$f"
    else
      echo "   Note: $f not found in candidate commit $SHORT_SHA; removing from worktree"
      git -C "$WT_DIR" rm -r -q --ignore-unmatch "$f"
    fi
  done
else
  echo "3. Overlaying target track-$TRACK from $SHORT_SHA (delete-aware)..."
  git -C "$WT_DIR" rm -r -q --ignore-unmatch "track-$TRACK"
  git -C "$WT_DIR" checkout "$CANDIDATE_SHA" -- "track-$TRACK"

  echo "4. Isolating non-target tracks: resetting to starter, preserving origin docs..."
  mkdir -p "$TEMP_PRESERVE"
  for t in "${ALL_TRACKS[@]}"; do
    if [[ "$t" == "$TRACK" ]]; then
      continue
    fi
    echo "   Resetting track-$t to starter ($STARTER_COMMIT)..."
    mkdir -p "$TEMP_PRESERVE/track-$t"
    if [[ -f "$WT_DIR/track-$t/.speed-demon.toml" ]]; then
      cp "$WT_DIR/track-$t/.speed-demon.toml" "$TEMP_PRESERVE/track-$t/"
    fi
    for md in "$WT_DIR/track-$t/"*.md; do
      if [[ -f "$md" ]]; then
        cp "$md" "$TEMP_PRESERVE/track-$t/"
      fi
    done

    # Remove all existing track-$t files in worktree
    git -C "$WT_DIR" rm -r -q --ignore-unmatch "track-$t"

    # Checkout starter content from cef6720
    git -C "$WT_DIR" checkout "$STARTER_COMMIT" -- "track-$t"

    # Replace starter docs with preserved origin docs
    rm -f "$WT_DIR/track-$t/.speed-demon.toml"
    for md in "$WT_DIR/track-$t/"*.md; do
      if [[ -f "$md" ]]; then
        rm -f "$md"
      fi
    done

    if [[ -f "$TEMP_PRESERVE/track-$t/.speed-demon.toml" ]]; then
      cp "$TEMP_PRESERVE/track-$t/.speed-demon.toml" "$WT_DIR/track-$t/"
    fi
    for md in "$TEMP_PRESERVE/track-$t/"*.md; do
      if [[ -f "$md" ]]; then
        cp "$md" "$WT_DIR/track-$t/"
      fi
    done

    git -C "$WT_DIR" add -A "track-$t"
  done
fi

WT_STATUS=$(git -C "$WT_DIR" status --porcelain)
if [[ -z "$WT_STATUS" && "$ALLOW_EMPTY" != true ]]; then
  echo "No differences between candidate folder(s) and origin/speed-demon."
  echo "Nothing new to push (use --empty for a noise re-run)."
  exit 0
fi

echo "5. Committing changes in worktree..."
COMMIT_ARGS=(-m "board($TRACK): candidate $SHORT_SHA ($HYPOTHESIS)")
if [[ "$ALLOW_EMPTY" == true ]]; then
  COMMIT_ARGS=(--allow-empty "${COMMIT_ARGS[@]}")
fi
git -C "$WT_DIR" commit "${COMMIT_ARGS[@]}"
WT_SHA=$(git -C "$WT_DIR" rev-parse HEAD)
echo "   Worktree commit: $WT_SHA"

echo "6. Pushing to origin/speed-demon (fast-forward only, no force)..."
# Plain push without force or +refspec; aborts cleanly if not fast-forward
git push origin "${WT_SHA}:refs/heads/speed-demon"

echo "SUCCESS: Pushed $WT_SHA to origin/speed-demon. Caller branch remains at $(git rev-parse --short HEAD)."
