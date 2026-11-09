# idea-bot

Discord bot that lets friends without GitHub accounts file ideas (`/idea`), answer Claude's
questions (`/answer`), and see what's needed (`/whatsneeded`), pending (`/status`) or how the game
plays right now (`/loop`). It posts back when an idea lands in the game, and after every few landed ideas
(`LOOP_EVERY`, default 3) it posts the gameplay loop that Claude keeps in `GAME_DESIGN.md` (#153).
Every day at noon it asks each still-unanswered question again in big text, until someone replies
(#157).
Every few hours it redraws its own profile picture from the game's campfire and cowboy models
and the current status: ideas landed, being built, waiting, and stuck (#170). Preview it with
`python3 avatar.py out.png 54 3 2 0` (needs `numpy` and `pillow`).

Screenshots (#167): each Claude run pushes a picture of its idea to the `screenshots` branch as
`issue-<N>.png` (`scripts/screenshot.sh` + `scripts/publish-screenshot.sh`). The "in the game"
post attaches it, and when the `latest` release is rebuilt the bot posts "New build is out" with
the ideas (and pictures) that landed since the previous build. The bot needs "Attach Files" in
the channel; without a picture it just posts text.

Issues and comments are created with the repo owner's token, so they count as trusted (`OWNER`) and
trigger Claude. The friend's Discord name is credited in the body.

## Setup

1. https://discord.com/developers/applications → New Application → Bot → copy the token.
   Then OAuth2 → URL generator → scopes `bot` + `applications.commands`, permissions
   "Send Messages" + "Attach Files" → open the URL and add the bot to your server.
2. Create a fine-grained GitHub PAT for **only** `cumsauce/friendSLOP` with Issues: read & write and
   Contents: read.
3. Run it. Env vars: `DISCORD_TOKEN`, `GITHUB_TOKEN`, `GITHUB_REPO=cumsauce/friendSLOP`,
   `ANNOUNCE_CHANNEL_ID` (right-click channel → Copy ID, with developer mode on), and `STATE_PATH`
   (defaults to `/data/state.json`, so mount a volume at `/data`). Optional `LOOP_EVERY`: landed
   ideas between gameplay-loop posts (`0` turns them off). Optional `REMINDER_TZ`: the IANA time
zone whose noon the daily question reminder uses (e.g. `Europe/London`; default `UTC`). Optional `AVATAR_HOURS`: how often the profile picture is redrawn (default `3`;
`0` leaves it alone). It's only re-uploaded when the status changed, to stay inside Discord's
avatar rate limit.

Image: `ghcr.io/cumsauce/friendslop-idea-bot:latest`, built by `.github/workflows/idea-bot.yml`
from `nix build .#idea-bot-image`.
