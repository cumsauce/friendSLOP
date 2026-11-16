# Surf map: new sections (shared brief)

doomy loves the surf map (#306) and wants more variety: (1) an area with lots of sky bridges to
dodge, (2) an area where you go inside a huge skyscraper complex, (3) an area where you ride
alongside a sky train. Three agents build one each, at the same time, in the same checkout.

## The course

- Code: `crates/side/src/features/surf.rs` (hall, ramps' colliders, city, timer), pure rules
  `crates/base/src/rules/surf.rs` (`Tuning`, `ridge(points, samples)`, `pieces`, `city`,
  `hall_at`), data `crates/game/assets/data/side/surf.ron` (stages: pad + ramps as ridge points,
  hall coordinates; `hall_at` = (4000, 900, 0) in the world). Ramps are ridges 12 m wide
  (`ramp_half_width` 6) and 10 m tall (`ramp_height`) with 59 degree faces; surfers slide along
  them at 20-60 m/s (Counter-Strike surf movement), mostly on the faces 0-10 m below the ridge.
- Stage pads (hall coords): s1 (0,-0.5,-5), s2 (41.9,-104.9,-501), s3 (24.5,-221.5,-999),
  s4 (326,-334.7,-1436), s5 (158.3,-447,-1871), finish (483.7,-563,-1939). Speed-check gaps
  (jumps between ramps) are listed in surf.ron's `speed_gaps`: keep clear of them.
- Generated props: `crates/side/src/features/surf_props.rs` (arches, rooftop props, sky
  bridges, floaters). Call `crate::features::surf_props::claim(centre, half)` (hall coordinates)
  from your plugin to keep its props out of your section's space.
- Build what you add when the `surf` area loads: `app.spawn_into("surf", your_build)` (see
  `surf.rs` `plugin`, `use friendslop_base::areas::AreaApp as _;`), entities with
  `DespawnOnExit(AppState::InGame)`, world position = `hall_at(&t) + hall coords`.

## Rules

- **Do NOT edit `surf.rs`, `rules/surf.rs` or `surf.ron`** (another session is changing surf
  movement in them right now) and don't edit the other two sections' files. If your section
  truly needs a course change, say so in your report.
- Your section is a new module `crates/side/src/features/<name>.rs` with its own data file
  `crates/game/assets/data/side/<name>.ron` (all positions, sizes, counts, speeds: data, not
  constants; commented), registered in `crates/side/src/lib.rs` (`pub mod`, `data_files()`) and
  `crates/game/src/features/mod.rs` (`app.add_plugins(<name>::plugin)` after
  `surf_props::plugin`). Those three files are shared with the other agents: make small,
  single-line Edit insertions and re-read before editing.
- Pure functions for the layout (where things go), plus thin systems; unit tests for the
  layout: things are where they should be and nothing blocks a ramp's surfable faces except
  what's meant to (`surf_props::clear` is a helper). A test that your data file parses and has
  every field.
- Colliders come from your data / layout, never from model files (gameplay never waits on a
  model). Moving things that should push surfers: kinematic bodies.
- Models from the modeler, via its CLI driver ONLY (never the `mcp__modeler__*` tools: other
  agents share one daemon): `cd /Users/doomy/Code/friendSLOP/crates/game && python3
  /Users/doomy/Code/modeler/bin/modeler/mcp_driver.py --connect --client <your-name> --doc
  <prefab> --json --calls <scratch>/calls.json --save-dir <scratch>/shots`; calls are
  `[{"name": "<tool without the mcp__modeler__ prefix>", "args": {...}}]`. Real size (1 unit =
  1 m), `style {"action":"load","name":"friendslop"}`, procedural pbr materials (prefix `sf-`;
  reuse the existing `sf-*` neon/steel/glass ones and the city's facade materials `sf-fa-*`;
  new materials need `material_catalog {"action":"save",...}`), `verify {"materials":"pbr"}`,
  `history_prune`, `save_prefab` (tags incl. "surf"), `export_model` to
  `models/features/surf_<name>_*.gltf` with optimize `{"preset":"game","merge":"by_material",
  "emissive_strength":4}`. Keep the pink-neon city look (doomy likes it). Mind performance:
  instance repeated pieces, share materials.
- Agents share one cargo target dir: `cargo test -p friendslop-side <filter>`,
  `cargo clippy -p friendslop-side --all-targets -- -D warnings`; never `--workspace`, never
  wasm/browser builds. Run the game only through `scripts/screenshot.sh` with
  `FRIENDSLOP_START=surf` (see its header; `clean` scene; camera "eye x,y,z,look x,y,z" in world
  coordinates) and write shots to `renders/tmp/`.
- Don't commit. Report: what you built and where (stage, hall coords), files, models, tests,
  screenshot paths, anything left rough.
