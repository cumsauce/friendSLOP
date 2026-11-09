#!/usr/bin/env python3
"""friendSLOP idea bot: Discord -> GitHub issues, and back.

Commands:
  /idea          open a form; files a GitHub issue labeled `idea`
  /answer        reply to one of Claude's `question` issues
  /whatsneeded   what the game still needs ideas for
  /status        open ideas and where they're at
  /loop          the current gameplay loop, start to finish

It also posts in ANNOUNCE_CHANNEL_ID when an idea lands in the game, when Claude asks the
group a new question, and every LOOP_EVERY landed ideas the gameplay loop (#153).

Env: DISCORD_TOKEN, GITHUB_TOKEN, GITHUB_REPO (owner/name), ANNOUNCE_CHANNEL_ID (optional),
STATE_PATH (optional, default /data/state.json), LOOP_EVERY (optional, default 3; 0 = never).
"""

import asyncio
import json
import logging
import os
import re
from pathlib import Path

import aiohttp
import discord
from discord import app_commands
from discord.ext import tasks

log = logging.getLogger("idea-bot")
logging.basicConfig(level=logging.INFO)

DISCORD_TOKEN = os.environ["DISCORD_TOKEN"]
GITHUB_TOKEN = os.environ["GITHUB_TOKEN"]
REPO = os.environ.get("GITHUB_REPO", "cumsauce/friendSLOP")
ANNOUNCE_CHANNEL_ID = int(os.environ.get("ANNOUNCE_CHANNEL_ID", "0") or 0)
STATE_PATH = Path(os.environ.get("STATE_PATH", "/data/state.json"))
API = f"https://api.github.com/repos/{REPO}"
RELEASE_URL = f"https://github.com/{REPO}/releases/tag/latest"
LOOP_EVERY = int(os.environ.get("LOOP_EVERY", "3") or 0)
DISCORD_LIMIT = 2000


def load_state() -> dict:
    try:
        return json.loads(STATE_PATH.read_text())
    except (OSError, ValueError):
        return {"announced_done": [], "announced_questions": [], "issue_channels": {}}


def save_state(state: dict) -> None:
    try:
        STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
        STATE_PATH.write_text(json.dumps(state))
    except OSError as e:
        log.warning("could not save state: %s", e)


state = load_state()


class GitHub:
    def __init__(self) -> None:
        self.session: aiohttp.ClientSession | None = None

    async def request(self, method: str, url: str, **kwargs):
        if self.session is None:
            self.session = aiohttp.ClientSession(
                headers={
                    "Authorization": f"Bearer {GITHUB_TOKEN}",
                    "Accept": "application/vnd.github+json",
                    "User-Agent": "friendslop-idea-bot",
                }
            )
        async with self.session.request(method, url, **kwargs) as resp:
            if resp.status >= 400:
                raise RuntimeError(f"GitHub {resp.status}: {await resp.text()}")
            if "json" in resp.headers.get("Content-Type", ""):
                return await resp.json()
            return await resp.text()

    async def create_issue(self, title: str, body: str, labels: list[str]) -> dict:
        return await self.request(
            "POST", f"{API}/issues", json={"title": title, "body": body, "labels": labels}
        )

    async def comment(self, number: int, body: str) -> dict:
        return await self.request("POST", f"{API}/issues/{number}/comments", json={"body": body})

    async def issues(self, labels: str, state_: str = "all", per_page: int = 30) -> list[dict]:
        return await self.request(
            "GET",
            f"{API}/issues",
            params={"labels": labels, "state": state_, "per_page": per_page, "sort": "updated"},
        )

    async def get_issue(self, number: int) -> dict:
        return await self.request("GET", f"{API}/issues/{number}")

    async def design_section(self, heading: str) -> str:
        """The text under '## <heading>' in GAME_DESIGN.md, without HTML comments."""
        text = await self.request(
            "GET",
            f"{API}/contents/GAME_DESIGN.md",
            headers={"Accept": "application/vnd.github.raw+json"},
        )
        section = re.search(rf"## {re.escape(heading)}\n(.*?)(?:\n## |\Z)", str(text), re.S)
        if not section:
            return ""
        return re.sub(r"<!--.*?-->", "", section.group(1), flags=re.S).strip()

    async def open_questions(self) -> list[str]:
        """Bullets under '## Open questions' in GAME_DESIGN.md."""
        return [
            line[2:].strip()
            for line in (await self.design_section("Open questions")).splitlines()
            if line.startswith("- ")
        ]

    async def gameplay_loop(self) -> str:
        """The '## Gameplay loop' section Claude keeps current, joined into paragraphs."""
        paragraphs = re.split(r"\n\s*\n", await self.design_section("Gameplay loop"))
        return "\n\n".join(" ".join(p.split()) for p in paragraphs if p.strip())


