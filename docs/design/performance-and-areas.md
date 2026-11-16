# Performance wins and areas (loading and unloading maps)

A plan; Part 2 (areas) is built, see "What's built" below. It has two parts: cheap performance fixes, and a rule that every place in
the world loads when someone needs it and unloads when nobody does. They are linked: the largest
performance win left is not keeping the whole world loaded.

Numbers come from profiling the forest on an M3 Pro (High preset, 15 November 2026). "Measured"
means it showed up in that profile. "Suspected" means it needs a quick check first.

## Where things stand

- **Everything loads at game start.** 72 files have a system that runs on entering the game and
  spawns its part of the world: about 57,000 entities and 20,000 meshes while you stand in the
  forest.
- **New York and Miami are about 4,500 of those meshes**, none of them visible from the commune.
- **Only two features put anything away** when you leave a site (pigeons and street people).
  Sites (`data/base/sites.ron`) put mechanics to sleep, but scenery, colliders and lights stay.
- **The wasteland already streams** (now joined to the areas' layers, see below) in chunks by who is near (`friendslop_world::wastes`, see
  `infinite-world.md`). The pattern exists for generated ground but not for hand-made maps.
- **High holds 60 fps and no more.** With the GPU at 5.9 ms a frame the game still sits at 60,
  while Low reaches 120. The CPU is the limit on High, and the release build's CPU has not been
  profiled yet.

## Part 1: easy performance wins

In rough order of value for effort.

| # | Change | Evidence | Effort |
|---|---|---|---|
| 1 | Profile the release build's CPU on High | High stays at 60 fps with the GPU at 5.9 ms; Low reaches 120. Not yet profiled. | Small. Do this first: it re-ranks the rest. |
| 2 | Find what rewrites materials every frame | Measured: about 240 material changes a second in the forest, each one re-uploaded. 12 files call `materials.get_mut`. | Small |
| 3 | Cut transform propagation | Measured: about 23% of a core, with lock contention. Suspected cause: swaying tree rigs and other things touching transforms that did not change. | Small to medium |
| 4 | Give building props a draw distance and no shadows | Measured: 825 window, 660 shelf and 240 door meshes counted as visible from the forest. `DrawRange` exists, but only ground scatter and terrain use it. | Small |
| 5 | FXAA instead of 4x MSAA on Medium | Measured: MSAA costs about 1.2 ms more at 2560x1440, and it does not smooth alpha-cut leaves. | Tiny |
| 6 | Two shadow cascades, or a shorter shadow distance, as a video option | Measured: about 0.8 to 1 ms per cascade. Costs some shadow crispness. | Small. A choice about the look. |
| 7 | Stop resampling the soundtrack | Measured: the audio thread uses about 12% of a core, mostly sample-rate conversion. It is off the frame's path, so it helps battery and weak CPUs, not fps. | Small |
| 8 | Spread tree-rig spawning over frames | Suspected: rigs spawn when you come within 40 m, which may cause the 25 to 35 ms hitches while walking. | Small |

How to measure any of these is in `AGENTS.md`, "Finding what's slow".

### Notes: CPU on High in the forest (#1, #2, #3, #8)

Release build, M3 Pro, High, `FRIENDSLOP_START=forest_clearing`, standing still, `sample <pid> 5`
60 s in. Other builds were running on the machine for the first runs, so frame rates between runs
are noisy; the counts are exact. `FRIENDSLOP_PERF` now logs `materials changed /s`,
`transforms changed /frame` and `hitches` (frames over 25 ms), and `=2` lists once which models
own the transforms that moved (`perf moved:`), naming unnamed ones by their components.

- **#1, the profile.** With the machine quiet, High runs at about 118 fps here before and after
  these changes, so it no longer sits at 60. What the CPU spends a frame on, in share of one core:
  transform propagation 46%, visibility checks 10%, marking dirty transform trees 9%, mesh
  extraction 7%, tree sway 3%. Inside propagation only about 8% is real work: the rest is Bevy's
  parallel propagation workers spinning on their shared queue's lock (`pthread_mutex_trylock`).
  That spin uses idle cores rather than lengthening the frame, and it lives in Bevy, so it is
  left alone.
- **#2, materials.** 240 changes a second was the doorway glow (`doorway::pulse`) rewriting its
  four materials every frame. It now steps in 1/32ths and only writes on a step: about 105 a
  second remain, from the day/night sky, ripples and embers, all already throttled.
