# CI/build speed — workstream S8 handoff (nightly hardening)

Branch `claude/nightly-ci`. All edits are workflow/YAML + test-file moves; no
game logic. Each numbered edit is its own commit so anything can be reverted
alone (commit list at the bottom).

## Measurements

Local baseline (macOS, shared `target/`, before any edit; `CARGO_INCREMENTAL=0`):

| command | wall clock |
|---|---|
| `cargo clippy -p friendslop --all-targets --locked` (warm) | 1m 44s |
| `cargo test -p friendslop --test it data_files --locked` (after `touch` of `crates/game/src/lib.rs`) | 5m 17s — 5m of it is rebuilding the lib and linking the `it` binary; the tests ran in 0.04s |
| `cargo test -p friendslop --test data_files_check --locked` (after `touch`, same) | 3m 47s — same rebuild, no `it` link |

The gap is the whole point of the data-test relocation: the data job no longer
links the game test binary.

**CI wall-clock baselines: TODO on the first CI run of this PR** — record each
job's duration from the GitHub Actions UI (cold vs warm cache) in the PR
description and compare after a few main runs. Per-job things to watch:
`test-it` (the it binary link dominates), `web` (wasm32 clippy), `data`
(should drop to a few minutes warm), `web.yml` (wasm-bindgen-cli install
should vanish on a Cargo.lock-stable cache hit).

## Job map

Before (ci.yml):

| job | trigger | notes |
|---|---|---|
| changes | always | outputs `rust`, `data` |
| lint | rust changes | fmt + clippy workspace, cache `key: lint` |
| test | rust changes | `cargo test --workspace --locked`, cache `key: test` |
| web | rust changes | wasm32 clippy of friendslop, cache `key: wasm` |
| data | data-only changes | world tests + fusion score + `--test it data_files` |
| ci-ok | always | aggregates the above |

After (ci.yml):

| job | trigger | notes |
|---|---|---|
| changes | always | outputs `rust`, `data`, **`wasm`** (own file list) |
| lint | rust changes | unchanged commands, cache `shared-key: linux` |
| **test-foundation** | rust changes | nextest: fusion, core, wire, kcc, wfc, structures, art, data, world |
| **test-game** | rust changes | nextest: base, play, places, village, story, afterlife, side + `friendslop --lib` |
| **test-it** | rust changes | nextest `friendslop --test it --test data_files_check`, then `cargo test --doc`, then the process-shared suspects |
| web | **wasm changes** | wasm32 clippy, cache `shared-key: linux` |
| data | data-only changes | `friendslop-world`, `fusion score_file`, **`friendslop --test data_files_check`** |
| ci-ok | always | aggregates the above (same single required check) |

Other workflows: `web.yml` caches `~/.cargo/bin/wasm-bindgen*` keyed
`wasm-bindgen-${{ hashFiles('Cargo.lock') }}` and builds with
`WEB_TEXTURE_MAX: "256"`; `web-release.yml` gets the same binary cache and
web.yml's version-check fallback (still `WEB_TEXTURE_MAX` default 512 — the
release zip is the high-quality artifact; change it if the two should match).
`claude.yml` restores the shared cache (`shared-key: linux`) but never saves.

`release.yml` untouched.

## Where the tests went

- `every_data_file_parses`: `crates/game/tests/it/data_files.rs` →
  `crates/game/tests/data_files_check.rs` (standalone test target, links the
  friendslop lib, reads `crates/game/assets/` itself). Auto-discovered by
  cargo, no `[[test]]` stanza needed.
- `there_is_a_name_for_every_villager`: → `crates/village/src/features/
  villagers.rs` `mod tests` (where `NAMES`/`COUNT` live, via
  `crate::rules::villagers`).
- `crates/game/tests/it/data_files.rs` is gone; `mod data_files;` removed from
  `tests/it/main.rs`. One home per test.

## Judgment calls (differs from the literal plan — say so if wrong)

1. **wasm-bindgen cache path.** The plan said `~/.cargo/bin/wasm-bindgen-cli`;
   that is the *crate* name. `cargo install wasm-bindgen-cli` puts binaries
   named `wasm-bindgen*` in `~/.cargo/bin`, so the cache globs
   `~/.cargo/bin/wasm-bindgen*` (otherwise the cache would always be empty).
2. **claude.yml `save-if: false`**, not `github.ref == 'refs/heads/main'`:
   issue/issue_comment/workflow_run events carry `github.ref` = the default
   branch, so the main-only expression would still save every run — exactly
   the bug being fixed. CI's main runs write the cache; claude just restores.
