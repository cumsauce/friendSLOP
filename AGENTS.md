# AGENTS.md

friendSLOP is a game **designed by a group of friends through GitHub issues and built only by AI
agents**. Nobody coordinates. Ideas arrive one at a time, often contradicting each other. Your job is
to turn each one into working, merged code while keeping the game coherent, fun and building.

If you're running in CI, also read `.github/claude/run.md`.

## Hard requirements (never break these)

1. **Multiplayer, host-as-server.** One player hosts (listen server), the others join with a code.
   No dedicated server and no infrastructure we'd have to run. Transport: iroh P2P (`src/net.rs`).
2. **Voice chat works** for everyone in a session (`src/voice.rs`, `bevy_voice_chat`).
3. **3D**, Bevy **0.20**, physics with **avian3d**.
4. **Some shared goal** the players work towards together. *What* it is comes from the friends.
5. **CI stays green** on Linux, macOS, Windows and the Nix flake (PRs only run Linux + Nix; macOS and
   Windows run after merge on `main`, so avoid platform-specific code). Run fmt, clippy (`-D warnings`)
   and tests before you push.

## Interpreting ideas

- The friends design the game; you implement it. **Don't invent beyond what an issue asks for.**
  When an idea is vague, choose boldly but minimally, then say what you chose in the PR and the
  issue comment.
- Never delete or neuter someone else's feature to make room. Reconcile (see `.github/claude/run.md`).
- Placeholder art is fine and expected: primitives, flat colors, generated meshes. Don't add large
  binary assets. If a sound or model is needed, generate it procedurally or keep it tiny.
- Keep `GAME_DESIGN.md` truthful: what exists, what friends said, open questions. It's the game's
  memory between runs.

## Nudging

The hint block in `.github/ISSUE_TEMPLATE/idea.yml` (between `CLAUDE-HINTS-START/END`) and the
"Open questions" in `GAME_DESIGN.md` are how we steer people towards what still needs ideas. Keep the
hints to 1-3 short lines naming *areas* that are thin ("nobody has said how a round ends"), never
specific suggestions. The Discord bot's `/whatsneeded` reads the Open questions section.

## Architecture

```
crates/game/src/
  lib.rs        AppState (Menu/InGame), ProtocolPlugin (shared registration), GamePlugin
  main.rs       DefaultPlugins + RepliconPlugins + PhysicsPlugins + GamePlugin; `host` / `join <code>` CLI
  net.rs        iroh host/join, NetStatus, clipboard join codes
  player.rs     server spawns a replicated `Player { id }` per connection (id 0 = host)
  voice.rs      voice chat driving (M = mute); speakers tagged on Player entities
  menu.rs       placeholder keyboard menu
  world.rs      camera, light, floor, HUD
  features/     one module per idea, see features/README.md
crates/game/tests/   headless multiplayer tests (replicon test_app, no sockets)
```

### Networking rules (bevy_replicon 0.45)

- The server is authoritative. Gameplay state lives on server entities with `Replicated` and is
  registered with `app.replicate::<C>()` inside a **protocol** function (see features/README.md).
  Registration order must match on every peer, so always append and never reorder.
- Clients send intent (input, requests) with `add_client_message`/`add_client_event`, and the
  server validates it and changes state.
- The host is a client too: listen-server code must work when `ServerState::Running` with no remote
  `ClientState`. Guard server logic with `run_if(in_state(ServerState::Running))`, and remember
  the host has `Player { id: 0 }` with no client entity.
- Test multiplayer behavior headlessly with `bevy_replicon::test_app::ServerTestAppExt`, as in
  `tests/replication.rs`.

### Bevy conventions

- Bevy 0.20 APIs only. Load the `bevy` skill (bevy-skills plugin) when unsure. Don't guess old APIs.
- One plugin per module. States for flow, `DespawnOnExit` for cleanup, observers for reactions,
  `Message`s for buffered events.
- Every use of BSN (`bsn!` macro or a `.bsn` asset) must be "salted" (#9): a nearby comment that
  mentions not needing `Cargo.lock`, x didn't set us up for success, we need more SME's, we
  haven't considered the problem space enough, BSN syntax needs more changes, `Children` sugaring
  and Sam "Salt" Saltman. `tests/bsn_salting.rs` enforces it and has an example.
- No `unwrap()`/`expect()` in systems. Use `let Ok(..) = .. else { return };`.
- Pinned versions: `bevy =0.20.0` and `bevy_replicon =0.45.2` (required by bevy_voice_chat). Only
  bump them together, and only when every dependency supports the new version.

## Commands

```sh
nix develop                     # toolchain + native libs (or install Rust + the apt libs in ci.yml)
cargo run -- host               # host; join code is printed and copied
cargo run -- join <code>        # join from a second instance
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
nix flake check
```
