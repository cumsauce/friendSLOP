# Handoff: next steps for friendSLOP

State as of 2026-11-08, updated the same day. Tasks 1 and 2 below are **done and merged**
(#95, #96); only task 3 and the "Other known gaps" remain.

## Done on 2026-11-08 (for the record)

- **#95 — z-fighting audit finished.** The branch's partial fixes landed, the layout-wide
  coplanar-face test passes, and the one extra offender found by hand (the gods tears' halo ring
  dipping through the ground plane) is fixed.
- **#96 — villager navmesh.** The host builds a navmesh over the terrain + village architecture
  (~3 s, logged), villagers get landmass agents and walk overland between districts; the old
  straight-line steering stays as fallback. **Note:** stock rerecast 0.5.0 cannot build a mesh
  over this map ("Invalid contour" on any settings; upstream rerecast #59/#64, parity fix in
  their PR #60 closed unmerged), so `vendor/rerecast/` carries the one-line C++-parity skip fix
  via `[patch.crates-io]` — see `vendor/rerecast/README-friendSLOP.md`. Drop the vendored copy
  once upstream ships the fix. `tests/navmesh.rs` is the regression test.

## Remaining: task 3 (unchanged)

### Vendor bevy_ahoy's character controller (not started; there's no branch)

- **Why vendor:** bevy_ahoy (https://github.com/janhohenheim/bevy_ahoy, MIT OR Apache-2.0) targets Bevy 0.19 and avian 0.7, and we're on Bevy 0.20 and avian 0.8.
- **Where to put it:** copy its controller core into `crates/game/src/kcc/` and port it.
  - Take `src/kcc.rs`, plus what you need from `lib.rs`, `input.rs` and `fixed_update_utils.rs`.
  - Skip `camera.rs`, `water.rs` and `pickup_glue.rs`.
  - Pin the commit sha, and add license and attribution notices.
- **Then:** replace the hand-written movement in `features/fps.rs`:
  - `collide_walkers`, `step`, `floor_under`, terrain-height snapping and the `buildings::WALKABLE` special cases
  - with step-up (0.5 m), a slope limit (about 45°), ground snapping, sliding along walls and jumping
  - keeping walk at 5 m/s and Shift-sprint at 10.5 m/s.
- **Keep:**
  - Movement controlled by the host: clients send `WalkInput`, the host moves the bodies, and `Walker.pos` replicates.
  - The freezes, the ±88 m world limit, the spawn points and the first-person camera.
  - Tests that pass with no colliders loaded.

## Other known gaps

- **Not yet seen in a running game:** the east village (stairs and the tower ramps), walking between the districts, and the frame rate on the bigger map. The release benchmark (`FRIENDSLOP_BENCH=1 cargo run --release -- host`) failed to run last time, with a toolchain libLLVM error. The navmesh, villagers with agents and the wider districts now ran for minutes without errors on 2026-11-08, but nobody has eyeballed them in a window yet.
- **Stutter:** p99 frame time was about 25–30 ms. Re-measure it on an idle machine.
- **Open issues:** #75 is a design question for the friends, so don't implement it. #1 needs a human.
- **Cat palette:** it makes greens look olive or yellow, and the commune flag reads as a Union Jack. This is a design limit of #12, not a bug.

## Working notes

- **Merging:** CI is Linux only (#88). Merge once fmt, `clippy -D warnings` and `cargo test --workspace` pass locally. There's no branch protection, and the user has said to merge without waiting for CI.
- **Shared build folder:** `CARGO_TARGET_DIR=/Users/lume/friendSLOP/target` with `CARGO_INCREMENTAL=0`.
  - Parallel worktrees overwrite each other's test binaries. Touch a source file before running a test, and do the final check on its own, with nothing else building.
  - Disk space is tight, so never create a second target folder. (2026-11-08: the disk hit 98% mid-build; sweeping stale duplicate artifacts in `target/debug/deps` — keep newest per artifact name — freed ~7.6 GB without a full clean.)
- **Screenshots:**
  1. Copy the built binary to a uniquely named file.
  2. Run it from `crates/game`.
  3. Get its window id with Swift `CGWindowListCopyWindowInfo`, matching the binary's owner name (`CFString` casts need `as! CFString` + `as String` under the current toolchain).
  4. Capture with `screencapture -x -l<id>`. (2026-11-08: this failed with "could not create image from window" — likely missing screen-recording permission for the terminal, not a wrong window id.)
