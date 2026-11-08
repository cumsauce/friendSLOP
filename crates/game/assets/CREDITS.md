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
- `models/props/revolver.glb`
  — Revolver_2 from the [Survival Pack](https://opengameart.org/content/lowpoly-survival-pack),
  converted FBX → glb (a static model; the animated gun in the separate Low
  Poly Animated Guns pack converted with a broken rig, so we don't use it).
- `models/props/bonfire.glb`, `bonfire_fire.glb`
  — from the Survival Pack (converted FBX → glb).
- `models/nature/*` — trees, bushes, grass, flowers, mushrooms, pebbles and
  rocks from the [Stylized Nature Mega Kit](https://opengameart.org/content/stylized-nature-megakit)
  (kept as `.gltf` + `.bin` + textures; the kit's normal-map textures were
  stripped to save weight — this art style doesn't use them).

Repack note: the cat-vision palette (#12) deliberately does not recolour
textured materials yet, so these models keep their natural colours for now
(see GAME_DESIGN.md open questions).

## Audio (`audio/`)

- `audio/sfx/gunshot.wav` — ".22 Pistol" from the
  [Gunshots pack](https://opengameart.org/content/gunshots), CC0.
- `audio/sfx/punch.wav` — from "[37 hits/punches](https://opengameart.org/content/37-hitspunches)"
  by independent_nu, CC0 (FLAC converted to WAV).
- `audio/sfx/crow_caw.wav` — "[Crow caw](https://opengameart.org/content/crow-caw)", CC0.
- `audio/sfx/eat.ogg` — "[apple bite](https://opengameart.org/content/apple-bite)", CC0.
- `audio/music/catsong.ogg` — "Catsong" by **Dan Knoflicek** (uploaded to
  [OpenGameArt](https://opengameart.org/content/catsong) as CC0).
