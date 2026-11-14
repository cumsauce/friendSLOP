# bugs-a handoff (bugfixer pass C+A, branch claude/nightly-bugs-a)

Scope: `crates/play/src/features/{duel,ragdoll,fate,stats,fps}.rs` (+ their data files,
untouched — the needed `fps.ron look_sensitivity` already existed). All fixes land with a
regression test and a `bugpass/fixes/<id>.md` report (untracked area). Worktree-only changes;
nothing outside `crates/play` and `crates/game/tests/it/` was touched.

## Fixed (8/8, one commit each)

| id | commit | regression test (red → green verified) |
|---|---|---|
| play:double-fought | fix(duel): one DuelFought per kill | `bugs_a::duel_shot_at_dead_victim_fires_no_second_fought` + `bugs_a::duel_two_shots_one_kill_fires_one_fought` |
| play:invites-window | fix(duel): guard survives the accept-to-spawn window | `duel::tests::accept_in_spawn_window_starts_one_duel` |
| play:invite-stale | fix(duel): prune stale invites (same commit as next two) | `duel::tests::invites_prune_departed` |
| play:control-spam | same commit | `duel::tests::duel_control_sent_on_change_only` |
| play:sensitivity | same commit | (reads fps.ron; covered by data-files test) |
| play:orphan-body | fix(ragdoll): body despawns with its player | `ragdoll::tests::ragdoll_body_despawns_with_player` |
| play:mishap-attrs | fix(fate): unknown clients blame no one | `bugs_a::unknown_client_mishap_not_player_zero` |
| play:mail-netting | fix(fate): the mail counts adds, not netted length | `fate::tests::mail_counts_adds_not_net` + `mail_memory_resets_with_the_session` |
| play:stats-guard | fix(stats): NewCampaign rerolls only on the running server | `stats::tests::reroll_only_server` |
| play:spawn-collide | fix(fps): one spawn slot per player | `fps::tests::spawn_points_unique_0_to_24` |

## Verified fixed on main (no code change; notes in bugpass/fixes/)

- **duel:nan** — `on_control` rejects non-finite stick/yaw/pitch (duel.rs:326-335).
- **duel:sameframe** — `invites.active` synchronous guard for same-batch accepts
  (duel.rs:278-283); strengthened across the spawn window by play:invites-window.
- **duel:anims** — `animate_duelists`/`pick_clip` one-shot clips with busy timers and
  delta bookkeeping (duel.rs:1018-1090). The delta-inference fragility vs late joiners
  belongs to clock:witness (server-authored anim events) and is still open there.

## Mechanism finding worth propagating (Bevy 0.20 flush semantics)

The S6 root-cause class ("deferred `Commands` despawn + same-batch handlers") needs a
nuance, verified by instrumenting `on_shoot` on this branch:

- **`commands.trigger` / queued triggers**: observer `Commands` (the despawns/spawns they
  queue) are applied before the next queued trigger command runs. Two same-batch
  `DuelShoot`s therefore do NOT both see the victim — the second sees zero duelists
  (`CommandQueue::apply` flushes the world's pending commands per command).
- **bare `world.trigger`**: observer commands stay pending until the next `World::flush()`
  / update.
- The hazard that IS real: any state claimed only by "the despawned entity is gone from the
  query" during the same batch — e.g. a handler that mutates through `Commands` while a
  query still sees the entity at `lives == 0` (my red test), or any future refactor of an
  observer into a message-reader loop (one system run iterating several messages shares one
  deferred flush — that shape reproduces the class for real).

So: guards in synchronous state (`lives == 0`, `DuelInvites.active`/`spawn_pending`,
`patch.eaten`-style flags) remain the right pattern — mushroom.rs's `patch.eaten` is the
canonical one — but a green same-batch test is not proof the handler is safe under all
flush semantics. Each fix here keeps the guard in synchronous state and the tests construct
the window state directly.

## Residual risk / notes

- **DuelControl change-only** (play:control-spam) trades a lost "stop input" packet for
  quiet frames on the unreliable channel; any mouse change re-syncs. If stuck-walk ever
  appears, add a periodic resend heartbeat to `DuelView::control`.
- **`tuning().lives.max(1)`** at duelist spawn keeps "alive ⇒ lives ≥ 1" true for the
  double-fought guard; `lives: 0` in duel.ron now means a one-life duel, not a stuck one.
- **Shared target dir**: one `cargo test` run failed a passing test with output identical
  to the pre-fix behavior (stale/clobbered binary while another agent built). Per
  HANDOFF.md: touch + rerun before trusting; the final gate below is under the build lock.
- My tests live in `crates/game/tests/it/bugs_a.rs` (registered in `main.rs`) and beside the
  features in `crates/play`. S1's races.rs names differ slightly
  (`duel_double_shoot_one_fought`, `ragdoll_body_dies_with_player`,
  `fate_unknown_client_never_blames_zero`) — when that branch merges, keep whichever names
  and fold duplicates; the behaviors locked are the same.

## Gate

`flock /Users/lume/friendSLOP/target/.build-lock` — fmt, clippy `-D warnings` (workspace,
all targets), `cargo test --workspace`, wasm clippy for `friendslop` — see the branch's last
commit status / PR.
