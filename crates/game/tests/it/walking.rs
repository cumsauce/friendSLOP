//! A physics world with bevy_ahoy's walker controller in it, for the movement
//! tests: one `app.update()` is one 60 Hz controller tick.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use friendslop::kcc::{Kcc, Walking};

pub const HZ: f64 = 60.0;

/// Physics, the controller and nothing else. Spawn the colliders, then call
/// [`settle`].
pub fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin::default(),
        PhysicsPlugins::default(),
    ))
    .add_plugins(friendslop::kcc::controller_plugins())
    .insert_resource(Time::<Fixed>::from_hz(HZ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / HZ,
    )));
    friendslop::kcc::plugin(&mut app);
    // Avian tracks mesh asset events for collider caches.
    app.init_asset::<Mesh>();
    app
}

/// Plugins' `finish` (avian's diagnostics resources), then a few frames for
/// the physics backend to pick up the colliders.
pub fn settle(app: &mut App) {
    app.finish();
    app.cleanup();
    for _ in 0..3 {
        app.update();
    }
}

/// A walker standing at `feet`, with its body made and set up.
pub fn spawn(app: &mut App, feet: Vec3) -> Entity {
    let e = app
        .world_mut()
        .spawn((Kcc::default(), Walking { feet, ..default() }))
        .id();
    // The body is made in `Update` and set up in the next `PreUpdate`.
    app.update();
    e
}

/// One tick with these wishes; returns the feet afterwards.
pub fn tick(app: &mut App, walker: Entity, wish: impl FnOnce(&mut Walking, &mut Kcc)) -> Vec3 {
    let mut e = app.world_mut().entity_mut(walker);
    let (Some(mut kcc), Some(mut w)) = (e.get::<Kcc>().copied(), e.get::<Walking>().copied())
    else {
        panic!("not a walker");
    };
    wish(&mut w, &mut kcc);
    e.insert((w, kcc));
    app.update();
    feet(app, walker)
}

pub fn feet(app: &App, walker: Entity) -> Vec3 {
    let Some(w) = app.world().get::<Walking>(walker) else {
        panic!("no walking");
    };
    w.feet
}

pub fn kcc(app: &App, walker: Entity) -> Kcc {
    let Some(k) = app.world().get::<Kcc>(walker) else {
        panic!("no kcc");
    };
    *k
}

/// Walk forward facing `yaw` for `ticks`; the end and the lowest feet on the way.
pub fn walk(app: &mut App, walker: Entity, yaw: f32, ticks: usize) -> (Vec3, f32) {
    let mut lowest = feet(app, walker).y;
    let mut at = Vec3::ZERO;
    for _ in 0..ticks {
        at = tick(app, walker, |w, _| {
            w.yaw = yaw;
            w.stick = Vec2::Y;
            w.jump = false;
            w.skip = false;
        });
        lowest = lowest.min(at.y);
    }
    (at, lowest)
}
