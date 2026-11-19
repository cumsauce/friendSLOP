# Surf course: themed areas (shared brief)

doomy loves the surf map (#306) and said the office and the mall are boring: "just a snake back and
right path". They want both **rethought to be fun**, plus **three new themed areas**: a **future neon
city** (what the city is now, turned up to the max) and **two more of our choice** (we chose an
**overgrown ruins** and a **foundry**). Five agents build one area each, at the same time, in the same
checkout. Read this whole file, `AGENTS.md` and `crates/side/src/features/surf_map.rs` first.

## What makes a surf area fun (do this, don't just make a longer snake)

From racing / surf level-design guides (SRB2 "Level Design 101/Circuit", SuperTuxKart "Making Tracks",
Magnopus on memorable tracks, the Momentum Mod surf-ramp guide):

- **Variety of line, not one curve.** Mix long fast straights, wide banked sweeps, hairpins/switchbacks,
  **helixes** (spiral down round a feature, 1.5+ turns), S-bends, **cross-overs** (a line passes over or
  under another: figure-8s), drop-down transfers onto a lower line, and jump gaps. Smooth transitions
  keep flow; turning too sharp loses speed (keep the ridge's radius of curvature >= 60 m; 85+ m for
  fast sections).
- **Pace it.** Alternate intense stretches with a calm one (a wide, mostly straight cruise with a view)
  so it isn't tiring or monotonous. A set piece every 150-250 m of ridge.
- **Forks with risk/reward.** At least two per area: the main line stays approachable (wide, gentle);
  a **shortcut** (a parallel ramp reached by a gap jump or a drop) is harder, skips ~10-20% and saves
  time. Both must end on the same next ramp or pad. Put the hardest content on the shortcuts.
- **Landmarks and sightlines.** One huge themed landmark per stage visible from far (and from the
  neighbouring areas); reveal the next set piece before you reach it; hide it for tension in a few places.
- **Teach, then twist.** The first stage of an area introduces its signature idea gently; later stages
  combine it with the others.
- **Theme in the surfing, not only the scenery:** the ramps weave through/around what the area *is*
  (through a mall's atrium, round a tower, along a collapsed highway, over a furnace).
- Detail makes a map; but entity counts matter (budgets below).

## The engine (what you can and can't do)

- A **ramp** is a ridge (a smooth curve through points, hall coordinates) with two 59 degree faces, 12 m
  wide (`ramp_half_width` 6), 10 m deep (`ramp_height`). Surfers slide along the faces at 20-60 m/s
  (Counter-Strike surf movement). Ramps only go **down**: slope ~0.12-0.18 m drop per metre of path
  keeps speed; flat bleeds it. **Don't make colliders block the faces.**
- A **stage** is a start **pad** (a checkpoint: R returns you there; falling off puts you back there) and
  its ramps (a list; gaps between consecutive ramps are jumps). A **`SpeedGap`** marks the gap after a
  ramp as a speed check (you need ~36 m/s to clear it; mark it with a neon arrow). Stages are numbered
  globally: base 0-4, **office 5, mall 6, neon 7-9, overgrown 10-12, foundry 13-15**.
- The map is **hall coordinates** (the start pad's top is 0,0,0; world = hall + (4000, 900, 0)).
  Ramps are **drawn and collided at run time from the `Ridge` markers** (`surf_map.rs`): you don't model
  ramps and you must not use the modeler's `tri_mesh` (it silently disappears). Give a stage's ramps a
  look with a `RampLook` marker (colour, glow, roughness, metal).
- A fall is a wipe-out: below `stage_low(stage) - 18 m` you're sent back to the pad. **Keep anything solid
  below that line or well away from the ramps**, or a fall lands on it (R still works, but it's bad).
- Tunnel rule: nothing solid within 13 m of a ridge sideways from the ramp body up to 26 m over the ridge,
  except deliberate obstacles that leave >= 3 m of free face (a "duck" bridge, a pillar in a face with the
  other face free). `surf_map.rs` has `nothing_solid_is_inside_a_ramp`; the older section modules have
  `in_tunnel` / `free_gap` helpers to copy.
- Movement checks to design with: `rules::surf::speed_needed`, `stage_step`, `through_checkpoint`.

## The contract (so five agents don't collide)

**Areas are separate maps**, each its own modeler document and `maps/surf_<name>.bsn` +
`.map.json` + `.gltf`, its own area in `data/base/areas.ron` (already added: `surf_office`, `surf_mall`,
`surf_neon`, `surf_overgrown`, `surf_foundry`; neighbours overlap by 100 m so the next area is loaded
before you reach it; **don't give an area `heights`**). `rules::surf::tuning()` merges every map's
`Pad` / `Ridge` / `SpeedGap` / `course_block` markers; a later map's stage replaces an earlier map's with
the same number; the last map with a `Pad { finish: true }` ends the course.

**Handoff pads** (fixed, hall coordinates, 16 x 1 x 16 unless noted; **an area ends by landing on the next
area's first pad**). Leaving a pad heading east (+x): the first ridge point is 12.35 m past the pad
centre along the heading and 5.5 m below the pad centre (a 4 m hop, like every stage). Arriving: the
last ridge point E is 30 m before the pad centre along the heading and 8.5 m **above** the pad (you fly
the last 30 m and land on it). Entry/exit heading **east**, z within +-40 m of -1935.

| Pad | Centre (x, y, z) | Starts | Owned by |
|---|---|---|---|
| P5 (24 m) | 483.65, -563.02, -1938.87 | office, stage 5 | office |
| P6 (24 m) | 1030.00, -645.88, -1938.00 | mall, stage 6 | mall |
| P7 (24 m) | 1814.85, -749.16, -1915.74 | neon, stage 7 | neon |
| P10 | 3200.00, -905.00, -1935.00 | overgrown, stage 10 | overgrown |
| P13 | 4400.00, -1060.00, -1935.00 | foundry, stage 13 | foundry |
| F | 5600.00, -1215.00, -1935.00 | the finish | foundry |

Regions (hall x; y descends ~140-150 m per area; z is free within -2500..-1400 but keep the line near
-1935 at the handoffs): office 471-1100, mall 1000-1900, neon 1800-3250, overgrown 3150-4450,
foundry 4350-5650. Each area has its **3 stage pads inside** (stages 7-9 etc.) and is ~1.2-1.3 km
of course. The pad that starts an area belongs to that area's map (pad + `course_block` pillar down to
your floor). Until the *next* area exists, add a clearly named **stand-in landing pad** for it in your
own ops (`Pad` + `course_block`, names starting `standin_`): a later map overrides it, duplicates are
harmless.

**You own, and only edit:** your module `crates/side/src/features/surf_<name>.rs` (pure layout
functions + tests, never registered at run time), data `crates/game/assets/data/side/surf_<name>.ron`,
the example `crates/side/examples/surf_pack_<name>.rs`, models `crates/game/assets/models/features/surf_<name>_*`,
prefabs `crates/game/.modeler/prefabs/surf-<name>-*`, the modeler doc `surf-<name>-map`, and
`crates/game/assets/maps/surf_<name>.*`. Shared files (`crates/side/src/lib.rs`: your `pub mod` line;
`areas.ron`; `components.json`): **single-line edits, re-read first**, and ask in your report before
anything bigger. **Don't edit `surf_map.rs`, `rules/surf.rs`, `surf.rs`, `surf_ops.rs`, `surf.ron`,
or another area's files**; if you need a shared change, say so in your report.

## How to build an area (copy `crates/side/examples/surf_pack_mall.rs` and `surf_mall.rs`)

1. **Layout in Rust** (pure functions of your data file) -> a `surf_ops::Ops` (see
   `crates/side/src/features/surf_ops.rs`): `model` (a placed model; unscaled yaw-only ones are a cheap
   `GltfSource` marker on the model file, scaled ones a prefab instance), `block` (a box/cylinder
   collider), `part`/`flat` (a box of geometry with a modeler material: **thousands merge into a few
   meshes, prefer these to instances**), `lamp`, `layout`, `course` (pads, ridge points, speed gaps,
   `course_block`s: `Scope` says which stages you own), `dressing` (pad neon, stage markers, lane lights).
   Extra components: `Body` (collider on a tilted model), `Wedge` (a launch kicker), `Bob`, `Spin`,
   `RampLook`, `SpeedGap`. Write ops with `o.write("surf_<name>")` -> `.modeler/gen/surf_<name>_ops.json`.
2. **Models from the modeler**, **via its CLI driver only** (never the `mcp__modeler__*` tools, other agents
   share the daemon): `cd /Users/doomy/Code/friendSLOP/crates/game && python3
   /Users/doomy/Code/modeler/bin/modeler/mcp_driver.py --connect --client <your-name> --doc <prefab>
   --json --calls <scratch>/calls.json --save-dir <scratch>/shots`; calls are `[{"name": "<tool without
   the mcp__modeler__ prefix>", "args": {...}}]`. Real size (1 unit = 1 m), `style {"action":"load",
   "name":"friendslop"}`, procedural pbr materials (new ones: `material_ops` then `material_catalog
   {"action":"save"}`; neon materials `sf-neon-pink/cyan/lime/orange/violet/coral/gold`, facades
   `sf-fa-*`, steel `sf-hull`, ... exist), `verify {"materials":"pbr"}`, `history_prune`, `save_prefab`,
   `export_model` to `models/features/surf_<name>_*.gltf` with optimize `{"preset":"game","merge":
   "by_material","emissive_strength":4}`.
3. **The map document**: create `surf-<name>-map`, load the materials your ops use
   (`material_catalog load`), `apply_ops {ops_file: ".modeler/gen/surf_<name>_ops.json"}`, then
   `export_model {path: "maps/surf_<name>.bsn", optimize: {...as above}}` and `save_prefab`. **Exports
   can silently land in `~/maps/` and `~/models/`** (the project root drops): move
   `surf_<name>.{bsn,map.json,bin,gltf}` into `crates/game/assets/maps/`, copy any new `textures/*.ktx2`
   into `crates/game/assets/textures/`, and rewrite the `.gltf`'s `"uri":"textures/` to
   `"uri":"../textures/` (prefab/model exports use `../../textures/`). Prefab instances land in
   `models/prefabs/` (check they exist). The daemon can restart (docs are lost; rebuild from the ops).
4. **Test it in the game**: `FRIENDSLOP_START=surf_stage_<N> cargo run --bin friendslop -- host` starts you on
   stage N's pad; screenshots: `FRIENDSLOP_START=surf_stage_<N> FRIENDSLOP_SCREENSHOT_WAIT=90
   scripts/screenshot.sh renders/tmp/x.png "eye x,y,z,look x,y,z" clean` (**world** coordinates; big
   packs need 80-110 s; a screenshot right after a heavy build can come out black: run it again). You can
   also surf it: `FRIENDSLOP_BOT=<route>` (`data/game/bot.ron`) with `FRIENDSLOP_PERF=1` logs frame times.

## Rules, budgets, tests

- **Data not constants**: sizes, counts, seeds, colours in your `.ron` (commented); deterministic seeded
  layout; every file parses (a test). Pure functions + thin wiring.
- **Budgets per area**: <= ~3,000 map entities (use `part`/`flat` and big merged pieces; instance repeated
  models), <= ~1,500 `Block`/`Wedge` colliders (use big boxes; one box per wall, not per tile), <= 40
  point lights (no shadows), glTFs < ~2 MB each, textures shared (`sf-*` materials). Every crate builds
  for the browser: no threads, `std::fs`, `Instant` in game code (the example/generator may use `std::fs`).
- **Tests** (in your module): the course geometry is valid (ridge radius, slope, handoff pads exactly as in
  the table, first ridge point 4 m from the pad, last point 30 m before the next pad), nothing solid blocks
  a ramp's faces (free-face/tunnel checks), forks both reach the next ramp, models/data exist, same data
  -> same layout. `cargo test -p friendslop-side <yourname>` and
  `cargo clippy -p friendslop-side --all-targets -- -D warnings` (the `friendslop-hrtf` crate currently
  fails -D warnings on its own: ignore that, check your files). **Never `--workspace`, never wasm/browser
  builds** (they hang the shared target dir).
- **Placeholder art is fine**: flat/procedural, but make it look good (it's a neon/sky game).
- No commits. Don't revert others' changes. Keep `GAME_DESIGN.md`'s "Surf map" entry truthful for your
  area (one short paragraph; re-read it first, it's shared).

## Report back

What you built (stages, ridge length, forks, set pieces, hall coordinates), files, models, tests (counts),
screenshot paths (`renders/tmp/`), perf numbers (entities, colliders, lights), what's rough, and any
shared-file change you need from me.

## The areas

- **Office tower (stage 5, x 471-1100)** *redesign*. Now: one ridge snaking down a 570 m glass lobby with
  a wall of offices. Make it a vertical, multi-line **atrium**: switchback/helix lines round central
  sculptures and elevator cores, cross-overs, a fork through the offices (a shortcut through an office
  floor's glass partition gaps), an indoor-sky garden calm stretch, a finish arriving on P6. Keep the
  glass roof + trees + executive-office feel and the existing models if useful.
- **Mall (stage 6, x 1000-1900)** *redesign*. Now: one S-ridge over a hall of shop columns. Make it a
  **multi-level mall**: concourse loops, escalator-like diagonal drops, a **food court** you spiral round,
  catwalks to dodge (keep the slalom-style bridges but as a gauntlet in the middle, not the whole thing),
  an atrium **helix** round a fountain/clock tower, a back-of-house shortcut through service corridors, a
  glass-roof calm stretch, arriving on P7.
- **Neon megacity (stages 7-9, x 1800-3250)** *new*: "what we have (a pink neon sky city) turned up to the
  max": mile-high towers wrapped in animated-looking neon (use `Bob`/`Spin`, emissive bands, giant
  holograms/billboards, flying cars/blimps/laser rings as set pieces), canyons of skyscrapers, ramps
  that corkscrew up the *outside* of a megatower, a monorail you race, maglev highways, a rooftop
  night-market calm stretch. Cyan/magenta/violet palette, `RampLook` glowing neon.
- **Overgrown ruins (stages 10-12, x 3150-4450)** *new* (abandoned/overgrown): a collapsed highway
  interchange, a half-fallen skyscraper and a drowned mall reclaimed by jungle: giant trees and vines
  pushing through, moss-green ramps (`RampLook`), broken ramps with jump gaps, waterfalls, ruined bridges,
  fireflies (small emissive parts), mist. Calmer colours, big organic shapes, more forks and shortcuts.
- **Foundry (stages 13-15, x 4350-5650, the finish)** *new*: a molten industrial complex: furnaces, lava
  falls, pistons and gears (`Spin`), conveyor belts, cooling towers, crane gantries, glowing orange/red
  ramps, sparks; ends with a grand finish arch on F (gold, a `Pad { finish: true }` + the course's end:
  the foundry owns the finish).
