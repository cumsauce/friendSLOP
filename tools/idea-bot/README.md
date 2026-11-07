# idea-bot

Discord bot that lets friends without GitHub accounts file ideas (`/idea`), answer Claude's
questions (`/answer`), and see what's needed (`/whatsneeded`) or pending (`/status`). It posts back
when an idea lands in the game.

Issues and comments are created with the repo owner's token, so they count as trusted (`OWNER`) and
trigger Claude. The friend's Discord name is credited in the body.

## Setup

1. https://discord.com/developers/applications → New Application → Bot → copy the token.
   Then OAuth2 → URL generator → scopes `bot` + `applications.commands`, permission
   "Send Messages" → open the URL and add the bot to your server.
2. Create a fine-grained GitHub PAT for **only** `cumsauce/friendSLOP` with Issues: read & write and
   Contents: read.
3. Run it. Env vars: `DISCORD_TOKEN`, `GITHUB_TOKEN`, `GITHUB_REPO=cumsauce/friendSLOP`,
   `ANNOUNCE_CHANNEL_ID` (right-click channel → Copy ID, with developer mode on), and `STATE_PATH`
   (defaults to `/data/state.json`, so mount a volume at `/data`).

Image: `ghcr.io/cumsauce/friendslop-idea-bot:latest`, built by `.github/workflows/idea-bot.yml`
from `nix build .#idea-bot-image`.