3. **data job cache** switched from `key: test` to `shared-key: linux` too
   (still `save-if: false`), otherwise it would restore a key nobody writes
   anymore. The wasm check job uses the same shared key; its `RUSTFLAGS`
   (`--cfg getrandom_backend="wasm_js"`) hashes differently in rust-cache's
   env-key, so wasm artifacts stay in their own cache entry — intended.
4. **`data_files_check` runs in test-it too**, not only in the data job: the
   data job is skipped whenever Rust changes, so without it a Rust change to a
   Tuning struct + a .ron file in one PR would never parse-check the data.
5. **Escape-hatch filters** `navmesh place_looks playthrough` are OR'd (checked
   empirically: libtest takes multiple filter args). They live in test-it,
   where the it binary is built anyway.

## nextest caveat (commit 3, revert it alone if trouble)

nextest runs **each test in its own process**. Tests leaning on process-shared
state — `terrain::HEIGHTS` statics, env vars, other singletons — can flake or
beef under isolation even when they pass under plain `cargo test` (which
threads them in one process). `.config/nextest.toml` pins `retries = 0`
(a retry hides flakes), `fail-fast = false`, `slow-timeout = "60s"`. The
"process-shared suspects" step re-runs `navmesh place_looks playthrough`
threaded as a hedge; if nextest misbehaves elsewhere, either add the offender
to that step or revert commit `b7c4de8` (and its split follow-through) back to
`cargo test --workspace --locked`.

## Follow-up (not done tonight): Bevy `2d` feature cut

Separate PR, never part of a CI-speed bundle. Today the workspace has
`bevy = { version = "=0.20.0", features = ["wav"] }` (default features on,
`2d` among them) and the grep-guard is clean:

```sh
rg 'bevy::sprite|bevy_sprite|Mesh2d|TextureAtlas' --glob '*.rs'   # must be empty
```

Procedure when it happens:

1. Grep-guard first (above). Any hit stops the cut.
2. In the root `Cargo.toml`, change the bevy line to
   `default-features = false` and list the wanted features minus `2d`
   (keep `wav` and every feature the game needs — read `cargo tree -e features
   -i bevy` before choosing).
3. Verify: theme/menu picking (`Hovered`), Gizmos (`pets.rs`), vorbis/ogg
   audio, and `cargo tree -e features -i bevy_sprite_render` (avian or
   another dep may re-enable `2d` through feature unification — if so the cut
   is a no-op and the PR is just documentation).
4. `cargo run -- host` smoke + `cargo test --workspace` + wasm clippy.
5. Rollback = revert the two dep lines (root `Cargo.toml` bevy line and any
   second line changed in step 2).

## GitHub hygiene note (not ours to touch)

Branches `web-multiplayer-106` / `claude/web-mp-tested` (plus
`origin/claude/issue-106-web-multiplayer`) rework `net_web.rs`/`net_wire.rs`
and `.github/` from before `net_web.rs` landed on main; no open PRs reference
them (open PRs tonight: #399, #397 — untouched). They look superseded:
recommend closing/deleting after confirming nothing in their diffs is
unlanded (`crates/game/tests/web_wire_format.rs` history is the one to check),
or reconciling the wire tests deliberately. Left alone tonight.

## Commit map

| commit | edit |
|---|---|
| `d1c8f44` | 1: wasm-bindgen-cli binary cache (web.yml, web-release.yml) |
| `c9e93ef` | 2: cache consolidation `shared-key: linux` (ci.yml, claude.yml) |
| `b7c4de8` | 3: nextest + .config/nextest.toml + doctests + suspects step (revert alone if flakes) |
| `7cd5717` | 4: data-test relocation (data_files_check.rs, villagers.rs test, data job) |
| `b04a3ae` | 5: split test job (test-foundation / test-game / test-it → ci-ok) |
| `ff4a176` | 6: wasm gating (changes output `wasm`, web job condition) |
| `93d06f2` | 7: WEB_TEXTURE_MAX 256 on web.yml build |
| (this file) | 8: handoff + Bevy 2d-cut procedure |

## Verification run before push

- `ruby -ryaml` parse of every edited workflow (PyYAML unavailable here).
- `cargo test -p friendslop --test data_files_check --locked` — pass.
- `cargo test -p friendslop-village --lib there_is_a_name --locked` — pass.
- Full gate (serialized): `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`.
