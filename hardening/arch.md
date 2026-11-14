# S4+S5 handoff — claims prep, side-nyc split, encapsulation

Workstream S4 (claims prep) + S5 (side crate split + convention leftovers) of the
overnight hardening run. Branch `claude/nightly-arch`, six commits, all local-gate
green (fmt, clippy `-D warnings`, full workspace tests, wasm clippy).

## What landed

### W1 — claims prep (safe, additive)

1. **scenery.rs import swaps** (`game/src/scenery.rs:22`): `parliament_at` and
   `arena_center`/`arena_half` now come from `friendslop_world::places` directly
   (the feature-path names were already re-exports of those). Only the two import
   lines changed.
2. **Additive `footprints::claim` calls** in the three protocol fns (covers() fns
   untouched — they stay in service until the follow-up):
   - `places/features/underground/mod.rs` — `claim(e.at + e.out * 0.8, 2.2)` per
     entrance (exactly the covers() circle).
   - `places/features/warehouse/mod.rs` — the lot as a circle grid (corners,
     mid-edges and between, radius half the shorter lot side) plus the 6 m yard
     circle; `claim_ground()` called first thing in `protocol()`.
   - `village/features/wendys.rs` — building + front-yard strip (the covers()
     rectangle) as a circle grid.
   Coverage tests in each module assert every old covers() sample point is
   `footprints::claimed` (`claims_cover_the_sample_points_covers_does`, taking
   the same sample points as the old covers tests).
3. **Marker components to `friendslop-core`**: `Sway` (was `game/src/ambience.rs`)
   and `TreeLod` (was `game/src/lod.rs`). Systems stay in the game; both modules
   re-export the types (`pub use friendslop_core::…`), so `crate::ambience::Sway`
   and `crate::lod::TreeLod` paths are unchanged.
4. **`overview_eye()`** in `friendslop-core` (from the `OVERVIEW_OFFSET` const,
   which `base::shared::camera::view_offset` now returns): `scenery.rs` keeps its
   clearing around the fixed camera eye without the
   `CameraRig::default().transform()` constant-in-disguise.

### W2 — `friendslop-side-nyc` (ONE atomic commit, rename-only)

New crate `crates/side-nyc` between story and side:
`story → {afterlife, side-nyc} → side`. Sixteen NYC features moved with
`git mv` and **zero content edits** (one doc-comment path in `ice_cream.rs` fixed
for the move — `[super::pigeons]` → plain `friendslop_side::features::pigeons`):
boba, fulton, fulton_hunt, hats, high_places, ice_cream, miami, nyc, nyc_traffic,
penn, street_people, toilets, trains, vessel, vessel_jump, wallet (~11.4k lines).

- `lib.rs` mirrors `places/src/lib.rs`: the `friendslop_base` shim, a `features`
  module globbing base/play/places/village/story features plus its 16 mods, and
  its own `data_files()` for the moved TUNINGs.
- `side`'s features re-export `friendslop_side_nyc::features::*`, so
  `super::fulton::`, `friendslop_side::features::trains` (the holes.rs tests use
  exactly these) and the game's protocol list all resolve unchanged. **Registration
  order untouched** — `game/src/features/mod.rs` not edited.
- Edge audit held: the only kept→moved code edge is `fusion/mod.rs:579`
  (`crate::features::nyc::in_the_city`, resolves via the glob); the only
  moved→kept edge was the ice_cream doc link.
- Deps: side-nyc takes `wfc`+`samiam` from side and also `friendslop-world`
  (moved files call it directly). Side **drops** `bevy_rerecast`, `bevy_landmass`,
  `landmass_rerecast`, `modeler_bevy` (verified unused; `avian_rerecast` stays
  for `infinite.rs`).
- `game/src/lib.rs` `data_files()` extends side's and side-nyc's (existing order
  untouched). AGENTS.md crate tree + layering line updated.

### W3 — encapsulation

