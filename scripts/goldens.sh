#!/usr/bin/env bash
# Golden screenshot capture and check (S3). Reads a shots file (default
# scripts/golden_shots.txt) of rows:
#   out_name start scene camera warehouse_seed wait
# (`-` = default; `scene` may carry a size like `play@1920x1080`).
#
#   scripts/goldens.sh --update       # capture into renders/goldens/<backend>/
#   scripts/goldens.sh --check        # capture fresh, compare against the goldens
#   scripts/goldens.sh --check-only   # compare existing captures (CHECK_DIR=...)
#
# Backend policy (S3): goldens are generated and compared on Linux with Mesa's
# software Vulkan (lavapipe) only, into renders/goldens/linux-lavapipe/. On
# macOS captures go to renders/goldens/dev-macos/ and are dev reference only.
# Rows sharing a run context (start/scene/size/seed/wait) batch into one game
# run via FRIENDSLOP_SCREENSHOT_SHOTS: the first row is the normal capture,
# later rows (which need explicit cameras) follow from the shot list.
#
# Extra env: BIN (a prebuilt binary; default builds with cargo),
# TIMEOUT (default 300 s per run), SKIP_LFS_PULL=1, WGPU_BACKEND override,
# CHECK_DIR (capture dir for --check-only; default renders/goldens/.check).
# Needs (Ubuntu): sudo apt-get install -y xvfb mesa-vulkan-drivers
# libxkbcommon-x11-0. `git lfs pull` runs first, or the pictures are pointers.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
mode=
shots_file="$root/scripts/golden_shots.txt"
while [[ $# -gt 0 ]]; do
  case $1 in
    --update) mode=update ;;
    --check) mode=check ;;
    --check-only) mode=check-only ;;
    -*) echo "unknown flag: $1" >&2; exit 2 ;;
    *) shots_file=$1 ;;
  esac
  shift
done
[[ -n $mode ]] || { sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }

goldens="$root/renders/goldens"
canonical="$goldens/linux-lavapipe"
masks="$goldens/masks"
if [[ $(uname) == Darwin ]]; then
  backend_dir="$goldens/dev-macos"
else
  backend_dir="$canonical"
fi
capture_dir=$backend_dir
[[ $mode == check ]] && capture_dir="$goldens/.check"
[[ $mode == check-only ]] && capture_dir="${CHECK_DIR:-$goldens/.check}"

if [[ $mode != check-only && -z ${SKIP_LFS_PULL:-} ]]; then
  git -C "$root" lfs pull >&2 || echo "warning: git lfs pull failed; captures may show pointer stubs" >&2
fi

if [[ $mode == check-only ]]; then
  exec python3 "$root/scripts/golden_compare.py" "$canonical" "$capture_dir" \
    --masks "$masks" --diff-dir "$capture_dir/diffs"
fi

if [[ -z ${BIN:-} ]]; then
  cargo build -p friendslop --manifest-path "$root/Cargo.toml"
  BIN="$root/target/debug/friendslop"
fi
BIN=$(realpath "$BIN")

# macOS has Metal and no coreutils `timeout`; Linux uses Vulkan (lavapipe).
backend=${WGPU_BACKEND:-vulkan}
[[ $(uname) == Darwin ]] && backend=${WGPU_BACKEND:-metal}

rm -rf "$capture_dir"
mkdir -p "$capture_dir"

# Run the game once with the RUN_* environment. A shot-list run
# (RUN_SHOTS set) keeps the game alive until RUN_LAST exists, then kills it;
# a single run quits by itself after its save.
run_game() {
  local -a envs=(
    FRIENDSLOP_SCREENSHOT="$RUN_OUT"
    FRIENDSLOP_SCREENSHOT_SCENE="$RUN_SCENE"
    FRIENDSLOP_SCREENSHOT_SIZE="$RUN_SIZE"
    FRIENDSLOP_SCREENSHOT_WAIT="$RUN_WAIT"
    FRIENDSLOP_INPUT=keyboard
    FRIENDSLOP_LOW=0
    WGPU_BACKEND="$backend"
  )
  [[ -n $RUN_CAMERA ]] && envs+=(FRIENDSLOP_SCREENSHOT_CAMERA="$RUN_CAMERA")
  [[ -n $RUN_WH ]] && envs+=(FRIENDSLOP_SCREENSHOT_WAREHOUSE="$RUN_WH")
  [[ -n $RUN_SHOTS ]] && envs+=(FRIENDSLOP_SCREENSHOT_SHOTS="$RUN_SHOTS")
  local -a limit=()
  command -v timeout >/dev/null && limit=(timeout "${TIMEOUT:-300}")
  local -a run=(env "${envs[@]}" ${limit[@]+"${limit[@]}"} "$BIN" host)
  (
    cd "$root/crates/game"
    if [[ -n ${DISPLAY:-} ]] || ! command -v xvfb-run >/dev/null; then
      "${run[@]}"
    else
      env -u WAYLAND_DISPLAY VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}" \
        xvfb-run -a -s "-screen 0 1920x1080x24" "${run[@]}"
    fi
  ) >/dev/null 2>&1 &
  local pid=$!
  if [[ -n $RUN_SHOTS ]]; then
    local deadline=$((SECONDS + ${TIMEOUT:-300}))
    while [[ ! -s $RUN_LAST && $SECONDS -lt $deadline ]]; do
      kill -0 $pid 2>/dev/null || break
      sleep 1
    done
    kill $pid 2>/dev/null || true
    pkill -f "$BIN" 2>/dev/null || true
    wait $pid 2>/dev/null || true
  else
    wait $pid 2>/dev/null || true
  fi
}

