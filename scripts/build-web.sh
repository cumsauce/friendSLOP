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
# The `web` profile (Cargo.toml) builds for size: phones download and compile
# the whole .wasm before anything shows (#191).
cargo build --profile web --locked --target wasm32-unknown-unknown -p friendslop --bin friendslop

rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen --no-typescript --remove-name-section --remove-producers-section --target web --out-dir "$OUT" --out-name friendslop \
  "$TARGET_DIR/wasm32-unknown-unknown/web/friendslop.wasm"
if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    -o "$OUT/friendslop_bg.wasm" "$OUT/friendslop_bg.wasm" || echo "wasm-opt failed; keeping the unoptimized wasm" >&2
fi
# index.html shows download progress; servers often gzip the .wasm (so
# Content-Length is the compressed size), so tell it the real size.
wasm_bytes=$(wc -c < "$OUT/friendslop_bg.wasm" | tr -d ' ')
sed "s/__WASM_BYTES__/$wasm_bytes/" web/index.html > "$OUT/index.html"
cp web/manifest.webmanifest "$OUT/"
# Bevy fetches assets over HTTP from ./assets (LFS files must be real, not pointers).
cp -R crates/game/assets "$OUT/assets"
# Phones (#191): decode the modeler's KTX2 textures (BC formats phones lack)
# to PNG and shrink the 2048 px textures to 512 px (~20 MB -> ~2.5 MB to
# download, and a fraction of the GPU memory). WEB_TEXTURE_MAX=0 skips it.
if [[ "${WEB_TEXTURE_MAX:-512}" != 0 ]]; then
  cargo run --release --locked --quiet --manifest-path tools/web-textures/Cargo.toml -- \
    "$OUT/assets" "${WEB_TEXTURE_MAX:-512}"
fi
echo "web build ready in $OUT ($(du -sh "$OUT" | cut -f1))"
