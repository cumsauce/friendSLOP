# friendSLOP improvement audit (2026-11-16)

Read-only audit of `main` at `4e71e4c`. Nothing was built, run or measured: every
performance and rendering payoff below is an estimate from reading code. Six audit agents
covered one area each; the claims marked **(checked)** were re-read by hand afterwards, the
rest are as the agents reported them with file and line.

Sizes: **S** under an hour of agent time, **M** a focused session, **L** multi-session or risky.

## Contents

1. [What is already in good shape](#1-what-is-already-in-good-shape)
2. [Findings by area](#2-findings-by-area)
3. [Parallel plan: 12 lanes, then two more waves](#3-parallel-plan)
4. [Worktrees and the shared target dir](#4-worktrees-and-the-shared-target-dir)

---

## 1. What is already in good shape

Don't spend agents here.

- **Crate layering**: every `Cargo.toml` matches the documented order; no foundation crate imports game code.
- **Protocol registration**: all 205 registrations sit in protocol functions, order locked by `tests/it/protocol_order.rs`.
- **Areas ratchet**: `NOT_YET_IN_AN_AREA` is empty; the 19 `OnEnter(InGame)` files are exactly `areas::GLOBALS`.
- **Rules**: no `unwrap`/`expect` in non-test code, no TODO/FIXME, no cfg-gated registrations, no unseeded RNG, `Instant` is `bevy::platform` everywhere.
- **Shared mechanisms already used**: click prompts (`interact`, 36 features), teleport (`fps::teleport`), place looks, HUD text change checks, shared people meshes.
- **Networking payloads**: no `Transform` replicated; `Walker` is a `Vec3` + `f32`.

---

## 2. Findings by area

### 2.1 Overlapping features to generalize

| # | What is duplicated | Evidence | Shared version | Size |
|---|---|---|---|---|
| G1 | "Who sent this" lookup: the `ClientId::Server => None` match appears **49 times**, `fn owner_of` is defined in **9 files** **(checked)**, plus `sender` ×2, `player_id` ×3, `player_of` ×1. This is the host-is-id-0 edge case written by hand each time. | `warehouse/mod.rs:331`, `smoking.rs:225`, `skills.rs:115`, `shipwreck.rs:239`, `kori.rs:335`, `affair.rs:191`, `vessel_jump.rs:101`, `villagers.rs:308`, `cat_grave.rs:86` | `PlayerOwner::of(ClientId)` in `base/src/player.rs` | S + sweep |
| G2 | "Move this body" wrapper `fn put(..)` around `fps::teleport`, identical in 4 files | `warehouse/mod.rs:322`, `infinite.rs:522`, `surf.rs:97`, `fulton.rs:2512` | `fps::Body` query + `fps::put` in `play/fps.rs` | S |
| G3 | Per-feature "News" toast events: **13 structs** **(checked)** shaped `{to, text}` each with its own sender and observer that only calls `Toast::show` | `boba.rs:139`, `toilets.rs:81`, `hats.rs:133`, `wallet.rs:243`, `ice_cream.rs:133`, `shipwreck.rs:180`, `hide_and_seek.rs:99`, `duel.rs:101` | one `hud::News { to: Option<u64>, text }` + `hud::tell`. Changes registration, so re-bless the protocol snapshots. Keep `HighNews` (extra flag). | M |
| G4 | Seeded RNG: the same LCG hand-written in **18 files** **(checked)**; a byte-identical splitmix `unit(seed)` in `toilets.rs:99` and `kori.rs:265` with five near copies | `stats.rs:81`, `caught.rs:91`, `mushroom.rs:81`, `watcher.rs:77`, ... | `friendslop_core::rng::{Dice, unit, hash2}`. Keep exact constants per call site or seeded layouts shift. | S + sweep |
| G5 | Data-file test loader: ~41 copies of the `CARGO_MANIFEST_DIR` + `fs::read` + `set_bytes` preamble; 73 copies of `pub fn tuning() -> Arc<Tuning>` | `boba.rs:486`, `toilets.rs:317`, `hats.rs:326` | `Data::<T>::load_for_test(..)` in `crates/data`; optional `tuning!` macro | S + sweep |
| G6 | "Give every player component X on join": ~16 near-identical `On<Add<Player>>` observers | `fart.rs:89`, `smoking.rs:239`, `wendys.rs:166`, `gamble.rs:275`, `affair.rs:205`, `wallet.rs:250` | `app.give_players(\|\| X::default())` in `base/player.rs` | S + sweep |
| G7 | WFC tile kit: `Side`, `Piece`, `TileSet`, `turned`, `variants` exist in both the warehouse and Fulton; `turned` is identical | `warehouse/wfc.rs:36-116`, `fulton.rs:350-486, 630-660` | `friendslop_wfc::kit` with a generic `Piece<Extra>` | M |
| G8 | Walk-in portals: a server loop over every `Walker`, a "are you in the trigger" test, teleport with a `+0.05` lift; and the same "too far away: walk closer" leave check | `penn.rs:559`, `high_places.rs:370`, `fulton.rs:2529`, `surf.rs:113`, `miami.rs:232`; leave: `fulton.rs:2552`, `surf.rs:138`, `nyc.rs:97`, `underground/mod.rs:385` | server-side `Portal { zone, to, yaw }` + `doorway::leave_via` in `play/doorway.rs` (today `Doorway` is visual only) | M |
| G9 | Shops: click, reach, check cash, fill broke/bought line, send news | `boba.rs:164-220`, `hats.rs:98-125`, `high_places.rs:307-331` | `wallet::charge` + `wallet::fill_line`; promote `hats::pay` | M |
| G10 | Small steering maths: `step_towards` identical in `kittens.rs:140` and `rats.rs:153`; yaw-towards in 4 files | `people.rs:969`, `doorway.rs:149`, `duel.rs:156` | `base::rules::steer` | S |

### 2.2 Structure and architecture

- **A1. Scenery/zones `covers` calls can go now (S).** `zones.rs:54-56` and `scenery.rs:108` still call `warehouse::covers`, `wendys::covers`, `underground::covers` **(checked)**. All three features already claim the same ground, with tests asserting `covers ⊆ claimed`. Delete the four calls, the three functions and their tests. The full move of scenery/zones into `friendslop-world` is **L** and blocked by more than AGENTS.md says: `areas::AreaApp`, `lod::StaticTree`, `ambience::Sway`, `screenshot::frozen`. Realistic target: pure layout generators move to world, a thin spawner stays in game.
- **A2. Sites have no ratchet (M).** `tests/it/sites.rs` only checks ids exist. About 75 feature files never call `sites::`. Files spawning with neither `sites::` nor `areas::`: `cat_grave.rs`, `toilets.rs`, `watcher.rs`, `romance.rs`, `doorway.rs`. Add a `NOT_YET_SITED` list modelled on the areas one, then shrink it.
- **A3. `side-nyc` can build in parallel with village and story (S, needs a build to confirm).** It declares `village` and `story` as deps **(checked)**, but its only use above `play` is `high_places.rs:37` importing from `smoking` (places). Dropping the two deps takes 15k lines off the serial chain.
- **A4. Oversized files with clean seams (M each, pure moves).**
  - `fulton.rs` (4104) → `fulton/{tuning,plan,layout,server,visuals,tests}.rs`, mirroring `warehouse/`.
  - `people.rs` (1872) → `people/{look,tint,rig,build}.rs`.
  - `menu.rs` (1621) → `menu/{picker,join,backdrop,mod}.rs`.
  - `duel.rs` (1549) → `duel/{mod,arena,view,hud}.rs`.
- **A5. Merge-conflict hotspots.** Every new feature edits `game/src/features/mod.rs` (53 touches in the last 300 commits), its crate's `lib.rs` and `data_files()`, and the two protocol snapshots. The protocol list and plugin list are separate hand-kept lists in different orders. Worth one registration macro or a single table per feature later; for now, see the merge rules in section 3.
- **A6. `ambience` exists twice** (`game/src/ambience.rs` and side's `features::ambience`), both plugins added. Rename one.
- **A7. Private-internals reach.** `high_places.rs:37` (smoking's components), `fat_cat.rs:273` (`villagers::VillageNav`), `kitty_lemonade.rs:18` (`skills::{Lapped, PeePuddle}`).

### 2.3 Performance (unmeasured)

| # | Finding | Evidence | Fix | Size | Measure first? |
|---|---|---|---|---|---|
| P1 | **Fulton creates and drops 1 mesh + 2 materials every frame.** `stream_levels` calls `PostLook::new` unconditionally **(checked)**; the struct comment says "made once". | `fulton.rs:2931`, `:2640` | `Local<Option<PostLook>>` | S | no |
| P2 | Fulton's eight local systems run for everyone whenever `Station` exists; `light_shadows` sorts every `CellLight` each frame | `fulton.rs:2599-2612`, `:3273` | gate on `areas::shown`; re-pick only when the camera changes cell | S | no |
| P3 | Fulton lights: one `PointLight` per cell over three streamed levels (up to ~108), range 9 m on 6 m cells, two shadowed point lights = 12 shadow views | `fulton.rs:2994`, `fulton.ron:26,47,55` | light only your level or shorten range; shadowed ones as down-facing `SpotLight` | S-M | `FRIENDSLOP_PERF=3 FRIENDSLOP_START=fulton` |
| P4 | `areas::follow::<C>` runs per replicated entity per frame for 18 types; each call clones an Arc, collects and sorts a `Vec`, clones a `String` | `areas.rs:129, 294, 347` | skip unless transform or `Loaded` changed; `area_at` returns an index | S-M | bot `tour` |
| P5 | Host simulation gated only by `ServerState::Running`, not its site (breaks the #412 rule) | `villagers.rs:150`, `pets.rs:113`, `rats.rs:113`, `affair.rs:183`, `watcher.rs:91`, `shipwreck.rs:151`, `fat_cat.rs:178`, `skills.rs:97`, `smoking.rs:215` | `.run_if(sites::awake(..))`; confirm each body has no early-out first | M | bot `tour` |
| P6 | One static body entity per building piece, per Fulton box, per tree and rock; no `Collider::compound` anywhere | `buildings.rs:1063`, `fulton.rs:2468`, `scenery.rs:402` | one compound per house/level, split by walkable layer | M | collider census |
| P7 | `fat_cat::blink` calls `materials.get_mut` per cat per frame before comparing | `fat_cat.rs:580` | `.get()` then compare | S | `materials changed /s` |
| P8 | Sound occlusion casts a ray for paused emitters too | `spatial.rs:845-878` | skip asleep emitters | S | no |
| P9 | `height_at` takes an RwLock and clones an Arc per sample; the ground collider samples 111×111 | `terrain.rs:222-244, 598` | fetch the grid once per build | S | no |
| P10 | Area loads run every spawn system for the area in one frame on the main thread | `areas.rs:216`, `buildings.rs:1013` | build in a task or spread over frames (browser has no threads) | L | bot `borders` |

### 2.4 Code quality

- **Q1. Unused dependencies copied between game-layer crates (M, low risk).** By grep, `play`, `story`, `village`, `afterlife`, `places`, `side`, `base` and `game` each list deps no source line names (rerecast/landmass crates, `serde_json`, `bevy_voice_chat`, `fusion`, `friendslop-art/-wfc/-structures`, `modeler_bevy`). `game` lists `chrono` but only `base/features/whispers.rs:93` uses it **(checked)**. Grep can miss macro and feature-activation uses, so confirm with `cargo machete` or a native + wasm `cargo check` per removal. Also move the `modeler` git rev (repeated in 7 manifests) and `foley` to `[workspace.dependencies]`.
- **Q2. A key written in shown text (S).** `sesbian_lex.rs:151` shows `[L]` **(checked)**; should be `{action_id}` through `input::fill`.
- **Q3. Client-trusted values (S each).**
  - `kori.rs:388-407`: `ClawEnd(Option<u8>)` lets the client name the prize it won; the server checks only that a go was paid for and the index is in range **(checked)**.
  - `fart.rs:176-180`: uses the client-sent position for shame judging though the server has the `Walker`.
- **Q4. Writing in Rust (M, per feature).** About 130 player-facing literals. Densest: `afterlife.rs` (8), `halt.rs:75-81` (monologue), `hide_and_seek.rs` (5), `fate.rs` (5), `warehouse/mod.rs` (5), `mute_spell.rs:160-185`, `random_dialogue.rs:126-145`, `voice_lines.rs:102-112`, `campaign.rs:57,106-112`, `journal.rs:605,622,743`.
- **Q5. Tuning and layout in Rust (M-L).** `house_odds.rs:102-118`, `ambience.rs:127-135` (no data file at all), `miami.rs:67-81`, `rules/nyc.rs:8-24`, `kcc.rs:38-64`, `trains.rs:144-230` (21 consts incl. `RIDE_GAP`, `TUNNEL_SPEED`), `wendys.rs:47-52`.
- **Q6. `GAME_DESIGN.md` compiled in with `include_str!`** at `fault.rs:16` and `journal.rs:30`. Either load it as data (touches `build-web.sh`) or record it as the allowed exception.
- **Q7. Small cleanups (S batch).** Blanket `#![allow(unused_imports)]` on six `base/src/rules/*.rs`; unreferenced `graphics::is_low`, `inst::by_name`, `campaign::goal_line`, `objective::objective_line`, `places::centre_at`; 13 unused colours in `art/theme.rs` (confirm they aren't the design palette); `tests/it/replication.rs:810` sleeps 5 ms of wall clock.
- **Q8. Long functions**, only when next touched: `warehouse/visuals.rs:395 build_inside` (464 lines, depth 8), `grammar.rs:345 build` (350), `menu.rs:957`, `scenery.rs:447 cluster` (233, depth 9).

### 2.5 Documentation

- **D1. Gameplay loop is 6,779 chars against the ~1,800 limit** **(checked)** (`GAME_DESIGN.md:15-88`). The Discord bot posts it. Rewrite and add a length test beside `bsn_salting.rs`.
- **D2. AGENTS.md architecture tree is stale.** Twelve files it lists under `crates/game/src/` live in `crates/base/src/` (`player`, `voice`, `assets`, `spatial`, `people`, `graphics`, `textures`, `place`, `areas`, `sites`, `rigged`, `web`); eleven real `game/src` files are missing (`atmosphere`, `camera`, `bot`, `zones`, `ambience`, `ui/`, ...); `crates/android` appears nowhere; `net_wire.rs` doesn't exist; feature lists per crate are incomplete. Replace with one line per crate plus "`ls` the features dir".
- **D3. AGENTS.md is ~31k chars (~8k tokens) in every agent's context.** Keep hard requirements, layering, protocol order and the check commands (~2.5k tokens). Move the modeler pipeline, maps, areas/sites, perf/bot and screenshot sections into `docs/agents/*.md` or project skills, each behind a one-line pointer. The modeler paragraph currently sits inside "Interpreting ideas".
- **D4. The protocol snapshot step is undocumented.** `protocol_order.rs:5` needs `BLESS=1 cargo test -p friendslop --test it protocol` after any registration change; no doc mentions `BLESS`. Parallel agents will all conflict on the two snapshot files.
- **D5. Env vars.** Nine implemented but missing from AGENTS.md (`FRIENDSLOP_LOW`, `_MUTE`, `_SEED`, `_TIME`, `_ASSETS`, `_BENCH`, `_SCREENSHOT_CAMERA`, `_SCREENSHOT_WAIT`, `_SCREENSHOT_WAREHOUSE`); the screenshot scene list appears twice with different members. One table.
- **D6. Stale root files.** `HANDOFF.md` describes `vendor/rerecast/` and a `[patch.crates-io]` that no longer exist **(checked)**; `hardening/*.md`, `ADMIN_TODO.md`, `shots/`, `SIGNAL.txt` are clutter agents will read. Delete or move to `docs/archive/`.
- **D7. Drift.** `features/README.md:4` omits `side-nyc`; docs say `cargo test --workspace` while CI runs nextest per layer; `cargo dev host` isn't in Commands; Open questions is 46 bullets (5.9k chars) and `/whatsneeded` reads all of it.

### 2.6 Agent tooling

- **T1. CI never runs `friendslop-side-nyc` unit tests** **(checked)**: `ci.yml:146-148` lists every game-layer crate except the largest one.
- **T2. No canonical check script.** No justfile, Makefile or xtask. Add `scripts/check.sh`: fmt, clippy, tests for changed crates and dependents only, wasm clippy only when the paths CI watches changed. Mirror CI's profile env so fingerprints don't fork.
- **T3. Scripts hardcode `$root/target/debug/friendslop`** **(checked)** (`screenshot.sh:25`, `goldens.sh:63`, `perf_sweep.sh:21`). They break under any `CARGO_TARGET_DIR`. Resolve through `cargo metadata`.
- **T4. `target/` is 143 GB** and nothing collects it (101 GB `debug/build`, 36 GB incremental). Add `cargo sweep --time 7` to the setup script.
- **T5. Runtime clashes between agents.** BRP port fixed at 15702 (`bot.rs:183`) **(checked)**: let `FRIENDSLOP_BRP=<port>` set it. `run.md` step 8 writes `/tmp/shot.png`. The perf bot needs a visible window, so serialize perf runs behind a lock.
- **T6. Modeler isolation.** One shared daemon, no project pinned per worktree; exports can land in the main checkout. Generate a per-worktree `.mcp.json` with `--project <worktree>/crates/game`; add a hook that rejects `mcp__modeler__*` calls without `doc`.
- **T7. No hooks, skills or commands in `.claude/`.** Useful ones: rustfmt on edited `.rs`; a guard that warns on non-append edits to `features/mod.rs`; skills `new-feature` (module, `lib.rs`, `mod.rs` append, `sites.ron`, data file + test, BLESS), `add-data-file`, `rebake-probes`, `screenshot`, `perf-bot`, `modeler-export`. These absorb the text D3 removes.
- **T8. Allowlist** lacks `scripts/*`, `git lfs`, `mcp__modeler__*`.
- **T9. `claude.yml`** restores the rust cache but never saves (`save-if: false`, line 92) and runs plain `cargo test --workspace` instead of nextest. `cargo-nextest` and `sccache` aren't in `flake.nix`.

### 2.7 Rendering

Current state: TonyMcMapface everywhere; HDR + bloom + auto exposure on Medium/High only; FXAA; SSAO on High; 3 sun cascades to 80 m; linear fog; hand-coloured sky dome with stars, moon, sphere clouds and no sun disc; all `StandardMaterial`, no custom shaders. Never used: TAA/SMAA, contact shadows, `ColorGrading` (only the weed effect), volumetric fog, shadow map size or bias tuning.

| # | Improvement | Evidence | Payoff / GPU cost | Size | Web-safe |
|---|---|---|---|---|---|
| R1 | **Dark interiors on Low/browser.** `place_looks.ron:21-24` relies on auto exposure, which is desktop-HDR only, and probes are skipped in wasm. Write a fixed `Exposure` from the look when there's no HDR, or a per-look `low_ambient`. | `graphics.rs:504`, `probes.rs:61` | high (browser) / 0 | S | yes |
| R2 | **Fog falloff.** `FogFalloff::Linear` **(checked)** reads as a flat wall. Use exponential-squared or `from_visibility_colors`; scale the sun-glow colour by daylight (it stays on at night). | `atmosphere.rs:305` | high / 0 | S | yes |
| R3 | **Colour grading per time of day and place**: warm dusk, blue night, green sewers, cold Fulton. Layer with `weed.rs:133-171`, which writes `ColorGrading` itself. Check whether grading applies on the non-HDR path before promising it for Low. | no `ColorGrading` on the camera | high / ~0 | S-M | check |
| R4 | **A visible sun**: emissive disc + soft halo along `sun_dir`, blooms on Medium/High; moon halo too. | `atmosphere.rs:556-617` | med-high / 0 | S | yes |
| R5 | **Better AA on High.** High sets `msaa: true` but MSAA only runs when SSAO is off **(checked)**, so High is FXAA. Use SMAA (works with SSAO). | `graphics.rs:687` | med-high / ~1 ms | S | no |
| R6 | **Water**: flat alpha disc with no normals. Scrolling ripple normal map, darker base, shore foam ring in the ground mesh. | `terrain.rs:724-729` | med-high / low | S-M | yes |
| R7 | **Ground**: 128 px greyscale grass tiled every 5 m, no normal map, uniform roughness. Generated normal map (needs tangents), a second detail scale, rougher dirt, anisotropy. True splatting needs the repo's first custom material: defer. | `terrain.rs:525, 648`, `art.rs:79` | med-high / low | M | yes |
| R8 | **Anisotropic filtering on model textures**: only procedural surfaces get 8×; NYC streets smear at grazing angles. | `main.rs:22-50`, `surfaces.rs:362` | med / tiny | S | lower value |
| R9 | **Blob shadows on Low**: shadows are off, characters float. | `graphics.rs:218`, `people.rs` | med (browser) / ~0 | S-M | yes |
| R10 | **Contact shadows on High** for feet and small props beyond cascade resolution. | unused | med / 0.5-1 ms | S | no |
| R11 | **Shadow map quality**: no `DirectionalLightShadowMap`, default biases; fade the sun through the horizon to hide the sun-to-moon pop. | `atmosphere.rs:256` | med / low-med | S | no |
| R12 | **Time-of-day reflections**: one noon bake used everywhere outdoors, only dimmed. Bake dusk and night variants and an NYC street probe; swap by darkness. | `probes.rs:78-92` | med / 0 at run time | M | desktop |
| R13 | **Volumetric light shafts** in forest and Fulton. | unused | high / 2-4 ms | M | High only |

Skip for now: irradiance volumes and lightmaps (no bake pipeline), procedural `Atmosphere` (fights the storybook sky), leaf transmission (already measured as too costly), SSR (needs deferred).

---

## 3. Parallel plan

### Step 0, before any agent starts (serial, you)

1. **Commit or stash the work in the tree.** `fulton.rs`'s prefabs and models, `surf.rs`, `rules/surf.rs`, `surf.ron` and `GAME_DESIGN.md` are modified, and worktrees branch from the last commit. Lanes 3 and 2 below touch those files.
2. **Decided: one shared target dir (setup B, section 4).** Land lane 1's shared-target prerequisites (script binary paths, `check.sh` scoped to `-p`, `FRIENDSLOP_BRP` port, per-worktree screenshot paths, perf lock) before the other lanes start, and give every worktree the same absolute `CARGO_TARGET_DIR`.

### Wave 1: 12 lanes with disjoint files

Each lane owns the files listed and touches nothing else. No lane in this wave edits `game/src/features/mod.rs`, a crate's `lib.rs` feature list, `data_files()` or the protocol snapshots, except where stated.

| Lane | Work | Owns | Items |
|---|---|---|---|
| 1. Tooling | check script, worktree setup script, script binary paths, BRP port env, hooks, allowlist, CI side-nyc tests | `scripts/*`, `.claude/*`, `.cargo/config.toml`, `.github/workflows/*`, `flake.nix`, `game/src/bot.rs` | T1-T5, T7-T9 |
| 2. Docs | slim AGENTS.md into `docs/agents/*`, fix the tree, env var table, BLESS step, archive stale files, shorten Gameplay loop and Open questions, loop-length test | `*.md`, `docs/`, `hardening/`, `.github/claude/run.md`, one new test file in `game/tests/it/` | D1-D7 |
| 3. Fulton | per-frame asset fix, gate local systems, light count, then split into `fulton/` | `side-nyc/src/features/fulton*`, `data/side/fulton.ron` | P1-P3, A4 |
| 4. Sky and light | fog falloff, grading, sun disc, Low interiors, sun fade | `game/src/atmosphere.rs`, `base/src/place.rs`, `place_looks.ron`, `day_night.ron`, `places/.../weed.rs` | R1-R4, R11 (fade) |
| 5. Camera | SMAA, shadow map size and bias, contact shadows, anisotropy | `base/src/graphics.rs`, `graphics.ron`, `base/src/textures.rs`, `game/src/main.rs` | R5, R8, R10, R11 |
| 6. Ground and water | water normals and foam, ground normal map and detail, heights fetched once | `world/src/terrain.rs`, `art/src/art.rs` | R6, R7, P9 |
| 7. Manifests | remove unused deps, workspace-level git pins, drop village/story from side-nyc | every `Cargo.toml`, `Cargo.lock`, `side-nyc/src/lib.rs` lines 18-22 | Q1, A3 |
| 8. Sites and covers | delete `covers`, add the sites ratchet, gate the host sims and the five unsited files | `game/src/{zones,scenery}.rs`, `game/tests/it/sites.rs`, `sites.ron`, and the feature files named in A1, A2, P5 | A1, A2, P5 |
| 9. Base hot paths | `areas::follow` allocations, occlusion rays for sleeping sounds | `base/src/areas.rs`, `base/src/spatial.rs` | P4, P8 |
| 10. People | split `people.rs`, then blob shadows on Low | `base/src/people*`, `base/src/rigged.rs` | A4, R9 |
| 11. Splits | split `menu.rs` and `duel.rs` (pure moves) | `game/src/menu*`, `play/src/features/duel*` | A4 |
| 12. Shared helpers | add the helpers and their tests only, no call-site sweeps: `PlayerOwner::of`, `give_players`, `fps::put`, `hud::News`, `core::rng`, `Data::load_for_test`, `rules::steer` | `base/src/player.rs`, `base/src/hud.rs`, `play/src/features/fps.rs`, `crates/core`, `crates/data`, `base/src/rules/steer.rs`, protocol snapshots (for `hud::News`) | G1-G6, G10 (helpers) |

Known overlaps to watch:

- Lanes 3 and 7 both touch `side-nyc/src/lib.rs` (different lines).
- Lane 8 edits `warehouse/mod.rs`, `underground/mod.rs`, `wendys.rs`, `villagers.rs` and friends, which wave 2 sweeps: finish lane 8 first.
- Lane 12 is the only wave 1 lane that re-blesses the protocol snapshots.
- Lanes 9, 10 and 12 all change `base`, so each rebuilds everything above it. That's a build-time cost, not a merge conflict.

Merge order: 7 (manifests) and 1 (tooling) first, since everything rebuilds after them; then the rest in any order; 12 last so wave 2 starts from it.

### Wave 2: one agent per crate (8 agents), after lane 12 merges

Each agent owns one crate's `src/features/`, its `lib.rs`, its `data_files()` and its files under `assets/data/<crate>/`. In that crate it:

- replaces the hand-written lookups and helpers with lane 12's (G1-G6, G10);
- moves player-facing text and tuning into data files with parse tests (Q4, Q5);
- applies the small fixes that live there (Q2, Q3, Q7, P7, A6, A7).

Crates: `base`, `play`, `places`, `village`, `story`, `afterlife`, `side-nyc`, `side`. Replacing the per-feature News events removes registrations, so each agent re-blesses the protocol snapshots after rebasing; merge these one at a time and re-bless on each rebase (D4). The RNG swap must keep each call site's exact output.

### Wave 3: larger pieces, after wave 2

| Work | Depends on | Can run alongside |
|---|---|---|
| G7 WFC tile kit (`crates/wfc`, warehouse, fulton) | lane 3 | G8, G9, R12 |
| G8 portals (`doorway.rs` + 7 features) | wave 2 | G7, R12 |
| G9 shops (`wallet.rs` + 4 NYC shops) | wave 2; overlaps G8 on `high_places.rs`, so same agent or after | G7, R12 |
| P6 compound colliders (`buildings.rs`, `structures/build.rs`) | measure first | G7-G9 |
| P10 async area loads (`areas.rs`, `buildings.rs`, `terrain.rs`) | lanes 6, 9 and P6 | nothing touching those files |
| R12 time-of-day probes (`game/src/probes.rs`, `probes.ron`, `assets/probes/`) | lane 4 | everything else here |
| R13 volumetric shafts (`graphics.rs`, `atmosphere.rs`) | lanes 4 and 5 | G7-G9 |
| A5 one registration table per feature | wave 2 | run alone: it rewrites the hotspot files |
| T6 modeler isolation | lane 1 | anything |

---

## 4. Worktrees and the shared target dir

What a single shared `CARGO_TARGET_DIR` does across worktrees (general Cargo behaviour, not tested here):

- **Registry and git dependencies are shared.** Bevy, avian and the rest compile once. This is the win.
- **Builds serialize.** Cargo takes one lock on the build directory per profile; a second `cargo` prints "Blocking waiting for file lock on build directory" and waits. Twelve agents running clippy and tests queue behind each other.
- **Workspace crates are not shared.** A path crate's identity includes its absolute path, so each worktree compiles its own copy of all 17 crates into the same dir. `target/` is already 143 GB; expect it to grow quickly.
- **The final binary is overwritten.** `target/debug/friendslop` is whichever worktree linked last, and three scripts run that path (T3). A screenshot can come from another agent's code.

Two workable setups:

**A. Per-worktree target + shared compile cache (recommended).** Each worktree keeps its own `target/`; `sccache` as `RUSTC_WRAPPER` with `CARGO_INCREMENTAL=0` shares dependency compilation, since registry paths are identical across worktrees. Builds run truly in parallel and binaries can't be mixed up. Cost: disk per worktree for the workspace crates and final links (seed each from a warm main target with an APFS clone, `cp -c -R`, which costs nothing until files diverge), and no incremental compilation for workspace crates.

**B. One shared target (what you asked for).** Works if you accept the queue. Needed first, all in lane 1:

- scripts resolve the binary via `cargo metadata` and copy it to a per-worktree path before running;
- agents run only `scripts/check.sh` scoped to their crate (`-p <crate>` and dependents), never `--workspace`;
- `FRIENDSLOP_BRP` takes a port; screenshots write under the worktree; perf-bot runs take a lock;
- a `cargo sweep` routine.

With B, wall-clock time for 12 agents is roughly the sum of their builds, and the lanes that change `base` (9, 10, 12) each trigger a rebuild of the full game layer. With A it's the longest single build.

Either way, per worktree:

- Git LFS files must be materialized (`git lfs pull`) and `BEVY_ASSET_ROOT` must point at that worktree's `crates/game`.
- Modeler calls pass `--doc <model>` and the daemon must be pointed at that worktree's `crates/game` (T6). Only lanes 4-6 and wave 3 might touch models; none of wave 1 requires it.
- Only one agent at a time can run the game with a visible window for perf numbers.
