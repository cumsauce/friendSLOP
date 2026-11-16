//! The wastes past the map's north edge (#391): the ground itself is the
//! world crate's (`friendslop_world::wastes`); this only tells it where
//! everyone is, so chunks stream in around every player and the camera.

use bevy::prelude::*;
use friendslop_world::wastes::{Viewers, WastesPlugin};

use crate::AppState;
use crate::sites::People;
use crate::world::MainCamera;

pub fn plugin(app: &mut App) {
    app.add_plugins(WastesPlugin).add_systems(
        Update,
        watch
            .before(friendslop_world::wastes::StreamSet)
            .run_if(in_state(AppState::InGame)),
    );
}

/// Every player's feet and the local camera.
fn watch(
    people: Res<People>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut viewers: ResMut<Viewers>,
) {
    viewers.0.clear();
    viewers.0.extend(people.0.iter().map(|p| p.xz()));
    viewers
        .0
        .extend(camera.iter().map(|t| t.translation().xz()));
}
