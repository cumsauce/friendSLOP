# friendSLOP: game UI brief for Figma

## What the game is

friendSLOP is a silly, cosy-chaotic 3D multiplayer game for 1-6 friends, designed by a Discord
group one idea at a time and built by AI. One friend hosts and the others join with a code or a
share link, with voice chat always on. It runs on desktop (keyboard and mouse, or gamepad) and in
phone browsers (touch, landscape).

The world is a bright storybook commune painted only in colours cats can see: no red/green
distinction, lots of blues, yellows, purples and soft pinks. The current UI is cream cards with
pink borders, rounded "squishy" buttons and a chunky rounded display font.

**One shared goal:** free the Commune from your landlord and get into heaven, over 5 chapters. Each
chapter is a checklist of jobs (visit Abuela in heaven, fetch burgers from Wendy's, bring Sewer
Paul crow eggs, find soul pieces in the warehouse for Jegg...), and each ticked job is a "Heaven
Pass" tier.

**The problem to solve:** the game has 40+ features, and every one added its own line of HUD text.
In play today the screen shows about 12 floating cards at once (objective, chapter checklist,
rent, fate, morality, gas meter, skills, fat-cat size, inventory counts, subtitles, toasts, help).
A chapter banner covers the middle of the screen, and nothing tells you what you can click. We
need a UI that keeps the chaos funny but readable.

## Everything the UI must show, by how often it matters

**Always visible (glanceable, about 1 second):**
1. **Goal and chapter progress:** chapter N/5, its jobs ticked (0/4), Heaven Pass tier (0/29).
   The single most important thing.
2. **Next thing to do:** a one-line pointer ("now: climb the sky islands to Abuela"), ideally with a
   direction or compass hint.
3. **Crosshair / aim point** in first person, which **changes when it's over something
   clickable**: a hand or sparkle icon plus a short verb label ("pet cat", "talk to Sewer Paul",
   "take burger", "vote to enter"). Show "too far" when out of reach.
4. **Mic state:** on/muted (M), and who is talking. Voice chat is core to the game.
5. **Gas meter** (0-100%, "F to fart" when ready). It's the most-used verb and it's funny, so give
   it a home that's fun to look at.

**Situational (pops in when relevant, fades out):**
6. **Toasts / event feed:** "Moss farted, then pointed at YOU", "you picked up a crow egg",
   level-ups, failures ("ACTION FAILED: everyone is bees"). There can be bursts of several, so it
   needs stacking, de-duplication and a fade.
7. **Subtitles:** NPC and voice lines (Abuela in old Spanish, the goat's passive-aggressive
   comments, the bear growling, crows cawing). Bottom centre, with the speaker's name.
8. **Danger / timers**, which must be unmissable:
   - ATTACK MODE (villagers chasing you, 60 s)
   - the Stillness (everything frozen for 20 s every minute)
   - rent due ("raise in 52 s")
   - the helicopter
   - being carried or floated by a villager
   - **the fat cat about to explode** (see below)
9. **Interaction prompts near things:** a door vote ("2/3 players agreed"), duel invite (Y/N),
   the pirate battle (enemy health), the chest ("open: take 25 doubloons").
10. **Pet: your fat cat.** Its size (1x-25x) grows while you're not looking at it. At critical mass
    it blinks like a bomb and you must **pet it smaller** before it explodes. This needs a size
    meter and a big countdown or warning state.
11. **Chapter intro:** big story text when a chapter starts. Today it blocks the screen; make it
    a dismissible or auto-shrinking card.

**On demand (hold Tab, or open a panel):**
12. **Scoreboard / party:** each player's name, gods' tears (and Abduction state), morality,
    organ wallet value, totems, pets, silly stats (Spoonth, Glimbo...), skills (Walking, Farting,
    Sleeping, Pissing, Smoking, Dueling, levels 1-100).
13. **Inventory:** burgers, cigs, joints, crow eggs, doubloons, soul pieces carried, organs.
    Today these are scattered single lines; they want one bag.
14. **Help / controls (H):** every "how do I..." line (30+). Needs grouping by place or feature,
    and search or filter.
15. **Join code / share link:** copy button (C), player count.

**Menus (out of play):**
16. **Title screen:** Host, Join (paste a code), Character creator, Settings, "Whose fault is it"
    (credits: every feature and the friend who asked for it), Quit. There's an animated 3D
    diorama behind it.
17. **Settings:** master and voice volume, mic toggle, "let HALT use my username", and **Video**
    (preset Low/Medium/High/Custom plus 9 toggles). Must work on phones.
18. **Character creator:** cycle through body parts and colours with live preview.
19. **Loading curtain** with a progress bar (web loads can take a while on phones).

## Controls to reflect

- **Desktop:** WASD, mouse look, left click interacts, F fart, M mute, V camera, H help, Tab
  scores, P smoke, Z sleep, X piss, Q mute spell, L lex pile, C copy code, Esc menu.
- **Phone:** left thumb joystick, right-side buttons. Touch targets must be at least 44 px. The
  most-used verbs (interact, fart, jump) get big buttons; the rest go in a radial or tray.
- **Gamepad:** A/B/X/Y plus a radial menu.

## Constraints

- 16:9 desktop (1280x720 and 1920x1080) **and** a landscape phone (about 844x390, with a notch).
  Show both.
- Readable over a very bright yellow-green world **and** in dark places (underground tunnels, the
  warehouse).
- Cat-visible palette: don't rely on red vs green. Danger is shown with shape, motion and icons,
  not colour alone.
- It's built in Bevy UI (flexbox), with no web overlay. Keep it to rounded rects, text, simple
  icons, nine-slice borders and basic tweens. No blur or heavy shaders.
- Tone: friendly, absurd, a bit crude ("you pocket a crow egg", "📧 emailing the developer a 😭").
  The UI should be in on the joke.

## Please explore these directions (one frame set each)

**A. "Cosy journal":** almost nothing on screen. A small top-left quest card (chapter, jobs,
next step) and a bottom-right status cluster (gas, mic, cat). Everything else lives in a
journal or notebook you open (Tab or J) with tabs for Party, Bag, Skills, Help and Credits. Toasts
are sticky notes that slide in and peel away. Best for readability.

**B. "Party game HUD":** big, loud and playful, like a party game show. Each player gets a
portrait strip along the top (talking ring, mic, mini stats). There's a chunky gas meter dial,
event toasts as comic speech bubbles from the edge nearest the event, and danger states take
over the screen frame (a flashing border during ATTACK MODE, a cartoon bomb fuse ring around the
cat icon). Best for streaming and chaos.

**C. "Diegetic / in-world":** as much as possible lives in the 3D world. Floating name tags
and speech bubbles over NPCs. The cat's size and bomb timer are shown on the cat itself, with
the cat's own warning lights. Clickable things get a soft outline and a verb tag next to them.
The screen keeps only the goal line, a compass ribbon with job markers, and a wrist "phone" you
raise for the party, bag and settings. Best for immersion.

**D. "Mobile-first radial":** designed for the phone first, then scaled up. One thumb
joystick, one big context button that changes its verb (Pet / Talk / Take / Enter), and a
radial wheel for the rarely-used verbs (fart, smoke, sleep, piss, spell, lex). Status is shown
as tiny icons around the radial. Best for the browser build.

For each direction, show these frames:
- the title screen
- normal play (desktop and phone)
- a busy moment: ATTACK MODE, three toasts and the cat in bomb state together
- hovering over something clickable
- the Tab panel
- settings with video options
- a chapter-start moment
- the loading screen

Include a small component sheet: buttons, toast, timer chip, meter, prompt tag, panel tabs and
icons.