# Read the rows (out start scene camera wh wait).
rows_out=() rows_start=() rows_scene=() rows_size=() rows_cam=() rows_wh=() rows_wait=()
while read -r out start scene camera wh wait; do
  [[ -z ${out:-} || $out == \#* ]] && continue
  [[ $scene == - ]] && scene=play
  row_size=1280x720
  if [[ $scene == *@* ]]; then
    row_size=${scene#*@}
    scene=${scene%@*}
  fi
  [[ $camera == - ]] && camera=
  [[ $wh == - ]] && wh=
  [[ $wait == - || -z ${wait:-} ]] && wait=12
  rows_out+=("$out") rows_start+=("$start") rows_scene+=("$scene")
  rows_size+=("$row_size") rows_cam+=("$camera") rows_wh+=("$wh") rows_wait+=("$wait")
done <"$shots_file"

fail=0
i=0
n=${#rows_out[@]}
while [[ $i -lt $n ]]; do
  # Batch the rows that share this run context (later ones need explicit
  # cameras — the shot list can't leave the camera on the spawn view).
  group=("$i")
  j=$((i + 1))
  while [[ $j -lt $n ]]; do
    if [[ ${rows_start[$j]} == "${rows_start[$i]}" && ${rows_scene[$j]} == "${rows_scene[$i]}"
          && ${rows_size[$j]} == "${rows_size[$i]}" && ${rows_wh[$j]} == "${rows_wh[$i]}"
          && ${rows_wait[$j]} == "${rows_wait[$i]}" && -n ${rows_cam[$j]} ]]; then
      group+=("$j")
    fi
    j=$((j + 1))
  done

  RUN_SCENE=${rows_scene[$i]} RUN_SIZE=${rows_size[$i]} RUN_WAIT=${rows_wait[$i]}
  RUN_WH=${rows_wh[$i]} RUN_SHOTS= RUN_LAST=
  if [[ ${#group[@]} -gt 1 ]]; then
    shots=$(mktemp)
    for j in "${group[@]:1}"; do
      echo "$capture_dir/${rows_out[$j]}.png ${rows_cam[$j]}" >>"$shots"
    done
    RUN_OUT="$capture_dir/${rows_out[$i]}.png" RUN_CAMERA=${rows_cam[$i]}
    RUN_SHOTS=$shots RUN_LAST="$capture_dir/${rows_out[${group[${#group[@]}-1]}]}.png"
    echo "== batch at '${rows_start[$i]}' (${rows_scene[$i]} ${rows_size[$i]}): ${#group[@]} shots"
    run_game
    rm -f "$shots"
  else
    RUN_OUT="$capture_dir/${rows_out[$i]}.png" RUN_CAMERA=${rows_cam[$i]}
    echo "== ${rows_out[$i]} (start '${rows_start[$i]}' ${rows_scene[$i]} ${rows_size[$i]})"
    run_game
  fi
  for j in "${group[@]}"; do
    if [[ ! -s "$capture_dir/${rows_out[$j]}.png" ]]; then
      echo "no screenshot was written for ${rows_out[$j]}" >&2
      fail=1
    fi
  done
  # Consume the grouped rows.
  i=$((i + ${#group[@]}))
done

if [[ $mode == update ]]; then
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $(git -C "$root" rev-parse --short HEAD) $(uname -s) $backend" \
    >>"$goldens/VERSIONS.txt"
  echo "goldens updated in $capture_dir"
  exit $fail
fi

python3 "$root/scripts/golden_compare.py" "$canonical" "$capture_dir" \
  --masks "$masks" --diff-dir "$capture_dir/diffs" || fail=1
exit $fail
