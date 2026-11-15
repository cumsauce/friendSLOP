# Infinite world (#391)

bben (#391): keep the designed world growing and editable, but when you reach its edge it
should go on as procedurally generated wasteland. This builds on #412 ("friendSLOP 2"): every
mechanic lives at a named site from one data file (`data/base/sites.ron`), and a site sleeps
until a player is near.

## Goals

- The hand-made map stays the centre. Nothing in it moves or changes.
- Past its edge the world goes on: terrain, then roads, props and sites, generated in chunks.
- Every peer builds the same world from a **world seed** and a chunk's coordinates. Nothing
  bulk-replicated: chunks are never sent over the wire.
- Chunks load near players and unload when nobody is near. A far chunk costs nothing.
- Only **player-made changes** persist (things dug, built, moved, taken). The generated base
  is never saved, because it can always be regenerated.
- Generated sites reuse existing mechanics in new combinations and use the same
  `sites::awake` / `sites::here` machinery as hand-placed ones.
- Fits the Steam Deck's frame budget and the web build's memory.

Non-goals: a second hand-made map, infinite NPC populations, generated quests or story.

## The world grid

- A **chunk** is a 55 m square on a grid anchored at the origin: chunk `(i, j)` covers
  `x in [55 i, 55 (i+1))`, `z in [55 j, 55 (j+1))`. The hand-made ground (±110 m) is exactly
  chunks `-2..=1` on both axes, so no chunk straddles its edge.
- **Reserved rectangles** already in use far from the centre (New York City around
  `(0, 223)`, the sky courses high up) are never generated over. Increment 1 avoids them by only
  opening the **north** side (`z < -110`); later increments read the reserved list from data.
- A chunk is a pure function: `generate(seed, ChunkCoord) -> Chunk`. A `Chunk` holds a height
  grid (`CELLS + 1` squared samples), and later a road mask, a prop list and at most one
  site. Its bytes (`Chunk::to_bytes`, little-endian, versioned) are what tests compare.
- Determinism: only `+ - * /`, `floor` and comparisons on `f32`, with the existing integer
  hash (`art::hash`). No `sin`, `exp` or `powf` in generators: those come from libm and can
  differ between native and wasm. No `HashMap` iteration order, no thread-dependent order.
  Generator code is versioned (`GEN_VERSION`): bumping it means old saves' changes may land on
  a different ground, so the save keeps the version it was made with.

## Generation pipeline (per chunk, in order)

1. **Heights.** Wasteland noise (broad dunes, mesas, dry washes) blended into the map's own
   edge height over a 40 m band, so the seam is continuous. `terrain::height_at` answers for
   wasteland points too, so walkers, scenery and every existing `height_at` caller just work.
2. **Roads** (increment 3). A coarse road graph on a 4×4-chunk super-cell grid: one node per
   super-cell, jittered from the seed, joined to its neighbours with a seeded choice. Every
   chunk can work out the roads crossing it from its own and neighbouring super-cells alone,
   so no chunk needs another chunk's output. Roads flatten the heights under them (step 1
   reads the mask).
3. **Props** (increment 4). Seeded scatter (Poisson-ish, one RNG per chunk seeded from
   `(seed, i, j, "props")`) of rocks, dead trees, wrecks, fences, using the existing prop
   library and models. Kept off roads and sites.
4. **Sites** (increment 5). A super-cell rolls whether it holds a site, picks a **recipe** from
   `data/world/wastes_sites.ron`, and places it at a flat spot. A recipe names a set of
   existing mechanics (a shipwreck, rats, a gamble table, kittens, a broadcast tower...) and
   their layout relative to the site middle. Generated sites register with `sites` as
   `Site { id: "wastes:<i>:<j>", place: At(..), reach }` at run time, so every mechanic's
   `run_if(sites::awake(..))` gating applies unchanged. A mechanic opts in by exposing a
   "spawn at" entry point that takes a site id and a point instead of its fixed `sites::at`.

## Streaming

- **Viewers**: every player's feet (`sites::People`, already known on every peer) plus the
  local camera. The game copies them into `wastes::Viewers` each frame.
- A chunk loads when any viewer is within `load` metres of it and unloads when every viewer
  is further than `unload` (hysteresis, `unload > load`). At most `per_frame` chunks are built
  per frame, nearest first, so walking never spikes a frame.
- Loading builds the mesh (vertex-coloured, one draw per chunk, shared material) and a
  heightfield collider. Both host and clients build colliders: the host's physics is
  authoritative, clients need them for their kinematic prediction and camera.