- **#3, transforms.** About 10,100 global transforms were rewritten every frame. The movers were
  the bones of swaying trees (~2,300), the code-built bodies' bones (557, everyone everywhere), and
  New York's 96 tunnel lights sliding while nobody was in a tunnel. Trees and bodies now animate
  only while they are in the camera's view (both are a function of time, so they are where they
  should be when you look back), and the tunnel lights only while one is on screen: about 4,900 a
  frame now. Propagation went from 46% to 42% of a core: the lock spin above doesn't shrink with
  the work.
- **#8, tree rigs.** Rigs used to start in the same frame for every tree that came within 40 m
  (dozens at once when you cross a wood). Now at most two start per frame, nearest first.
  A camera-jump test (the screenshot shot list) couldn't show the difference cleanly, because
  each screenshot's readback is a hitch of its own; walking it by hand is still to do.

## Part 2: areas

### The rule

**Everything spawned into the world belongs to exactly one area, or is on a short named list of
globals.** An area is loaded while someone needs it and despawned when nobody does.

This is the same question `sites` already asks ("is anyone there, am I there"), applied to
scenery and colliders as well as mechanics. The rule goes into `AGENTS.md` next to "Every
mechanic has a site", so the agents that build each new idea follow it without being told.

- **Area:** a line in `data/base/areas.ron`: id, bounds, map file, how far out it loads, and how
  much further before it unloads. Each site names the area it is in.
- **Ownership:** everything an area spawns sits under that area's root entity (or carries an
  `InArea` marker). Unloading is despawning the root.
- **Global:** sky, camera, HUD, input, players, replicated gameplay state. Listed by name in one
  place.
- **New ideas:** a feature spawns its world dressing in a "when my area loads" system, never at
  game start.

### Multiplayer

Players can be in different places and the host simulates for everyone, so each area has three
layers with different rules:

| Layer | Loaded when | Why |
|---|---|---|
| Colliders and simulation | anyone is there (host); you are there (client) | the host is authoritative for every player |
| Replicated gameplay state | always | few entities; keeping them avoids losing progress and leaves replication order alone |
| Visuals, lights, sound | you are there | purely local, and the bulk of the cost |

So a host with a friend in New York keeps New York's colliders but not its meshes.

### The guards

These are what make the rule hold for every place, not just the first one migrated. They are
built before anything is migrated.

1. **Ratchet test.** A test lists every file that still spawns at game start. It fails if a file
   is added, and fails if a migrated file is still listed, so the list can only shrink. Today
   the list is 72 files.
2. **Ownership test.** A headless test loads the game, unloads every area, and fails if any
   mesh, collider or light is left that is not a global.
3. **Site test.** Every site in `sites.ron` must name an area that exists, alongside the id
   check in `tests/it/sites.rs`.
4. **Census in the perf log.** `FRIENDSLOP_PERF` prints entities, meshes and colliders per
   area, and a count for "no area". That last number is the progress bar.

### The inventory

The 72 files that spawn at game start today, grouped by where they would go. The grouping is a
first read from file names and needs confirming per file. Some features (`trains`,
`helicopter`) span two areas and will need splitting.

| Area | Features that spawn at start today |
|---|---|
| Global (stay) | `hud`, `ui`, `settings_panel`, `camera`, `controls`, `loading`, `atmosphere`, `ambience`, `sites`, `fps`, `stats`, `fate`, `voice_lines`, `campaign`, `objective`, `morality` |
| Commune | `world`, `scenery`, `zones`, `terrain`, `landmarks`, `holes`, `bear`, `crows`, `dave_commune`, `great_storm`, `landlord`, `gamble`, `totems`, `pets`, `goat`, `abuela`, `mushroom`, `gods_tears`, `halt`, `wendys`, `hide_and_seek`, `punishment`, `organ_currency`, `duel`, `climb`, `smoking`, `weed`, `fart`, `fart_sound`, `skills`, `helicopter`, `shipwreck`, `broadcast`, `clues`, `kori`, `lily_garden`, `surf`, `affair`, `sesbian_lex` |
| New York (with Penn, Fulton and Miami stacked) | `nyc`, `penn`, `fulton`, `fulton_hunt`, `miami`, `trains`, `vessel`, `vessel_jump`, `high_places`, `hats`, `ice_cream`, `boba`, `jokah`, `wallet` |
| Interiors | `warehouse`, `underground` |
| Off-map places | `afterlife`, `mute_spell`, `infinite` (the sky courses) |

