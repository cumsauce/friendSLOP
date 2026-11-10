# Admin to-do (#232)

Things the AI runs **can't** do themselves. They need the repo owner, an admin, or someone
outside the repo (the Discord server, the bot's host, a real wallet). Gathered on 2026-11-10.
Tick things off or delete them as they're done. Runs only add to this list when something new
gets stuck.

## Stuck issues (labeled `needs-human`)

- [ ] **#225 "Take a break from coding and donate $10 to the food bank."** Runs can't spend
      real money. A person has to donate (or close it). If it was meant as an in-game food
      bank, file that as a new idea.
- [ ] **#182 "add a horror arg".** Five runs picked it up and none produced a PR, and the logs
      don't say why. Either look at those run logs, or ask catherine for a narrower first step
      (one specific scary moment). Remove `needs-human` to put it back in the queue.
- [ ] **#226 "improve the ui".** Labeled `needs-human` with no comment saying why. The HUD,
      title screen and settings were redesigned since (#219, #220, #221). Decide whether that
      covers it (close it), or remove the label so a run picks it up again.

## Repo settings, secrets and accounts

- [ ] **`BOT_PAT` secret.** Every Claude run, PR, merge and the lockfile workflow use it. When
      it expires everything stops quietly. Check its expiry date and renew it ahead of time.
- [ ] **`CLAUDE_CODE_OAUTH_TOKEN` secret.** Same: the Claude runs die without it.
- [ ] **`CLAUDE_MAX_PARALLEL` variable** (optional, default 6). It caps how many ideas get
      built at once. Raise or lower it under Settings → Variables.
- [ ] **GitHub Pages** is set to "GitHub Actions" and the first web deploy (#230) is running.
      Check that https://cumsauce.github.io/friendSLOP/ loads once the Web workflow finishes.
- [ ] **Git LFS quota.** Models, sounds and fonts are in LFS, and CI, the web deploy, releases
      and screenshot runs all pull them. Watch the LFS bandwidth/storage under Billing so
      pulls don't start failing.
- [ ] **Branch protection** is off on purpose (runs merge their own PRs without waiting for
      CI). Leave it that way, or runs can't land anything.

## Releases

- [ ] **Run the Release workflow** (Actions → Release → Run workflow). It only runs by hand.
      The last `latest` build is from 2026-11-10 05:33 UTC. Since then the UI redesign
      (#219-#221), gibberish voices (#222), browser voice chat (#223) and the seeded world
      (#224, #228) have landed. The Discord bot posts "New build is out" when it's rebuilt.

## Discord bot (`tools/idea-bot`)

- [ ] **Redeploy the bot** wherever it runs, after the image
      `ghcr.io/cumsauce/friendslop-idea-bot:latest` is rebuilt. CI builds the image but
      doesn't restart the running bot. Last bot changes: shadowban + batched posts (#179),
      screenshots (#178) and the profile picture (#172).
- [ ] **Discord permissions.** The bot needs "Attach Files" in the announce channel, or posts
      go out without screenshots.
- [ ] **Bot env vars** (`DISCORD_TOKEN`, `GITHUB_TOKEN`, `ANNOUNCE_CHANNEL_ID`, optional
      `REMINDER_TZ`, `LOOP_EVERY`, `BATCH_MINUTES`, `AVATAR_HOURS`, `BLOCKED_USERS`) live on
      the bot's host, not in the repo. Its fine-grained GitHub PAT expires too, so renew it.

## Upstream (outside the repo)

- [ ] **rerecast navmesh fix.** `vendor/rerecast/` carries a one-line patch because stock
      rerecast 0.5.0 can't build our map's navmesh (upstream issues #59/#64, fix PR #60 closed
      unmerged). Someone could push upstream to land it, then the vendored copy can go.
- [ ] **Bevy / replicon versions.** We're pinned to `bevy =0.20.0` + `bevy_replicon =0.45.2`
      because `bevy_voice_chat` needs them. Upgrading waits on that crate's maintainers.

## Needs someone with a real machine

- [ ] **Play it in a window.** Nobody has watched the east village (stairs, tower ramps),
      walking between districts, or the frame rate on the bigger map in a real game. CI
      screenshots use software rendering. (From `HANDOFF.md`.)
- [ ] **Measure frame time.** Last p99 was ~25-30 ms. Re-run
      `FRIENDSLOP_BENCH=1 cargo run --release -- host` on an idle machine with a GPU. It
      failed last time with a toolchain libLLVM error. This also helps #227 (performance).
- [ ] **Phone check.** #226 and #227 both ask for mobile. Someone should open the web build on
      a phone and report what's broken.
