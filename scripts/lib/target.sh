# Shared helpers for scripts that build or run the game. Source it:
#   . "$(dirname "$0")/lib/target.sh"
#
# Several worktrees (one per agent) share ONE cargo target dir. That means:
# - CARGO_TARGET_DIR defaults to the main checkout's target/, so every worktree
#   reuses the same compiled dependencies (set it yourself to override);
# - target/debug/friendslop belongs to whichever worktree linked last, so
#   scripts never run it in place: `friendslop_bin` builds, then copies the
#   binary into this worktree's .run/ while holding a lock, and runs the copy;
# - things only one agent can do at a time (perf runs need the window) take a
#   named lock with `with_lock <name> <cmd...>`.

root=${root:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}

# The main checkout (the one that owns .git), even from inside a worktree.
main_root() {
  local common
  common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || {
    echo "$root"
    return
  }
  dirname "$common"
}

if [[ -z ${CARGO_TARGET_DIR:-} ]]; then
  CARGO_TARGET_DIR="$(main_root)/target"
fi
export CARGO_TARGET_DIR

locks_dir="$CARGO_TARGET_DIR/.friendslop-locks"

# Take lock <name>, waiting for it. A lock whose holder died is taken over.
# macOS has no flock(1), so the lock is a directory holding the owner's pid.
lock_take() {
  local dir="$locks_dir/$1" waited=0
  mkdir -p "$locks_dir"
  until mkdir "$dir" 2>/dev/null; do
    local pid
    pid=$(cat "$dir/pid" 2>/dev/null || true)
    if [[ -n $pid ]] && ! kill -0 "$pid" 2>/dev/null; then
      echo "lock '$1': holder $pid is gone, taking it over" >&2
      rm -rf "$dir"
      continue
    fi
    if [[ $waited -eq 0 ]]; then
      echo "lock '$1' held by pid ${pid:-?} ($(cat "$dir/who" 2>/dev/null || echo '?')); waiting..." >&2
    fi
    waited=1
    sleep 2
  done
  echo $$ >"$dir/pid"
  echo "$root" >"$dir/who"
}

lock_drop() {
  rm -rf "${locks_dir:?}/$1"
}

# Run a command while holding lock <name>.
with_lock() {
  local name=$1
  shift
  lock_take "$name"
  local rc=0
  "$@" || rc=$?
  lock_drop "$name"
  return $rc
}

# Build the game and print the path of this worktree's private copy of it.
# BIN (if set) wins and is used as is.
friendslop_bin() {
  if [[ -n ${BIN:-} ]]; then
    realpath "$BIN"
    return
  fi
  local copy="$root/.run/friendslop"
  mkdir -p "$root/.run"
  # Build and copy under one lock, so another script can't relink the shared
  # binary between this build and the copy.
  lock_take build-bin
  if ! cargo build -p friendslop --manifest-path "$root/Cargo.toml" >&2; then
    lock_drop build-bin
    return 1
  fi
  # The binary is ~370 MB: clone it on APFS (macOS `cp -c`), copy elsewhere.
  rm -f "$copy.tmp"
  cp -c "$CARGO_TARGET_DIR/debug/friendslop" "$copy.tmp" 2>/dev/null \
    || cp -f "$CARGO_TARGET_DIR/debug/friendslop" "$copy.tmp"
  lock_drop build-bin
  mv -f "$copy.tmp" "$copy"
  echo "$copy"
}
