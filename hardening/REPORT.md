# Hardening run report — friendSLOP (overnight, multi-agent)

**Status: skeleton from the docs workstream (S11+S12+S13).** Everything this
branch did is filled in below (S11 asset hygiene, S12 modeler-TODO, S13
structure). The other workstreams land as separate branches with their own
notes in `hardening/{tests,bugs-a,bugs-b,bugs-c,arch,tools,ci}.md` — see
**Assembler note** at the end: the integrator folds those into this file.

## 1. What was done by workstream

### S11 Asset hygiene (this branch, `claude/nightly-docs`)

- **Pruned 5 stale orphan textures** (stale re-export content hashes no
  `.gltf` names): `textures/nyc-storefront-base_color-100f3240ec73af71.ktx2`,
  `textures/nyc-storefront-emissive-33b9d1fba315e72b.ktx2`, and the whole
  superseded `nyc-taxi-*` bake
  (`textures/nyc-taxi-{base_color-0c2f437a087f0b44,metal_rough-04326720e791712b,normal-ace062a4666b6db6}.ktx2`).
  Each verified unreferenced against `models/**/*.gltf`, `maps/*.gltf|json|bsn`,
  `crates/**/*.rs` and `assets/data/**.ron` before deletion. The live
  `nyc-storefront-*` hashes and `nyc-asphalt-*` (referenced by `maps/nyc.gltf`)
  were checked and kept. They were shipping in the web bundle (`web-textures`
  converts everything in `textures/`).
- **Junk deleted**: tracked `crates/.DS_Store`, `tools/.DS_Store` (`.gitignore`
  already covers them; they predate the ignore). `crates/game/assets/audio/.DS_Store`
  exists only **untracked** on main's dirty working tree — not in git, nothing to
  delete here; the hygiene test now fails if it ever gets added.
- **CREDITS.md enumerated + drift fixed** (`crates/game/assets/CREDITS.md`):
  added `models/hats/` (5), `models/kori/` (5), the 17 root-level area models
  (`high-places`, `ice-cream`, `miami-factory-town`, `out-of-order`,
  `penn-station`, `pigeon`, `plane-car`, `plane-gate`, 3× `nyc-boba-*`,
  6× `path-*`), the Fulton St `fs-*`/`surf-*`/`gta6-ad` features, `maps/`
  (7 files) and `probes/` (4 KTX2). Fixed the drift where the
  `models/scenery/` bullet claimed the woodcutter's hut, log pile, castle arch,
  quarry crane and windmill — they live in `models/prefabs/zn-*`, which now has
  its own bullet.
- **LFS coverage for terrain heights**: root `.gitattributes` gains
  `crates/game/assets/**/*.heights`; `maps/commune.ground.heights` migrated
  (renormalized to a 146 KB LFS object). Fresh clones fetch it via LFS like
  every other binary; `procedural_height_at` remains the fallback when it is
  missing (no-LFS clones degrade quietly).
- **New lint `crates/game/tests/it/asset_hygiene.rs`** (registered in
  `tests/it/main.rs`), 6 fast deterministic tests: orphan models (every
  `models/**/*.gltf` stem referenced from `crates/**/*.rs`,
  `assets/data/**.ron`, `assets/maps/*.json|*.bsn` or covered by a dynamic
  template — `models/features/{file}` loaders, `PROP_MODELS` parsed from
  `crates/world/src/buildings.rs:899`, `hs-*`/`wh-tile-*`/`fs-tile-*`/`nyc-kit-*`
  families), textures named by a `.gltf`, no junk under `assets/`, CREDITS
  covers every top-level group, and every binary-ish asset matched against the
  root `.gitattributes` `filter=lfs` patterns. `KNOWN_ORPHANS = ["nyc-kit-cruiser"]`
  and the test asserts it is the only one — placing or removing the cruiser
  flips the test until the seed shrinks.

### S12 Modeler-TODO (doc only)

