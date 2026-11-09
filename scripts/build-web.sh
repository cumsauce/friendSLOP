#!/usr/bin/env bash
# Build the browser version (#106) into dist/web/ (or $1):
#   index.html + friendslop.js + friendslop_bg.wasm + assets/
# Serve it with any static server, e.g. `python3 -m http.server -d dist/web 8080`.
#
# Needs: rustup target add wasm32-unknown-unknown
#        cargo install wasm-bindgen-cli --version <the wasm-bindgen in Cargo.lock>
# Optional: wasm-opt (binaryen) on PATH shrinks the .wasm by ~30%.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-dist/web}"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"

want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/version = |"/, ""); print; exit}' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
if [[ "$have" != "$want" ]]; then
  echo "wasm-bindgen-cli $want is required (found: ${have:-none}):" >&2
  echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

# iroh's TLS (ring) compiles a little C for wasm32. Apple's ar can't archive
# wasm objects (the link then fails with undefined ring_core_* symbols), so on
# macOS use Homebrew's LLVM if it's there. Linux clang + llvm-ar just work.
if [[ "$(uname)" == Darwin && -z "${AR_wasm32_unknown_unknown:-}" && -x /opt/homebrew/opt/llvm/bin/llvm-ar ]]; then
  export CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang
  export AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar
fi

# getrandom's browser backend (also in .cargo/config.toml, but a RUSTFLAGS
# env var would replace that, so pass it here too).
export RUSTFLAGS="${RUSTFLAGS:-} --cfg getrandom_backend=\"wasm_js\""
cargo build --release --locked --target wasm32-unknown-unknown -p friendslop --bin friendslop

rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen --no-typescript --remove-name-section --remove-producers-section --target web --out-dir "$OUT" --out-name friendslop \
  "$TARGET_DIR/wasm32-unknown-unknown/release/friendslop.wasm"
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    -o "$OUT/friendslop_bg.wasm" "$OUT/friendslop_bg.wasm" || echo "wasm-opt failed; keeping the unoptimized wasm" >&2
fi
cp web/index.html web/manifest.webmanifest "$OUT/"
# Bevy fetches assets over HTTP from ./assets (LFS files must be real, not pointers).
cp -R crates/game/assets "$OUT/assets"
echo "web build ready in $OUT ($(du -sh "$OUT" | cut -f1))"
