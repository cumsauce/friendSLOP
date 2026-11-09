#!/usr/bin/env bash
# Publish a screenshot of idea #N (#167) to the `screenshots` branch as
# issue-N.png, where the Discord bot picks it up for its "in the game" and
# new-build posts. Prints the picture's URL (for the issue comment).
#
#   scripts/publish-screenshot.sh 167 shot.png
#
# The branch holds only pictures (never merged into main). Safe to run from
# several parallel runs: it retries on a push race.
set -euo pipefail

n=${1:?usage: scripts/publish-screenshot.sh <issue> <file.png>}
png=${2:?usage: scripts/publish-screenshot.sh <issue> <file.png>}
[[ $n =~ ^[0-9]+$ ]] || { echo "issue must be a number" >&2; exit 1; }
[[ -s $png ]] || { echo "no such picture: $png" >&2; exit 1; }
case $png in /*) ;; *) png="$PWD/$png" ;; esac

branch=screenshots
repo=$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null || echo cumsauce/friendSLOP)
tmp=$(mktemp -d)
trap 'git worktree remove --force "$tmp/wt" >/dev/null 2>&1 || true; rm -rf "$tmp"' EXIT

for attempt in 1 2 3 4 5; do
  git worktree remove --force "$tmp/wt" >/dev/null 2>&1 || true
  if git fetch -q origin "$branch" 2>/dev/null; then
    git worktree add -q --detach "$tmp/wt" FETCH_HEAD
  else
    # First ever picture: start the branch with no history.
    git worktree add -q --detach "$tmp/wt"
    git -C "$tmp/wt" checkout -q --orphan "$branch"
    git -C "$tmp/wt" rm -rfq . >/dev/null 2>&1 || true
    printf 'Screenshots of landed ideas (#167), read by tools/idea-bot. Not code.\n' >"$tmp/wt/README.md"
    git -C "$tmp/wt" add README.md
  fi
  cp "$png" "$tmp/wt/issue-$n.png"
  git -C "$tmp/wt" add "issue-$n.png"
  git -C "$tmp/wt" commit -qm "screenshot for #$n" || true
  if git -C "$tmp/wt" push -q origin "HEAD:refs/heads/$branch"; then
    echo "https://github.com/$repo/blob/$branch/issue-$n.png?raw=true"
    exit 0
  fi
  echo "push raced (attempt $attempt), retrying" >&2
  sleep $((attempt * 2))
done
echo "could not publish the screenshot" >&2
exit 1
