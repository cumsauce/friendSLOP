#!/usr/bin/env bash
# Run a command holding the shared "perf" lock, so only one worktree at a time
# runs a game for frame times (the bot needs its window visible and alone):
#
#   scripts/perf-lock.sh env FRIENDSLOP_BOT=tour FRIENDSLOP_PERF=1 .run/friendslop host
#
# scripts/perf_sweep.sh takes the same lock by itself.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/lib/target.sh"
[[ $# -gt 0 ]] || { sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
with_lock perf "$@"
