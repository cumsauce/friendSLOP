#!/usr/bin/env bash
# Trim the shared cargo target dir. Every worktree compiles its own copy of the
# workspace crates into it, so it grows fast; this removes build output that
# nothing has used for DAYS days (default 7). Only cache is removed: anything
# needed again is rebuilt.
#
#   scripts/sweep-target.sh          # older than 7 days
#   DAYS=2 scripts/sweep-target.sh
#
# Uses cargo-sweep when installed (`cargo install cargo-sweep`); otherwise it
# deletes stale incremental-compilation dirs, the biggest share. Don't run it
# while agents are building.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/lib/target.sh"
days=${DAYS:-7}
echo "target dir: $CARGO_TARGET_DIR ($(du -sh "$CARGO_TARGET_DIR" 2>/dev/null | cut -f1))"
lock_take build-bin
trap 'lock_drop build-bin' EXIT
if command -v cargo-sweep >/dev/null; then
  cargo sweep --time "$days" "$CARGO_TARGET_DIR"
else
  echo "cargo-sweep not installed: removing incremental dirs untouched for $days days"
  find "$CARGO_TARGET_DIR"/*/incremental -mindepth 1 -maxdepth 1 -type d -mtime +"$days" \
    -exec rm -rf {} + 2>/dev/null || true
fi
echo "now: $(du -sh "$CARGO_TARGET_DIR" 2>/dev/null | cut -f1)"
