# Backrooms bathroom: modeling brief (shared by every modeling agent)

doomy wants the warehouse's nightmare bathroom (#161) to feel like **the Backrooms, but a public
restroom**: an endless, humming, liminal maze of tiled bathroom. Monotone, slightly yellowed
fluorescent light, aged off-white and sickly-cream tile, chrome gone dull, porcelain stained,
everything familiar and wrong. Not gory: grubby, damp, abandoned, uncanny. The friendslop
style is chunky, readable, slightly toy-like low-poly with procedural PBR materials.

The bathroom is cells of 5 m x 5 m on the warehouse grid, ceiling 11 m up. Each closed side of a
cell is a wall 5 m long; the game furnishes it with a "wall kind" (stalls, urinals, sinks, ...).
Furniture along a wall must fit in **3.6 m of wall length** and at most **1.6 m out from the
wall**, so walkways stay open. Existing pieces you should match (load them with `catalog_describe`
/ look at them): `pk-toilet`, `pk-stall-panel`, `pk-stall-door`, `bt-tube-light`, `bt-stall-rail`,
`bt-puddle`, `bt-poop-*`, `bt-swatch-tiles`. Existing materials to reuse where they fit:
`pk-porcelain`, `pk-stall`, `pk-sewage`, `bt-tiles`, `bt-tube`, `fs-subway-tile`; check
`material_catalog` for chrome/steel. Make new materials only when nothing fits, prefixed `bt-`.

## How to drive the modeler (IMPORTANT: other agents use it at the same time)

Do NOT use the `mcp__modeler__*` tools: they share one active document between agents and
`new_model` / `export_model` would land on someone else's model. Use the CLI driver from Bash,
always from `/Users/doomy/Code/friendSLOP/crates/game` (that picks the project), with your own
client name and one document per model:

```bash
cd /Users/doomy/Code/friendSLOP/crates/game && python3 /Users/doomy/Code/modeler/bin/modeler/mcp_driver.py \
  --connect --client <your-agent-name> --doc <prefab-name> --json \
  --calls <scratch>/calls.json --save-dir <scratch>/shots
```

`calls.json` is `[{"name": "<tool name without the mcp__modeler__ prefix>", "args": {...}}, ...]`
(e.g. `scene_summary`, `apply_ops`, `material_ops`, `material_catalog`, `describe_op`,
`catalog_describe`, `verify`, `agent_views`, `screenshot`, `save_prefab`, `export_model`,
`history_prune`, `style`). `--doc` opens the prefab from the catalog or makes a new one; every
call in the run goes to it, including `export_model` and `save_prefab`. Keep your calls JSON
and screenshots in your own scratch folder (the path you're given), not in the repo.
Start each new model with `style {"action": "load", "name": "friendslop"}`.

## Rules for every model

- Real size, 1 unit = 1 m, Y up, floor at y = 0. Front faces **+Z** (into the room).
- **Wall pieces**: the wall is the plane z = 0 behind the piece; the piece's back touches z = 0
  and it extends toward +Z. Origin on the floor (y = 0) at the centre of its length (x = 0).
  Bake the mounting height in (a mirror's glass at ~1.1-2.0 m, a dryer at ~1.2 m), so the game
  only places the origin at the foot of the wall.
- **Freestanding pieces**: origin at the floor centre of the footprint.
- Shape first, then a procedural `pbr` material on every visible part (`verify` with
  `{"materials": "pbr"}` must pass). Group parts that share a material; prefer tile-domain
  materials so models share bakes. Keep triangle counts game-sized (a fixture < ~3k tris,
  a big multi-piece wall set < ~12k).
- Look at it (`agent_views` or `screenshot`) and fix what reads wrong before saving.
- `history_prune {"keep": 5}`, then `save_prefab` (description, tags incl. "bathroom",
  "backrooms", and the prompt), then
  `export_model {"path": "models/features/<file>.gltf", "optimize": "game"}`.
- No colliders in the model: the game makes them from boxes you report.
- Don't edit any Rust, data file or anyone else's prefab. Don't commit.

## What to report back (exactly this, per model)

```
<prefab name> -> models/features/<file>.gltf
  kind: wall | free | ceiling     size (x,y,z m): ...     tris: ...
  solid boxes (centre xyz, size xyz, model space): ...   (only what a player should bump into)
  notes: anything the placement code must know (variants, which way it opens, ...)
```
