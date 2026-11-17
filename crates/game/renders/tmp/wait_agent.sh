#!/usr/bin/env bash
# Wait until the other agent's files stop changing for 90s and side-nyc compiles.
cd /Users/doomy/Code/friendSLOP
files="crates/game/src/features/mod.rs crates/game/src/footsteps.rs \
crates/game/assets/data/side/fulton.ron crates/game/assets/data/play/noise.ron \
crates/game/assets/models/features/fs_hunter.gltf crates/side-nyc/src/features/fulton_hunt.rs \
crates/play/src/features/fps.rs crates/play/src/lib.rs crates/play/src/features/noise.rs"
deadline=$(( $(date +%s) + 2400 ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  sleep 75
  newest=$(stat -f "%m" $files 2>/dev/null | sort -n | tail -1)
  now=$(date +%s)
  quiet=$(( now - newest ))
  if [ "$quiet" -ge 90 ]; then
    if cargo build -p friendslop-side-nyc -p friendslop-play >/dev/null 2>&1; then
      echo "quiet ${quiet}s and both crates compile — agent looks done"
      exit 0
    fi
    echo "quiet ${quiet}s but build still failing; waiting"
  fi
done
echo "timed out after 40 min"
exit 1
