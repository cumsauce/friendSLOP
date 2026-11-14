# features/

Each implemented idea lives in its own module, `src/features/<short_name>.rs` (or a folder), in
one of the game-layer crates (`crates/base`, `play`, `places`, `village`, `story`, `afterlife`,
`side`; see "Crates" in AGENTS.md). A new standalone idea usually goes in `crates/side`.

```rust
//! <one line>. From issue #N by <author>.
use bevy::prelude::*;

/// Registered on every peer: replicated components, messages.
pub fn protocol(app: &mut App) { /* app.replicate::<Thing>(); */ }

/// Local-only systems: input, visuals, UI.
pub fn plugin(app: &mut App) { /* app.add_systems(...); */ }
```

Add `pub mod <short_name>;` to that crate's `lib.rs`, then add it to `FeaturesProtocolPlugin` and `FeaturesPlugin` in `mod.rs`. Put it in the protocol list
**at the end**, so features stay in the same order on every peer.

**Give it a site (#412).** A mechanic lives at a place and sleeps when nobody is there: add a line to
`assets/data/base/sites.ron` (or reuse a site) and gate its systems with `sites::awake("<id>")`
(simulation), `sites::here("<id>")` (local visuals and sound) and `sites::arrived` / `sites::left`
(bring local dressing out, put it away); see "Every mechanic has a site" in AGENTS.md.

**Loading assets.** Keep a strong handle to anything you're waiting on (in a resource or
component). Calling `assets.load(..)` each frame and dropping the result throws the half-finished
load away, so it restarts every frame and never finishes. The duel's cowboy models did this,
re-parsing two glTFs every frame (~5 ms). `FRIENDSLOP_BENCH=1 cargo run -- host` prints frame times plus
mesh/material churn per second; steady churn there means something rebuilds GPU buffers each frame.

**Keep the logic pure.** Put a feature's decisions in plain functions (values in, values out) and
keep its systems as thin wiring. Anything that needs no game state (a mesh or sound generator, a
format, maths) belongs in a foundation crate instead; see "Crates" in AGENTS.md.