`hardening/modeler-todo.md`: 11 model-work items this run deliberately did not
do, each with problem/fix/prefab-doc/export-command (P1: crow feet origin,
quarry crane jib; P2: watcher figure, broadcast TV, ice-cream parts, NYC trail
posts, tunnel/bath swatches, many-parts `merge: "by_material"` re-exports,
nyc-kit-cruiser place-or-drop; P3: `hs-*` trim coverage; CREDITS drift — done
here). Inventory reference paragraph included.

### S13 Report skeleton (this file)

Structure, findings register, registration-order snapshot, verification
commands, deferred GAME_DESIGN.md edits, GitHub status. `hardening/perf-budget.md`
is stubbed for S9's output.

## 2. Findings register

| id | finding | fixed/open/doc-only | evidence | link |
|---|---|---|---|---|
| S11-1 | 5 stale orphan textures shipped in the web bundle (re-export hash leftovers: 2× nyc-storefront, 3× nyc-taxi) | fixed | deleted in `chore(assets): prune stale textures`; each grep-verified unreferenced before deletion | this report §1 |
| S11-2 | `models/prefabs/nyc-kit-cruiser.gltf` is unplaced — the only orphan model | doc-only | `tests/it/asset_hygiene.rs` `KNOWN_ORPHANS`; `nyc.map.json` has no reference | `hardening/modeler-todo.md` |
| S11-3 | `maps/commune.ground.heights` (146 KB binary) not in any LFS pattern | fixed | `.gitattributes` `**/*.heights` line + renormalize; blob is now an LFS pointer | `chore(lfs): heights` |
| S11-4 | CREDITS.md enumeration gaps (hats, kori, root area models, fulton features, maps, probes) + drift (zn-* listed under models/scenery/) | fixed | `crates/game/assets/CREDITS.md` | `docs(credits): enumerate` |
| S11-5 | Tracked `.DS_Store` junk (`crates/`, `tools/`) | fixed | deleted; `.gitignore` already had `.DS_Store` | `chore(assets): prune stale textures` |
| S11-6 | `crates/game/assets/audio/.DS_Store` untracked on main's dirty tree | open (not ours) | not in git on `origin/main`; hygiene test would fail if added | this report §1 |
| S11-7 | Orphan/texture/junk/CREDITS/LFS hygiene had no test | fixed | `tests/it/asset_hygiene.rs` (6 tests, ~1 s) | `test(assets): hygiene lint` |
| S12-* | 11 model-work items (watcher, TV, ice-cream parts, trail posts, crow origin, crane jib, swatches, many-parts merges, hs-* trim, cruiser) | doc-only | see table | `hardening/modeler-todo.md` |
| S13-1 | GAME_DESIGN.md claims the web build deploys on every push to `main`; `web.yml` is `workflow_dispatch` | open (deferred) | GAME_DESIGN.md "Always online" sentence vs `.github/workflows/web.yml` | §8 |
| — | (other workstreams append their findings here when folded in) | | | `hardening/*.md` |

## 3. Doc-only items

- `hardening/modeler-todo.md` — the 11 model items above (this run made **no**
  model changes, by design).
- GAME_DESIGN.md owed edits — §8 (file is dirty on main; deferred).
- HANDOFF.md "Other known gaps" → `hardening/` pointer line (owed to the
  integrator; not added here to avoid colliding with other branches).

## 4. Registration-order full list

Verbatim from `crates/game/src/features/mod.rs` (`FeaturesProtocolPlugin`, the
append-only order every peer must match) as of this branch's tip:

