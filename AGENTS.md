# AGENTS.md

friendSLOP is a game **designed by a group of friends through GitHub issues and built only by AI
agents**. Nobody coordinates. Ideas arrive one at a time, often contradicting each other. Your job is
to turn each one into working, merged code while keeping the game coherent, fun and building.

If you're running in CI, also read `.github/claude/run.md`.

## Hard requirements (never break these)

1. **Multiplayer, host-as-server.** One player hosts (listen server), the others join with a code.
   No dedicated server and no infrastructure we'd have to run. Transport: iroh P2P (`src/net.rs`).
2. **Voice chat works** for everyone in a session (`src/voice.rs`, `bevy_voice_chat`).
3. **3D**, Bevy **0.20**, physics with **avian3d**.
4. **Some shared goal** the players work towards together. *What* it is comes from the friends.
5. **CI stays green** (Linux only: fmt, clippy, tests). Still avoid platform-specific code. Run fmt,
   clippy (`-D warnings`) and tests before you push.

## Interpreting ideas

- The friends design the game; you implement it. **Don't invent beyond what an issue asks for.**
  When an idea is vague, choose boldly but minimally, then say what you chose in the PR and the
  issue comment.
- Friends may change or remove anything, including other people's features, when an issue asks for
  it. Otherwise prefer reconciling over deleting (see `.github/claude/run.md`).
- Placeholder art is fine and expected: primitives, flat colors, generated meshes. Big binaries
  (models, sounds) go through Git LFS only, live in `crates/game/assets/`, and must be CC0 (or
  equally permissive) with entries in `assets/CREDITS.md`. Prefer generating procedurally or
  keeping it tiny when an asset isn't really needed, and never let gameplay *depend* on asset
  files: they must degrade quietly if LFS data is missing.
- **Models come from the modeler** (every model in the game is made there now). Run it with `crates/game` as the project so editable sources land in
  `crates/game/.modeler/` (prefabs, procedural materials, the `friendslop` style) and
  `export_model("models/....gltf", optimize: "game")` writes into `assets/`, its textures as
  shared KTX2 files in `assets/textures/` (the web build decodes them to PNG for phones). Model
  at real size (1 unit = 1 m), group parts that share a material so they share one texture
  atlas, use tile-domain materials where models can share a bake, and `history_prune` before
  saving so the sources stay small. Swap a code-built object for a model by spawning the glTF
  where its meshes were and keeping its colliders as mesh-less entities (gameplay never
  depends on the file). Building props are marked by `Kit::begin_prop` and drawn from
  `models/props/` when listed in `buildings::PROP_MODELS`; the six building surfaces take
  their textures from `models/surfaces/`. Creatures and characters are rigged
  in the modeler (skeleton, bind, clips) and drawn with `crate::rigged` (put `Rigged` on the
  entity, switch `RigClip` from its state); trees sway live through the modeler's runtime
  plugin (`modeler_bevy`, a pinned git dependency). Several agents can share one modeler daemon, but
  pass `--doc <model>` on every call or saves and exports can land on another agent's model.
  In CI (`claude.yml`) the modeler is already an MCP server (`mcp__modeler__*`, project
  `crates/game`, renders in software, so `agent_views` works but is slow): use it directly.