- **LOD** (increment 6): far chunks use a quarter of the cells and no collider; a ring
  beyond `load` uses an even coarser mesh only, so the horizon isn't empty.
- **Async** (increment 6): on native, chunk generation moves to `AsyncComputeTaskPool`; the
  web build stays on the main thread but under the same per-frame budget.

## Networking

- The **world seed** is host state: increment 2 replicates one `WorldSeed` resource-like
  entity (a few bytes, appended to the protocol list). Until then it is the data file's
  `seed`, the same on every peer that has the same files.
- Chunks themselves are never replicated. Peers agree because they run the same code on the
  same seed. Generator version mismatches are caught by the join handshake's version check.
- **Player changes** (increment 7) are server entities like any other gameplay state, tagged
  with their chunk. They replicate only to clients whose viewers are near that chunk
  (replicon visibility), so a big edited world doesn't flood a joining player.

## Persistence

- The host saves a per-chunk **delta list** (`chunk -> [Change]`): placed or removed props,
  dug cells, built pieces. A chunk with no changes has no entry. Loading a chunk applies its
  delta after generating it.
- The save holds the seed and `GEN_VERSION`. Native saves go to the existing save folder;
  the web build uses localStorage (small: deltas only).

## Precision far from the origin

- `f32` has about 1 mm of precision at 8 km and 4 mm at 32 km. Physics (avian, f32) and the
  kinematic controller start to jitter noticeably past ~10 km.
- Increment 1 caps the wastes at `limit` metres from the origin (data, default 4 km) with the
  same soft wall the map uses today.
- **Floating origin** (increment 8, only if friends walk that far): when the local camera is
  more than 2 km from the render origin, shift every `Transform` (and the physics world) by a
  whole number of chunks. Gameplay positions on the server stay in world coordinates for
  networking; only rendering and local physics rebase. Chunk coordinates are `i32`, so the
  grid itself has no limit. avian has no built-in origin shifting, so this is our own code and
  the largest risk in the plan.

## Budgets

| Target | Loaded chunks | Build per frame | Memory |
| --- | --- | --- | --- |
| Desktop | radius 220 m (~60 chunks) | 2 chunks, < 2 ms | ~25 MB |
| Steam Deck | same radius, LOD ring from 110 m | 1 chunk, < 2 ms | ~20 MB |
| Web | radius 165 m (~35 chunks) | 1 chunk | < 15 MB |

A 50-cell chunk is 2601 vertices (~150 KB with colours and normals) and a 26×26 heightfield.
Budgets are data (`data/world/wastes.ron`), so the Deck and web numbers can be tuned without a
rebuild. `FRIENDSLOP_PERF=1` reports loaded chunk count.

## Navmesh

The villager navmesh (#96) is built once over the hand-made map (~3 s). The wastes do not get
one in increments 1-5: nothing walks there but players. When generated sites bring NPCs
(increment 5+), each site gets a small navmesh tile over its own reach, built when the site
wakes and dropped when it sleeps (rerecast tile builds, bounded by the site's reach).

## Risks

- **Seams**: neighbouring chunks must agree on their shared edge. Heights come from one
  continuous function of world position, never from per-chunk state, so edges match exactly.
- **Determinism across targets**: libm differences (see above); a test compares chunk bytes
  against a checked-in hash so a drifting generator fails CI.
- **Frame spikes** from generation on the Deck: per-frame budget now, async later.
- **Precision** past ~10 km: soft limit now, floating origin later.
- **Mechanics with fixed positions**: most features read `sites::at("<id>")` once; reusing
  them at generated sites needs a small refactor per mechanic (one per PR, opt-in).
- **Save growth**: deltas only, per chunk.

## Increment plan

1. **Wasteland terrain north of the map** (this PR): `world::wastes` generator, streaming
   with load/unload and a per-frame budget, heightfield colliders, the north soft wall opened,
   tuning in `data/world/wastes.ron`, determinism tests, a CPU perf measurement.
2. **Host world seed**: replicate the seed; open the east and west sides with the reserved
   rectangles from data.
3. **Roads**: super-cell road graph, flattened road beds, a road out of the map's north path.
4. **Props**: seeded scatter from the prop library, with draw ranges.
5. **Generated sites**: recipes in data, the first two mechanics made placeable (shipwreck,
   rats), registered as runtime sites.
6. **LOD + async generation**: coarse far ring, background tasks on native, Deck tuning.
7. **Player changes and persistence**: per-chunk deltas, save/load, replicon visibility by
   chunk.
8. **Floating origin** (only if needed): rebasing render and local physics.
