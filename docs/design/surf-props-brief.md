# Surf map set pieces: modeling brief (shared by every modeling agent)

doomy wants the surf map (#306) to have more fun, BIG stuff: cranes, arches, sky bridges and the
like. The surf course hangs ~900 m up over a generated city of glass towers (12-24 m wide,
rising 6-70 m above the ramps) with neon accents (hot pink, cyan, lime, orange, violet: each
stage has its own neon colour). The ramps are ridge-shaped wedges 12 m wide and 10 m tall with
59 degree faces; players slide along them at 20-60 m/s, so set pieces are seen big and fast:
bold silhouettes, readable from 100 m, chunky friendslop low-poly with procedural PBR materials,
emissive neon where it fits (it blooms). Night matters too: give each piece some glowing bits
(lit windows, neon tubes, aircraft warning lights) so it reads after dark.

## How to drive the modeler (IMPORTANT: other agents use it at the same time)

Do NOT use the `mcp__modeler__*` tools: they share one active document between agents. Use the
CLI driver from Bash, always from `/Users/doomy/Code/friendSLOP/crates/game`, with your own
client name and one document per model:

```bash
cd /Users/doomy/Code/friendSLOP/crates/game && python3 /Users/doomy/Code/modeler/bin/modeler/mcp_driver.py \
  --connect --client <your-agent-name> --doc <prefab-name> --json \
  --calls <scratch>/calls.json --save-dir <scratch>/shots
```

`calls.json` is `[{"name": "<tool name without the mcp__modeler__ prefix>", "args": {...}}, ...]`
(e.g. `scene_summary`, `apply_ops`, `describe_op`, `material_ops`, `material_catalog`,
`catalog_list`, `catalog_describe`, `verify`, `agent_views`, `screenshot`, `save_prefab`,
`export_model`, `history_prune`, `style`). `--doc` opens the prefab or makes a new one; every call
in the run goes to it, including `export_model` and `save_prefab`. Keep calls JSON and screenshots
in your scratch folder, not the repo. Start each model with `style {"action":"load","name":"friendslop"}`.
New materials are only written to disk by `material_catalog {"action":"save", "name", "description"}`:
call it for every material you make. Prefix new materials `sf-`. Reuse existing ones where they
fit (`material_catalog` lists them: steel, glass, concrete, paint...).

## Rules for every model

- Real size, 1 unit = 1 m, Y up. Front faces +Z. Origin as each item says.
- Big but cheap: a whole crane or bridge under ~15k triangles, smaller props under ~5k. Use
  instancing-friendly repetition (lattice bays, segments). Export with
  `optimize: {"preset": "game", "merge": "by_material", "emissive_strength": 4}` unless a part must
  move (then say which node, and don't merge it).
- `verify {"materials": "pbr"}` must pass. Look at it (`agent_views`) from far and near and fix
  what reads wrong.
- `history_prune {"keep": 5}`, `save_prefab` (description, tags incl. "surf", the prompt), then
  `export_model {"path": "models/features/<file>.gltf", ...}`.
- No colliders in the model: the game makes them from boxes you report.
- Don't edit Rust, data files or anyone else's prefab. Don't commit. Delete stale textures your
  own earlier exports left in `assets/textures/` (only your own new, untracked files).

## Report back, per model

```
<prefab> -> models/features/<file>.gltf
  origin: ...   size (x,y,z m): ...   tris: ...
  solid boxes (centre xyz, size xyz, model space): ...  (what a surfer should hit; can be none)
  glow: which parts are emissive     notes: anything placement must know
```
