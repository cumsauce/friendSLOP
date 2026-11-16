#!/usr/bin/env bash
# Get a git worktree ready for an agent that shares the main checkout's cargo
# target dir. Run it once per worktree (it's safe to run again):
#
#   scripts/worktree-setup.sh .claude/worktrees/<name>    # or no arg: this checkout
#
# It writes the worktree's .claude/settings.local.json (git-ignored) so every
# command the agent runs gets:
#   CARGO_TARGET_DIR      the main checkout's target/ (shared dependency builds)
#   FRIENDSLOP_BRP_PORT   a port of its own, from the worktree's name
# and it pulls the Git LFS files (models, textures, sounds) the game needs.
# Existing keys in settings.local.json are kept.
set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
tree=$(cd "${1:-$here}" && pwd)
root=$tree
. "$here/scripts/lib/target.sh"

# 15703..16702, stable per worktree path; the main checkout keeps 15702.
if [[ $tree == "$(main_root)" ]]; then
  port=15702
else
  port=$((15703 + $(printf '%s' "$tree" | cksum | cut -d' ' -f1) % 1000))
fi

mkdir -p "$tree/.claude"
python3 -I - "$tree/.claude/settings.local.json" "$CARGO_TARGET_DIR" "$port" <<'PY'
import json, os, sys
path, target, port = sys.argv[1], sys.argv[2], sys.argv[3]
s = {}
if os.path.exists(path):
    with open(path) as f:
        s = json.load(f)
env = s.setdefault("env", {})
env["CARGO_TARGET_DIR"] = target
env["FRIENDSLOP_BRP_PORT"] = port
with open(path, "w") as f:
    json.dump(s, f, indent=2)
    f.write("\n")
PY

git -C "$tree" lfs pull || echo "warning: git lfs pull failed; models and sounds may be pointer stubs" >&2

echo "worktree: $tree"
echo "  CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
echo "  FRIENDSLOP_BRP_PORT=$port"
echo "  (in .claude/settings.local.json; start the agent's session after this)"