1. **`fps::teleport(walker, walkinput, kcc, to, yaw)`** in `play/features/fps.rs`
   (sets pos/yaw, zeroes the walk input and KCC velocity/grounded). Every
   `walker.pos =` outside fps.rs's own movement systems goes through it now:
   the local `put()` wrappers (afterlife, warehouse, surf, fulton, infinite) now
   delegate; hatch/door/train/elevator hops, holds (caught, hide_and_seek,
   villagers), knockbacks (fat_cat flung, shipwreck), ragdoll stand-up/feet,
   climb's step, fulton_hunt lockers/catch, miami's fall respawn, start.rs and
   the probe-bake stand-ins call it directly. Sites that mean to keep momentum
   (infinite's hallway wrap) or carry motion on (climb) pass `None` for the
   pieces they keep. **Standardisation note**: the old sites varied in what they
   reset (some only zeroed stick); teleport's contract resets all of stick/jump/
   run/vy/climbing + velocity/grounded when given the refs. fps.rs's own
   movement writes and the `give_bodies` spawn constructor stay direct (they are
   movement/construction, not relocation). Headless unit test
   `teleport_puts_you_there_and_stops_you_dead`. The it-test harnesses in
   `game/tests/it/*` still place walkers with `w.pos = at` (test setup; left for
   the tests workstream).
2. **Smoking API** (`places/features/smoking.rs`): `purchase(cash, stash, price)`
   (buy a joint), `Stash::{earn,spend}(Item)`, and event `Lit` — only
   `smoking::start` inserts `Smoking` now (callers trigger `Lit`). `Lit` carries
   `secs` because the high-places lounge joint is 40 s (`high_places.ron`) while
   a P-key smoke is 8 s (`smoking.ron`). `high_places.rs`'s dealer uses
   `purchase`/`spend` and no longer writes `Stash` fields or constructs
   `Smoking` across crates. Test `you_buy_with_what_you_have`.
3. **`Stats` → `base::rules::stats`** with `Stats::earn_pelf` (caps at 10);
   `pub use` re-export at its old home (`play/features/stats.rs`), so
   `crate::features::stats::Stats` keeps resolving. `fulton.rs`'s reward calls
   `earn_pelf` instead of writing `stats.pelf`; its `rewarded_pelf` helper is
   gone (the cap lives on the type; fulton's `the_reward_caps_at_ten` test now
   exercises `earn_pelf`).

## Owed — the W1 FOLLOW-UP (NOT done here; zones.rs was off-limits tonight)

This is the other half of S4 and the AGENTS.md "next split". It lands after the
NYC-furniture agent and the four unmerged branches (bugpass, house-parts-kit,
modeler-remaining, modeler-buildings-props) stop touching `scenery.rs`/`zones.rs`:

1. Delete the three `covers()` fns (underground, warehouse, wendys) and their
   callers in `game/src/scenery.rs` and `game/src/zones.rs` — switch those to
   `footprints::claimed`. **Audit the "covered" helper semantics first**:
   `claimed()` includes ALL claims (and map props), not just buildings, so it is
   slightly wider than the old covers() lists.
2. `git mv game/src/{scenery,zones}.rs → crates/world/src/`, move the `SCENERY`
   Data static with them, re-export from `base::shared::{zones,scenery}`.
   `friendslop-world` must gain no feature imports (`landmarks.rs` is the shape);
   the W1 prep (claims in protocol fns, places-accessor imports, `overview_eye`
   in core) is what makes this possible.
3. Optional: `footprints::claim_rect` if the circle approximation leaves visible
   gaps (the warehouse/wendys grids cover strictly, but rectangles match the
   covers() shape exactly).
4. Leftovers hardcoded in code worth pushing into `commune.map.json` markers:
   castle curtain wall + archway gap skip (`zones.rs`), `in_set_piece` rects,
   `PLAY_HALF` (`scenery.rs`), camera corridor, spawn rows. Scenery's tree rng
   should take `WorldSeed::for_`.

Also owed from W3's sweep (sites deliberately left): `game/tests/it/*` test
harnesses write `w.pos = at` directly (fine for test setup; convert if the tests
workstream wants uniformity).

## Contested-file notes (for rebases)

- `warehouse/mod.rs`, `wendys.rs`: claim additions are self-contained
  (`claim_ground()` + one call in `protocol()` + one test); expect to rebase
  against the bugfixers.
- `smoking.rs`: new API is additive (Item/impl Stash/purchase/Lit/start +
  `add_observer(start)`); the only touched existing lines are the `smoke`
  handler's insert (now `commands.trigger(Lit …)`) and `take_egg`'s
  `stash.earn(Item::Egg)`. The smoke:double-light bugfix (immediate busy marker)
  sits in the same handler — merge carefully, the fixes compose.
- `fps.rs`/`stats.rs`: `teleport` is a new pub fn near `spawn_point`; the Stats
  type moved out with a re-export. The bugfix pass A's spawn-point collision
  fix and `reroll_stats` guard are untouched.

## Verification

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `cargo clippy -p friendslop --target
  wasm32-unknown-unknown` — all green at the branch tip (serialized final gate
  under `flock /Users/lume/friendSLOP/target/.build-lock`).
- New tests: `fps::tests::teleport_puts_you_there_and_stops_you_dead`,
  `smoking::tests::you_buy_with_what_you_have`,
  `claims_cover_the_sample_points_covers_does` ×3 (underground, warehouse,
  wendys).
- Registration order: `game/src/features/mod.rs` untouched; `add_observer(start)`
  in smoking's protocol is not a registration. The protocol-hash snapshot
  (tests workstream) must come out identical.
