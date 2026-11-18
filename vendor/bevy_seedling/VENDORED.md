# Vendored bevy_seedling

The game's audio graph (`crates/base/src/sound.rs`). Upstream bevy_seedling
0.8 targets Bevy 0.19, so this is the Bevy 0.20 port from
<https://github.com/CorvusPrudens/bevy_seedling/pull/137>:

- fork: `eswartz/bevy_seedling-fork`, branch `migrate-to-bevy-0.20`
- commit: `56e710c04b818189ae064c918150e1ed7a4d44be` ("Migrate to Bevy 0.20.")
- firewheel comes from crates.io (0.15.0 already targets Bevy 0.20).

License: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`, kept as shipped).

## Our changes

- `Cargo.toml`: dropped dev-dependencies, benches, docs metadata and the
  in-repo `firewheel-ircam-hrtf` (`hrtf`) and `firewheel-web-audio`
  (`web_audio`) subcrates, with their features. The game keeps its own HRTF
  (`friendslop-hrtf`), and the browser plays through cpal's Web Audio backend
  like rodio did (no threads needed).
- Removed the code behind those features (`src/platform/web_audio.rs`, the
  `hrtf` spatial system and prelude exports, the web-audio check in
  `platform/cpal.rs`); the mock backend is test-only.
- Not copied: `examples/`, `benches/`, `assets/`, `crates/`, `.github/`.

When upstream releases a Bevy 0.20 version, replace this directory with the
crates.io dependency and drop these notes.
