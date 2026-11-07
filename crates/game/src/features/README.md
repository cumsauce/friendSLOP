# features/

Each implemented idea lives in its own module: `features/<short_name>.rs` (or a folder).

```rust
//! <one line>. From issue #N by <author>.
use bevy::prelude::*;

/// Registered on every peer: replicated components, messages.
pub fn protocol(app: &mut App) { /* app.replicate::<Thing>(); */ }

/// Local-only systems: input, visuals, UI.
pub fn plugin(app: &mut App) { /* app.add_systems(...); */ }
```

Then add it to `FeaturesProtocolPlugin` and `FeaturesPlugin` in `mod.rs`. Put it in the protocol list
**at the end**, so features stay in the same order on every peer.
