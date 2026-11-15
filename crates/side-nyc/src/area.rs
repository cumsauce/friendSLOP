//! New York's area (`nyc` in `data/base/areas.ron`, `friendslop_base::areas`):
//! Penn Station, Fulton St and Miami are stacked over and under the city, so
//! everything in this crate lives in it. The helpers keep each feature's
//! migration to a line or two:
//!
//! - [`on_sim`] / [`on_show`] run a system on the frame the colliders or the
//!   visuals load (in `PreUpdate`, so colliders are there before the frame's
//!   physics: a teleport never lands on missing ground);
//! - [`sim`] / [`show`] tag what such a system spawns, so it goes when its
//!   layer unloads;
//! - [`doorway`] is `doorway::spawn` in the visuals layer;
//! - [`Dress`] marks a local child that draws a replicated entity (the entity
//!   stays: replicated state is always kept), so it can be dressed again the
//!   next time the area shows.

use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use friendslop_base::areas::{self, AreaSet};
pub use friendslop_base::areas::{InArea, Layer};

use crate::AppState;

/// New York's area id.
pub const NYC: &str = "nyc";

/// In New York's colliders and simulation layer (gone when it unloads, and
/// when the game ends).
pub fn sim() -> impl Bundle {
    (DespawnOnExit(AppState::InGame), InArea::sim(NYC))
}

/// In New York's visuals layer (gone when you leave it, and when the game ends).
pub fn show() -> impl Bundle {
    (DespawnOnExit(AppState::InGame), InArea::show(NYC))
}

/// In `area`'s visuals layer: for things that wander between areas.
pub fn show_in(area: &str) -> impl Bundle {
    (DespawnOnExit(AppState::InGame), InArea::show(area))
}

/// Run `systems` on the frame New York's colliders load.
pub fn on_sim<M>(app: &mut App, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
    app.add_systems(PreUpdate, at_load(systems, Layer::Sim));
}

/// Run `systems` on the frame New York's visuals load.
pub fn on_show<M>(app: &mut App, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
    app.add_systems(PreUpdate, at_load(systems, Layer::Show));
}

fn at_load<M>(
    systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    layer: Layer,
) -> ScheduleConfigs<ScheduleSystem> {
    systems
        .into_configs()
        .after(AreaSet)
        .run_if(in_state(AppState::InGame))
        .run_if(areas::loaded(layer, NYC))
}

/// A way in or out ([`crate::features::doorway::spawn`]) in New York's visuals.
pub fn doorway(
    commands: &mut Commands,
    at: Vec3,
    yaw: f32,
    d: crate::features::doorway::Doorway,
) -> Entity {
    let e = crate::features::doorway::spawn(commands, at, yaw, d);
    commands.entity(e).insert(InArea::show(NYC));
    e
}

/// A local child drawing a replicated entity (tag it with [`show`] or
/// [`show_in`] as well).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Dress;

/// Has `children` a [`Dress`] among them?
pub fn dressed(children: Option<&Children>, dresses: &Query<(), With<Dress>>) -> bool {
    children.is_some_and(|c| c.iter().any(|e| dresses.contains(e)))
}
