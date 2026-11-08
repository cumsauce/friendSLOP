# Game design (living document)

Claude keeps this up to date after every change. It's the game's memory: what exists, what friends
asked for, and what nobody has decided yet.

## Fixed

- Multiplayer: one friend hosts, others join with a code.
- Voice chat.
- 3D.
- There will be some goal everyone shares: so far, beating your landlord (#11).

## What exists

- An empty floor in an empty world. Players exist only as names on a list (and get bodies only
  while duelling).
- **Currency: organs** (#22). Each organ's value = creature worth (human 10, cow 5, pig 4,
  chicken 2, rat 1) × organ importance (brain 10, heart 9, lungs 7, liver 6, kidney 4,
  stomach 3, appendix 1). Every player has an organ purse, shown top-right. Everyone starts with
  a rat appendix, a chicken kidney and a pig stomach (worth 21) so there's something to see.
  Nothing yet earns or spends organs.
- **Crows** (#2). Three cute black crows hang around the floor, hopping and wandering about. Every
  so often one puffs up and caws (shown as "caw!" text; no sound yet). The server spawns them;
  every peer animates them locally.
- **The bear** (#4). Exactly one cute, round, friend-shaped brown bear ambles slowly around one
  side of the floor. Every ~13 seconds it rears up and does a scawy (cute) growl: a short
  procedurally generated rumbly sound plus "grrr~" text. It doesn't do anything else yet.
- **Voice-altering mushroom** (#6). One red spotty mushroom grows somewhere on the floor. Click it
  to eat it (there are no player bodies yet, so clicking is how you "find" it). Your own voice
  then becomes much higher (1.5-1.9x) or much lower (0.55-0.7x) at random for 45 seconds, run
  through a small pitch-shifter so everyone hears it. A new mushroom grows elsewhere 10 s later.
- **Cat-vision palette** (#12). Everything is shown in a bright, cheery storybook-MMO palette, but
  only in colours cats can see: blues and yellows, no red-green difference. Every material is
  run through a cat-vision filter (then lightened and saturated), and the sky is bright blue. So
  the "red" mushroom and the green floor both look yellowish to us, like they would to a cat.
- **Duels** (#14). Press a number key to challenge another player; they press Y/N. The two
  duelists get capsule bodies in a small walled arena on the floor and play first-person
  (WASD, mouse, click) while everyone else watches. "One in the chamber" rules: one bullet,
  every hit kills, a kill refunds your bullet, with no bullets you punch up close, you respawn
  with one bullet. 3 lives each; the last one standing wins. One duel at a time. Nothing is
  won or lost beyond bragging rights yet.
- **Commune of Dave** (#3). Parliament is a smouldering ruin in a corner of the floor, with a
  red-and-black flag, Dave himself (fist raised) and a banner at the top of the screen. Scenery
  only for now; every peer spawns it locally.
- **The judgmental goat** (#20). A white goat with half-closed eyes stands on the floor, slowly
  shaking its head. It comments passive-aggressively (on-screen text) on every choice it can see:
  someone joining or leaving, the mushroom being eaten, you muting or unmuting, and, after 40 s of
  nothing happening, doing nothing. There are few choices in the game yet, so it judges those.

- **Morality** (#21). Every player has a morality score (starts at 0), shown bottom-left with a
  label (reprehensible / dubious / neutral / decent / saintly). Actions have a random moral value
  within a sensible range: eating the mushroom is -2..+2. Harming anyone is -5..+3, except harming
  the nobility, capitalists, landlords or billionaires, which is **always** -6..-10. Only the
  mushroom is wired up so far (nothing can be harmed yet), and morality doesn't change anything
  else in play yet.
- **Farting** (#18). Every player's gas pressure builds from 0 to 100% over 40 s (shown
  bottom-right). Press F (with at least 20%) to fart; since players have no bodies outside duels,
  your bottom is wherever the mouse cursor points on the floor (or your duel body when duelling).
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
  in place; eating the mushroom and shooting are ignored; a big message with a countdown shows in
  the middle of the screen. Voice chat keeps working. The host's clock decides when it happens.
- **Gods tears** (#16). Glowing blue teardrop gems fall from the sky onto the floor (up to 4 at
  a time, a new one every ~8 s). Click one to use it. Each use transforms you a little further
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
  from his stash, which counts as harming a landlord (always reprehensible, #21). Act 3: after 3
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
  turn into potted petunias. The results are only on-screen text for now; they don't change play.
- **Nonsensical stats** (#7). Every player rolls three stats (1-10) on joining, shown on the left:
  **Wombility** makes your duel speed swing up and down following `sin(x)` of time (more
  Wombility, bigger swings); it should also make purple-haired characters like you and
  green-haired ones despise you, but nobody has hair yet. **Rumpet** tints the duel arena's walls
  yellow (by the room's best Rumpet), and at 8+ white rabbits appear hopping on the floor.
  **John** gives a slight jump boost in meadows, but there's no jumping and no meadows yet.
- **Totems and locked doors** (#5). Three glowing totems (Wind, Frost, Fire) are the world's
  magic. Click one to carry it; press R to cast the spell of every totem you hold (shown as text
  to everyone; spells have no effect yet). A small stone vault on the east side has a Wind door
  and a Frost door, with the Fire totem inside. Click a door while holding its totem to insert
  it: the door sinks open, but that totem can't be cast while it's in there. Doors open from
  either side, but you can only take the totem back out from the side you put it in (players
  have no bodies, so "your side" is the side of the door you click on). The Fire totem can only
  be picked up while a vault door is open. Just one small puzzle for now.
- **Pets** (#15). Four stray animals (dog, cat, bunny, duck) sit on the floor. Click one to adopt
  it (one pet each); it trots after your mouse pointer on the floor, since you have no body.
  Adopted pets standing close together are joined by a pink leash-line, and the HUD (bottom
  right) counts how many are connected. When every adopted pet is linked into one pack, everyone
  sees "ALL PETS CONNECTED!". That's a first stab at "connecting them is a core loop": nothing
  is rewarded for it yet. A leaving player's pet goes stray again.

## Ideas so far

- Freyja Moth (#22): the only legal tender is organs, human and animal, valued by the creature
  and by how important the organ is to it (brain valuable, appendix less so).

- corvy (#2): cute crows that caw occasionally.
- arseeN (#4): add a bear, just one. Cute and friend shaped, maybe scawy (cute) growling noises.
- Freyja Moth (#21): many actions have a (mostly random, within reason) moral value that affects
  gameplay; harming the nobility, capitalists, landlords and/or billionaires is always reprehensible.
- Kori.exe (#19): NPCs and random "musical objects" play voice lines (asked for audio from a 1993
  game; done as original lines instead).
- corvy (#6): a mushroom players can find that randomly makes their voice much higher or lower.
- just meat and cheese (#12): colour palette based on Maplestory, but only colours cats can see.
- just meat and cheese (#17): every 60 s all actions and NPC behaviour freeze for 20 s so players
  can socialize and stare with passion.
- Freyja Moth (#16): gods tears, teardrop gems that rained from the sky when humans came to be.
  They hold great power; using them transforms you, eventually into a fetid, always-hungry
  Abduction that devours tears no matter who's in the way. Nobody knows what happens if they succeed.
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
- (#7): nonsensical RPG stats affecting combat, dialogue and exploration, e.g. Wombility,
  Rumpet, John.
- (#5): magical totems are the magic system; holding one lets you cast its spell. Magic doors
  unlock when a matching totem is inserted (then it can't be cast), unlock from both sides, but
  the totem only comes out from the side it went in. Doors should make puzzles and shortcuts.
- doomy (#15): some animal pets you can have follow you; maybe connecting them is a core game loop.
- doomy (#14): you can challenge a player to a duel (FPS, one in the chamber gameplay).

- Kori.exe (#18): gas pressure builds over time; release it away from NPCs, or a "shame" effect
  blurs the screen and plays a scary distorted chiptune for 20 seconds.

- doomy (#26): a tunable, physically accurate fart synth with controllable timbre, length and
  pitch.

- (#3): Dave and his mates started an anarcho-syndicalist revolt, burned down parliament and
  founded the Commune of Dave.
- #20: a judgmental goat joins the players and makes passive-aggressive comments about every choice.
- doomy (#25): a proper, really cute multiplayer menu to host or join, see who's in the lobby,
  then start; minimal settings; playable with just one person for testing.

## Open questions

<!-- Short, open-ended. Derived only from what friends have proposed. -->
- Everything: what are we, where are we, and what are we trying to do together?
- What does "still recovering" from the storm mean for us: are we rebuilding, scavenging, or something else?
- Everything: what are we, and where are we? (The goal so far: deal with the landlord.)
- What legal and less-than-legal ways are there to raise rent money? And how should the landlord fight back?
- Does the commune do anything, or is it just where we are?
- Organs are money, but how do you get them, and what are they for?
- Players only have bodies in a duel. What are you the rest of the time?
- Should there be a lobby/waiting room before the game starts, and who decides when it starts?
- What power do gods tears actually give, and what does an Abduction do (or get) when it feeds?
- Does holding in gas (or farting) do anything beyond shame? Who else counts as an NPC?
- Should a failed action's result (petunias, hampter, the world ending...) actually change play?
- Stats exist but there's no hair, jumping, meadows or dialogue for them to act on. Which should come?
- What do totem spells actually do, and what lies beyond the locked doors?
- What does connecting all the pets achieve, and what else do pets do?
- What does your morality actually change? And which actions count as good or bad?
