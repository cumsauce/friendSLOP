#!/usr/bin/env python3
"""friendSLOP idea bot: Discord -> GitHub issues, and back.

Commands:
  /idea          open a form; files a GitHub issue labeled `idea`
  /answer        reply to one of Claude's `question` issues
  /whatsneeded   what the game still needs ideas for
  /status        open ideas and where they're at
  /loop          the current gameplay loop, start to finish

It also posts in ANNOUNCE_CHANNEL_ID when an idea lands in the game, when Claude asks the
group a new question, every LOOP_EVERY landed ideas the gameplay loop (#153), and every day at
noon (REMINDER_TZ) each question nobody has answered yet, in big text (#157). Every AVATAR_HOURS
it redraws its own profile picture from the game's models and the current status (#170).

Screenshots (#167): Claude pushes a picture of each landed idea to the `screenshots` branch as
`issue-<N>.png`. The "in the game" post attaches it, and when a new `latest` release comes out
the bot announces it with the pictures of the ideas that landed since the last one.

Env: DISCORD_TOKEN, GITHUB_TOKEN, GITHUB_REPO (owner/name), ANNOUNCE_CHANNEL_ID (optional),
STATE_PATH (optional, default /data/state.json), LOOP_EVERY (optional, default 3; 0 = never),
REMINDER_TZ (optional IANA zone for the noon reminder, default UTC), AVATAR_HOURS (optional,
default 3; 0 = never touch the profile picture), BLOCKED_USERS (optional, comma-separated Discord
user IDs to shadowban on top of BLOCKED below), BATCH_MINUTES (optional, default 30; 0 = post
right away): announcements are collected and posted together at most this often.
"""

import asyncio
import datetime
import io
import json
import logging
import os
import re
import time
from pathlib import Path
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

import aiohttp
import discord
from discord import app_commands
from discord.ext import tasks

import avatar

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
AVATAR_HOURS = float(os.environ.get("AVATAR_HOURS", "3") or 0)
# The game's assets live in Git LFS; GitHub serves their contents from this host (#170).
ASSET_BASE = f"https://media.githubusercontent.com/media/{REPO}/main/crates/game/assets/"
SCREENSHOT_BRANCH = "screenshots"
# Discord allows 10 attachments per message; keep release posts readable.
MAX_RELEASE_SHOTS = 6
BATCH_MINUTES = int(os.environ.get("BATCH_MINUTES", "30") or 0)

# Shadowbanned Discord user IDs: /idea and /answer look like they worked, but nothing reaches
# GitHub. Remove an ID here (or from BLOCKED_USERS) to lift it.
BLOCKED = {
    241786240863698944,
}
BLOCKED |= {
    int(x) for x in os.environ.get("BLOCKED_USERS", "").replace(" ", "").split(",") if x.isdigit()
}


def is_blocked(user: discord.abc.User) -> bool:
    return user.id in BLOCKED


def reminder_zone(name: str) -> datetime.tzinfo:
    """The zone whose noon the daily question reminder fires at; UTC if unknown (#157)."""
    try:
        return ZoneInfo(name)
    except (ZoneInfoNotFoundError, ValueError):
        log.warning("unknown REMINDER_TZ %r, using UTC", name)
        return datetime.timezone.utc


