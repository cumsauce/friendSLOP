# Modeler TODO — model work owed (NOT done in this hardening run)

These are model/asset changes this run deliberately did **not** make: the
overnlying hardening pass is code, tests and docs only, and every model edit
needs the modeler + a re-export that could collide with live exports. Each row
is ready for whoever picks it up: the problem, the fix, the
`.modeler/prefabs/<doc>` to edit (run the modeler with `crates/game` as the
project so sources land in `crates/game/.modeler/`), and the `export_model`
call. Priority: **P1** visibly broken, **P2** placeholder/rough, **P3** nice.

Export commands use the AGENTS.md forms: `export_model("models/<path>.gltf",
optimize: "game")` for normal models, `optimize: {preset: "game", merge:
"by_material"}` when a model has many parts (one mesh per material) but the
game does not look its parts up by name. Both write into `assets/`, textures
as shared KTX2 under `assets/textures/`. `history_prune` before saving.

| model | problem | fix | prefab doc + export cmd | priority |
|---|---|---|---|---|
| Watcher figure | Built from code primitives, "tall black column with long thin arms and a smooth blank head" (`crates/side/src/features/watcher.rs:173`) | Model a rigged creature (body + clips) and swap the primitives for the model the way `people.rs`/`rigged.rs` features do | new doc `watcher.json`; `export_model("models/creatures/watcher.gltf", optimize: "game")` | P2 |
| Broadcast TV | The tower's TV is placeholder primitives in flat colours, half sunk in the floor (`crates/side/src/features/broadcast.rs:138`) | Model the TV as a prop (set, screen, aerial); the screen material stays code-driven (lit while broadcasting) | new doc `broadcast-tv.json`; `export_model("models/features/broadcast_tv.gltf", optimize: "game")` | P2 |
| Ice-cream machine parts | The machine's lost parts are placeholder `Cuboid`s (`crates/side/src/features/ice_cream.rs:314`, `PartMesh`) | Model each quest part (and the machine body if thin) so they read as machine parts in the shops | extend `nyc-ice-cream.json` + new `nyc-ice-cream-part.json`; `export_model("models/features/ice_cream_part.gltf", optimize: "game")` | P2 |
| NYC trail posts | The wayfinding posts to/from the city are code-built boxes, "placeholder, not modeler models" (GAME_DESIGN.md NYC section; `data/side/nyc.ron` spots) | Model an `nyc-kit` post (sign board on a post) and place it on `nyc-map` at the two posts | new doc `nyc-kit-trail-post.json`; `export_model("models/prefabs/nyc-kit-trail-post.gltf", optimize: "game")`, then place in `nyc.map.json` | P2 |
| Crow model origin | Legs sit half-buried: the feet are not at the model's origin the runtime contract expects (bugpass `crows:legs`; `crates/base/src/features/crows.rs:79`) | Move the crow's feet to its origin (y=0 at the feet) so `FEET` offset works; re-export over the same path | edit `pt-crow.json`; `export_model("models/creatures/crow.gltf", optimize: "game")` | P1 |
| Quarry crane | The jib floats — part origins don't snap coherently (bugpass `zones:crane`) | Make the crane's parts share one ground-anchored origin so placement snaps whole | edit `zn-quarry-crane.json`; `export_model("models/prefabs/zn-quarry-crane.gltf", optimize: "game")` | P1 |
| Tunnel / bath swatches | Flat colours persist until the swatch textures load (`crates/places/src/features/underground/visuals.rs:252`, `warehouse/visuals/bathroom.rs:130`) | Confirm `tn-swatch-*` / `bt-swatch-tiles` exports carry their textures (re-bake if a channel is flat); the flat fallback then lasts only load time | edit `tn-swatch-{brick,flags,planks,plaster}.json`, `bt-swatch-tiles.json`; `export_model("models/surfaces/<swatch>.gltf", optimize: "game")` | P2 |
| Many-parts set pieces | High draw calls from models with many small parts (see `FRIENDSLOP_PERF=2` "which models own the meshes") | Re-export offenders with `merge: "by_material"` (one mesh per material) — only where the game does not look parts up by name | any doc named by `FRIENDSLOP_PERF=2`; `export_model(..., optimize: {preset: "game", merge: "by_material"})` | P2 |
| hs-* trim coverage | `barn_door` is one 3×3.2 m trim scaled to every barn hole (`crates/structures/src/lib.rs:589`); other grammar hole kinds may lack trim or stretch badly | Check each hole kind the grammar emits has a trim piece (`hs-*`, `vb-barn-door`) that survives non-uniform scale; add variants where it doesn't | edit `hs-{window,door,shutter,shopfront,awning,chimney-cap}.json`, `vb-barn-door.json`; `export_model("models/props/<name>.gltf", optimize: "game")` | P3 |
| CREDITS drift (zn-*) | `CREDITS.md` listed the quarry crane, castle arch, log pile, woodcutter's hut and windmill under `models/scenery/` while they live in `models/prefabs/zn-*` | **Done in this run** (`docs(credits): enumerate`): scenery bullet corrected, `models/prefabs/` bullet added | no model work | P3 (done) |
| nyc-kit-cruiser | `models/prefabs/nyc-kit-cruiser.gltf` is unplaced — nothing loads it (the only orphan model; `KNOWN_ORPHANS` in `tests/it/asset_hygiene.rs`) | Either place it on `nyc.map.json` (parked car along the kerb) or drop the model; whichever happens, shrink `KNOWN_ORPHANS` | edit `nyc-kit-cruiser.json` + `nyc-map.json`; `export_model("models/prefabs/nyc-kit-cruiser.gltf", optimize: "game")` if changed | P2 |

## Inventory (at plan time; the NYC exports have since grown it)

346 exported models (features 126, prefabs 48, props 43, people 33, nature 29,
scenery 22, creatures 21, surfaces 11, hats 5, kori 5, ...), ~340 `.modeler`
prefab docs + 165 material swatches + the `friendslop` style. Naming families:
`wh-tile-*`, `hs-*`, `tn-*`/`sf-*`, `nyc-kit-*`, `nyc-*`, `fs-tile-*`, `zn-*`,
`vb-*`, `fu-*`, `bt-*`, `ct-*`/creatures. Current tree (after the street-kit
exports): 367 `.gltf` under `assets/models/`, 365 prefab docs. A model is
referenced by stem from code/data/maps — `tests/it/asset_hygiene.rs` fails on
anything orphaned, so after placing or dropping the cruiser update its
`KNOWN_ORPHANS` seed (the test flips otherwise).
