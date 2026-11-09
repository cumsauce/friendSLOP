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

## What exists

- A grassy meadow floor (see "Nicer look" below).
- **First-person bodies** (#53). Everyone has a body (your own character, see below) and
  plays in first person: WASD walk, mouse look, Space jump, Shift run, cursor locked. Left click
  uses whatever is under the crosshair (mushroom, tears, landlord, totems, pets); doors are used
  from the side you stand on; you fart where you stand; your pet follows you. Bodies can't walk
  through trees, rocks, walls, sheds, bales or crates: the host walks them with a kinematic
  character controller (`src/kcc`, ported from bevy_ahoy at `630a4573`, MIT OR Apache-2.0) — it
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
- **Commune villagers** (#61, the "NPCs" part). Ten villagers (Kevin, Brenda, Old Tom, Moss, ...)
  built by the same character system as players, rolled from the same option pool, so hiders
  can blend in. The
  server moves them at 5 Hz: wander between the bonfire, ruins, pond and cover spots, pause or sit,
  never enter the duel arena, stop during the stillness. Clicking one says hello; a seeker who
  clicks one while seeking gets "that's just Kevin from the commune" and is frozen for 3 s
  (everyone hears about it). Nearby villagers sometimes mutter a line (toast). On the host they
  sweep a ball along each step and veer around trees, rocks and walls (or pick a new spot if boxed
  in). Once the host is in the game it builds a navigation mesh (rerecast, off the main thread,
  ~3 s; see `tests/navmesh.rs`) over the terrain and the village architecture, and most trips then
  go overland between the districts — commune centre, east village market, the maze's statue, the
  castle ruins, the forest clearing and the lake shore — via a landmass agent per villager. Until
  the mesh is ready they fall back to straight-line steering between the local spots. Scattered
  trees, rocks and props are deliberately left out of the mesh (they punch holes that trip
  rerecast; see `vendor/rerecast/README-friendSLOP.md` for the carried fix); `avoid_cover` still
  steers around them. They have no name label.
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
  is ±90 m (was ±25), with a soft wall at ±88 and hills rising beyond it (`terrain.rs`). The
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
  (`landmarks.rs`), and a generated, adaptive 90s jazz-fusion soundtrack (`features/fusion`,
  asked for in chat: "change the music ... jazz fusion ... 90s/FM synth vibe ... adapts to the
  game state"; it replaced the looping "Catsong"). The music lives in its own Bevy-free crate
  (`crates/fusion`; `features/fusion` is just the game glue). Live FM voices (DX7-style EP,
  slap and fretless bass, lead/brass with portamento, bell pad, kalimba), subtractive voices (a
  CS-80 style horn section that shouts, pushes and swells around the lead, string machine,
  Minimoog-style lead), a synthesized kit with ride, clap, shaker, congas and toms, and
  foley-synth note-block bells/flute/harp/guitar. The lead tells a story over eight bars
  (statement, answer, development to a peak in bar seven, restatement, cadence; chord tones on
  long notes and guide tones on changes), a second voice answers it in its gaps (flute, harp,
  kalimba, strings or horns by mood), and solos trade between two synths. The sound evolves:
  brightness and reverb/echo amounts follow the sections, pads and horns drift slowly, and a
  reversed cymbal rises into big sections. Per-bus EQ, chorus, a tempo-synced echo, a wet room
  and per-mood make-up gain keep the mix balanced (checked against a typical jazz/pop octave-band
  balance with `cargo run -p fusion --release --bin fusion-render -- stems`). A director picks the next bars at every bar line from a mood: menu (signature hook
  over "Just the Two of Us" changes), meadow, commune/village (city-pop), forest/valley (flute,
  harp), maze/ruins (minor, bells), sky islands (dreamy), near Abuela (bolero), hide-and-seek
  hiding/seeking, duel (150 bpm), landlord (chromatic, building), stillness (holds on a pad),
  plus a victory kime when a round/duel/chapter/landlord is won. Sections (intro, A, hook, B,
  solo, breakdown), key changes, fills, 7/8 kime bars, layers faded by intensity. Local only,
  ~2.5% of a core, respects master volume. Plus real sound effects
  for crows, duels and mushroom bites, and an ambient bed (#62): quiet birdsong, a synthesized
  wind loop, and a spatial crackling-fire loop at the campfire that gets louder as you approach.
  Procedural sounds (bear, HALT, voice lines, farts) render at 44.1 kHz with band-limited
  oscillators, a gentle low-pass, click-free fades, normalised levels and dithered output. All of it comes from CC0 asset packs stored in Git LFS
  (`crates/game/assets`, credits in `crates/game/assets/CREDITS.md`); every peer renders the
  same scene from a fixed seed, and if the files are missing the game quietly falls back to its
  procedural-primitive look.
- **Currency: organs** (#22). Each organ's value = creature worth (human 10, cow 5, pig 4,
  chicken 2, rat 1) × organ importance (brain 10, heart 9, lungs 7, liver 6, kidney 4,
  stomach 3, appendix 1). Every player has an organ purse, shown top-right. Everyone starts with
  a rat appendix, a chicken kidney and a pig stomach (worth 21) so there's something to see.
  Nothing yet earns or spends organs.
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
  with +6..+10 (corrected by Freyja Moth in #43). Only the
  mushroom is wired up so far (nothing can be harmed yet), and morality doesn't change anything
  else in play yet.
- **Farting** (#18). Every player's gas pressure builds from 0 to 100% over 40 s (shown
  bottom-right). Press F (with at least 20%) to fart where your body stands (in the overview
  camera: wherever the mouse cursor points on the floor; your duel body when duelling).
  If any NPC (the bear, a crow, or Dave at parliament) is within 4 m, you are **shamed**: for
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

- **Voice lines** (#19). Two glowing purple music boxes ("musical objects") sit on the floor.
  Every 17 s someone delivers a dramatic, over-acted voice line, taking turns: a music box (which
  plays a little jingle first and hops), the bear, or a crow. It shows as a subtitle with a
  procedurally generated babble voice. The issue asked for audio from a specific 1993 game; we
  can't ship someone else's copyrighted audio, so the lines and voices are original.
- **Main menu** (#25). A cute pastel clickable menu: Host a game (works solo; the join code is
  copied to the clipboard), Join (type or paste a friend's code, then Join! or Enter), and
  Settings: master volume, voice volume, microphone on/muted (M still toggles it). Settings are
  per player and not saved between runs. `cargo run -- host` / `join <code>` still skip the menu.
  There is no separate lobby screen yet: hosting drops you straight into the world, and friends
  appear on the player list as they join.
- **The Stillness** (#17). Every 60 seconds the whole world freezes for 20 seconds so everyone
  can socialize and stare with passion. Crows, the bear, physics, duel movement and all timers stop
  in place, gas stops building, and the server ignores every player request (farting, prying,
  duels, totems, pets, tears, the landlord, the mushroom, tuning your fart); a big message with a
  countdown shows in the middle of the screen. Voice chat keeps working. The host's clock decides
  when it happens.
- **FPS overlay** (dev tool, not a friend idea): F3 toggles frame rate and frame time, top-right.
- **Gods tears** (#16, corrected in #43). The tears rained down only once, when humans came into
  existence: 12 glowing blue teardrop gems fall onto the floor when the world begins, and no more
  ever fall. The HUD shows how many are left. Click one to use it. Each use transforms you a little further
  (untouched, tear-touched, changing, barely human), and after 7 you become an **Abduction**: a
  fetid creature that hungers for tears. Everyone's state is shown on the right of the screen.
  Tears don't grant any other power yet, and being an Abduction changes nothing else yet.
- **The great storm of '21** (#13). World history: in 1921 a great storm (to the British, "the
  storm of it's a bit windy innit") did untold damage and toppled several governments, and the
  world is still recovering. The floor is strewn with its wreckage: two fallen trees, a toppled
  lamppost, scattered roof slates and a stone memorial plaque (its text shows at the bottom of the
  screen). Every 25 s a leftover gust rattles the debris for a few seconds and the text switches
  to "it's a bit windy innit". Scenery only; gusts pause during the Stillness. (The burned
  parliament of the Commune of Dave fits as one of the toppled governments.)
- **Landlords** (#11). The game's story. A landlord in a top hat stands on the floor, and as the
  game starts he has just raised your rent (the inciting incident; shown top-right). Act 1, legal:
  click him to pay with every organ you carry; he raises the rent again every 60 s (and whenever
  you pay it off). Act 2, less than legal: after 3 raises, clicking him pickpockets an organ back
  from his stash, which counts as harming a landlord (always raises your morality, #21/#43). Act 3: after 3
  heists he grows huge for a **boss fight**; 12 clicks knock him down, everyone gets the stash
  back and the game says THE END. The whole group shares one landlord and one story, so this is
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
  sideways, every landlord briefly feels shame, or you turn the sea into soup. The results are only on-screen text for now; they don't change play.
- **Nonsensical stats** (#7, reworked by #48). Every player rolls six stats (1-10) on joining,
  shown on the left. #7's Wombility, Rumpet and John were only examples and were replaced (#48):
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
  5 *The Eviction* (Fire totem, 6 tears, 3 hide and seek, 3 duels, HALT x3). Ticks never untick.
  Hide and seek and duels are waived when you play alone. When a chapter's list is done, a
  summary appears and anyone presses Enter for the next chapter (the round resets; features
  observe the server-side `RoundReset`). After chapter 5 the Commune is free, there's an ending
  (with the dedication just meat and cheese asked for), and Enter starts again from chapter 1.
  The 4-6 hours is a target, not measured: the counts are easy to tune in `campaign.rs`.
  "It can be like getting into heaven": heaven is the end of the campaign, so the goal reads
  "Free the Commune from your landlord and get into heaven" and the ending opens the pearly
  gates. "It can be like fortnite": we read that as a season-pass ladder (no copied names or
  looks). Every ticked task is one tier on the **Heaven Pass** (24 tiers over the five
  chapters), shown under the goal line, with a "tier unlocked" toast; the top tier is heaven.
- **East village district** (feedback: "higher buildings or stuff u can go inside and like
  walls", more scale, places to run around and hide). East of the meadow (x 35..80, z -35..35,
  reached by a path arriving at (35, 0)), built from code in `buildings.rs`: seven enterable
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
  wasm + assets) into `dist/web`; the Release workflow attaches it as `friendslop-web.zip` and,
  if the repo has Pages enabled, hosts it there. WebGL **1.0** isn't possible: Bevy 0.20's
  renderer (wgpu) needs WebGL2 at minimum (every current browser, iOS 15+). Phone-friendly:
  no zoom/scroll/bounce, 100dvh, "Tap to play" goes fullscreen and locks landscape where the
  browser allows (Android Chrome); iOS Safari can do neither for a page, so a "turn your phone
  sideways" card covers portrait and "Add to Home Screen" gives the fullscreen look; audio
  unlocks on the first tap. **Multiplayer in the browser:** a browser can **join** a desktop host
  with the normal join code, and can **host** too (desktop or browser friends join it). In the
  browser iroh can't use UDP, so its connections always go through iroh's free public relays
  (a bit more lag than a direct desktop link); `net_web.rs` runs iroh in browser async tasks and
  speaks exactly `bevy_replicon_iroh`'s wire format (`net_wire.rs`). Paste works with Ctrl/Cmd+V
  or the Paste button (the browser may ask for clipboard permission). **No voice in the browser
  yet:** the voice plugin is built and its messages registered (so the protocol matches desktop
  peers), but cpal has no microphone or speakers there, so browser players can't talk or hear;
  desktop players still talk to each other. Character and graphics settings are
  saved in localStorage.
- **Touch and gamepad controls** (#106). Touch (overlay appears on first touch): thumb anywhere
  on the left third is a joystick, drag on the right to look, round buttons for Jump, Use (click
  at the crosshair), Run (toggle), Fart, Help, Score, View and Leave. Climbing is still "hold
  forward". Gamepad: left stick move, right stick look, A jump, B/RB/RT use, LB or left-stick
  click run, X fart, Y help, Back scoreboard, D-pad up view, Start = Esc; in the menu A/Start
  hosts. Both press the same keys/mouse buttons features already listen to (`controls.rs`).
- **Graphics quality** (#106). Settings → Graphics: **Low** (default in the browser, or
  `FRIENDSLOP_LOW=1`) turns off sun shadows, bloom and MSAA, draws at 1x pixel density, pulls
  fog and draw distance in (140 m), and halves grass tufts and ambient particles (those two on
  the next world build). **High** is the full look. Saved locally. 60 fps on low-end phones is
  the aim, not measured.

## Ideas so far

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
- doomy (#100): make your own character so others can see you; rigged but procedural, and a
  reusable system that NPCs can use too.

## Open questions

<!-- Short, open-ended. Derived only from what friends have proposed. -->
- Hide and seek: should found hiders seek, or just watch? Should the world change each round?
- What does "still recovering" from the storm mean for us: are we rebuilding, scavenging, or something else?
- Everything: what are we, and where are we? (The goal: free the Commune from the landlord.)
- What legal and less-than-legal ways are there to raise rent money? And how should the landlord fight back?
- Does the commune do anything, or is it just where we are?
- Who lives in the east village, and is it part of the Commune or somewhere else?
- Organs are money, but how do you get them, and what are they for?
- You can be a person or a creature now. Does what you are change anything in play?
- Browser players can join and host now but not talk: does browser voice matter to you?
- Should there be a lobby/waiting room before the game starts, and who decides when it starts?
- What power do gods tears actually give, and what does an Abduction do (or get) when it feeds?
- The tears are limited. What happens when they're all used up?
- Does holding in gas (or farting) do anything beyond shame? Who else counts as an NPC?
- What's up on the lower floating islands? (The top one is heaven, with Abuela.) (#99)
- An NPC can fall for you now (#97). Should that lead anywhere?
- Should a failed action's result (petunias, hampter, the world ending...) actually change play?
- Stats barely touch play (Frumbly needs bread and bakeries; none affect dialogue). What should they change?
- What do totem spells actually do, and what lies beyond the locked doors?
- What does connecting all the pets achieve, and what else do pets do?
- What does your morality actually change? And which actions count as good or bad?
- Heaven is now a little garden with Abuela on the top island. What else is up there, and what happens once the campaign lets you in? Should Heaven Pass tiers give rewards?
- Is the campaign the right length? Which chapters drag, and which fly by?
- Can anyone ever truly escape HALT, and what would be on the other side?
- The cat-vision filter only recolours flat colours — the new textured models (trees, cowboys)
  keep their natural greens and reds. Should the filter learn to shift textures too? (#12)