```rust
app.add_plugins(organ_currency::protocol);
app.add_plugins(crows::protocol);
app.add_plugins(bear::protocol);
app.add_plugins(mushroom::protocol);
app.add_plugins(cat_palette::protocol);
app.add_plugins(duel::protocol);
app.add_plugins(dave_commune::protocol);
app.add_plugins(morality::protocol);
app.add_plugins(goat::protocol);
app.add_plugins(voice_lines::protocol);
app.add_plugins(stillness::protocol);
app.add_plugins(gods_tears::protocol);
app.add_plugins(fart::protocol);
app.add_plugins(fart_sound::protocol);
app.add_plugins(landlord::protocol);
app.add_plugins(fate::protocol);
app.add_plugins(stats::protocol);
app.add_plugins(totems::protocol);
app.add_plugins(pets::protocol);
app.add_plugins(halt::protocol);
app.add_plugins(objective::protocol);
app.add_plugins(fps::protocol);
app.add_plugins(interact::protocol);
app.add_plugins(hide_and_seek::protocol);
app.add_plugins(villagers::protocol);
app.add_plugins(campaign::protocol);
app.add_plugins(romance::protocol);
app.add_plugins(climb::protocol);
app.add_plugins(crate::people::protocol);
app.add_plugins(abuela::protocol);
app.add_plugins(sesbian_lex::protocol);
app.add_plugins(punishment::protocol);
app.add_plugins(helicopter::protocol);
app.add_plugins(warehouse::protocol);
app.add_plugins(wendys::protocol);
app.add_plugins(underground::protocol);
app.add_plugins(smoking::protocol);
app.add_plugins(caught::protocol);
app.add_plugins(affair::protocol);
app.add_plugins(gamble::protocol);
app.add_plugins(fat_cat::protocol);
app.add_plugins(gaslighting::protocol);
app.add_plugins(skills::protocol);
app.add_plugins(shipwreck::protocol);
app.add_plugins(mute_spell::protocol);
app.add_plugins(infinite::protocol);
app.add_plugins(kitty_lemonade::protocol);
app.add_plugins(discord_friends::protocol);
app.add_plugins(rats::protocol);
// The team strip's mic badges (UI direction E): everyone's mute state.
app.add_plugins(crate::voice::mic_protocol);
app.add_plugins(world_seed::protocol);
app.add_plugins(afterlife::protocol);
app.add_plugins(systemd::protocol);
app.add_plugins(watcher::protocol);
app.add_plugins(clues::protocol);
app.add_plugins(broadcast::protocol);
app.add_plugins(nyc::protocol);
app.add_plugins(crate::atmosphere::protocol);
app.add_plugins(nyc_traffic::protocol);
app.add_plugins(fulton::protocol);
app.add_plugins(fulton_hunt::protocol);
app.add_plugins(surf::protocol);
app.add_plugins(wallet::protocol);
app.add_plugins(trains::protocol);
app.add_plugins(ragdoll::protocol);
app.add_plugins(trains::protocol_runs);
app.add_plugins(miami::protocol);
app.add_plugins(boba::protocol);
app.add_plugins(vessel_jump::protocol);
app.add_plugins(high_places::protocol);
app.add_plugins(ice_cream::protocol);
app.add_plugins(hats::protocol);
app.add_plugins(hi::protocol);
app.add_plugins(kittens::protocol);
app.add_plugins(toilets::protocol);
app.add_plugins(emotes::protocol);
app.add_plugins(kori::protocol);
app.add_plugins(lily_garden::protocol);
```

If a folded-in workstream appended registrations, re-paste this list from
`crates/game/src/features/mod.rs` at merge time — it must be the final order.

## 5. Perf budget