gh = GitHub()


def chunks(text: str, limit: int = DISCORD_LIMIT) -> list[str]:
    """Split text into Discord-sized messages, at paragraph breaks where possible."""
    out: list[str] = []
    current = ""
    for para in text.split("\n\n"):
        while len(para) > limit:
            out += [current] if current else []
            current = ""
            cut = para.rfind(" ", 0, limit)
            cut = cut if cut > 0 else limit
            out.append(para[:cut])
            para = para[cut:].lstrip()
        joined = f"{current}\n\n{para}" if current else para
        if len(joined) > limit:
            out.append(current)
            current = para
        else:
            current = joined
    if current:
        out.append(current)
    return out


def credit(user: discord.abc.User) -> str:
    return f"**{user.display_name}** (via Discord)"


class IdeaModal(discord.ui.Modal, title="Add an idea to the game"):
    idea_title = discord.ui.TextInput(label="The idea in a few words", max_length=120)
    details = discord.ui.TextInput(
        label="Tell us more",
        style=discord.TextStyle.paragraph,
        max_length=3000,
        required=False,
    )

    def __init__(self, hint: str | None) -> None:
        super().__init__()
        if hint:
            self.details.placeholder = f"Still undecided: {hint}"[:100]

    async def on_submit(self, interaction: discord.Interaction) -> None:
        await interaction.response.defer(thinking=True)
        body = f"From {credit(interaction.user)}:\n\n{self.details.value or '_(no details: surprise me)_'}"
        try:
            issue = await gh.create_issue(str(self.idea_title.value), body, ["idea"])
        except Exception as e:  # noqa: BLE001
            log.exception("create issue failed")
            await interaction.followup.send(f"Couldn't file that, sorry: {e}", ephemeral=True)
            return
        state["issue_channels"][str(issue["number"])] = interaction.channel_id
        save_state(state)
        await interaction.followup.send(
            f"💡 **{issue['title']}** → {issue['html_url']}\n"
            "Claude will pick it up shortly. I'll post here when it's in the game."
        )


intents = discord.Intents.default()
client = discord.Client(intents=intents)
tree = app_commands.CommandTree(client)


@tree.command(name="idea", description="Add an idea to the game")
async def idea(interaction: discord.Interaction) -> None:
    hint = None
    try:
        questions = await gh.open_questions()
        hint = questions[0] if questions else None
    except Exception:  # noqa: BLE001
        log.exception("fetching open questions failed")
    await interaction.response.send_modal(IdeaModal(hint))


@tree.command(name="answer", description="Answer one of Claude's questions")
@app_commands.describe(number="The question's issue number", answer="Your answer")
async def answer(interaction: discord.Interaction, number: int, answer: str) -> None:
    await interaction.response.defer(thinking=True)
    try:
        issue = await gh.get_issue(number)
        await gh.comment(number, f"From {credit(interaction.user)}:\n\n{answer}")
    except Exception as e:  # noqa: BLE001
        await interaction.followup.send(f"Couldn't post that: {e}", ephemeral=True)
        return
    await interaction.followup.send(f"🗳️ Answered **{issue['title']}** → {issue['html_url']}")


@tree.command(name="whatsneeded", description="What the game still needs ideas for")
async def whatsneeded(interaction: discord.Interaction) -> None:
    await interaction.response.defer()
    lines: list[str] = []
    try:
        lines += [f"• {q}" for q in await gh.open_questions()]
        for q in await gh.issues("question", "open", 10):
            lines.append(f"• ❓ #{q['number']} {q['title']} (`/answer {q['number']}`)")
    except Exception as e:  # noqa: BLE001
        await interaction.followup.send(f"Couldn't check: {e}")
        return
    await interaction.followup.send(
        "**Still up for grabs:**\n" + ("\n".join(lines) or "Anything! Use `/idea`.")
    )


