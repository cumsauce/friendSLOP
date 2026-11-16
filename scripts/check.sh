#!/usr/bin/env bash
# The check loop to run before you push: fmt, clippy (-D warnings) and tests,
# for the crates your branch changed plus every crate that depends on them.
# Never builds the whole workspace unless asked, so agents sharing one target
# dir don't queue behind each other's full builds.
#
#   scripts/check.sh            # changed crates since the merge-base with main
#   scripts/check.sh --fast     # fmt + clippy only, no tests
#   scripts/check.sh --all      # the whole workspace (what CI covers)
#   scripts/check.sh -p friendslop-side -p friendslop-play   # these, plus dependents
#   scripts/check.sh --list     # only print which crates it would check
#
# Extra env: BASE (ref to diff against; default origin/main, else main).
# The browser clippy runs when a .rs/Cargo/.cargo/web file changed, like CI.
# Data-only changes run the data-file test. Uses nextest when installed.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
. "$root/scripts/lib/target.sh"
cd "$root"

fast=0 all=0 list=0 picked=()
while [[ $# -gt 0 ]]; do
  case $1 in
    --fast) fast=1 ;;
    --all) all=1 ;;
    --list) list=1 ;;
    -p) picked+=("$2"); shift ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

base=${BASE:-}
if [[ -z $base ]]; then
  if git rev-parse -q --verify origin/main >/dev/null; then base=origin/main; else base=main; fi
fi
files=$( (git diff --name-only "$(git merge-base HEAD "$base")"; git ls-files --others --exclude-standard) | sort -u)

rust=0 wasm=0 data=0
while IFS= read -r p; do
  [[ -z $p ]] && continue
  case $p in
    *.rs|*Cargo.toml|Cargo.lock|rust-toolchain*|.cargo/*) rust=1 ;;
    crates/game/assets/data/*|crates/game/assets/maps/*.json|crates/game/assets/maps/*.heights) data=1 ;;
  esac
  case $p in
    *.rs|*Cargo.toml|Cargo.lock|rust-toolchain*|.cargo/*|web/*|scripts/build-web.sh) wasm=1 ;;
  esac
done <<<"$files"

# Which workspace packages: the picked ones or those owning changed files,
# then everything that depends on them (cargo metadata, in python).
pkgs=$(cargo metadata --format-version 1 --no-deps --offline 2>/dev/null \
  | python3 -I -c '
import json, os, sys
meta = json.load(sys.stdin)
root, picked, files, every = sys.argv[1], sys.argv[2].split(), sys.argv[3].splitlines(), sys.argv[4] == "1"
pkgs = {p["name"]: os.path.dirname(p["manifest_path"]) for p in meta["packages"]}
if every:
    print(" ".join(sorted(pkgs))); sys.exit()
deps = {p["name"]: {d["name"] for d in p["dependencies"] if d["name"] in pkgs} for p in meta["packages"]}
want = set(picked)
for f in files:
    path = os.path.join(root, f)
    owner = max((d for d in pkgs.values() if path.startswith(d + os.sep)), key=len, default=None)
    if owner:
        want |= {n for n, d in pkgs.items() if d == owner}
    elif f in ("Cargo.toml", "Cargo.lock") or f.startswith(".cargo/"):
        want |= set(pkgs)
grew = True
while grew:
    more = {n for n, ds in deps.items() if ds & want} - want
    want |= more
    grew = bool(more)
print(" ".join(sorted(want)))
' "$root" "${picked[*]:-}" "$files" "$all")

if [[ -z $pkgs ]]; then
  if [[ $data == 1 ]]; then
    echo "== data files only: data check"
    cargo test -p friendslop --test data_files_check
  else
    echo "nothing Rust changed since $base; nothing to check"
  fi
  exit 0
fi

[[ $list == 1 ]] && { echo "$pkgs"; exit 0; }
flags=()
for p in $pkgs; do flags+=(-p "$p"); done
echo "== crates: $pkgs"
echo "== target dir: $CARGO_TARGET_DIR"

echo "== fmt"
cargo fmt "${flags[@]}"
echo "== clippy"
cargo clippy "${flags[@]}" --all-targets -- -D warnings
if [[ $wasm == 1 || $all == 1 ]] && [[ " $pkgs " == *" friendslop "* ]]; then
  echo "== clippy (browser)"
  cargo clippy -p friendslop --target wasm32-unknown-unknown -- -D warnings
fi
[[ $fast == 1 ]] && { echo "ok (no tests: --fast)"; exit 0; }

echo "== tests"
if command -v cargo-nextest >/dev/null; then
  cargo nextest run "${flags[@]}"
else
  cargo test "${flags[@]}"
fi
echo "ok"