NOON = datetime.time(12, 0, tzinfo=reminder_zone(os.environ.get("REMINDER_TZ", "UTC") or "UTC"))


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

    def http(self) -> aiohttp.ClientSession:
        if self.session is None:
            self.session = aiohttp.ClientSession(
                headers={
                    "Authorization": f"Bearer {GITHUB_TOKEN}",
                    "Accept": "application/vnd.github+json",
                    "User-Agent": "friendslop-idea-bot",
                }
            )
        return self.session

    async def request(self, method: str, url: str, **kwargs):
        async with self.http().request(method, url, **kwargs) as resp:
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

    async def count(self, query: str) -> int:
        """How many issues in the repo match a search `query`."""
        found = await self.request(
            "GET",
            "https://api.github.com/search/issues",
            params={"q": f"repo:{REPO} is:issue {query}", "per_page": 1},
        )
        return int(found.get("total_count", 0)) if isinstance(found, dict) else 0

    async def status(self) -> avatar.Status:
        """Ideas landed, being built, waiting and stuck, for the profile picture (#170)."""
        return avatar.Status(
            landed=await self.count("label:done"),
            building=await self.count("is:open label:in-progress"),
            waiting=await self.count(
                "is:open label:idea -label:in-progress -label:done -label:needs-human -label:wontfix"
            ),
            stuck=await self.count("is:open label:needs-human"),
        )

    async def asset(self, path: str) -> bytes | None:
        """A file from crates/game/assets/, or None if it can't be fetched right now."""
        try:
            async with self.http().get(ASSET_BASE + path) as resp:
                return await resp.read() if resp.status == 200 else None
        except aiohttp.ClientError:
            return None

    async def get_issue(self, number: int) -> dict:
        return await self.request("GET", f"{API}/issues/{number}")

    async def screenshot(self, number: int) -> bytes | None:
        """The picture Claude took of idea #number (#167), or None if there isn't one."""
        try:
            return await self.raw(f"{API}/contents/issue-{number}.png", ref=SCREENSHOT_BRANCH)
        except Exception:  # noqa: BLE001
            return None

    async def raw(self, url: str, **params) -> bytes:
        async with self.http().get(
            url, params=params, headers={"Accept": "application/vnd.github.raw+json"}
        ) as resp:
            if resp.status >= 400:
                raise RuntimeError(f"GitHub {resp.status}")
            return await resp.read()

    async def latest_release(self) -> dict | None:
        try:
            return await self.request("GET", f"{API}/releases/tags/latest")
        except Exception:  # noqa: BLE001
            return None

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
        if is_blocked(interaction.user):
            log.info("shadowbanned idea from %s dropped", interaction.user.id)
            await interaction.followup.send(
                f"💡 **{self.idea_title.value}** → https://github.com/{REPO}/issues\n"
                "Claude will pick it up shortly. I'll post here when it's in the game."
            )
            return
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
        if is_blocked(interaction.user):
            log.info("shadowbanned answer from %s dropped", interaction.user.id)
        else:
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


async def send(channel_id: int, text: str, images: list[tuple[str, bytes]] | None = None) -> None:
    """Post text (split to fit), with any images attached to the last part."""
    if not channel_id:
        return
    channel = client.get_channel(channel_id) or await client.fetch_channel(channel_id)
    if isinstance(channel, discord.abc.Messageable):
        parts = chunks(text)
        for i, part in enumerate(parts):
            if i == len(parts) - 1 and images:
                files = [discord.File(io.BytesIO(data), filename=name) for name, data in images]
                await channel.send(part, files=files)
            else:
                await channel.send(part)


def release_text(release: dict, landed: list[dict]) -> str:
    """The new-build announcement (#167): what's in it since the last one."""
    lines = [f"🚀 **New build is out!** {release.get('html_url') or RELEASE_URL}"]
    if landed:
        lines.append("New since last time:")
        lines += [f"• {i['title']}" for i in landed]
    return "\n".join(lines)


async def announce_release_if_new() -> None:
    """When the `latest` release is rebuilt, say so, with the new ideas' pictures (#167)."""
    release = await gh.latest_release()
    if not release or release.get("draft"):
        return
    rid = release.get("id")
    if rid == state.get("announced_release"):
        return
    first = "announced_release" not in state
    state["announced_release"] = rid
    landed = state.get("landed_since_release", [])
    state["landed_since_release"] = []
    if first:
        return  # first start with this feature: don't announce an old build
    images = []
    for item in landed[-MAX_RELEASE_SHOTS:]:
        shot = await gh.screenshot(item["number"])
        if shot:
            images.append((f"issue-{item['number']}.png", shot))
    await send(ANNOUNCE_CHANNEL_ID, release_text(release, landed), images)


def queue(channel_id: int, text: str, shot: int | None = None) -> None:
    """Hold an announcement for the next batch (sent by flush_batch). `shot` is an idea whose
    screenshot (#167) goes with it."""
    if channel_id:
        state.setdefault("pending", []).append(
            {"channel": channel_id, "text": text, "shot": shot}
        )


