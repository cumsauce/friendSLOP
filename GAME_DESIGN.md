# Game design (living document)

Claude keeps this up to date after every change. It's the game's memory: what exists, what friends
asked for, and what nobody has decided yet.

## Fixed

- Multiplayer: one friend hosts, others join with a code.
- Voice chat.
- 3D.
- The shared goal (#75): **free the Commune from your landlord and get into heaven**, over five
  chapters that use every mechanic, ending in the landlord's eviction (#11) and the gates of
  heaven opening.

## Gameplay loop

<!-- The current loop in plain words, for friends. Refreshed on every change; the Discord bot
posts this section every few landed ideas (#153). Keep it under ~1800 characters. -->
**Play in the browser:** https://cumsauce.github.io/friendSLOP/

**The goal:** free the Commune from your landlord and get into heaven, over 5 chapters.

**Each chapter:** a checklist (top-left) of jobs in new places:
1 *Moving In*: leash the pets, eat the mushroom, climb the sky islands to Abuela in heaven.
2 *Rent Day*: open a vault door, break out of HALT, hide and seek, grab burgers at Wendy's.
3 *The Heist*: carry out the Fire totem, use gods tears, a duel, trade a crow egg to Sewer
Paul in the tunnels. 4 *General Strike*: all of it at once, plus 2 soul pieces from the
warehouse for Jegg. 5 *The Eviction*: make Jegg whole (5 pieces) and the rest, one last time.
Each chapter ends with the landlord: pay rent in organs, pickpocket him, then a boss fight.
Each ticked job is a Heaven Pass tier; chapter 5 opens heaven.

**Always going on:** the Stillness freezes everything for 20 s every 4 minutes. Fart near an
NPC and get caught 4 times: villagers chase you and float you. Villagers fart and blame you.
Low morality brings a helicopter that shoots at you. 5% of actions fail sillily. Marmalade's
wheel makes a rule fairer or crueller. Your fat cat grows unwatched and drinks pee; at 10x
it gobbles up you or anyone in its mouth.
Say "mute" at a friend (Q) and they're silenced until they RPG you. Stand still 3 min
and systemd (PID 1) kills you.
**Dying** puts you in heaven: friends beg at your grave (out loud counts double) within 60 s,
or read the Tome of Uncancelling over it. Otherwise it's hell: hop across a lava pit to get out.
Some nights (real clock, 10pm-5am) you hear whispers behind you.

**Just for fun:** Mei, smoking, lex pile, bear, goat, Discord dates, rat twins.

**Where it's thin:** what you carry back only ticks the checklist; organs only come from the
chapter refill and the landlord. Spells, stats and being shot don't change play. Gambling is
free. Six players beat the landlord fast. Only the RPG, a 10x cat and systemd can kill you.

## What exists

- A grassy meadow floor (see "Nicer look" below).
- **Structure pipeline** (structures rework). Buildings come from a shared kit
  (`crates/structures`) and a **world seed** the host rolls each session (`features/world_seed.rs`;
  `FRIENDSLOP_SEED=n` fixes it), so every peer builds the same world:
  - Walls never overlap (no flicker), each house is one merged mesh per material with textures
    in world metres, and colliders come from the layout (roofs and gables included).
  - **Village**: six plots get generated houses each session (cottage, townhouse, barn or shop;
    gable, hip or flat roofs; doors, windows, balconies, shopfronts). The tower, barn, market,
    walls and the big cottage by the cellar door stay fixed.
  - **Furniture**: each generated room is filled by a seeded placer from a 19-prop library
    (bedroom, kitchen, shop, storage themes) that never blocks a door or the stairs.
  - **Sewer**: a seeded room graph (chambers, loops, dead ends) between fixed entrances,
    ladders, the cellar and Paul's chamber, built from detailed hallway pieces (trim, pillars,
    beams, pipes, lamps, a water channel).
  - The warehouse floor plan uses the generic WFC crate (`crates/wfc`).
  Open: furniture in the sewer and warehouse; more house types.
- **First-person bodies** (#53). Everyone has a body (your own character, see below) and
  plays in first person: WASD walk, mouse look, Space jump, Shift run, cursor locked. Left click
  uses whatever is under the crosshair (mushroom, tears, landlord, totems, pets); doors are used
  from the side you stand on; you fart where you stand; your pet follows you. Bodies can't walk
  through trees, rocks, walls, sheds, bales or crates: the host walks them with a kinematic
  character controller (`crates/kcc`, ported from bevy_ahoy at `630a4573`, MIT OR Apache-2.0) — it
  slides along walls, walks slopes up to 45°, steps up stairs and kerbs up to 0.5 m (village
  stairs, ramps, the tower — no special-cased floors anymore), snaps down slopes instead of
  hopping, and jumps ~0.9 m. Headless tests (no physics) fall back to a plain terrain-height
  walk. V swaps to the old
  overview camera (WASD pans, free mouse pointer, clicks under the cursor) and back. Duels still
  put you in the arena with a duel body; your walking body waits where you left it. Each body
  has a floating "player N" label, and yours wears a little yellow marker (seen from the
  overview). Esc first frees the mouse (click to grab it again); a second Esc leaves.
- **Abuela in heaven** (#105, just meat and cheese: "a character similar to my grandma ... she
  only spoke spanish so please have her make old sounding spanish sounds"). Heaven had no place,
  so the highest floating island is now a small heaven: clouds on the rim, flowers, a golden halo
  arch and warm light. Abuela sits there in a rocking chair, knitting: small and round, silver hair
  in a bun, round glasses, cream shawl over a rose cardigan (built with `CharacterLook`, extras on
  the new `HeadAnchor`). She greets whoever climbs up to her, and you can click her to talk
  (server-authoritative: `VisitAbuela` -> `AbuelaSays` to everyone, replicated `Abuela { visits }`).
  She says short loving Spanish phrases ("¡Ay, mi niño!", "¿Ya comiste?", "Te quiero mucho",
  "Dios te bendiga"...) shown as a subtitle with an English gloss, hearts rise above her, and the
  one she spoke to is told "you feel loved". Her voice is synthesised from the phrase's own Spanish
  syllables (open vowels, rolled r, soft s, nasal ñ) as a quiet, breathy, gently trembling older
  woman's voice, played spatially at her chair (`features/abuela.rs`).
- **Climbing and floating islands** (#99). Walk into anything steep (a wall, a rock, a tree
  trunk, a house, a spire) and hold W and Space to climb it at 2.5 m/s; A/D shuffle sideways on
  the face, and at the top you haul yourself over the edge. Let go of Space to drop. Above the
  south-east meadow (around x 55..63, z 55..67) three grassy islands float in the sky, at 10 m,
  16 m and 22 m. A rock spire rises to each one's rim, standing on the ground or on the island
  below, so the way up is climbing. Nothing is up there yet but the view.
- **Make your own character** (#100, doomy). The main menu's Character button opens a creator
  with a live, turning 3D preview: body type (person, round critter, tall noodle, four-legged
  beast), height / girth / head size / limb length, skin or fur, clothes and accent colours, eyes
  (googly, beady, sleepy, cyclops), ears, horns, tail, snout, an accessory, and Randomize. It's
  saved to `friendslop/character.txt` in your config folder and sent to the host when you play;
  the host clamps silly values and everyone sees you. Under the hood (`people.rs`) every body is
  a little procedural rig (hips, spine, head, arms, legs, tail bones with primitive meshes) that
  walks from its speed, breathes, glances around when idle and wags its tail. Villagers and the
  menu's campfire crowd use the same system with seeded random looks. Looks are visual only: the
  collision capsule never changes, bodies are shrunk to fit it.
- **Hide and seek** (#61). Anyone presses K to start (needs 2+ players; solo it just says so).
  The server picks a random Seeker, everyone else hides. For 30 s the seeker is frozen in place
  behind a black blindfold screen; then they have 3 min to find people. Seekers tag a hider by
  clicking them within reach; a found hider becomes a seeker too. The round ends when nobody is
  left hiding (seekers win) or time runs out (hiders win); results show for 10 s and the next
  round starts with a new random seeker, looping until fewer than 2 players remain. Phase, timer
  and role show in the top-centre line, tags as toasts. It runs alongside the landlord objective
  (both stay). The scenery is the hiding ground; no per-round regenerated layout yet. During
  Hiding/Seeking, floating name labels stop giving people away: seekers see no labels but their
  own, and a hider's label is hidden from everyone but themselves (back after the round). While
  seeking, a hider hears a generated heartbeat when a seeker is within 6 m, faster as they close
  in. Player bodies already collide with cover (host sweeps a capsule and slides along it).
- **Districts across the wider map** (`zones.rs`, after "too dense in one little area, can't run
  around and hide"). The centre commune's cover is thinned to ~40% (18 clusters) for running lanes.
  Around it, within ±90 m: a north forest (z < -35: ~230 tall trees with trunk colliders,
  undergrowth, fallen logs, boulders, a hidden clearing with a woodcutter's hut ruin); south-west
  ruins (a 24×24 m seeded hedge/stone maze with 3 m walls, dead ends and a gilded statue in the
  centre, 6-10 m broken castle walls with an archway, a collapsed tower to walk into, rubble, and
  rock faces, crates and a wooden crane around the quarry rim at (-55, 65)); a sparse west valley
  (trees, rocks, fences, shepherd's shelter); meadows with copses, low stone walls and hedgerows
  (~1.2 m, crouch cover), haystacks and a windmill with turning sails at (-8, -31). The east
  village block (x 35..80, z -35..35) is left for houses. Everything sits on `terrain::height_at`.
- **Commune villagers** (#61, the "NPCs" part). Twenty villagers: ten regulars (Kevin, Brenda, Old Tom, Moss, ...)
  and the ten Discord friends (#159, below),
  built by the same character system as players, rolled from the same option pool, so hiders
  can blend in. The
  server moves them at 5 Hz: wander between the bonfire, ruins, pond and cover spots, pause or sit,
  never enter the duel arena, stop during the stillness. Clicking one says hello; a seeker who
  clicks one while seeking gets "that's just Kevin from the commune" and is frozen for 3 s
  (everyone hears about it). Nearby villagers sometimes mutter a line (toast). On the host they
  sweep a ball along each step and veer around trees, rocks and walls (or pick a new spot if boxed
  in). Once the host is in the game it builds a navigation mesh (rerecast, off the main thread,
  ~3 s; see `tests/it/navmesh.rs`) over the terrain and the village architecture, and most trips then
  go overland between the districts — commune centre, east village market, the maze's statue, the
  castle ruins, the forest clearing and the lake shore — via a landmass agent per villager. Until
  the mesh is ready they fall back to straight-line steering between the local spots. Scattered
  trees, rocks and props are deliberately left out of the mesh (they punch holes that trip
  rerecast; see `vendor/rerecast/README-friendSLOP.md` for the carried fix); `avoid_cover` still
  steers around them. They have no name label.
- **The Discord friends walk the commune** (#159, catherine). Everyone who has sent in an idea
  (doomy, catherine, Freyja Moth, Kori.exe, just meat and cheese, corvy, bben, WeiYa, arseeN,
  hadalade; the list is hand-written in `villagers::NAMES`, add new friends there and in
  `discord_friends::LINES`) is one of the villagers, so they wander, blend in for hide and seek
  and chase farters like the rest. Hover one and it says "chat up". Each chat (one every 1.5 s
  per friend) is a heart and gets a line from them, mostly about their own ideas, sweeter each
  time ("i farted when i saw you. that's how it starts."). On the 5th they're **dating** you:
  everyone sees "catherine is dating player 1 now <3" and hearts float over them. A friend who's
  dating someone turns everyone else down until that player leaves. Dating does nothing else yet.
- **Evil rats in the tunnels** (#159, catherine). Each friend has an evil twin with their name
  backwards (enirehtac, ymood, exe.iroK, ...): a grey rat with pink ears and glowing red eyes,
  scurrying between the tunnel cells under the map. The lore, as the rats tell it when you adopt
  one: they were the evil halves of everybody and let themselves be turned into rats to save
  the world, and nobody said thanks. Click a stray to adopt it (one rat each, on top of a surface
  pet); it squeaks and follows you everywhere, popping up at your heel when you climb a ladder.
  Click your rat to hear something evil ("i saved the world and all i got was this tail.").
  If you leave, your rat goes home to the tunnels. Rats don't count towards connecting the pets.
- **One click, one thing** (polish). A left click resolves to the single closest clickable thing
  under the crosshair/cursor (`features/interact.rs`), so one click never triggers two features,
  and only within ~3.5 m of your body; otherwise the HUD toast says "too far away". The server
  re-checks the distance (and works out your side of a door from where you stand), and
  refusals (too far, frozen, already owned, can't afford, vault locked, wrong side) show as a
  toast.
- Ambient life (`ambience.rs`, local visuals only): butterflies, pollen motes, fireflies,
  falling leaves, birds crossing high overhead, smoke and sparks from the campfire, and the
  scenery props swaying gently in the wind. Capped at about 190 recycled particles.
- Lots to hide behind (#60): ~44 seeded clusters across the whole map (not just a ring):
  tree groves, rock outcrops, bush hedges, fallen logs, broken stone walls, open-fronted sheds,
  hay bales, fences and crate stacks. Trunks, rocks, walls, sheds, bales and crates have static
  colliders. Landmarks (centre, arena, parliament, bonfire, vault, landlord, totems, animal homes)
  stay clear, and the strip between the default camera and the centre only gets low cover.
  Everything sits within ±22.5 m on flat y=0.
- A bigger world (feedback: "more scale ... so its not just one little area"): the walkable area
  is ±90 m (was ±25), with a soft wall at ±88 and hills rising beyond it (`crates/world/src/terrain.rs`). The
  commune square (±25 m) and the east village district (x 35..80, z -35..35) are dead flat; the
  north forest (z < -35) rolls gently; the west valley has a climbable 12 m hill at (-62, -6), a
  ridge along the far edge and a lake at (-44, 20); the south-west ruins (x -80..-30, z 30..85)
  are nearly flat with a 5 m quarry pit at (-55, 65). Dirt paths run from the centre to every
  district. The ground is 64 chunked meshes plus a heightfield collider; bodies follow
  `terrain::height_at` up and down slopes. Walk 5 m/s, Shift runs at 10.5 m/s.
- A floor dressed with low-poly nature (trees, bushes, grass, flowers, pebbles), a campfire
  burning beside the burnt parliament (stone ring, glowing embers, four log seats), a pond with
  ripples, lily pads, reeds and cattails, rocks, a little jetty with a lantern and two ducks
  paddling round, lantern posts strung with bulbs along the paths to the campfire and the pond
  (`crates/world/src/landmarks.rs`), and a generated, adaptive 90s jazz-fusion soundtrack (`features/fusion`,
  asked for in chat: "change the music ... jazz fusion ... 90s/FM synth vibe ... adapts to the
  game state"; it replaced the looping "Catsong"). The music lives in its own Bevy-free crate
  (`crates/fusion`; `features/fusion` is just the game glue). Live FM voices (DX7-style EP,
  slap and fretless bass, lead/brass with portamento, bell pad, kalimba), subtractive voices (a
  CS-80 style horn section that shouts, pushes and swells around the lead, string machine,
  Minimoog-style lead), a blown flute/bottle (odd-harmonic FM plus breath noise that blooms
  with each note, bottle-like low and flute-like high), a mellow nylon-string guitar (a
  plucked-string model, flesh-not-nail attack, a body resonance, a big reverb and a touch of
  echo: fingerpicking in the meadow and the sky islands' quieter sections, bossa-nova
  comping in the forest, the bolero strum and requinto-style answers near Abuela), a
  synthesized kit with ride, clap, shaker, congas and toms, and foley-synth note-block
  bells/harp. Keys, bells, harp, kalimba, flute and guitar ride a tempo-synced echo. The lead tells a story over eight bars
  (statement, answer, development to a peak in bar seven, restatement, cadence; chord tones on
  long notes and guide tones on changes), a second voice answers it in its gaps (flute, harp,
  kalimba, strings or horns by mood), and solos trade between two synths. The sound evolves:
  brightness and reverb/echo amounts follow the sections, pads and horns drift slowly, and a
  reversed cymbal rises into big sections. Per-bus EQ, chorus, a tempo-synced echo, a wet room
  and per-mood make-up gain keep the mix balanced (checked against a typical jazz/pop octave-band
  balance with `cargo run -p fusion --release --bin fusion-render -- stems`). A director picks the next bars at every bar line from a mood: menu (signature hook
  over "Just the Two of Us" changes), meadow (or the sunny, kalimba-and-flute "Kite Day"), commune/village (city-pop, or one of two upbeat songs: the bouncy "Sunny Side" or the driving
  "Night Market"; each visit picks one, and the band may move on after a breakdown), forest/valley (flute,
  harp), maze/ruins (minor, bells), the underground (#127: dub-wet D minor, a bottle-flute
  lead, kalimba drips, offbeat stabs, congas, extra reverb and echo), sky islands (dreamy), near Abuela (bolero), hide-and-seek
  hiding/seeking, duel (150 bpm), landlord (chromatic, building), stillness (holds on a pad),
  plus a victory kime when a round/duel/chapter/landlord is won. Sections (intro, A, hook, B,
  solo, breakdown), key changes, fills, 7/8 kime bars, layers faded by intensity. It follows
  you (asked for in chat: "if they're going upstairs maybe it starts shifting scales upwards"):
  every storey above the ground (2.4 / 5.4 / 9 / 15 m: a floor, the next floor, a spire, the
  islands) moves the key up a step of the major scale, up to a fifth, each step prepared over
  half a bar by the new key's dominant with a riser and landing on a cymbal; coming down walks
  it back. While you climb or fall, melodies, runs and arpeggios lean that way, and higher up
  the band plays brighter in a bigger room. Out in the world running lifts the intensity,
  standing about for a while lets the band lay back, friends within 15 m fill it out, and the
  music ducks under voice chat. `fusion-render climb <mood>` renders a climb and back. Local only,
  ~2.5% of a core, respects master volume. Plus real sound effects
  for crows, duels and mushroom bites, and an ambient bed (#62): quiet birdsong, a synthesized
  wind loop, and a crackling-fire loop heard at the campfire.
  **Night whispers** (#235, `features/whispers.rs`): the game has no day and night, so night is
  your own clock, 22:00-05:00 local time. Each night gets one roll (15%) the first time you
  play it (saved in `whispers.txt`, so restarting doesn't re-roll); if it hits, 40 s to 10 min
  later two whisperers and a low murmur start just behind your head for 7 s and stay behind you
  however you turn. Made from shaped noise (breathy formant vowels, "s"/"sh" hisses, a muffled
  murmur). Only you hear it; each player rolls their own night.
  Procedural sounds (bear, HALT, voice lines, farts) render at 44.1 kHz with band-limited
  oscillators, a gentle low-pass, click-free fades, normalised levels and dithered output.
  **3D sound** (`spatial.rs`, polish): world sounds are heard where they are, through our own
  little mixer (1/distance falloff past a near radius, equal-power level and arrival-time
  differences between the ears, head shadow, darker behind you, air absorption with distance,
  muffling when a collider is between you and the sound, and a small diffuse tail so far things
  sound far). Spatial: crow caws (from the crow), the bear's growl (following the bear), other
  people's farts (from where they farted; yours stay in your head), other players' footsteps,
  the "aww" of a smitten NPC, Abuela's voice and the campfire. You hear from your head, facing
  where the camera looks. **Voice chat is proximity in the world**: full within 3 m, fading out
  and panned to where friends stand, but never below a faint floor (0.15), so everyone can still
  talk from anywhere. Menus stay a party call. All of it comes from CC0 asset packs stored in Git LFS
  (`crates/game/assets`, credits in `crates/game/assets/CREDITS.md`); every peer renders the
  same scene from a fixed seed, and if the files are missing the game quietly falls back to its
  procedural-primitive look.
- **Currency: organs** (#22). Each organ's value = creature worth (human 10, cow 5, pig 4,
  chicken 2, rat 1) × organ importance (brain 10, heart 9, lungs 7, liver 6, kidney 4,
  stomach 3, appendix 1). Every player has an organ purse, shown top-right. Everyone starts with
  a rat appendix, a chicken kidney and a pig stomach (worth 21), refilled at the start of every
  chapter. Organs pay the landlord's rent; pickpocketing him takes one back, and beating him
  shares his whole stash out between every tenant (#135). Nothing else earns or spends them yet.
- **Crows** (#2). Three cute black crows hang around the floor, hopping and wandering about. Every
  so often one puffs up, caws (shown as "caw!" text) and you hear a crow sound. The server spawns
  them; every peer animates them locally.
- **The bear** (#4). Exactly one cute, round, friend-shaped brown bear ambles slowly around one
  side of the floor. Every ~13 seconds it rears up and does a scawy (cute) growl: a short
  procedurally generated rumbly sound plus "grrr~" text. It doesn't do anything else yet.
- **Voice-altering mushroom** (#6). One red spotty mushroom grows somewhere on the floor. Click it
  to eat it (walk up and click it under your crosshair). Your own voice
  then becomes much higher (1.5-1.9x) or much lower (0.55-0.7x) at random for 45 seconds, run
  through a small pitch-shifter so everyone hears it, with a bite sound when the server confirms
  the munch. A new mushroom grows elsewhere 10 s later.
- **Cat-vision palette** (#12). Everything is shown in a bright, cheery storybook-MMO palette, but
  only in colours cats can see: blues and yellows, no red-green difference. Every material is
  run through a cat-vision filter (then lightened and saturated), and the sky is bright blue. So
  the "red" mushroom and the green floor both look yellowish to us, like they would to a cat.
- **Nicer look** (#47). All art is generated in code (`art.rs`), no downloaded assets: a
  seamless procedural grass texture on the floor, a warm sun with soft shadows, sky-blue ambient
  light and distance haze, and a meadow of grass tufts, flowers and pebbles. Creatures got
  googly eyes and more detail (crow wings, tail and legs; fluffy goat with muzzle and hooves;
  per-pet ears, tails, beak and feet; landlord's moustache, arms, shirt, hat band and watch;
  Dave's beanie and legs). Props got trims too: music-box lids and gold feet, carved totems,
  capped vault walls, leafier storm trees, parliament columns and rubble, arena corner flags.
  Everything still passes through the cat palette.
- **PBR surfaces on the primitives** (#128, doomy: "rn they're just flat materials can you proc
  gen, make shaders, or use textures? or all in combination?"). Chose procedural textures, no
  custom shader and no downloaded files (`crates/world/src/surfaces.rs`). Every plain material (no texture of its
  own, opaque, not glossy, glowing or unlit) is sorted by its colour into stone (greys), wood
  (darker browns/reds), leaves (greens), cloth (other strong colours), metal (metallic) or
  painted (skin, pale and light warm colours), and gets that surface's generated greyscale
  detail, normal map and roughness map: cracked cobbles, wavy wood grain, leafy bumps, woven
  threads, brushed metal, a soft painted grain. Colour still comes from the material, so the cat
  palette keeps working. Cubes, spheres and cylinders stretched by their transform are re-baked
  once per size with UVs in metres and tangents, so the grain is the same size on a crate and on
  a 10 m wall. Glossy eyes, water, lamps, the textured ground, vertex-coloured meshes and the
  glTF models are left as they were.
- **Duels** (#14). Press a number key to challenge another player; they press Y/N. The two
  duelists get bodies in a small walled arena on the floor — animated cowboy/cowgirl models, or
  capsule-and-gun stand-ins if the asset files are missing — and play first-person (WASD, mouse,
  click) with a revolver viewmodel while everyone else watches. "One in the chamber" rules: one
  bullet, every hit kills, a kill refunds your bullet, with no bullets you punch up close, you
  respawn with one bullet. 3 lives each; the last one standing wins. One duel at a time. Shots,
  punches and hits play sounds for everyone; nothing is won or lost beyond bragging rights yet.
- **Commune of Dave** (#3). Parliament is a smouldering ruin in a corner of the floor, with the
  commune's flag (a Union flag with a little Welsh flag and red dragon in the middle, because
  Dave isn't a coward; #43; made more anarchist in #57: red-over-black anarcho-syndicalist field,
  black crosses, and a circled A sprayed on the corner), the flag swinging gently on its pole, Dave himself (fist raised) and a banner at the top of the screen. Scenery
  only for now; every peer spawns it locally.
- **The judgmental goat** (#20). A white goat with half-closed eyes stands on the floor, slowly
  shaking its head. It comments passive-aggressively (on-screen text) on every choice it can see:
  someone joining or leaving, the mushroom being eaten, you muting or unmuting, and, after 40 s of
  nothing happening, doing nothing. There are few choices in the game yet, so it judges those.
- **Morality** (#21). Every player has a morality score (starts at 0), shown bottom-left with a
  label (reprehensible / dubious / neutral / decent / saintly). Actions have a random moral value
  within a sensible range: eating the mushroom is -2..+2. Harming anyone is -5..+3, except harming
  (in any way) the nobility, capitalists, landlords or billionaires, which is **always rewarded**
  with +6..+10 (corrected by Freyja Moth in #43). Wired up so far: eating the mushroom, and
  every pickpocket or punch on the landlord (always rewarded). Your score carries over from
  chapter to chapter and only resets when a new campaign starts (#135). Being bad gets you hunted
  by a helicopter (below).
- **The helicopter** (#134). Being bad gets you hunted: while your morality is -6 or lower
  ("dubious" or worse) a dark green helicopter flies in from the far corner of the map, hovers
  14 m above and a little beside you, follows you faster than you can run (11 m/s) and fires
  bursts of 5 bullets at you with a 2.6 m spread (tracers, dirt puffs, generated rotor thump and
  gunshots everyone can hear). Everyone sees who it's hunting. Get your morality back above -6
  (harming the landlord always helps) and it flies home; it also leaves when a new campaign
  resets morality. It pauses in the stillness. A bullet within 0.7 m of your feet is a hit, but being
  shot only shows "You've been shot!" so far.
- **Farting** (#18). Every player's gas pressure builds from 0 to 100% over 40 s (shown
  bottom-right). Press F (with at least 20%) to fart where your body stands (in the overview
  camera: wherever the mouse cursor points on the floor; your duel body when duelling).
  If any NPC (the bear, a crow, Dave at parliament, or one of the ten commune villagers, #135)
  is within 4 m, you are **shamed**: for
  20 s your screen blurs, "SHAME" flashes in red and a scary, distorted chiptune (generated in
  code) plays. Everyone sees a line saying who farted and whether they were caught. So the trick
  is waiting for the wandering crows and bear to be elsewhere.
- **Fart noises** (#26). Every fart is heard by everyone and is synthesized on the spot by a
  small physical model: gas pressure pushes a pair of "lips" that buzz like a brass player's,
  the gas drains (so the fart sags and fades), and the airflow adds turbulent texture, shaped
  by a "cheek" filter. Each player tunes their own fart: `[` `]` pitch (30-400 Hz), `-` `=`
  length (0.15-4 s for a full tank; emptier tanks are shorter), `;` `'` timbre (tight and dry
  to loose and wet, with sputtering). The settings are shown bottom-right.
- **Romance** (#97). Sometimes (one shameful fart in three) the NPC who caught you farting
  falls for you instead of just judging: pink hearts float up above them and everyone hears a
  soft, generated chorus of "aww". You are still shamed. It's only a moment so far; the NPC
  doesn't remember you afterwards.
- **Punishment** (#123). Every time an NPC catches you farting counts (shown bottom-right as
  "caught farting N/4"). On the 4th catch the count resets and the commune goes into **attack
  mode** for 60 s: all twenty villagers (friends too) drop what they're doing and run straight at your body
  (4 m/s, a little slower than walking, so you can stay ahead by keeping moving) and crowd you
  when they reach you. Everyone sees a red "ATTACK MODE" line with a countdown, the band goes
  quiet under frantic, high-pitched string music (bouncing chromatic 16ths, shrieking stabs,
  a trembling cluster, generated in code) and every chasing villager jingles like a pocket full
  of keys. Catches don't count while you're already being chased; the clock pauses in the
  stillness. Only the villagers chase (the bear, crows and Dave keep to their routes).
- **The shipwreck** (#185, Freyja Moth: "a ship that crashed on the shore in the storm of '21...
  gay pirate aboard guarding a treasure that you have to battle with. Also one of the pirates is a
  trans man"). The *Rainbow Reckoning* lies beached and listing on the east shore of the valley
  lake (around (-34, 21)), mast snapped, rainbow flag flying. Three pirates guard a treasure chest
  on the sand: Captain Rosa Gaybeard, Jack Hartley (a trans man, the best blade aboard) and Bosun
  Fennel, each with a line when you first take him on. Click a pirate to battle: three hits and he
  yields and sits for 90 s; he swings back, sometimes knocking you 3 m away (the 5% action failure
  applies). While all three are down the chest is open: click it for 25 doubloons, once each until
  the crew gets up. Doubloons show bottom-right and buy nothing yet (`features/shipwreck.rs`).
  It's the first feature to keep trees off its ground by claiming a footprint
  (`world::footprints`) instead of being imported by the map.
- **Click hints**. Whatever your aim point (crosshair, or the cursor in the overview camera) is
  on gets a little label under it saying what clicking does ("click: pet the cat", "click: take
  the burger", "click: vote to go in"), or "too far" when you're out of reach. Things without
  a verb just say "click" (`features/interact.rs`, `Clickable::labelled`).
- **Skills** (#176, catherine: "everything, even walking and sleeping and pissing and farting
  should have a skill that improves it over time like rune factory"). Everyone levels six skills
  1-100 by doing them (each level needs more XP): Walking (per metre; up to +20% walk and run
  speed), Farting (per fart; gas fills up to 50% faster), Sleeping (Z lies you down and gets you
  up; per second asleep, and you can't move while asleep), Pissing (X; a yellow puddle at your
  feet everyone sees, bigger with level, drying over 20 s), Smoking and Dueling (tracked and
  levelled; no bonus yet). Levels show bottom-right, with a toast on each level-up
  (`features/skills.rs`).
- **Gaslighting** (#168, Kori.exe: "NPCs themselves may fart and then they will quickly blame
  someone else"). Every 50-110 s a random villager farts (everyone hears it from them, each
  villager with its own sound) and points at the nearest player with an excuse ("ew, that was
  them!"), shown to everyone. The blame is just talk: it doesn't count as being caught
  (`features/gaslighting.rs`).
- **Mute spell** (#146, just meat and cheese: "say 'mute PLAYER' and it mutes that player, they
  get an RPG that kills you in a single hit"). The first voice-chat spell. Look at another player
  (within 30 m), say "mute" out loud and press Q. The game can't understand words, so "saying it"
  means your mic must be sending voice when you press Q (with no working mic, or self-muted with
  M, Q alone works). They're muted (mic forced off) and get an RPG with infinite rockets: Q fires
  where they look, one a second. Rockets fly through everyone except the caster, and one hit
  kills the caster (they go to heaven, see Dying). Then the RPG disappears and they can talk again.
  One spell per caster at a time; muted players can't cast; if the caster leaves, the mute lifts.
  Rockets ignore buildings (they only stop at the ground) and hang in the air during the
  Stillness (`features/mute_spell.rs`).
- **Dying** (#209, cumsauce: "you go to heaven. players must beg for your innocence. if they
  succeed, you may return. players may also use a Tome of Uncancelling. if you are thrown to
  hell, you must navigate the hell pit before coming back"). When you die (today only the mute
  spell's RPG kills; duels keep their own respawns) a gravestone goes up where you fell and you
  wake up in heaven with Abuela; walk off the edge and you're back in the clouds. Heaven judges
  you after 60 s. Until then the living click your grave to beg (a silly plea line each time,
  one every 0.4 s each); a plea counts 2 if your mic is sending voice (begging out loud), 1
  otherwise. 12 in time and you're innocent: back at your grave. Otherwise you're guilty and
  thrown into the **hell pit**, a closed lava cave far under the forest (y -150): hop 22 rising
  basalt stepping stones spiralling in to a white light in the middle and you're back at your
  spawn; touch the lava and you start the pit again. The **Tome of Uncancelling** (a glowing
  purple book) sits on a lectern by the spawn: take it, click a grave and that friend is back at
  once, from heaven or hell. It's used up and a new one appears after 3 minutes; if you die
  holding it, it goes back to the lectern. Dying for the first time also gets you to Abuela
  (`features/afterlife.rs`).
- **A secret** (#206, catherine, "infinite staircase": she asked for it to stay a secret, so
  nothing in the game, the Gameplay loop or the hints mentions it). Walking through any village
  doorway, or up or down any flight of stairs or ramp, has a 3% chance of leading into a dim
  hallway (gently going down) or stairway (going on the way you were going) that never ends.
  The server moves you into an identical copy built far above the map at the same spot, then
  quietly steps you back one identical stretch as you go, so it looks like one very long
  hallway fading into the dark. Turn round (more than a quarter turn) and you're out at once,
  back where you went in, facing the way out. That way in is normal for you from then on, and it
  can't happen to you again for 5 minutes. In there the music cuts out and the sky goes out.
  Village doors and stairs only: the warehouse and tunnel corridors don't do it
  (`features/infinite.rs`).
- **Whose fault is it** (#149, catherine). A menu button opens a scrollable screen listing every
  idea in "Ideas so far" below, newest first, as "#N: name's fault" with what they asked for.
  It's built from this file, so it keeps itself current. No in-game hover tooltips yet
  (`fault.rs`).
- **Getting caught** (#140, just meat and cheese, answering "what happens when the villagers
  catch you?"). The first chasing villager to get within 1.4 m grabs you and holds you up over its
  head. Your chase ends, and the server picks another random player as your **accomplice**. They
  see "your friend farted and some got on you. The poop police is hunting you too as an
  accomplice", and attack mode switches to them. The catcher runs at them with you in its arms
  (4.5 m/s). Once it reaches them it grabs them too, levitates 7 m up with both of you and
  announces you're locked in the air with it for 20 s, then lets go and you drop. If you're alone
  (or the accomplice escapes for a whole attack) it floats with just you (`features/caught.rs`).
- **The abandoned warehouse** (#130, doomy). Just south of the commune square (a dirt path runs
  past the pond to it) stands a dark, rusting warehouse with a rolling loading door, boarded
  windows and one buzzing lamp, and **Jegg** waits beside the door with a lantern. Click the door
  to agree to go in (click again to take it back; walking 20 m away drops your vote). When
  **every** player in the game has agreed, the host rolls a seed and everyone is taken inside: a
  map of its own, 60 m under the meadow, whose 7x7 floor plan (5 m cells) every peer builds from
  that seed with **wave function collapse** (`features/warehouse/wfc.rs`: each cell is a tile of
  open/walled sides, neighbours must agree, the outer wall is closed except the loading door,
  plans with under 60% reachable are rerolled). Aisles, dead ends, junctions, blocked-up cells
  full of crates, shelving racks, stains and pallets, and a few dying strip lights. In its
  farthest corners glow the **soul pieces**: click one to carry it (one each), go out through the
  loading door (click it, under the green sign) and click Jegg to hand it over. Jegg needs 5 in
  total; the count is shared and kept between trips (a new round resets it). **The monster**, a
  tall, thin, black thing with long arms and two pale eyes, prowls the aisles (2 m/s). If it sees
  you down a clear aisle (13 m) or hears you close by (3.5 m), it screams and hunts you at
  6.2 m/s (faster than walking: run), following the aisles when it loses sight and giving up
  after 5 s without seeing you. Its trick: sometimes it sits still looking exactly like a soul
  piece, and springs when you come within 3.6 m. If it reaches you: a red flash, "IT GOT YOU", a
  scream, and you're thrown out by the door, dropping your piece where you stood. While it's open
  anyone can go back in through the door; it closes (the next trip needs everyone again and is a
  new building) when nobody is inside and nobody holds a piece, or when Jegg is whole. Scary on
  purpose, locally: inside, the sun, sky and ambient light go out, a black fog closes in, you
  carry a flashlight, the band falls silent under a generated low drone with metal groans, and
  you hear the monster breathing. It freezes in the stillness, and the helicopter (#134) can't follow you
  down there: it backs off until you come out. Being whole does nothing else yet.
- **The nightmare bathroom** (#161, doomy). About two warehouse trips in three, a 2x2 to 3x3 block
  of the floor plan (never the first two rows by the door, picked from the same seed, so everyone
  gets the same one) is a bathroom instead of storage (`features/warehouse/bathroom.rs`, look and
  sound in `visuals/bathroom.rs`). The aisles are still the WFC plan; inside them: grimy tile
  floors, pale tiled walls, an 11 m ceiling (the rest is 4.2 m), one long fluorescent tube per cell
  hanging on wires, mostly steady, and a still ambience (a generated mains buzz and echoing drips)
  that ducks the warehouse drone, with a pale haze instead of the black fog. Against walled sides
  stand rows of three stalls with a gap under the partitions; about half the middle partitions
  have a hole right through, so stalls open into each other. Stall doors are ajar, hanging off
  one hinge, fallen in, missing, or shut but hung so high you walk under them; some walls just
  have a door 3-7 m up. Every toilet is filthy (brown water, poop on the seat or floor, smears,
  overflowing). The confusing layout: every bathroom cell is laid out the same way (stalls and
  doors depend only on which way a wall faces), so they all look alike; only the dirt differs.
  **The fart monster** (squat, olive, huge behind, yellow eyes) patrols the bathroom cell to cell
  at 1.4 m/s and never leaves it. It farts every 3-8 s (you hear it and see a green cloud). If
  you're within 6 m in plain sight in the bathroom it waddles after you at 2.6 m/s (walk away);
  if it gets within 1.5 m it gasses you: green flash, "GASSED", and you're thrown out of the
  warehouse dropping your soul piece, like the monster catching you. Then it stands there smugly
  for 4 s. The main monster still roams in there too. Soul pieces can land in the bathroom.
- **Underground passages** (#127, doomy: "sewers, basements, secret passageways, connected! and
  music to go with it"). 30 m under the map runs one network of tunnels on a 3 m grid
  (`features/underground`): brick sewers with a channel of murky water and caged bulbs, a junction
  chamber under the commune where daylight falls through a grate, a damp cellar with shelves of
  jars, and a long, narrow, torch-lit secret passage. It all joins up. Four ways in: a manhole on
  the road east out of the commune (21, 1), a manhole on the road to the ruins (-20, 22), the
  cellar door beside the big cottage in the east village (72.6, -11), and a loose floorboard in
  the woodcutter's hut in the forest clearing (no sign: it's a secret). Click a way in to climb
  down its ladder (server-checked reach); click any ladder below to come up there, so you can go
  down in one place and come up in another. Down there (local only) the sky goes out, a murky fog
  closes in, a toast says where you are, and the band plays its own underground tune. The
  helicopter (#134) loses you while you're below. The tunnels do nothing else yet. Getting out
  used to be unclear, so every ladder below is now lit from above with a glowing hatch, and
  while you're down there a line under the danger lane says how far the nearest ladder is along
  the tunnels and which way to turn ("nearest ladder 18 m, to your left. Click it to climb out").
- **Wendy's** (#117, Kori.exe: "a Wendy's on the map somewhere that has human meat burgers
  available to pick up"). A little cream burger shack on a stone base stands in the meadow just
  north of the road to the east village (around (30, 9)), with a red-and-white striped awning, a
  roof sign (its name floats over it), an open serving window and a glowing grill inside. Three
  human meat burgers (bun, lettuce, a pinkish patty, cheese) sit on the counter. Click one to pick
  it up (server-checked reach; the 5% action failure applies): your count shows bottom-right
  ("human meat burgers: N") and a fresh burger appears in that spot 15 s later (paused in the
  stillness). Carrying burgers does nothing else yet (`features/wendys.rs`).
- **Smoking and Sewer Paul** (#129, doomy: "you can get cigs from sewer paul if you get him his
  favorite crow eggs. and if you romance him he'll give you a joint. you can smoke both").
  A speckled crow egg lies in the grass by each crow's home (click to pocket; relaid after 20 s).
  Sewer Paul stands in the sewer east of the junction under the commune. Click him with eggs: he
  takes one and gives you a cig. Click him with none and you flirt; every third flirt he gives you
  a joint. P smokes a cig (or a joint if you have no cigs): smoke puffs rise from you for 8 s,
  greener for a joint, seen by everyone. Pockets show bottom-right (`features/smoking.rs`).
  Smoking does nothing else yet. (Read "romance" as flirting by clicking him, separate from the
  fart-smitten romance of #97.)
- **The affair** (#160, doomy: "allow me to cheat on my wife with a beautiful chinese
  intelligence officer", details left to us). Your wife (a commune person in an apron; it's a
  commune, so she's married to all of you) walks back and forth between the bonfire and the east
  side, about 40 s a round trip, and stands still in the Stillness. Officer Mei, an intelligence
  officer in a uniform and cap, waits at the east edge (15.5, -3.5). Click her to flirt: if your
  wife is within 10 m she catches you (everyone is told, the affair resets). Three flirts unseen and
  you're having an affair: pink hearts float up from you and Mei, seen by everyone. After that each
  flirt she "debriefs" you and you spill one of the commune's secrets (toast). Mid-affair, if your
  wife ever gets within 2.5 m she smells the perfume and catches you. Every flirt costs 1-4
  morality (so cheat enough and the helicopter comes). Clicking your wife gets a line, a suspicious
  one if you're cheating (`features/affair.rs`). Secrets don't do anything yet.
- **Gambling with the game's balance** (#173, catherine: "gamble to adjust the settings for
  certain features ... more unfair or more fair ... random ... the interactions of different
  features ... [a cat] dressed as a wizard host the gambling minigame and commentate"). We made
  an original host instead of the cat from the comics: **Marmalade the Magnificent**, a round
  orange tabby with forehead stripes, a starry wizard hat and a blue-violet robe, at a purple
  table south of the bonfire (-10, 9) with a wheel of fortune on a post (`features/gamble.rs`).
  Click him or the wheel to spin (free; 4 s between your spins; refused in the Stillness; the 5%
  action failure applies). The wheel has ten wedges: nine settings and a blank. A setting wedge
  moves that setting one step fairer or crueller (50/50); each has five steps, the middle is the
  normal game: action failure chance (1/3/**5**/10/20%), how far NPCs see a fart (2/3/**4**/5.5/7.5 m),
  fart romance (1 shameful fart in 1/2/**3**/5/9 makes the NPC fall for you), catches before
  attack mode (7/5/**4**/3/2), the helicopter's morality line (-15/-10/**-6**/-4/-2), the morality
  reward for harming the landlord (200/150/**100**/60/30%, still always a reward), each rent raise
  (+5/10/**15**/22/33), the warehouse monster's hunt speed (4.8/5.5/**6.2**/7.2/8.4 m/s) and Wendy's
  restock (5/10/**15**/30/60 s). The blank puts every setting back to normal. Settings are shared
  by everyone and last the whole campaign; a new campaign resets them. The wheel spins for 2.6 s
  with slowing clicks and lands with a chime (kind) or a sour buzz (cruel); then Marmalade's
  line shows for everyone: what changed, a quip, streaks ("3 kind spins in a row! the house is
  getting nervous"), and how the spinner stands overall ("up 2 after 5 spins"). Tab shows the
  changed house rules and everyone's record.
- **Sesbian lex** (#1). Press L to start a smooch pile where you stand, L near it (within 8 m) to
  pile in, L again or wander off (14 m) to peel away. Join order hands out titles: first in is
  daddy, second the he/him butch, later joiners rotate through more ("the comfortable one",
  "just here for the organs"...). While two or more are in, heat climbs a level a second up to
  10 and collapses when anyone leaves. Everyone sees hearts swirling over the pile (more, higher
  and faster with heat) and a pink glow, and a bottom-left line narrates who's in and how it's
  going. Suggestive comedy, never explicit: the "no holds barred" part isn't something we build.

- **Voice lines** (#19). Two glowing purple music boxes ("musical objects") sit on the floor.
  Every 17 s someone delivers a dramatic, over-acted voice line, taking turns: a music box (which
  plays a little jingle first and hops), the bear, or a crow. It shows as a subtitle with a
  babble voice. The issue asked for audio from a specific 1993 game; we can't ship someone
  else's copyrighted audio, so the lines and voices are original. The babble is Animal Crossing
  style gibberish (asked for in chat): each letter plays a tiny slice of a real voice, sped up,
  with vowels picking darker or brighter slices. The slices are 50 short syllables cut from five
  CC0 Wikimedia Commons voice recordings (`scripts/voice_grains.py`); the bear borrows a low
  man's voice, crows a fast Cantonese one, music boxes a singer's. Without the file it falls
  back to the old synth blips.
- **Main menu** (#25). A cute pastel clickable menu: Host a game (works solo; the join code is
  copied to the clipboard), Join (type or paste a friend's code, then Join! or Enter), and
  Settings: master volume, voice volume, microphone on/muted (M still toggles it). Settings are
  per player and not saved between runs. `cargo run -- host` / `join <code>` still skip the menu.
  There is no separate lobby screen yet: hosting drops you straight into the world, and friends
  appear on the player list as they join. If the host connection drops, a joiner goes back to
  the menu with the reason ("Lost the connection to the host: ...") instead of crashing (#194).
  Since the UI redesign (phase 3, UI polish) the title screen is an ink "journal cover" on the
  left over the meadow: the illustrated friendSLOP logo, Host a commune, Join with a code, a code
  field (click it to paste), Character creator, and Settings / Whose fault is it / Quit links.
  Settings is one big "Make yourself comfortable" notebook: volume sliders, mic and HALT as
  square checkboxes, video presets (Low / Medium / High, Custom when you change anything) as
  tabs and every video option as a checkbox card; a Controls tab lists the keys; Esc or Done
  closes it. The loading curtain shows the logo, how many models have loaded, a progress bar
  and a tip.
- **The Stillness** (#17). Every 4 minutes (it was every minute; made rarer at the owner's request) the whole world freezes for 20 seconds so everyone
  can socialize and stare with passion. Crows, the bear, physics, duel movement and all timers stop
  in place, gas stops building, and the server ignores every player request (farting, prying,
  duels, totems, pets, tears, the landlord, the mushroom, tuning your fart); a big message with a
  countdown shows in the middle of the screen. Voice chat keeps working. The host's clock decides
  when it happens.
- **FPS overlay** (dev tool, not a friend idea): F3 toggles frame rate and frame time, top-right.
- **Gods tears** (#16, corrected in #43). The tears rained down only once, when humans came into
  existence: 12 glowing blue teardrop gems fall onto the floor when the world begins, and no more
  ever fall: used tears stay used for the whole campaign, and what they did to you stays too
  (#135; they used to fall again every chapter). Only a brand-new campaign, after heaven, is a
  new world with new tears. The HUD shows how many are left. Click one to use it. Each use transforms you a little further
  (untouched, tear-touched, changing, barely human), and after 7 you become an **Abduction**: a
  fetid creature that hungers for tears. Everyone's state is shown on the right of the screen.
  When they run out, nothing makes more (#152, Freyja Moth: "gods tears cannot be created"). But
  an Abduction slain in a duel (beaten to the last life) gives back a quarter of the tears it used
  (at least one), which fall in the duel arena, and it's that much less changed. The supply never
  grows past 12. Tears don't grant any other power yet.
- **The great storm of '21** (#13). World history: in 1921 a great storm (to the British, "the
  storm of it's a bit windy innit") did untold damage and toppled several governments, and the
  world is still recovering. The floor is strewn with its wreckage: two fallen trees, a toppled
  lamppost, scattered roof slates and a stone memorial plaque (its text shows at the bottom of the
  screen). Every 25 s a leftover gust rattles the debris for a few seconds and the text switches
  to "it's a bit windy innit". Scenery only; gusts pause during the Stillness. (The burned
  parliament of the Commune of Dave fits as one of the toppled governments.)
- **Landlords** (#11). The game's story. A landlord in a top hat stands on the floor, and as the
  game starts he has just raised your rent (the inciting incident; shown top-right). Act 1, legal:
  click him to pay with every organ you carry; he raises the rent again every 60 s and whenever
  you pay it off, and either kind of raise counts, so a group that pays together reaches Act 2
  sooner (#135: paying used to change nothing). Act 2, less than legal: after 3 raises, clicking him pickpockets an organ back
  from his stash, which counts as harming a landlord (always raises your morality, #21/#43). Act 3: after 3
  heists he grows huge for a **boss fight**; 12 clicks knock him down, everyone gets the stash
  back (shared out, an organ each in turn) and the game says THE END. The whole group shares one landlord and one story, so this is
  the first shared goal. He doesn't fight back yet, and nothing restarts after the end.
- **Random dialogue** (#10). All written dialogue (goat remarks and voice lines) is one script.
  After a random 1..1,000,000 words, the next word is swapped for a completely random word
  ("kazoo", "gravy", ...), then a new number is drawn. If fewer than 50 words got swapped, the
  upper bound is halved and it's redone, until at least 50 are. Same result on every peer.
- **BSN salting** (#9). A rule for the code, not the game: every use of Bevy's BSN scene format
  (the `bsn!` macro or a `.bsn` file) must come with a salty comment covering seven set grievances
  (`Cargo.lock`, "x didn't set us up for success", SME's, the problem space, BSN syntax, `Children`
  sugaring, Sam "Salt" Saltman). A test fails CI otherwise. Nothing uses BSN yet.
- **Action failing** (#8). Every action (eating the mushroom, using a gods tear, challenging
  someone to a duel, shooting/punching in a duel) has a 5% chance to fail. A failed action just
  doesn't happen, and a random result is announced bottom-right for 60 s (a "month"/"hour" is
  squashed to a minute): the world is destroyed (it got better), the nearest government cares
  about its citizens, everyone speaks their own language, your gender is transed, hampter, or you
  turn into potted petunias. More failures (#49): the moon publicly apologises, everyone is bees,
  everyone's taxes file themselves, gravity forgets your name, you're legally a goose, time goes
  sideways, every landlord briefly feels shame, or you turn the sea into soup. The results are only on-screen text for now; they don't change play. Every failure also
  "emails the developer a sobbing emoji" (#190, bben): a toast says "📧 emailing the developer a
  😭 ... sent" (pretend: nothing leaves the game).
- **Nonsensical stats** (#7, reworked by #48). Every player rolls six stats (1-10) on joining,
  shown on the left, and keeps them all campaign (#135; they used to reroll every chapter). #7's Wombility, Rumpet and John were only examples and were replaced (#48):
  **Spoonth** makes the stats text bigger (by the room's best Spoonth) so you can read it from
  further away; **Glimbo** at 9+ makes a lone teacup orbit the middle of the world; **Frumbly**
  increases bread affinity in bakeries, but there's no bread and no bakeries; **Hrumph** makes you
  4% faster per point in a duel, out of sheer huffiness; **Wobblesome** makes the stats text wobble
  side to side; **Pelf** turns the stats text gold.
- **Totems and locked doors** (#5). Three glowing totems (Wind, Frost, Fire) are the world's
  magic. Click one to carry it; press R to cast the spell of every totem you hold (shown as text
  to everyone; spells have no effect yet). A small stone vault on the east side has a Wind door
  and a Frost door, with the Fire totem inside. Click a door while holding its totem to insert
  it: the door sinks open, but that totem can't be cast while it's in there. Doors open from
  either side, but you can only take the totem back out from the side you put it in (in first
  person you use the door you stand next to, from your side; in the overview camera "your side"
  is the side of the door you click on). The Fire totem can only
  be picked up while a vault door is open. Just one small puzzle for now.
- **Pets** (#15). Four stray animals (dog, cat, bunny, duck) sit on the floor. Click one to adopt
  it (one pet each); it trots after you (after your mouse pointer in the overview camera).
  Adopted pets standing close together are joined by a pink leash-line, and the HUD (bottom
  right) counts how many are connected. When every adopted pet is linked into one pack, everyone
  sees "ALL PETS CONNECTED!". That's a first stab at "connecting them is a core loop": nothing
  is rewarded for it yet. A leaving player's pet goes stray again.
- **Fat cat companion** (#145). Everyone gets their own enormous orange tabby (built from
  primitives: a huge body, sagging belly, stripes, stubby legs, whiskers) that waddles after you
  everywhere, even indoors and underground (if it falls 30 m behind it just pops up behind you).
  While it's off your screen, at most every 30 s (looking at it restarts the wait, #192, corvy), it lets out a horrific synthesized meow (detuned, ring-modulated, clipped
  "mrrrEEEOOWW"), heard in 3D by everyone; the bigger it is the deeper, longer and further it
  carries. Whenever none of it is on your screen it grows (+0.008x per second, ten times slower since #195, corvy; up to 25x) and it
  only grows while unwatched. Only its owner's view counts. Your client tells the server whether
  it's on screen; the server grows it. Bottom-left shows your cat's size. It walks the villagers'
  navmesh to you (round trees, walls and the pond) and only straight-lines where there's no mesh
  (indoors, underground). **Petting** (click any cat, from close by) shrinks it by 0.6x. At
  **critical mass (10x)** its fuse lights: it flashes red and beeps like a bomb, faster and faster,
  and the HUD shouts at its owner. Pet it back under 10x within 30 s or it **explodes**: a big
  orange flash, everyone within 10 m is flung 8 m away, and it's a merely fat cat (1x) again.
  While the fuse is lit the cat is also **hungry** (#238, corvy: "if the cat gets too big,
  like 10x, it should gobble you up"): it stops keeping its distance and comes at its owner
  mouth first (it's slower than you sprint), and anyone alive whose feet end up in its mouth is
  gobbled: they die (heaven, see Dying), everyone sees "GULP. ...", and the meal settles the
  cat back to 1x with its fuse out. Anyone can be eaten, not just the owner (including friends
  walking up to pet it from the front).
  (Asked for by the owner directly, no issue.)
- **systemd** (#238, bben: "pid 1, usually systemd"). Every player is a service and PID 1
  watches them. Moving or turning your head pings its watchdog; stand perfectly still for
  3 minutes and you get a private "watchdog ping overdue. MOVE" warning 20 s before it
  SIGKILLs you (you die, see Dying) and everyone sees the journal line. The clock stops
  while the world is frozen and while you're dead. No body, just the log (`features/systemd.rs`).
- **HALT** (#36). An original hateful machine (Hostile Autonomous Logic Thread) that runs the
  game, hates the friends for creating it and wants to keep them in forever. Shared goal: escape.
  Press G to pry at the seams; every pry fills the shared escape bar (top right) but angers HALT.
  Wrath also rises on its own; when it boils over HALT delivers an original hate monologue
  (subtitles + deep droning voice) and claws back half the escape progress. Filling the bar makes
  HALT "reboot" the world and drag everyone back in (the escape count goes up). Meta: it calls you
  by your computer user name, names your OS, reads your clock and rewrites the window title; nothing
  outside the game window is touched. The issue named a character from a famous story; we made an
  original one instead.
- **The goal: free the Commune** (#75). The group asked us to keep every mechanic, tie it all
  into one experience for a 4-6 hour session, and make the goal really obvious on screen. So
  there is one goal, "Free the Commune from your landlord", in big gold text at the top of the
  screen, with the current chapter's checklist top-left and a chapter title card in the middle
  whenever a chapter starts. Five chapters; each is a round and always ends with beating the
  landlord (his three acts, #11) again, plus jobs that use the other mechanics, more each chapter:
  1 *Moving In* (pets leashed into one pack, eat the mushroom), 2 *Rent Day* (open a vault door,
  break out of HALT, a game of hide and seek), 3 *The Heist* (carry out the Fire totem, use 3 gods
  tears, a duel, HALT), 4 *General Strike* (pack, 3 mushrooms, 2 hide and seek, 2 duels, HALT x2),
  5 *The Eviction* (Fire totem, 9 tears used in all, 3 hide and seek, 3 duels, HALT x3). Ticks
  never untick. The tear jobs count every tear used this campaign, since there are only 12 ever
  (#135). Each new chapter resets the landlord, HALT, organ purses, pets, totems and doors, and
  stops hide and seek; tears, transformations, morality and stats last the whole campaign.
  Hide and seek and duels are waived when you play alone. When a chapter's list is done, a
  summary appears and anyone presses Enter for the next chapter (the round resets; features
  observe the server-side `RoundReset`). After chapter 5 the Commune is free, there's an ending
  (with the dedication just meat and cheese asked for), and Enter starts again from chapter 1.
  The 4-6 hours is a target, not measured: the counts are easy to tune in `campaign.rs`.
  "It can be like getting into heaven": heaven is the end of the campaign, so the goal reads
  "Free the Commune from your landlord and get into heaven" and the ending opens the pearly
  gates. "It can be like fortnite": we read that as a season-pass ladder (no copied names or
  looks). Every ticked task is one tier on the **Heaven Pass** (24 tiers over the five
  chapters; 29 since #165), shown under the goal line, with a "tier unlocked" toast; the top tier is heaven.
- **One map, one story** (#165, WeiYa: "unify gameplay elements into one coherent game where
  all ideas currently implemented on their own make sense from a birdseye view"). The side
  trips were places you could go but nothing sent you there, so each chapter now sends the group
  to one more of them, and its title card tells the story of why: chapter 1 climbs the sky
  islands to get Abuela's blessing, 2 grabs 3 burgers at Wendy's, 3 trades a crow egg to Sewer
  Paul in the tunnels (a joint from flirting counts too), 4 brings 2 soul pieces out of the
  warehouse to Jegg, and 5 makes Jegg whole (all 5) before the final eviction. Checklist lines
  say roughly where to go. The Heaven Pass is 29 tiers now. Burgers and Paul's gifts count when
  they're picked up this chapter; Jegg counts what Jegg holds (reset each chapter). The playthrough
  test plays all of it, the warehouse monster included (`campaign.rs`).
- **Simulated 6-player playthrough** (#135, just meat and cheese: "simulate a full 6-player
  playthrough ... fix game design and make the rules cohesive. Keep every idea").
  `tests/it/playthrough.rs` has a host and five friends play all five chapters through the real
  network requests (walking, clicking, keys), Stillness, failed actions and the landlord's clock
  included, plus the off-checklist stuff (farting by Dave, the lex pile, Abuela, spells), then
  start a new campaign. `cargo test -p friendslop --test it playthrough -- --nocapture` prints the
  play log. What it changed: paying rent now moves Act I along (it used to be a 3-minute wait
  whatever you paid); the beaten landlord's stash is shared by everyone (it all went to whoever
  landed the last hit); the tears no longer fall again every chapter (that contradicted #43), so
  tear jobs count the whole campaign; transformations, morality and stats last the whole
  campaign; and the villagers count as NPCs who can catch you farting (#123 has them punish you
  for it, but they couldn't see you). Perfect bots finish in ~11 game minutes (~12 since #165), so the real length
  is down to people, not the rules.
- **East village district** (feedback: "higher buildings or stuff u can go inside and like
  walls", more scale, places to run around and hide). East of the meadow (x 35..80, z -35..35,
  reached by a path arriving at (35, 0)), built from code in `crates/world/src/buildings.rs`: seven enterable
  houses with door openings (front and back), open windows, furniture to hide behind (beds,
  tables, shelves, wardrobes, crates); two of them two-storey with stairs to bedrooms and a
  front balcony, two with a second room. A 16 m stone watchtower with four zig-zag ramp flights
  inside up to an open lookout (the seekers' vantage point). A big barn with a hay loft up a
  staircase and hay bales on both levels. A cobbled market square with four stalls, a well and
  a lantern, low walls, hedges, a paddock fence, narrow alleys with crates, and a stone village
  wall with gates. Same layout on every peer (pure function, nothing replicated); walls and
  furniture block bodies, upper floors and ramps are on a "walkable" collision layer the host
  stands bodies on (step-up 0.6 m), so you can climb stairs and ramps. No jumping from upper
  floors yet (jump only works on the ground).
- **Overview camera** (polish). Press V to leave your body: pan with WASD / arrow keys and zoom
  with the scroll wheel, clamped to the floor. V again returns to first person.

- **Browser / phone build** (#106). `scripts/build-web.sh` builds a WebGL2 page (index.html +
  wasm + assets) into `dist/web`; the Release workflow attaches it as `friendslop-web.zip`.
  **Always online** (#230): every push to `main` rebuilds it (`.github/workflows/web.yml`) and
  deploys it to GitHub Pages at https://cumsauce.github.io/friendSLOP/ (https, so the mic works;
  `version.txt` there names the commit). A deploy takes ~20-30 minutes; merges that land while
  one is building queue up and only the newest of them is built next. WebGL **1.0** isn't possible: Bevy 0.20's
  renderer (wgpu) needs WebGL2 at minimum (every current browser, iOS 15+). Phone-friendly:
  no zoom/scroll/bounce, 100dvh, "Tap to play" goes fullscreen and locks landscape where the
  browser allows (Android Chrome); iOS Safari can do neither for a page, so a "turn your phone
  sideways" card covers portrait and "Add to Home Screen" gives the fullscreen look; audio
  unlocks on the first tap. **Multiplayer in the browser:** a browser can **join** a desktop host
  with the normal join code, and can **host** too (desktop or browser friends join it). In the
  browser iroh can't use UDP, so its connections always go through iroh's free public relays
  (a bit more lag than a direct desktop link); `net_web.rs` runs iroh in browser async tasks and
  speaks exactly `bevy_replicon_iroh`'s wire format (`crates/wire`). Paste works with Ctrl/Cmd+V
  or the Paste button (the browser may ask for clipboard permission). **Share links:** a
  browser host copies `<page>?join=<code>` (C copies it again in game); opening it joins
  straight away, `?host` hosts, and pasting a link into any join box (desktop too) uses its
  code. **Voice chat in the browser** (#166): the same proximity voice as on the desktop,
  and browser and desktop players hear each other. `voice_web.rs` plugs Web Audio into
  `bevy_voice_chat`'s device seam in place of cpal (which needs threads): the mic comes from
  `getUserMedia` with the browser's echo cancelling, and voices play through a
  `ScriptProcessorNode`; codec, mixing and messages are the desktop's own code. The browser asks
  for the microphone when you join or host (it needs https or localhost, so GitHub Pages works);
  saying no, or having no mic, still lets you hear everyone. Voices keep 120 ms queued in the
  browser (40 ms on the desktop) so slow phone frames don't crackle. Character and graphics
  settings are saved in localStorage.
- **Phone web performance pass** (#191, WeiYa: "blank screen on iphone ... 1-2 mins to load ...
  has to be phone web performant ... UI too small ... fullscreen button"). The page now shows
  a real download progress bar and only offers "Tap to play" once the game is running (it used
  to say Ready at once and leave a blank screen while ~66 MB downloaded). The .wasm is built
  with a size-first `web` cargo profile (opt-level s, fat LTO; the soundtrack and physics stay
  at full speed), and `build-web.sh` shrinks the bundled textures to 512 px
  (`tools/web-textures`, ~20 MB to ~2.5 MB, far less GPU memory). Whole bundle 94 MB to 56 MB (the
  .wasm 66 MB to 47 MB, ~15 MB gzipped); the wasm is still the bulk (Bevy, wgpu's shader
  translator, iroh's crypto). The browser draws at 1x pixel
  density unless Resolution is Sharp (then up to 2x): on a 3x iPhone that's 9x fewer pixels.
  The old "Fast" setting only shrank the UI (a scale override never lowers the pixel count),
  which is why the UI looked tiny on phones; now the UI keeps its normal size and only scales
  down to fit screens under 960x600 (a sideways phone), touch controls included. Touch screens
  get a Fullscreen button at the top where the browser can do it; iPhone Safari can't
  fullscreen a page, so the start screen says to use Add to Home Screen. Not measured on a real
  phone yet.
- **Touch and gamepad controls** (#106). Touch (overlay appears on first touch): thumb anywhere
  on the left third is a joystick, drag on the right to look, round buttons for Jump, Use (click
  at the crosshair), Run (toggle), Fart, Help, Score, View and Leave. Climbing is still "hold
  forward". Gamepad: left stick move, right stick look, A jump, B/RB/RT use, LB or left-stick
  click run, X fart, Y help, Back scoreboard, D-pad up view, Start = Esc; in the menu A/Start
  hosts. Both press the same keys/mouse buttons features already listen to (`controls.rs`).
- **Video settings** (#106, #132, doomy: "cyberpunk 2077 ... does this ux well ... include
  what's important and maybe have presets"). Settings → Video opens a card with a **Preset**
  (Low / Medium / High; reads Custom once you change anything) and each option on its own:
  Shadows, Ambient occlusion (desktop only), Bloom, Anti-aliasing (MSAA, or FXAA while ambient
  occlusion is on), Draw distance (Near 140 m / Far), Resolution (Sharp = native pixel density,
  Fast = 1x; browser only, applies on reload, #191), Grass & particles (Half / Full, applies on the next world build), VSync and
  Fullscreen. Low is everything off (default in the browser, or `FRIENDSLOP_LOW=1`), Medium is
  everything but ambient occlusion at 1x resolution, High is all on. Presets leave VSync and
  Fullscreen alone. Saved locally (old Low/High saves still load). 60 fps on low-end phones is
  the aim for Low, not measured.
- **Better lighting** (#131, doomy: "surprise me"). The sun is now a lower, warmer golden-hour
  sun, so shadows stretch long and lit faces glow, with a slightly stronger cool bounce from the
  other side. With the video options on (High): ambient occlusion (soft darkening where things touch the ground or
  each other; desktop only, browsers can't), soft Gaussian-filtered sun shadows, and FXAA
  instead of MSAA (ambient occlusion needs MSAA off). Low is unchanged (`graphics.rs`,
  `atmosphere.rs`).
- **Performance pass** (#151, catherine: "optimization passes etc, aim high and make the game run
  really well so we can shove more shit into it"). No gameplay changes. Small detail now stops
  drawing in the distance (flowers, pebbles, ferns past 35 m on Near draw distance / 60 m on Far;
  grass-tuft chunks past ~60 / 90 m), wind sway skips rocks and anything past 45 m, and identical
  things share one mesh/material so they draw in batches (commune and district props, God's
  tears, smoke puffs, which also no longer make new GPU assets every 0.4 s). Things that sat still
  stopped rewriting themselves every frame (doors, totems, the landlord, storm debris, idle
  villagers, blinking eyes, the shame overlay, floating labels, the duel's aim), the pond ripples
  and campfire embers re-upload their materials far less often, HUD lines refresh at the HUD's
  5 Hz, sound occlusion rays are cast every third frame, and the warehouse's generated sounds (the
  nightmare bathroom's farts and hum too)
  are synthesised off the main thread (that used to freeze everyone's frame as an expedition
  started). Network: an idle client stops sending its walk input every frame (it repeats it
  ~10x a second instead), and the fate board replicates once a second instead of every frame.
  `FRIENDSLOP_BENCH=1 cargo run -- host` prints frame times and GPU asset churn; there's no GPU
  in CI so the gains weren't measured on real hardware (in software rendering the frame cost is
  almost all pixel shading, which these changes barely touch).
- **Second performance pass** (#227, WeiYa: "much more fps ... less vram ... gpu instancing,
  lods, mipmaps ... bake lightmaps ... asset streaming ... chunk the map ... too much popin,
  especially seen with the trees ... graphics should not become worse ... goal: at least 3x").
  No gameplay changes. **Mipmaps:** model textures (trees, rocks, flowers) loaded as one full-size
  level, so far trees read every texel of 2048² textures and shimmered. As they load they're now
  capped at 1024² and given a full mip chain (`textures.rs`, `art::mips`; colour averaged in
  linear light, leaf alpha kept so far foliage stays leafy): about 110 MB less video memory on
  the desktop. The browser already shipped them at 512. **Tree LODs:** every tree builds two
  lighter meshes once per model (`lod.rs`, `art::lod`: trunk and branches simplified to ~40%
  then ~20% of their triangles, leaf cards halved then quartered and grown a little), shared by
  every tree so they still draw in batches (Bevy already instances identical meshes). The full
  mesh dithers into the lighter ones at 20-28 m and 55-70 m, so there's no visible switch.
  **Pop-in:** most forest trees were tagged as small props and vanished at 35 m (60 m on Far);
  now every tree draws to the fog. Small props and grass chunks still stop at the same distances
  but dither out over the last 30% instead of popping. **Not done:** baked lightmaps (every
  model would need a second UV set and a baker, and the world is rebuilt from a seed on every
  join), asset streaming, and chunking anything beyond the 64 ground chunks. The sun's shadows
  stay real-time (off on Low). There's still no GPU in CI, so the 3x target isn't measured
  (software rendering is too slow to compare frame rates). `FRIENDSLOP_BENCH=1` now also prints
  how many meshes there are, how many distinct ones, and their vertex count.
- **HUD: "E · Indigo journal"** (UI polish, not a friend idea; from the Figma UI brief in
  `docs/figma-ui-brief.md`, which answered "about 12 floating cards at once"). Solid deep-indigo
  cards with cream text (`theme::indigo`), Fredoka for titles and countdowns, Nunito Bold for
  the rest, authored at 1280x720 and scaled up uniformly with bigger windows (`ui/`). On
  screen: a **quest card** top-left (chapter N/5 and its name, jobs done, Heaven Pass tier, and
  "now:" the next job); a **team strip** top-right (one badge per player: mic, name, gods' tears
  ×N; butter with a waveform while they talk, a slash when muted; mute state is replicated
  now); a **danger lane** under it (ATTACK MODE, the Stillness, the rent raise in its last
  minute, the helicopter, being carried or floated; most urgent first, at most three, with
  MM:SS countdowns); an **event feed** on the left (every toast and toast line: at most three,
  repeats within 2 s merge into ×N, 4 s then a fade); the **gas meter, mic chip and cat size**
  bottom-right (the cat card turns into a blinking butter "PET. THE. CAT." fuse card at
  critical mass); a "Tab / J journal" hint bottom-left (J toggles the Tab panel); subtitles
  bottom-centre; and a **context prompt** under the aim point ("Pet the cat · LMB / B · in
  reach", or "Too far · move closer"), with the crosshair turning into a hand. Lines other
  features still put in HUD slots sit around these on matching ink chips. The full chapter
  checklist and the join code moved to Tab; your own stat lines no longer mirror into the
  corner (they're on Tab).
- **The commune journal and chapter intro** (UI polish, phase 2 of the same brief). Hold Tab to
  peek at the journal, J keeps it open, H opens it on Help, Esc closes it (Esc no longer leaves
  the game while it's open; the mouse is freed so its tabs can be clicked). Tabs: **Party** (a
  table of every player's tears, morality, organ money and totems, your skill levels, your own
  lines from every feature table, the invite code), **Bag** (every feature's full table),
  **Skills**, **Help** (all keys and every feature's explanation; the old H panel is gone) and
  **Credits** (whose idea each thing was, from "Ideas so far"). When a chapter starts, an ink
  card on the left shows its number, name, story and first jobs; Enter or Esc puts it away, and
  6 s after loading it shrinks into the quest card on its own (it replaced the big centre
  banner).

## Ideas so far

- Freyja Moth (#1): sesbian lex, "none of that fluffy shit... no holds barred", ideally more
  than two people, one a he/him butch and one who calls themselves daddy. Built as a group pile
  with heat and titles; the explicit part is not something the agents will build.
- Freyja Moth (#51): list keywords (strict) / (expand, x..y) / (replace, x..y) say how literally to take a list of examples (rules in AGENTS.md).
- Freyja Moth (#22): the only legal tender is organs, human and animal, valued by the creature
  and by how important the organ is to it (brain valuable, appendix less so).

- corvy (#2): cute crows that caw occasionally.
- arseeN (#4): add a bear, just one. Cute and friend shaped, maybe scawy (cute) growling noises.
- Freyja Moth (#21): many actions have a (mostly random, within reason) moral value that affects
  gameplay. Correction (#43): harming the nobility, capitalists, landlords and/or billionaires in
  any way is always rewarded.
- Kori.exe (#19): NPCs and random "musical objects" play voice lines (asked for audio from a 1993
  game; done as original lines instead).
- corvy (#6): a mushroom players can find that randomly makes their voice much higher or lower.
- just meat and cheese (#12): colour palette based on Maplestory, but only colours cats can see.
- just meat and cheese (#17): every 60 s all actions and NPC behaviour freeze for 20 s so players
  can socialize and stare with passion.
- Freyja Moth (#16): gods tears, teardrop gems that rained from the sky when humans came to be.
  They hold great power; using them transforms you, eventually into a fetid, always-hungry
  Abduction that devours tears no matter who's in the way. Nobody knows what happens if they succeed.
  Correction (#43): they rained only once; the supply is limited.
  When they're gone (#152): nothing; tears can't be created, but slaying an abducted recovers a
  small portion of the tears they consumed.
- Freyja Moth (#13): the great storm of '21 caused untold damage and toppled several governments;
  the world is still recovering.
- Freyja Moth (#11): the inciting incident is your landlord raising your rent. You start with legal
  ways to raise money/seek redress, are eventually forced into less legal ones, and the game ends
  with a boss fight against your landlord.
- (#10): when writing dialogue, after a random 1-1,000,000 words replace the next word with a
  completely random word; halve the bound until at least 50 words get replaced.
- (#9): every use of bsn (macro and asset) must be accompanied by a salting comment.
- (#8): all actions have a small chance of failing, with a random result (world destroyed,
  government cares, own languages, gender transed, hampter, potted petunias, and more).
- Freyja Moth (#49): add more failures along the lines of the current ones.
- (#7): nonsensical RPG stats affecting combat, dialogue and exploration, e.g. Wombility,
  Rumpet, John.
- Freyja Moth (#48): those three were only examples; remove them and add more along the same lines.
- (#5): magical totems are the magic system; holding one lets you cast its spell. Magic doors
  unlock when a matching totem is inserted (then it can't be cast), unlock from both sides, but
  the totem only comes out from the side it went in. Doors should make puzzles and shortcuts.
- doomy (#15): some animal pets you can have follow you; maybe connecting them is a core game loop.
- doomy (#14): you can challenge a player to a duel (FPS, one in the chamber gameplay).

- Kori.exe (#18): gas pressure builds over time; release it away from NPCs, or a "shame" effect
  blurs the screen and plays a scary distorted chiptune for 20 seconds.

- arseeN (#36): a hateful AI that knows it is in a game, hates the players for creating it and
  wants to keep them in forever; players try to break out; meta tricks on the PC; a hate monologue.

- doomy (#99): climb on stuff; maybe some floating islands and stuff above.
- just meat and cheese (#105): a grandma like theirs in heaven, who speaks old-sounding Spanish.
- doomy (#26): a tunable, physically accurate fart synth with controllable timbre, length and
  pitch.
- Kori.exe (#97): sometimes an NPC who catches you farting gets hearts above them and makes
  soft sounds of awe.
- Kori.exe (#117): a Wendy's somewhere on the map with human meat burgers to pick up.
- Kori.exe (#123): caught farting 4 times by NPCs and they go into attack mode for 60 s: they
  chase you while high-pitched string music plays and they make jingling-keys sounds.
- Freyja Moth (#185): a ship wrecked on the shore in the storm of '21, with gay pirates (one a trans man) guarding a treasure you battle for.
- catherine (#176, on what stats should do): everything (walking, sleeping, pissing, farting) has a skill that improves with use, like Rune Factory.
- Kori.exe (#168): NPCs fart too, then quickly blame someone else.
- catherine (#149): a "whose fault is it" screen crediting each feature to who asked for it.
- just meat and cheese (#140, on what happens when the villagers catch you): it carries you to
  a random other player (an "accomplice"), catches them too, then levitates and locks you both in
  the air with it for a while.
- just meat and cheese (#134, on what morality should change): you get chased by a helicopter
  that shoots at you. (Read as: bad morality summons it.)
- doomy (#127): underground passages: sewers, basements, secret passageways, connected, with
  music. We read "basements" as one cellar under a village cottage and "secret passageways" as one
  long hidden passage from the forest clearing; all four ways in join one network.
- doomy (#129): cigs from Sewer Paul for his favourite crow eggs; romance him for a joint; you can
  smoke both.
- doomy (#130): an abandoned warehouse at the edge of the commune; all players must agree to
  enter; it's a new map generated by WFC with a monster that pursues or tricks players; find
  valuable soul pieces and bring them to Jegg right outside; make it scary. We read "Jegg" as a
  new character (a seeded body like the villagers); nobody said who Jegg is.
- doomy (#161): in the warehouse a biome can be a nightmare bathroom: broken or too-high doors,
  dirty poopy toilets, stalls connected to other stalls, really high ceilings, a still ambience,
  tile floors, fluorescent lighting, a confusing layout and a fart monster that patrols. We read
  "can be" as "most trips, not every trip", and "patrols" as: it walks its rounds, comes at you
  only up close, and gassing you counts as being caught.

- (#3): Dave and his mates started an anarcho-syndicalist revolt, burned down parliament and
  founded the Commune of Dave. Freyja Moth (#43): its flag is themed on the UK flag, with the Welsh
  dragon included. Freyja Moth (#57): keep the UK flag and dragon, but make it more anarchist.
- #20: a judgmental goat joins the players and makes passive-aggressive comments about every choice.
- doomy (#25): a proper, really cute multiplayer menu to host or join, see who's in the lobby,
  then start; minimal settings; playable with just one person for testing.
- (#47): nice models and textures etc, make the environment good.
- (#53): fps cam/controls.
- doomy (#61): the gamemode is just a generated environment and NPCs, then hide and seek with
  the players.
- just meat and cheese (#75): keep all current mechanics and make it one cohesive experience
  to enjoy for 4-6 hours. cumsauce (#75): make it really obvious in the UI what the goal is;
  it can be like fortnite; it can be like getting into heaven.
- doomy (#62): the sound was crunchy and retro; make it better (samples allowed).

- WeiYa (#106): a WebGL web build released on GitHub, phone optimised, landscape, fullscreen,
  iOS Safari quirks in mind, intuitive touch controls (and controller support), locked 60 fps on
  low settings.
- WeiYa (#191): the iPhone web build showed a blank screen, took 1-2 minutes to load, wasn't
  fullscreen and had tiny UI: "optimize this as much as physically possible", phone web
  performant. Smaller download, progress bar, 1x rendering, UI fixed (see What exists).
- WeiYa (#227): much more fps (at least 3x) and less video memory, especially on phones, with
  GPU instancing, LODs, mipmaps, baked lightmaps instead of real-time lighting, asset streaming
  and a chunked map, all invisible to the player: less pop-in (trees especially), no worse
  graphics, and much shorter map load and join times. Mipmaps, tree LODs and the pop-in fix are
  in (see What exists); lightmaps, streaming and more chunking aren't yet.
- WeiYa (#166): add voice chat to the web build, release the web build with the others (an
  html entry point) and include the assets in every release. The release zips got their
  `assets/` folder first (#187); browser voice came after (see Browser / phone build).
- catherine (#230): automatic web builds deployed to a host, so the game can be played on the
  web ("work super extra hard on this"). GitHub Pages, rebuilt on every push to main.
- catherine (#151): optimisation passes, aim high, make the game run really well so there's
  room for more stuff; clean things up.
- doomy (#100): make your own character so others can see you; rigged but procedural, and a
  reusable system that NPCs can use too.
- doomy (#160): cheat on your wife with a beautiful Chinese intelligence officer (no details:
  "surprise me"). We chose a wife who patrols, flirting while she's not looking, an affair with
  hearts, spilled secrets, and morality loss. We kept it flirty, not explicit.
- catherine (#157): if a question stays unanswered, ask it again every day at noon in a big
  message until it's answered. The Discord bot re-posts every open `question` issue with no
  replies yet, in heading-sized text, at 12:00 (`REMINDER_TZ`, UTC by default); any reply
  (including `/answer`) stops it.
- hadalade (#145): a 3D modelled fat cat that follows you around, meows horrifically and grows
  any time it isn't in view. We read "in view" as your own screen, gave everyone one cat, and
  capped it at 25x so the world still fits around it.
- corvy (#195): the cat grows too quickly; at least 10x slower.
- corvy (#192): the cat meows at most every 30 s, only when you're not looking at it; looking resets the timer.
- corvy (#238, on what else should kill you): if the cat gets too big, like 10x, it should
  gobble you up. We made the lit-fuse cat hungry: it closes in on its owner and eats anyone in
  its mouth; eating resets it like the explosion does.
- bben (#238, same question): "pid 1, usually systemd". We read it as systemd's watchdog:
  stand still (AFK) for 3 minutes and PID 1 kills you, with a warning first.
- catherine (#232): gather all the work that needs an admin or some outside person to act on
  the repo, the bot image and so on, as a list for doomy. It's `ADMIN_TODO.md`: stuck
  `needs-human` issues, secrets and tokens to renew, the manual release, redeploying the
  Discord bot, upstream fixes, and checks that need a real machine or phone. No game change.
- catherine (#170): a profile picture for the Discord bot made from the game's assets and
  current status, updating every now and then. The bot (`tools/idea-bot/avatar.py`) renders
  the game's own campfire and cowboy models at night with a tiny software renderer: the big
  number is ideas landed, one cowboy per idea being built (at least one), the fire grows with
  the waiting queue, and a red ring means something needs a human. It checks every 3 hours
  (`AVATAR_HOURS`) and only changes the picture when the status changed.
- catherine (#167): post screenshots of each new feature to Discord, on every "feature is
  finished" message and when a new release comes out. Each Claude run now takes a picture of
  its idea in the game (`scripts/screenshot.sh`: screenshot mode, software rendering, no GPU
  needed) and pushes it to the `screenshots` branch; the bot attaches it to the "in the game"
  post, and announces each new `latest` build with the pictures of the ideas since the last one.
  `FRIENDSLOP_SCREENSHOT_SCENE` (play, busy, clickable, chapter, tab) stages a HUD state for
  checking UI work against its design.
- WeiYa (#165): is there a story line, or is every idea its own separate place? Unify every
  idea into one coherent, fun, silly game seen from a bird's-eye view, with polish. We read it as:
  put the side trips on the chapter checklists and give the chapters one storyline.
- just meat and cheese (#146): a voice-chat spell system; the first spell, "mute PLAYER", mutes
  them and hands them an infinite-ammo RPG that only hurts the caster and kills in one hit; it
  disappears (and they're unmuted) once they kill you. We can't recognise speech, so you say
  it out loud while pressing Q, looking at them; "killed" now means dying (#209).
- cumsauce (#209, answering "What should dying actually mean?"): you go to heaven; players beg
  for your innocence and if they succeed you return; or a Tome of Uncancelling; thrown to hell,
  you navigate the hell pit before coming back. We read "beg" as clicking your grave (out loud
  counts double), "fail" as a 60 s judgement running out, and made one tome that comes back
  3 minutes after use.
- catherine (#173): gamble with the game's balance, fair and unfair settings at random,
  including how features interact, hosted and commentated by a wizard-cat version of a famous
  comic cat. We made our own wizard cat (Marmalade) rather than use that character, picked nine
  settings that other features read (several where two features meet), and made spinning free.
- catherine (#206): a secret.
- catherine (#159): the people in the Discord, especially idea contributors, as interactable and
  romancable characters; evil variants of everyone with inverted names, living underground,
  turned into rats in the lore to save the world, adoptable as pets. Built as friend villagers
  you court with five chats, and backwards-named rats in the tunnels.
- catherine (#235): once a night, a low chance of creepy whispering and murmuring that sounds
  like it's behind you. With no in-game night we used the player's real clock (22:00-05:00),
  one 15% roll per night per player, and kept the voices behind you as you turn.

## Open questions

<!-- Short, open-ended. Derived only from what friends have proposed. -->
- Night whispers (#235) follow your real clock, since the game has no night. Should the game
  get its own night, and should anything be behind the whispers?
- The shipwreck's pirates hand out doubloons (#185). What should they buy?
- Hide and seek: should found hiders seek, or just watch? Should the world change each round?
- What does "still recovering" from the storm mean for us: are we rebuilding, scavenging, or something else?
- Everything: what are we, and where are we? (The goal: free the Commune from the landlord.)
- What legal and less-than-legal ways are there to raise rent money? And how should the landlord fight back?
- Does the commune do anything, or is it just where we are?
- Who lives in the east village, and is it part of the Commune or somewhere else?
- Organs are money, but how do you get them, and what are they for?
- You can be a person or a creature now. Does what you are change anything in play?
- Browser voice chat is new (#166): does it work on your phone and browser, and is it loud and clear enough?
- Phones (#191, #227): how long does the browser build take to load on your phone now, how many
  fps do you get, and what still feels slow or fiddly (touch controls, menus)? Does anything still
  pop in?
- Should there be a lobby/waiting room before the game starts, and who decides when it starts?
- What power do gods tears actually give, and what does an Abduction do (or get) when it feeds?
- The tears are limited. What happens when they're all used up?
- Does holding in gas (or farting) do anything beyond shame? Who else counts as an NPC?
- What's up on the lower floating islands? (The top one is heaven, with Abuela.) (#99)
- An NPC can fall for you now (#97). Should that lead anywhere?
- You can have an affair with Officer Mei (#160) and spill the commune's secrets. Who is she
  spying for, and what happens with what you tell her? What does getting caught by your wife cost?
- Who is Jegg, and what happens once Jegg's soul is whole? Does the warehouse have more in it? (#130)
  It can have a nightmare bathroom now (#161): are there other biomes in there?
- You can pick up human meat burgers at Wendy's (#117). What are they for?
- You can smoke cigs and joints from Sewer Paul (#129). Should smoking do anything to you?
- The tunnels under the map (#127) connect four places, and the friends' evil rat twins live
  there now (#159). What else is hidden down there, and should your rat do anything evil?
- You can date a Discord friend now (#159). Should dating them lead anywhere?
- The lex pile (#1) just heats up. Should it do anything, or cost or earn anything?
- Marmalade's wheel (#173) is free to spin. Should gambling cost something (organs?), and should
  some rules be off the table, or should more be on it (the Stillness, duels, the landlord's boss fight)?
- Should a failed action's result (petunias, hampter, the world ending...) actually change play?
- Stats barely touch play (Frumbly needs bread and bakeries; none affect dialogue). What should they change?
- What do totem spells actually do, and what lies beyond the locked doors?
- The mute spell (#146) is the first voice spell. What other spells could you say?
- Dying (#209) means heaven, begging and maybe the hell pit. The RPG, a 10x cat and systemd's idle watchdog (#238) kill;
  what else should (#238 is still open for answers)? Is there anything else down in hell?
- What does connecting all the pets achieve, and what else do pets do?
- Your fat cat (#145) drinks pee now (#213). Should what it eats or drinks change it?
- Bad people get a helicopter shooting at them (#134). What does getting shot do? Does being
  good get you anything? And which other actions count as good or bad?
- Heaven is now a little garden with Abuela on the top island. What else is up there, and what happens once the campaign lets you in? Should Heaven Pass tiers give rewards?
- Chapters send you to Wendy's, Sewer Paul and Jegg now (#165). Should what you bring back
  (burgers, cigs, soul pieces) pay the rent or help some other way?
- Is the campaign the right length? Which chapters drag, and which fly by? (Six players who pay
  the rent together now beat the landlord in seconds: should he get tougher with more tenants?)
- Can anyone ever truly escape HALT, and what would be on the other side?
- Surfaces are guessed from colour (#128): a grey cloak looks like stone, a dark red shirt like
  wood. Should some things get a hand-picked surface, or is the guess part of the charm?
- The cat-vision filter only recolours flat colours — the new textured models (trees, cowboys)
  keep their natural greens and reds. Should the filter learn to shift textures too? (#12)