### Order of work

1. **Area core, the globals list and the four guards**, in `friendslop-base` next to `sites`:
   the data file, loaded/unloaded state with hysteresis, root-entity ownership, and the loading
   curtain reused for teleports (helicopter, train). Nothing unloads yet; every place is simply
   on the ratchet list.
2. **New York**, as the proof: it is the biggest and already a separate map file and site.
3. **Off-map places and interiors:** afterlife, sky courses, warehouse, underground. Small and
   self-contained, and they flush out "feature holds a reference into another area" problems
   early.
4. **The commune**, the largest group. It only pays off when everyone is elsewhere, so it comes
   after the places people go to. Splitting it into districts (valley, forest, village) is a
   later, optional step.
5. **Wasteland chunks** joined to the same "who is near" input.

### Per-area checklist

An area is finished only when all of these hold:

- its files are off the ratchet list
- the ownership test passes with it unloaded
- the census shows zero meshes and colliders for it when nobody is there
- the two-player test passes: a host elsewhere keeps its colliders while a client is inside,
  and neither side holds the other's visuals
- its `FRIENDSLOP_START` point still works
- walking or travelling in shows no missing ground and no fall through the floor

The whole job is done when the ratchet list holds only globals and the census "no area" count is
zero.

### Risks

- **Features holding entity references into another area.** They need to look things up by
  site or spot instead.
- **Load hitch on arrival.** Teleports can hide it behind the curtain; walking in needs the
  load to start early and spread over frames.
- **Shared systems built for the whole map:** the navmesh, reflection probes and music signals
  each need a look.
- **Leaks the tests miss.** The tests catch spawn-at-start. Something spawned later by a global
  system and never tagged would only show in the census "no area" count, so that number needs
  watching until it reaches zero.

### What's built (November 2026)

All five steps. The ratchet list is empty (only `areas::GLOBALS` spawn at game start) and the
census "no area" line is zero in the forest, New York and the wastes.

- **Core** (`crates/base/src/areas.rs`): areas, layers, roots, `InArea`, `spawn_into`,
  `on_sim` / `on_show`, `Cover`, the four guards.
- **Every hand-made place is an area:** `nyc`, `commune`, `warehouse`, `underground`,
  `afterlife`, `infinite`.
- **The wastes** (`friendslop_world::wastes`) stream by chunk with the same "who is near"
  input: `Viewers { sim, show }` from `sites::People` and `place::Here`. Colliders stream
  around everyone on the host (only you on a client), meshes only around you. They are one
  `wastes` census line. `FRIENDSLOP_START=wastes` starts out there.
- **The census** puts everything in one line: its area; `globals` (`areas::Global`, or UI:
  camera, sun, clouds, sky, ambience particles, ears, the lexing hearts); `replicated`
  (replicated gameplay state and the local dressing under it); `wastes`; or "no area".
  Engine bookkeeping without a `Transform` (observers, resources) isn't counted.
  `FRIENDSLOP_PERF=2` logs once what "no area" holds. `nothing_in_the_world_is_in_no_area`
  checks it headless.
- One-shot world sounds belong to the visuals of the area they play in (`global` outside
  every area). Loops are global: the feature that starts one stops it.

- **Replicated things' dressing** follows its thing's area: a feature calls
  `areas::dress_in_area::<C>()` and dresses on `Added<areas::Shown>`. While the area the thing
  stands in shows, it has `Shown` and its collider-free child trees are filed in that area's
  visuals (`areas::Dress`). When the area stops showing, they go, and the rig, character rig,
  mesh and model on the thing itself are taken off. The thing stays. Villagers, kittens, rats,
  pets, crows, the bear, the goat, fat cats, Abuela, the wife, totems, burgers, the tome, the
  treasure and the tears use it. Replicated meshes in the forest dropped from 914 to 183
  (mostly players' bodies), and New York no longer draws the commune's creatures.
- The bonfire's smoke and sparks are the commune's (`ambience::spawn_bonfire`).

### Open decisions

- **Commune granularity:** one area for the whole commune (simpler, recommended first) or
  districts from the start.
- **Walking between New York and the commune:** if you can walk there, New York has to stream
  in on approach; if travel is always by vehicle, a curtain is enough.
