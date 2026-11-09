#!/usr/bin/env bash
# Take one screenshot of the game and quit (#167). Works without a GPU or a
# display: it uses a virtual X screen and Mesa's software Vulkan (lavapipe).
#
#   scripts/screenshot.sh out.png                    # the normal spawn view
#   scripts/screenshot.sh out.png "0,12,30,0,0,0"     # eye x,y,z then look-at x,y,z
#
# Extra env: FRIENDSLOP_SCREENSHOT_WAIT (seconds after loading, default 10),
# BIN (a prebuilt binary; default builds with cargo), TIMEOUT (default 300 s).
# Needs (Ubuntu): sudo apt-get install -y xvfb mesa-vulkan-drivers libxkbcommon-x11-0
# and `git lfs pull` first, or models and HUD text are missing from the picture.
set -euo pipefail

out=${1:?usage: scripts/screenshot.sh out.png [x,y,z,lx,ly,lz]}
camera=${2:-}
root=$(cd "$(dirname "$0")/.." && pwd)
case $out in /*) ;; *) out="$PWD/$out" ;; esac

if [[ -z ${BIN:-} ]]; then
  cargo build -p friendslop --manifest-path "$root/Cargo.toml"
  BIN="$root/target/debug/friendslop"
fi
BIN=$(realpath "$BIN")

run=(env FRIENDSLOP_SCREENSHOT="$out" FRIENDSLOP_SCREENSHOT_CAMERA="$camera"
  WGPU_BACKEND="${WGPU_BACKEND:-vulkan}" timeout "${TIMEOUT:-300}" "$BIN" host)
rm -f "$out"
cd "$root/crates/game"
if [[ -n ${DISPLAY:-} ]] || ! command -v xvfb-run >/dev/null; then
  "${run[@]}" || true
else
  # Force X11 (no Wayland socket under xvfb) and Mesa's software Vulkan.
  env -u WAYLAND_DISPLAY VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}" \
    xvfb-run -a -s "-screen 0 1280x720x24" "${run[@]}" || true
fi
[[ -s $out ]] || { echo "no screenshot was written" >&2; exit 1; }
echo "saved $out"
