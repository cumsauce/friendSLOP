# Tools workstream handoff (S2 mismatch surfacing, S3 golden tooling, S9 perf, S10 web)

Branch `claude/nightly-tools`. **No protocol registrations changed** — the
protocol hash snapshot is unaffected: the `ProtocolMismatch` handlers are
observers (`add_observer`), never `replicate`/`add_*_message`/`add_*_event`.

## Bug fixes (scope add, ids for the findings register)

- **net:join_code** (fixed) — `back_to_menu` never cleared `status.join_code`
  (asymmetric vs `leave_game` at menu.rs:1045), so a dropped connection left
  the menu offering a join code for a dead session. Fixed in both
  `crates/game/src/net.rs` (desktop) and `crates/game/src/net_web.rs`
  (browser): `back_to_menu` now clears it.
- **player:spawn_race** (fixed) — `spawn_client_player`
  (`crates/base/src/player.rs`) spawned the `Player` through deferred
  commands while `despawn_client_player`'s query ran immediately: an
  Add+Remove of `AuthorizedClient` in one tick left a permanent ghost `Player`
  nothing despawns. Fixed: the spawn runs at flush time through
  `spawn_player_for`, which skips when the owner is no longer
  `AuthorizedClient` (a despawned client entity counts as gone). Regression
  tests beside it in player.rs.
- **voice:host_stale** (fixed) — `HostVoiceSpeaker` was inserted on
  `OnEnter(ServerState::Running)` and never removed, so after
  stop-hosting-then-join it could keep tagging voice as the host's.
  Fixed: `clear_host_voice` resets it on `OnExit(ServerState::Running)` —
  `HostVoiceSpeaker(None)`, not `remove_resource`, because
  `bevy_voice_chat` reads `Res<HostVoiceSpeaker>` unconditionally.

## S2 — ProtocolMismatch surfacing

Observers in `net.rs` and `net_web.rs` map `ProtocolMismatch` to the drop
reason "this build doesn't match the host's game version". First-write-wins
(`note_drop`) so the generic close reason can't clobber it — the server sends
the mismatch and disconnects immediately. The browser transport gained the
DropReason channel it never had: `client_receive` notes the close reason,
`client_stopped` surfaces it outside `Connected`, `back_to_menu` shows it via
`drop_message` (same wording as desktop). Unit tests sit beside
`drop_message_names_the_reason` in net.rs.

## S3 — golden tooling (capture/compare runs happen in a later wave)

- `FRIENDSLOP_TIME=0..1` pins time of day: `atmosphere.rs::day_start_at`
  computes `DayStart` backwards so `time_of_day` reads the target at session
  start (`DayStart` replicates; host env only). Test: `a_pinned_time_of_day_hits_the_target`.
- `screenshot::frozen()` freezes the world for captures. The same check
  lower crates use is `friendslop_base::settings::frozen()` (both read
  `FRIENDSLOP_SCREENSHOT`). Gates: cloud drift (`atmosphere.rs`), tree sway
  and wind (`lod.rs`), gait phases (`base/people.rs`), rig clip switches
  (`base/rigged.rs`), the bonfire flicker (`scenery.rs`, one gate only),
  villager brains (`village/villagers.rs`), and the creator RNG seed is
  pinned in frozen mode (`creator.rs::clock_seed`).
- `scripts/goldens.sh --update|--check|--check-only` (xvfb+lavapipe pattern
  from screenshot.sh; `BIN`, `TIMEOUT`, `WGPU_BACKEND`; batches same-start
  rows via `FRIENDSLOP_SCREENSHOT_SHOTS`), `scripts/golden_compare.py`
  (Pillow; per-channel ±2/255 deadzone; fail >1.5% pixels beyond 8/255 or
  RMSE >3.0; optional masks `renders/goldens/masks/`; diff heatmaps; fails
  on missing/blank PNGs), `scripts/golden_shots.txt` (the 10 designed shots
  — commune_bonfire, penn_street, fulton, penn, warehouse seed 7,
  ui_play@1920x1080, underground via shot-list camera, village, lake,
  wendys), `renders/goldens/README.md` (regen + tolerance + VERSIONS.txt
  convention), and `.gitattributes` gained `renders/goldens/**/*.png` LFS.
- Backend policy: **Linux lavapipe only** (`renders/goldens/linux-lavapipe/`);
  macOS is dev reference (`dev-macos/`). Generate the first golden set after
  the open renderer PRs (#397/#399) land, or regenerate once after they do.
- The underground row's camera is a `# tune` placeholder (tunnels are at
  TUNNEL_Y = -30 under the entrance at 21,1); `place::Here` follows the
  feet when a body exists — the capture wave may need to verify the shot
  picks up the underground look and adjust the row.

## S9 — perf (`perf.rs`; no renderer changes)

- Draw calls: a `RenderApp` system counts binned batches + sorted items over
  every view (camera passes, prepasses, shadow maps) into a shared atomic;
  `report` falls back to the (mesh, material) proxy among visible meshes
  (log marks it `(proxy)`). Note: bevy 0.20's real GPU batch lists are
  `pub(crate)` (`BinnedRenderPhaseBatchSets`), so the count is the queued
  bins/items — close to, slightly above, the batches the GPU ends up with.
- Texture VRAM estimate (loaded images only, BC-aware texel sizes, ×4/3 for
  mips; shadow maps excluded), per-owner batch tallies in the breakdown,
  `FRIENDSLOP_PERF=3` = `=2` plus the breakdown every 30 s after 25 s, and a
  first line logging graphics preset + resolution + WGPU_BACKEND.
- `scripts/perf_sweep.sh` batches `FRIENDSLOP_PERF=3 FRIENDSLOP_START=<name>`
  runs and appends their `perf:` rows to `hardening/perf-sweep-raw.txt`.
- `hardening/perf-budget.md` holds the table header and the budget rule
  (median ≤ 16.6 ms, p95 ≤ 33 ms at 1280x720 High). **Rows are filled in a
  later wave** from the sweep log; the six predicted-worst starts first.

## S10 — web

- `scripts/build-web.sh`: `wasm-opt` is hard-required now (clear error to
  install binaryen; the silent unoptimized fallback is gone).
- `tools/web-textures`: the **256** size rung is documented (512 stays the
  local default; 0 skips).
- **For the CI agent (web.yml is theirs, untouched here):** the deploy step
  should set `WEB_TEXTURE_MAX=256` — half the texels and about a quarter of
  the bytes for phones. Worth a quick eyeball of blur before merging.
- Handshake parity: the handshake greeting bytes are pure fns in
  `crates/wire` (`handshake_greeting`, `handshake_greeting_ok`), called by
  `net_web.rs`; `handshake_greeting_matches_bevy_replicon_iroh` pins them to
  `bevy_replicon_iroh`'s marker handshake. Header/datagram edge fuzz
  (`header_roundtrips_edge_sizes`, `datagram_edge_roundtrips`,
  `header_fuzz_lcg`) belongs to the tests workstream (`claude/nightly-tests`,
  S2 wire tests) — not duplicated here to keep the shared `crates/wire`
  tests mod conflict-free.

## Verification (run before push)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo clippy -p friendslop --target wasm32-unknown-unknown
```

plus the serialized final gate under
`flock /Users/lume/friendSLOP/target/.build-lock`.
