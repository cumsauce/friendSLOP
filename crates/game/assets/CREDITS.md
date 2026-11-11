# Asset credits

Everything in `assets/` is CC0 (public domain) so the game can ship it
anywhere. Git LFS stores the binaries (see `.gitattributes` at the repo root).

If a clone has no LFS data (bandwidth quota, plain `git clone` without lfs),
the game quietly skips these files and keeps its procedural-primitive look —
nothing in the gameplay depends on them.

## Models (`models/`)

All by **Quaternius** ([quaternius.com](https://quaternius.com)), CC0, via
their OpenGameArt upload pages:

- `models/characters/Cowboy_Male.glb`, `Cowboy_Female.glb`
  — from the [Ultimate Animated Character Pack](https://opengameart.org/content/animated-characters-pack)
  (converted FBX → glb with FBX2glTF; the rig's 11 built-in animations come
  along and the game plays Idle/Walk/Shoot/RecieveHit).
- `models/nature/*.gltf` — the trees, from the
  [Stylized Nature Mega Kit](https://opengameart.org/content/stylized-nature-megakit)
  (kept as `.gltf` + `.bin` + textures; the kit's normal-map textures were
  stripped to save weight — this art style doesn't use them).

Repack note: the cat-vision palette (#12) deliberately does not recolour
textured materials yet, so these models keep their natural colours for now
(see GAME_DESIGN.md open questions).

### Made for friendSLOP with the modeler

Built by agents with the [modeler](https://github.com/cumsauce/modeler) MCP tool and released
as **CC0**. The editable sources (op graphs, procedural materials, the
`friendslop` style) live in `crates/game/.modeler/`, outside `assets/`; open
the catalog there and `export_model` again to change one. We're replacing the
kit models above with these over time.

Each is a `.gltf` + `.bin`; their baked textures are shared KTX2 files in
`textures/` (named by material, channel and content hash).

- `models/nature/rock_1..3`, `pebble_1..3` — faceted stones (stone
  material: cracks, moss on top for the rocks).
- `models/nature/mushrooms` — a cluster of four spotted mushrooms.
- `models/nature/bush_1`, `bush_2` — bushes of faceted leaf clumps
  (the second with white blossoms).
- `models/nature/grass_1`, `grass_2`, `fern`, `plant_1`, `plant_7`,
  `clover` — ground cover.
- `models/nature/flower_1..3` — the tall meadow flowers (pink, an orange
  group, purple).
- `models/props/campfire`, `campfire_fire` — the commune campfire
  (ash bed with embers, stone ring, log teepee) and its emissive flames.
- `models/props/revolver` — the duel revolver (blued steel, walnut grips).
- `models/props/` (the rest) — the building props the room placer and house
  grammar lay out: bed, table, chair, stool, counter, cupboard, shelf,
  crates, crates_single, barrel, rug, wall_lamp, hanging_lamp, pipe_run,
  cobweb, debris, books, cup_plate, bucket, sack. Their kit boxes still
  collide.
- `models/surfaces/` — one swatch per building surface (stone, wood, leaves,
  cloth, metal, painted): their tile textures replace the generated ones on
  every plain material (`friendslop_world::surfaces`).
- `models/scenery/` — the scenery and district set dressing: logs, stumps,
  ruin blocks, rubble, the lean-to shed, hay bales, crates, the woodcutter's
  hut and log pile, the maze statue, the castle arch, the quarry crane, the
  windmill and its sails, the jetty, log seats, string-light posts, reeds and
  lily pads.
- `models/features/` — the feature set-pieces and objects: the shipwreck,
  treasure chest, Wendy's shack and burger, the helicopter and its rotors,
  the burnt parliament and clock tower, the commune flag pole, the duel
  fence, barrel and hay bale, the storm's fallen tree, lamppost, slates and
  stele, the warehouse exterior, oil drum and lantern, gravestone, lectern,
  the totem vault, totem base and magic door, the gambling table and wheel,
  the magic mushroom, music box, teacup, RPG and rocket, god's tear, crow
  egg, Abuela's rocking chair and knitting, the toilets, stall doors and
  panels, and the underground's manhole, cellar door, trapdoor, grate, jar
  shelf and ladders.
- `textures/*.ktx2` — the baked textures of the models above, shared by
  every model using the same material.

## Audio (`audio/`)

- `audio/sfx/gunshot.wav` — ".22 Pistol" from the
  [Gunshots pack](https://opengameart.org/content/gunshots), CC0.
- `audio/sfx/punch.wav` — from "[37 hits/punches](https://opengameart.org/content/37-hitspunches)"
  by independent_nu, CC0 (FLAC converted to WAV).
- `audio/sfx/crow_caw.wav` — "[Crow caw](https://opengameart.org/content/crow-caw)", CC0.
- `audio/sfx/eat.ogg` — "[apple bite](https://opengameart.org/content/apple-bite)", CC0.
- `audio/music/catsong.ogg` — "Catsong" by **Dan Knoflicek** (uploaded to
  [OpenGameArt](https://opengameart.org/content/catsong) as CC0). No longer played (the
  generated fusion soundtrack replaced it); kept for credit and possible reuse.
- Generated music (`features/fusion`) uses the note-block instrument synthesizers of
  [foley-synth](https://github.com/cumsauce/foley) (crate `foley-synth`, MIT OR Apache-2.0),
  as a git dependency rendering samples at startup. No audio files involved.
- `audio/ambience/birds.ogg` — "[Ambient Bird Sounds](https://opengameart.org/content/ambient-bird-sounds)"
  by isaiah658, CC0 (file: https://opengameart.org/sites/default/files/birds-isaiah658_0.ogg).
- `audio/ambience/fire_crackle.ogg` — "[Fire Crackling](https://opengameart.org/content/fire-crackling)"
  by AntumDeluge, CC0 (file: https://opengameart.org/sites/default/files/fire-1_0.ogg).
- The wind bed is synthesized in code (`features/ambience.rs`), no file.
- `audio/voice/grains.wav` — the gibberish voice bank: 50 short voiced syllables (90 ms each)
  cut by `scripts/voice_grains.py` from five **CC0** recordings on Wikimedia Commons (only the
  bits used are kept): "Sophie in 't Veld - voice - en.flac", "Female-vibrato-1.wav",
  "Brian Wong - voice - cantonese.flac", "Adrian Goldsworthy - voice - en.flac" and
  "Michael Noer - voice - da.flac" (https://commons.wikimedia.org/wiki/File:<name>).

## Fonts (`fonts/`)

- `fonts/Nunito.ttf` — **Nunito** (variable weight) by The Nunito Project Authors
  (https://github.com/googlefonts/nunito), from
  [google/fonts](https://github.com/google/fonts/tree/main/ofl/nunito), licensed under the
  [SIL Open Font License 1.1](https://openfontlicense.org). Used for HUD text; the HUD falls
  back to Bevy's built-in font if the file is missing.
- `fonts/Sniglet-ExtraBold.ttf` — **Sniglet** ExtraBold by Haley Fiege, Brenda Gallo and Pablo
  Impallari, from [google/fonts](https://github.com/google/fonts/tree/main/ofl/sniglet), licensed
  under the [SIL Open Font License 1.1](https://openfontlicense.org). Round display font for
  titles and banners; they keep the HUD font if the file is missing.
- `fonts/Fredoka.ttf` — **Fredoka** (variable weight and width) by The Fredoka Project Authors
  (https://github.com/hafontia/Fredoka-One), from
  [google/fonts](https://github.com/google/fonts/tree/main/ofl/fredoka), licensed under the
  [SIL Open Font License 1.1](https://openfontlicense.org). Display face of the "Indigo
  journal" HUD (card titles, countdowns, the prompt's verb), used at SemiBold; that text
  falls back to Nunito ExtraBold if the file is missing.
- `ui/*.png` (mic, mic_slash, tear, gas, cat, warn, target, hand) — the HUD's glyphs, drawn
  for friendSLOP in its Figma UI kit ("E · Indigo journal") and released as **CC0**.
  Rasterised white at 64 px with `resvg` and tinted in game; a missing file draws nothing
  (every glyph has a text label beside it).
- `ui/logo.png` (the illustrated friendSLOP logo) and `ui/check.png` (the checkbox tick) —
  exported from the same Figma UI kit, **CC0**. A missing logo just leaves its space empty on
  the title screen and loading curtain; a missing tick leaves checked boxes plain indigo.