@tree.command(name="status", description="Open ideas and where they're at")
async def status(interaction: discord.Interaction) -> None:
    await interaction.response.defer()
    try:
        ideas = await gh.issues("idea", "open", 15)
    except Exception as e:  # noqa: BLE001
        await interaction.followup.send(f"Couldn't check: {e}")
        return
    if not ideas:
        await interaction.followup.send(f"No pending ideas. Latest build: {RELEASE_URL}")
        return
    lines = []
    for i in ideas:
        labels = {lbl["name"] for lbl in i["labels"]}
        icon = "🔨" if "in-progress" in labels else "🆘" if "needs-human" in labels else "⏳"
        lines.append(f"{icon} #{i['number']} {i['title']}")
    await interaction.followup.send("\n".join(lines) + f"\n\nLatest build: {RELEASE_URL}")


@tree.command(name="loop", description="The current gameplay loop, start to finish")
async def loop(interaction: discord.Interaction) -> None:
    await interaction.response.defer()
    try:
        text = await gh.gameplay_loop()
    except Exception as e:  # noqa: BLE001
        await interaction.followup.send(f"Couldn't check: {e}")
        return
    for part in chunks(f"🔁 **The gameplay loop right now**\n\n{text or '_Not written yet._'}"):
        await interaction.followup.send(part)


async def send(channel_id: int, text: str) -> None:
    if not channel_id:
        return
    channel = client.get_channel(channel_id) or await client.fetch_channel(channel_id)
    if isinstance(channel, discord.abc.Messageable):
        for part in chunks(text):
            await channel.send(part)


async def post_loop_if_due() -> None:
    """Every LOOP_EVERY landed ideas, share the current gameplay loop (#153)."""
    if LOOP_EVERY <= 0 or state.get("done_since_loop", 0) < LOOP_EVERY:
        return
    try:
        text = await gh.gameplay_loop()
    except Exception:  # noqa: BLE001
        log.exception("fetching the gameplay loop failed")
        return
    if text:
        landed = state["done_since_loop"]
        await send(
            ANNOUNCE_CHANNEL_ID,
            f"🔁 **Where the game is at** ({landed} new ideas since last time)\n\n{text}",
        )
    state["done_since_loop"] = 0


@tasks.loop(minutes=2)
async def announce() -> None:
    try:
        done = await gh.issues("done", "all", 20)
        questions = await gh.issues("question", "open", 10)
    except Exception:  # noqa: BLE001
        log.exception("poll failed")
        return
    for issue in reversed(done):
        n = issue["number"]
        if n in state["announced_done"]:
            continue
        state["announced_done"].append(n)
        state["done_since_loop"] = state.get("done_since_loop", 0) + 1
        channel = state["issue_channels"].get(str(n), ANNOUNCE_CHANNEL_ID)
        await send(
            channel,
            f"✅ **{issue['title']}** is in the game! {issue['html_url']}\n"
            f"New build in ~15 min: {RELEASE_URL}",
        )
    for q in reversed(questions):
        n = q["number"]
        if n in state["announced_questions"]:
            continue
        state["announced_questions"].append(n)
        await send(
            ANNOUNCE_CHANNEL_ID,
            f"❓ **Claude asks:** {q['title']}\n{q.get('body') or ''}\n"
            f"Reply with `/answer {n} <your answer>`",
        )
    await post_loop_if_due()
    save_state(state)


@announce.before_loop
async def seed_announced() -> None:
    """On first ever start, don't spam announcements for old issues."""
    await client.wait_until_ready()
    if state.get("seeded"):
        return
    try:
        state["announced_done"] = [i["number"] for i in await gh.issues("done", "all", 100)]
        state["announced_questions"] = [
            i["number"] for i in await gh.issues("question", "all", 100)
        ]
        state["seeded"] = True
        save_state(state)
    except Exception:  # noqa: BLE001
        log.exception("seeding failed")


@client.event
async def on_ready() -> None:
    await tree.sync()
    if not announce.is_running():
        announce.start()
    log.info("logged in as %s", client.user)


def main() -> None:
    asyncio.run(client.start(DISCORD_TOKEN))


if __name__ == "__main__":
    main()