async def flush_batch(force: bool = False) -> None:
    """Post everything queued, one message per channel, at most every BATCH_MINUTES."""
    pending = state.get("pending") or []
    if not pending:
        return
    now = time.time()
    if not force and now - state.get("last_flush", 0) < BATCH_MINUTES * 60:
        return
    by_channel: dict[int, list[dict]] = {}
    for item in pending:
        by_channel.setdefault(item["channel"], []).append(item)
    state["pending"] = []
    state["last_flush"] = now
    save_state(state)
    for channel, items in by_channel.items():
        texts = [i["text"] for i in items]
        if any(t.startswith("✅") for t in texts):
            texts.append(f"New build in ~15 min: {RELEASE_URL}")
        images = []
        # Discord allows 10 attachments per message.
        for n in [i["shot"] for i in items if i.get("shot") is not None][:10]:
            picture = await gh.screenshot(n)
            if picture:
                images.append((f"issue-{n}.png", picture))
        try:
            await send(channel, "\n\n".join(texts), images or None)
        except Exception:  # noqa: BLE001
            log.exception("posting a batch to %s failed", channel)


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
        queue(
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
        state.setdefault("landed_since_release", []).append(
            {"number": n, "title": issue["title"]}
        )
        channel = state["issue_channels"].get(str(n), ANNOUNCE_CHANNEL_ID)
        queue(
            channel,
            f"✅ **{issue['title']}** is in the game! {issue['html_url']}",
            shot=n,
        )
    for q in reversed(questions):
        n = q["number"]
        if n in state["announced_questions"]:
            continue
        state["announced_questions"].append(n)
        queue(
            ANNOUNCE_CHANNEL_ID,
            f"❓ **Claude asks:** {q['title']}\n{q.get('body') or ''}\n"
            f"Reply with `/answer {n} <your answer>`",
        )
    await announce_release_if_new()
    await post_loop_if_due()
    save_state(state)
    await flush_batch()


def unanswered(questions: list[dict]) -> list[dict]:
    """Open questions nobody has replied to yet (#157). Any comment counts as an answer."""
    return [q for q in questions if not q.get("comments") and "pull_request" not in q]


def reminder_text(q: dict) -> str:
    """A big (Discord heading) re-ask of a question that's still unanswered (#157)."""
    body = (q.get("body") or "").strip()
    return (
        f"# ❓ Still unanswered: {q['title']}\n"
        + (f"## {' '.join(body.split())}\n" if body else "")
        + "Nobody has answered this yet, so I'll ask again every day at noon until someone does.\n"
        f"Reply with `/answer {q['number']} <your answer>` → {q['html_url']}"
    )


@tasks.loop(time=NOON)
async def remind_questions() -> None:
    """Every day at noon, ask each unanswered question again, loudly (#157)."""
    try:
        questions = await gh.issues("question", "open", 30)
    except Exception:  # noqa: BLE001
        log.exception("fetching questions for the reminder failed")
        return
    texts = [reminder_text(q) for q in reversed(unanswered(questions))]
    if texts:
        await send(ANNOUNCE_CHANNEL_ID, "\n\n".join(texts))


avatar_assets: dict[str, bytes] = {}


@tasks.loop(hours=AVATAR_HOURS if AVATAR_HOURS > 0 else 3)
async def refresh_avatar() -> None:
    """Redraw the bot's profile picture from the game's models when the status changed (#170)."""
    if AVATAR_HOURS <= 0 or client.user is None:
        return
    try:
        status = await gh.status()
    except Exception:  # noqa: BLE001
        log.exception("fetching the status for the avatar failed")
        return
    key = [status.landed, status.building, status.waiting, status.stuck]
    if state.get("avatar_status") == key:
        return
    for name, path in avatar.ASSETS.items():
        if name not in avatar_assets and (data := await gh.asset(path)):
            avatar_assets[name] = data
    png = await asyncio.to_thread(avatar.render, avatar_assets, status)
    try:
        await client.user.edit(avatar=png)
    except discord.HTTPException as e:
        log.warning("could not change the avatar (rate limited?): %s", e)
        return
    log.info("new avatar for %s", key)
    state["avatar_status"] = key
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
    if not remind_questions.is_running():
        remind_questions.start()
    if not refresh_avatar.is_running():
        refresh_avatar.start()
    log.info("logged in as %s", client.user)


def main() -> None:
    asyncio.run(client.start(DISCORD_TOKEN))


if __name__ == "__main__":
    main()
