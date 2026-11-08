# Handoff: next steps for friendSLOP

State as of 2026-11-08. `main` is at `92c785a`, and all finished work is merged.

## Unfinished branches (pushed, not merged)

### 1. `wip/navmesh-villagers`: villager pathfinding with a navigation mesh

- **What's on it:** only `Cargo.toml` and `Cargo.lock` changes, which add the navmesh crates. There's no code yet.
- **Crates:**
  - `bevy_rerecast` 0.6 (Bevy 0.20)
  - `avian_rerecast` 0.7, which builds the navmesh from avian colliders
  - `bevy_landmass` 0.13 and `landmass_rerecast` 0.4, which handle agents following paths on the mesh
- **Licenses:** confirm they're all MIT/Apache.
- **Task:** in `features/villagers.rs`, build the navmesh once on the server or host, after terrain, scenery, zones and buildings have spawned. Then make villagers walk real paths between districts.
  - Districts: the commune centre, the village houses and market (east, x 35..80), the maze and ruins (south-west), the forest clearing (-28, -62) and the lake (`terrain::LAKE_AT`).
  - Agent radius 0.35, height 1.7, max climb 0.5, max slope about 40°.
- **Keep:**
  - the 5 Hz replicated `Villager { pos, yaw, sitting }`
  - sitting and idling
  - the hide-and-seek "that's just Kevin" freeze
  - the old direct steering as a fallback when the navmesh isn't ready. Headless tests rely on that.
- **Startup time:** build the navmesh off the main thread or behind the loading screen, and log how long it takes.

### 2. `wip/zfight-audit`: flickering overlapping surfaces

- **The problem:** the user saw flicker on windowsills, where two surfaces sat on exactly the same plane. The sills themselves are fixed in #93.
- **What's on it:** partial fixes in `buildings.rs`, `landmarks.rs`, `scenery.rs` and `zones.rs`. They're uncommitted-quality, so review the diff first.
- **Task:** finish the audit across every model built in code. That covers the files above plus `people.rs`, `art.rs`, `terrain.rs`, `menu.rs`, `features/dave_commune.rs`, `features/duel.rs` (arena sand and fence), the totems, landlord and vault, and the gods_tears rings.
- **Common culprits:**
  - flat decals lying exactly at y=0 (the ground is at y=0 in the flat zones)
  - boxes that share a face
  - floor slabs whose tops are at the same height
- **Fix:** move one surface 1–3 cm (or add a `DECAL_LIFT` constant or `depth_bias`). If practical, add a test that scans `buildings::layout()` for overlapping faces on the same plane.

### 3. Vendor bevy_ahoy's character controller (not started; there's no branch)

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

- **Not yet seen in a running game:** the east village (stairs and the tower ramps), walking between the districts, and the frame rate on the bigger map. The release benchmark (`FRIENDSLOP_BENCH=1 cargo run --release -- host`) failed to run last time, with a toolchain libLLVM error.
- **Stutter:** p99 frame time was about 25–30 ms. Re-measure it on an idle machine.
- **Open issues:** #75 is a design question for the friends, so don't implement it. #1 needs a human.
- **Cat palette:** it makes greens look olive or yellow, and the commune flag reads as a Union Jack. This is a design limit of #12, not a bug.

## Working notes

- **Merging:** CI is Linux only (#88). Merge once fmt, `clippy -D warnings` and `cargo test --workspace` pass locally. There's no branch protection, and the user has said to merge without waiting for CI.
- **Shared build folder:** `CARGO_TARGET_DIR=/Users/lume/friendSLOP/target` with `CARGO_INCREMENTAL=0`.
  - Parallel worktrees overwrite each other's test binaries. Touch a source file before running a test, and do the final check on its own, with nothing else building.
  - Disk space is tight, so never create a second target folder.
- **Screenshots:**
  1. Copy the built binary to a uniquely named file.
  2. Run it from `crates/game`.
  3. Get its window id with Swift `CGWindowListCopyWindowInfo`, matching the binary's owner name.
  4. Capture with `screencapture -x -l<id>`.