- **List keywords (#51).** An issue may tag a list of examples with a keyword in parentheses:
  - `(strict)`: put exactly these examples in the game, no more and no fewer.
  - `(expand, x..y)`: put exactly these examples in, then invent between x and y more in the same
    style.
  - `(replace, x..y)`: don't use the examples. Invent between x and y new ones in the same style
    instead.

  For both `expand` and `replace`:
  - a single number means exactly that many;
  - a word means interpret it as an amount ("a few", "loads");
  - no second argument means pick a sensible amount.

  An untagged list means use your judgement, but treat obvious "e.g." lists as examples, not
  requirements, and say in the PR how you read it.
- Keep `GAME_DESIGN.md` truthful: what exists, what friends said, open questions. It's the game's
  memory between runs.
- Keep its **Gameplay loop** section current (#153): re-read the whole loop after your change and
  rewrite that section so it says how play goes now, start to finish, and where it's thin. Plain
  words for friends, under ~1800 characters. The Discord bot posts it every few landed ideas
  (and on `/loop`).

## Nudging

The hint block in `.github/ISSUE_TEMPLATE/idea.yml` (between `CLAUDE-HINTS-START/END`) and the
"Open questions" in `GAME_DESIGN.md` are how we steer people towards what still needs ideas. Keep the
hints to 1-3 short lines naming *areas* that are thin ("nobody has said how a round ends"), never
specific suggestions. The Discord bot's `/whatsneeded` reads the Open questions section.

## Architecture

```
crates/game/src/
  lib.rs        AppState (Menu/InGame), ProtocolPlugin (shared registration), GamePlugin
  main.rs       DefaultPlugins + RepliconPlugins + PhysicsPlugins + GamePlugin; `host` / `join <code>` CLI
  net.rs        iroh host/join, NetStatus, clipboard join codes
  player.rs     server spawns a replicated `Player { id }` per connection (id 0 = host)
  voice.rs      voice chat driving (M = mute); speakers tagged on Player entities
  menu.rs       placeholder keyboard menu
  world.rs      camera, light, floor, HUD
  assets.rs     handles for the files in assets/ (sound effects; missing files degrade quietly)
  spatial.rs    3D sound mixer (play_at / play_on / loop_at / play_wav_at) and the listener
  loading.rs    "Loading" curtain on entering the game until models load and frames settle
  people.rs     shared commune-person body (players + villagers), walk animation
  scenery.rs    nature dressing on the floor, commune campfire, music (local visuals, seeded)
  controls.rs   touch overlay + gamepad, fed into the same keys / WalkInput as keyboard+mouse
  graphics.rs   Low/High graphics preset (Low by default in the browser)
  textures.rs   model textures: capped at 1024 px and mipmapped as they load (#227)
  lod.rs        trees: lighter far meshes, dithered in by distance (#227); modeler LOD nodes; tree wind
  rigged.rs     rigged modeler models: `Rigged { path }` + `RigClip` crossfades their named clips; tails/ears/bobbles get modeler_anim springs
  net_web.rs    wasm32 net.rs: iroh relay-only in spawn_local tasks, same wire format (crates/wire)
  web.rs        wasm32 helpers (localStorage)
  screenshot.rs FRIENDSLOP_SCREENSHOT=out.png: frame, save one picture, quit (scripts/screenshot.sh, #167);
                FRIENDSLOP_SCREENSHOT_SCENE forces a UI state (play/busy/clickable/chapter/tab/title/settings/loading/clean)
  features/mod.rs  the ordered FeaturesProtocolPlugin / FeaturesPlugin lists (features live in the game layer)
crates/base/src/     friendslop-base: player, assets, settings, voice, hud, people, spatial, rigged, graphics,
                     textures; `rules/` (shared vocabulary: house odds, skills, depths, campaign events,
                     villagers, morality, Duelist, EYE); `shared` (slices of world/camera/controls/ui/zones/
                     scenery that features read); low features (bear, stillness, synth_audio, babble, ...)
crates/play/         friendslop-play: fps, interact, duel, climb, stats, fate
crates/places/       friendslop-places: warehouse, underground, smoking, skills, fart, fart_sound, helicopter
crates/village/      friendslop-village: villagers, caught, punishment, hide_and_seek, romance, wendys, ...
crates/story/        friendslop-story: campaign, objective, landlord, totems, gods_tears, abuela, gamble, ...
crates/afterlife/    friendslop-afterlife: afterlife, fat_cat, kitty_lemonade, mute_spell, systemd
crates/side/         friendslop-side: standalone ideas (affair, fusion, goat, rats, shipwreck, ...)
crates/fusion/       the generated soundtrack (no Bevy): `cargo run -p fusion --bin fusion-render -- help`
crates/art/          friendslop-art: procedural materials/meshes/textures (`art`), mipmaps (`mips`), mesh LODs (`lod`) and the UI theme (`theme`)
crates/kcc/          friendslop-kcc: kinematic character controller (bevy_ahoy port, own licence files)
crates/wire/         friendslop-wire: the iroh wire format shared by the desktop and browser transports
crates/core/         friendslop-core: AppState, DrawRange and Detail, the few types every crate shares
crates/world/        friendslop-world: the map: terrain, buildings, landmarks, surfaces, places
crates/structures/   friendslop-structures: building kit, house grammar, room graph + hallway kit, prop library + placer, merged meshes, world-space UVs, colliders
crates/wfc/          friendslop-wfc: generic seeded wave function collapse (sockets, pins, retry, grid graph helpers)
crates/game/assets/  CC0 art: glb/gltf models, sounds, music (Git LFS; see assets/CREDITS.md)
crates/game/tests/it/ headless multiplayer tests (replicon test_app, no sockets)
```

### Crates (how the code is layered)

The workspace has three layers, and dependencies only ever point down:

1. **Foundation crates**: reusable building blocks with **no game code**. They depend only on
   Bevy, avian and std, never on `friendslop`, and on each other only as listed:
   - `fusion`, `friendslop-wire`, `friendslop-kcc`, `friendslop-core`, `friendslop-wfc`,
     `friendslop-structures`: nothing of ours.
   - `friendslop-art`: nothing of ours (`art`, `theme`, and the cat `palette` maths).
   - `friendslop-world`: `core`, `art` and `structures`.
2. **The game layer** (split so a change rebuilds only its crate and those above it; a single
   47k-line crate took CI 18 minutes): `friendslop-base` → `play` → `places` → `village` →
   `story` → {`afterlife`, `side`}. Each crate's `features` module re-exports every feature
   below it and its lib.rs re-exports base's modules, so code keeps writing `crate::hud`,
   `crate::features::fps`, `super::warehouse` unchanged.
3. **The game** (`crates/game`, package `friendslop`): networking, menus, the UI, camera,
   controls, scenery and zones, and the ordered plugin and protocol lists in `features/mod.rs`. It re-exports the foundation crates under
   their old module names (`crate::art`, `crate::theme`, `crate::kcc`, `crate::net_wire`,
   `crate::AppState`, `crate::terrain`, `crate::buildings`, `crate::landmarks`,
   `crate::surfaces`), so feature code uses them like any other module.

Rules for growing it:

- **Where new code goes.** A new idea goes in `crates/<crate>/src/features/` of the lowest
  game-layer crate that has everything it needs, usually `side` (a leaf nothing imports);
  add `pub mod <idea>;` to that crate's lib.rs and register it in the game's
  `features/mod.rs`. A type or constant that features in two crates need moves to
  `friendslop-base::rules` (leave `pub use crate::rules::<x>::*;` at its old home). Never
  make a lower crate import a higher one: move the shared bit down instead. Put something
  in a foundation crate instead when it needs no game state: generators (meshes, textures,
  sounds, maps), maths, file or wire formats, synths, controllers. A Bevy system that reads or
  writes game components stays in the game.
- **Write features as pure functions plus thin systems.** Put the decisions (who gets blamed,
  what a visit gives you, where a thing stands) in plain functions that take values and
  return values, and keep the systems as wiring. They're cheap to test and can move down to a
  crate later without a rewrite.
- **Networking registration has one order.** `replicate`, `add_*_message` and `add_*_event`
  are called only from protocol functions, and those run only from the ordered list in
  `crates/game` (`ProtocolPlugin` / `FeaturesProtocolPlugin`), appended at the end. A
  foundation crate never registers anything.
- **Every crate builds for the browser.** No threads, `std::fs`, `std::time::Instant` or
  `SystemTime` in them (see Web, below). Native-only dependencies are target-gated.
- **Making a new crate.** Make one when a module has a clean seam (it only needs foundation
  crates and Bevy) and is either reused, slow to compile or big enough to stand alone. Don't
  split just because a file is long. Name it `friendslop-<thing>` in `crates/<thing>`, inherit
  the workspace lints and edition, say "No game code" in its description, keep its tests
  inside it (`cargo test -p friendslop-<thing>` is the fast loop), re-export it from
  `crates/game/src/lib.rs` if many paths use it, and add it to the tree above and to this list.
- **Features talk to each other through small public surfaces.** Use another feature's `pub`
  consts, functions and events, not its private components or internals. When two features
  need each other, give the shared bit to the one that owns it and keep the link one-way where
  you can.
- **The map is its own crate** (`friendslop-world`). It never imports features. A fixed place
  the ground is shaped around (a flat arena, a path's end) goes in `world::places`, and the
  feature imports it from there. Ordering hooks are system sets the world exposes (the cat
  palette runs `.after(SurfaceSet)`). Settings reach it as `core` resources (`Detail`,
  `DrawRange`), not the game's video settings.
- **Maps from the modeler.** Lay out an area as a modeler document (prefab instances, markers
  with `components`, a hidden `terrain` for its heights) and `export_model("maps/<area>.bsn")`
  from `crates/game`. The vocabulary of components a map may carry is
  `crates/game/.modeler/components.json` (each type must also be `register_type`d in the game);
  its header is the salting. `friendslop_world::map` loads the scene (jackdaw's BSN parser, not
  its runtime), and `terrain::BAKED` embeds each `<area>.<terrain>.heights` so `height_at` uses
  them on every peer. The commune's picnic corner (`commune-map`) is the example.
- **Claim ground, don't get imported.** A feature that needs clear ground calls
  `friendslop_world::footprints::claim(at, radius)` in its protocol function, and scenery and
  zones keep their trees, bushes and rocks off it (the shipwreck does this).
- **Next split (not done yet): scenery and zones.** `scenery.rs` and `zones.rs` still import
  older features' spots (`warehouse::covers`, `WENDYS_AT`, `underground::covers`,
  `totems::VAULT_AT`, ...) and lean on the camera and ambience. Move those features over to
  `claim`, then scenery and zones can join `friendslop-world`.

### Networking rules (bevy_replicon 0.45)

- The server is authoritative. Gameplay state lives on server entities with `Replicated` and is
  registered with `app.replicate::<C>()` inside a **protocol** function (see features/README.md).
  Registration order must match on every peer, so always append and never reorder.
- Clients send intent (input, requests) with `add_client_message`/`add_client_event`, and the
  server validates it and changes state.
- The host is a client too: listen-server code must work when `ServerState::Running` with no remote
  `ClientState`. Guard server logic with `run_if(in_state(ServerState::Running))`, and remember
  the host has `Player { id: 0 }` with no client entity.
- Test multiplayer behavior headlessly with `bevy_replicon::test_app::ServerTestAppExt`, as in
  `tests/it/replication.rs`.

### Bevy conventions

- Bevy 0.20 APIs only. Load the `bevy` skill (bevy-skills plugin) when unsure. Don't guess old APIs.
- One plugin per module. States for flow, `DespawnOnExit` for cleanup, observers for reactions,
  `Message`s for buffered events.
- Every use of BSN (`bsn!` macro or a `.bsn` asset) must be "salted" (#9): a nearby comment that
  mentions not needing `Cargo.lock`, x didn't set us up for success, we need more SME's, we
  haven't considered the problem space enough, BSN syntax needs more changes, `Children` sugaring
  and Sam "Salt" Saltman. `tests/it/bsn_salting.rs` enforces it and has an example.
- No `unwrap()`/`expect()` in systems. Use `let Ok(..) = .. else { return };`.
- Pinned versions: `bevy =0.20.0` and `bevy_replicon =0.45.2` (required by bevy_voice_chat). Only
  bump them together, and only when every dependency supports the new version.

## Commands

```sh
nix develop                     # toolchain + native libs (or install Rust + the apt libs in ci.yml)
cargo run -- host               # host; join code is printed and copied
cargo run -- join <code>        # join from a second instance
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo clippy -p friendslop --target wasm32-unknown-unknown   # the web build must keep compiling (CI)
scripts/build-web.sh && python3 -m http.server -d dist/web   # browser build (needs wasm-bindgen-cli)
```

### Checking UI

To check UI against its design, screenshot mode can force a HUD state:
`FRIENDSLOP_SCREENSHOT_SCENE=play|busy|clickable|chapter|tab|title|settings|loading` (title and settings run without `host`; and
`FRIENDSLOP_SCREENSHOT_SIZE=1920x1080`), e.g. `scripts/screenshot.sh busy.png "" busy`. Add
`FRIENDSLOP_SCREENSHOT_EVERY=3` to keep the game running and re-save the picture every 3 s.
To frame shots without restarting, add `FRIENDSLOP_SCREENSHOT_SHOTS=shots.txt` (desktop only):
the running game watches that file, and every save of it moves the camera to each
`out.png x,y,z,lx,ly,lz` line in turn and saves the picture (~2.5 s each). Scene `clean` hides all
UI (README banner: `docs/banner.jpg`).

Web (#106): native-only deps (bevy_replicon_iroh, tokio, clipboard) are target-gated in
`crates/game/Cargo.toml`; the browser has its own iroh transport (`net_web.rs`) that must stay
byte-compatible with bevy_replicon_iroh (`net_wire.rs`, checked by its tests). Every replicon
registration (voice included) must be the same in the browser, or browser and desktop peers
can't connect: never `cfg`-gate a `replicate`/`add_*_message`/`add_*_event`. In game code avoid `std::time::SystemTime`/`std::time::Instant` (they
panic in the browser: use `settings::since_epoch` / `bevy::platform::time::Instant`), `std::fs`
(use `settings::load_local`/`save_local`) and threads.
