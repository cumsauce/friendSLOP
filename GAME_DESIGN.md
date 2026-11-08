# Game design (living document)

Claude keeps this up to date after every change. It's the game's memory: what exists, what friends
asked for, and what nobody has decided yet.

## Fixed

- Multiplayer: one friend hosts, others join with a code.
- Voice chat.
- 3D.
- There will be some goal everyone shares. Nobody has said what yet.

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

- **Voice lines** (#19). Two glowing purple music boxes ("musical objects") sit on the floor.
  Every 17 s someone delivers a dramatic, over-acted voice line, taking turns: a music box (which
  plays a little jingle first and hops), the bear, or a crow. It shows as a subtitle with a
  procedurally generated babble voice. The issue asked for audio from a specific 1993 game; we
  can't ship someone else's copyrighted audio, so the lines and voices are original.

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
- doomy (#14): you can challenge a player to a duel (FPS, one in the chamber gameplay).

- (#3): Dave and his mates started an anarcho-syndicalist revolt, burned down parliament and
  founded the Commune of Dave.
- #20: a judgmental goat joins the players and makes passive-aggressive comments about every choice.

## Open questions

<!-- Short, open-ended. Derived only from what friends have proposed. -->
- Everything: what are we, where are we, and what are we trying to do together?
- Does the commune do anything, or is it just where we are?
- Organs are money, but how do you get them, and what are they for?
- Players only have bodies in a duel. What are you the rest of the time?
- What does your morality actually change? And which actions count as good or bad?
