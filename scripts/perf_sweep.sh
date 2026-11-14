#!/usr/bin/env bash
# Batch FRIENDSLOP_PERF=3 runs over start points (S9) and append their perf
# lines to a log. The log's rows are what hardening/perf-budget.md is
# assembled from (start | median/p95/worst ms | fps | draw calls | meshes |
# lights | tex VRAM | entities | verdict).
#
#   scripts/perf_sweep.sh                 # the six predicted-worst starts
#   scripts/perf_sweep.sh lake village    # specific starts
#
# Extra env: BIN (a prebuilt binary), SECS (seconds per run, default 45 —
# long enough for the 25 s breakdown and one 30 s repeat),
# OUT (log file, default hardening/perf-sweep-raw.txt).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
starts=("$@")
[[ ${#starts[@]} -gt 0 ]] || starts=(fulton penn_street chambers_st warehouse forest_clearing factory_town)

if [[ -z ${BIN:-} ]]; then
  cargo build -p friendslop --manifest-path "$root/Cargo.toml"
  BIN="$root/target/debug/friendslop"
fi
BIN=$(realpath "$BIN")
out=${OUT:-"$root/hardening/perf-sweep-raw.txt"}
secs=${SECS:-45}
mkdir -p "$(dirname "$out")"

for start in "${starts[@]}"; do
  echo "== $start ($secs s)" | tee -a "$out"
  {
    echo "# start=$start $(date -u +%Y-%m-%dT%H:%M:%SZ) $(uname -s) $(uname -m)"
  } >>"$out"
  run=(env FRIENDSLOP_PERF=3 FRIENDSLOP_START="$start"
    WGPU_BACKEND="${WGPU_BACKEND:-$([[ $(uname) == Darwin ]] && echo metal || echo vulkan)}"
    "$BIN" host)
  # No display: the same virtual X + lavapipe setup as screenshot.sh.
  if [[ -n ${DISPLAY:-} ]] || ! command -v xvfb-run >/dev/null; then
    (cd "$root/crates/game" && timeout "${secs}" "${run[@]}" 2>&1 || true) | grep "^perf" | tee -a "$out"
  else
    (cd "$root/crates/game" && env -u WAYLAND_DISPLAY \
      VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}" \
      timeout "${secs}" xvfb-run -a -s "-screen 0 1280x720x24" "${run[@]}" 2>&1 || true) \
      | grep "^perf" | tee -a "$out"
  fi
done
echo "rows appended to $out"
