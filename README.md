# friendSLOP

![The commune, Wendy's, the windmill, the maze, the warehouse, the shipwreck and the village](docs/banner.jpg)

A multiplayer game that **nobody is designing**. Friends throw ideas over the wall, Claude builds
them, and whatever passes CI ships. Expect chaos.

- 🌐 **Play in your browser:** **https://cumsauce.github.io/friendSLOP/**, rebuilt automatically
  from every change to `main` (allow the mic for voice chat).
- 🎮 **Play:** grab the build for your OS from [Releases → latest](../../releases/tag/latest).
  One person presses **H** to host (a join code is copied to their clipboard) and shares the code.
  Everyone else copies it and presses **J**. Talk with your mic, **M** to mute.
- 💡 **Add an idea:** see below.
- 🤖 **How it gets built:** [AGENTS.md](AGENTS.md) · current state: [GAME_DESIGN.md](GAME_DESIGN.md)

## How to add an idea

- **On Discord:** type `/idea`. Use `/whatsneeded` to see what the game still lacks, and `/answer` to
  reply to Claude's questions.
- **On GitHub:** [open an idea issue](../../issues/new?template=idea.yml).

Any size is fine: "there should be a frog", "the floor is lava on Tuesdays", "we're all trying to
build a rocket". Claude interprets it, builds it, and tells you how it turned out.

## Developing

```sh
nix develop          # or: Rust stable + (Linux) libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev
cargo run -- host
cargo run -- join <code>
```
