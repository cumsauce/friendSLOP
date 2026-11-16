//! Walking up the Vessel with the real walker controller against its real
//! colliders: from the plaza to the top walkway, both along one lane and
//! switching lanes on every landing (the hairpin onto the flight above).

use avian3d::prelude::*;
use bevy::prelude::*;
use friendslop::features::vessel;

use super::walking::{self, spawn, tick};

fn world() -> App {
    let mut app = walking::app();
    // The plaza, its top at y = 0.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(80.0, 1.0, 80.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    for p in vessel::pieces() {
        let s = p.size;
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(s.x, s.y, s.z),
            p.transform,
        ));
    }
    walking::settle(&mut app);
    app
}

/// Walk through `waypoints` in turn; returns where you got to and how far
/// along the list.
fn follow(app: &mut App, start: Vec3, waypoints: &[Vec3]) -> (Vec3, usize) {
    let w = spawn(app, start);
    let mut feet = start;
    for (i, to) in waypoints.iter().enumerate() {
        let mut ticks = 0;
        while Vec2::new(to.x - feet.x, to.z - feet.z).length() > 0.3 {
            let d = *to - feet;
            // The stick's forward is -Z turned by yaw.
            let yaw = (-d.x).atan2(-d.z);
            feet = tick(app, w, |w, _| {
                w.yaw = yaw;
                w.stick = Vec2::Y;
            });
            ticks += 1;
            // Never more than ~15 s per waypoint, and never falling off.
            if ticks > 900 || feet.y < to.y - vessel::RISE - 1.0 {
                return (feet, i);
            }
        }
    }
    (feet, waypoints.len())
}

/// The flights of one climb: from landing `k` on the ground, at every
/// landing take the inner lane (`true`) or the outer one as `lanes` says.
fn route(lanes: impl Fn(usize) -> bool) -> Vec<Vec3> {
    let flights = vessel::flights();
    let mut at = (0, 0);
    let mut out = Vec::new();
    for level in 0..vessel::LEVELS - 1 {
        let inner = lanes(level);
        let up: Vec<_> = flights.iter().filter(|f| f.from == at).collect();
        // Inner-lane flights sit nearer the middle.
        let Some(f) = up.iter().min_by(|a, b| {
            let r = |f: &vessel::Flight| Vec2::new(f.a.x, f.a.z).length();
            let (ra, rb) = (r(a), r(b));
            if inner {
                ra.total_cmp(&rb)
            } else {
                rb.total_cmp(&ra)
            }
        }) else {
            panic!("no flight up from {at:?}");
        };
        // Across the landing through its middle, then up the flight.
        out.extend([f.stops.0, f.a, f.b, f.stops.1]);
        at = f.to;
    }
    out
}

fn climb(lanes: impl Fn(usize) -> bool) {
    let mut app = world();
    let way = route(lanes);
    // Start a couple of metres before the first flight, on the plaza.
    let first = way[1];
    let start = first - (way[2] - first).with_y(0.0).normalize() * 2.0 + Vec3::Y * 0.05;
    let (feet, reached) = follow(&mut app, start, &way);
    assert_eq!(
        reached,
        way.len(),
        "stuck at {feet} on the way to {} (waypoint {reached} of {})",
        way[reached.min(way.len() - 1)],
        way.len()
    );
    let top = (vessel::LEVELS - 1) as f32 * vessel::RISE;
    assert!(feet.y > top - 0.3, "at the top: {feet}");
}

#[test]
fn walks_up_the_vessel_on_one_lane() {
    climb(|_| true);
}

#[test]
fn walks_up_the_vessel_switching_lanes_every_landing() {
    climb(|level| level % 2 == 0);
}
