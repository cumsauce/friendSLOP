//! Start somewhere else, for testing: `FRIENDSLOP_START=<name>` puts the host
//! there as soon as they're in the game (`FRIENDSLOP_START=fulton cargo run --
//! host`). A name is any spot on the commune map (`friendslop_world::places`,
//! e.g. `warehouse`, `lake`, `village`), or a start a feature adds with
//! [`StartPoints::add`] for a place the map doesn't have (Fulton St's top
//! level is `fulton`). An unknown name does nothing.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use super::fps::{WalkInput, Walker, teleport};
use crate::player::PlayerOwner;

/// Named places a body can start at, added by the features that build them.
#[derive(Resource, Default)]
pub struct StartPoints(HashMap<String, Vec3>);

impl StartPoints {
    /// Add (or move) the start called `name`: feet at `at`.
    pub fn add(&mut self, name: &str, at: Vec3) {
        self.0.insert(name.to_owned(), at);
    }
}

/// Where the start called `name` is: a feature's own start first, then a
/// spot on the map, on the ground.
pub fn find(points: &StartPoints, name: &str) -> Option<Vec3> {
    points.0.get(name).copied().or_else(|| {
        let p = friendslop_world::places::spot(name)?;
        Some(Vec3::new(p.x, crate::terrain::height_at(p.x, p.y), p.y))
    })
}

pub fn plugin(app: &mut App) {
    app.init_resource::<StartPoints>().add_systems(
        Update,
        start
            .run_if(in_state(crate::AppState::InGame))
            .run_if(in_state(ServerState::Running)),
    );
}

/// Server: once, put the host at `FRIENDSLOP_START` when it's known.
fn start(
    points: Res<StartPoints>,
    mut done: Local<bool>,
    mut bodies: Query<(
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    if *done {
        return;
    }
    let Ok(name) = std::env::var("FRIENDSLOP_START") else {
        *done = true;
        return;
    };
    // A feature's start may only be added once its place is built: wait.
    let Some(at) = find(&points, &name) else {
        return;
    };
    let Some((_, mut walker, mut input, mut kcc)) = bodies.iter_mut().find(|b| b.0.0.is_none())
    else {
        return;
    };
    let yaw = walker.yaw;
    teleport(
        &mut walker,
        input.as_deref_mut(),
        kcc.as_deref_mut(),
        at + Vec3::Y * 0.05,
        yaw,
    );
    *done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_start_wins_and_unknown_names_are_none() {
        let mut p = StartPoints::default();
        p.add("somewhere", Vec3::new(1.0, -20.0, 3.0));
        assert_eq!(find(&p, "somewhere"), Some(Vec3::new(1.0, -20.0, 3.0)));
        assert_eq!(find(&p, "no such place anywhere"), None);
    }
}