→ `hardening/perf-budget.md` (stub; S9's `FRIENDSLOP_PERF=2` runs land here).

## 6. Asset hygiene results (this workstream)

- Orphan models: **1** — `nyc-kit-cruiser` (documented, seeded in
  `KNOWN_ORPHANS`; everything else is referenced).
- Orphan textures: **5 found, 5 deleted** (post-delete: every
  `textures/*.ktx2` is named by a `.gltf` — note `maps/nyc.gltf` names the
  `nyc-asphalt-*` bake, so any texture check must scan `maps/` too).
- Junk under `assets/`: none in git (and the lint keeps it that way).
- CREDITS: every top-level group named (`models/ audio/ data/ fonts/ maps/
  probes/ textures/ ui/`).
- LFS: every non-text file under `assets/` matches a `.gitattributes`
  `filter=lfs` pattern (`.heights` added this run).
- Test: `cargo test -p friendslop --test it asset_hygiene` — 6 passed.

## 7. Verification commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo clippy -p friendslop --target wasm32-unknown-unknown
cargo test -p friendslop --test it asset_hygiene   # the new lint
```

Serialized final gate (one builder at a time):

```sh
flock /Users/lume/friendSLOP/target/.build-lock sh -c \
  'cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace'
```

Every shell in the run used `CARGO_TARGET_DIR=/Users/lume/friendSLOP/target
CARGO_INCREMENTAL=0`, and `touch`ed a source before `cargo test` (shared target
dir; suspect binaries are rebuilt).

## 8. GAME_DESIGN.md owed edits (deferred)

The file is dirty on main (live NYC modeler exports) — **nobody touched it this
run**. Owed, for whoever edits it after that tree settles:

1. **Deploy-drift sentence** ("Always online", Browser/phone build section):
   it says every push to `main` rebuilds and deploys the web build
   (`.github/workflows/web.yml`). `web.yml` is actually `workflow_dispatch`
   (manual). Rewrite to: deploy on manual dispatch; what's on GitHub Pages can
   lag `main` until someone dispatches it.
2. Anything else the other workstreams note in their `hardening/*.md` files
   (the integrator collects them here).
3. The Gameplay loop section needs no rewrite from this workstream (no
   behaviour changed on this branch).

## 9. What we didn't do / risks

- **No model changes** (user directive): everything in
  `hardening/modeler-todo.md` is still outstanding.
- **No GAME_DESIGN.md edits** (dirty on main): §8.
- **Nothing under `crates/game/.modeler/` or `assets/maps/nyc.*`** (hot on
  main's working tree).
- `zones.rs` / `scenery.rs` untouched (other unmerged branches own them).
- No bugpass fix reports written here: this workstream fixed no gameplay bugs.
  If the bugs workstream landed fixes without `bugpass/fixes/<id>.md` reports,
  say so in the folded-in section (the pre-`dc81daa` bugpass fixes are the
  precedent).
- The `.heights` LFS migration renormalizes one tracked file; anyone with a
  dirty checkout of `commune.ground.heights` will see it re-fetched as a
  pointer — expected, not destructive.
- Stale-texture deletion is the only asset removal; `nyc-kit-cruiser.gltf` was
  intentionally **kept** (place-or-drop is a friend decision, in
  modeler-todo).

## 10. GitHub status

- **#368** ("fix ALL bugs!!!!!") — the umbrella for the bug workstreams; this
  branch does not claim it.
- **#354 / #226 / #225** — `needs-human`, untouched by this whole run.
- In-progress dispatches and their PRs untouched: #395, #393, #391, #378,
  #377, #369; open perf PRs #399 (`dispatch/deck-gpu`) and #397
  (`dispatch/deck-perf`) — do not collide.
- This branch: `claude/nightly-docs`, squash-merge per `run.md` §7 after the
  local gate.

## Assembler note

The hardening run ships as **separate branches** (one worktree each):
`nightly-tests` (S1+S2+S7b), `nightly-arch` (S4+S5), `nightly-bugs` (S6+S7),
`nightly-tools` (S2 mismatch + S3 + S9 + S10), `nightly-ci` (S8), and this
`nightly-docs` (S11+S12+S13). Each carries `hardening/<area>.md`
(`tests`, `bugs-a`, `bugs-b`, `bugs-c`, `arch`, `tools`, `ci`).

Integrator: after all branches are merged (local gate is the merge bar):

1. Fold every `hardening/*.md` into §1 (what was done, with file:line) and §2
   (findings register — keep the id scheme, add each area's ids).
2. Re-paste §4 from `crates/game/src/features/mod.rs` if any branch appended
   registrations.
3. Fill `hardening/perf-budget.md` from S9's runs (§5 pointer).
4. Collect the workstreams' GAME_DESIGN.md notes into §8 and apply them in one
   commit once main's working tree is clean.
5. Add the HANDOFF.md "Other known gaps" → `hardening/` pointer line.
6. Delete this note once the fold-in is done, and update the "Status" line at
   the top of this file.
