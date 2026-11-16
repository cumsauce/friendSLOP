//! Holes in the ground: the kinematic walker goes down Fulton St's entrance
//! stairs through the hole in New York's street, past where the street's
//! slab used to be solid, and back up again.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::walking;
use friendslop_side::features::{fulton, nyc};

fn tuning() -> fulton::Tuning {
    let d = &fulton::TUNING;
    let path = format!("{}/assets/{}", env!("CARGO_MANIFEST_DIR"), d.path());
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("no {path}");
    };
    assert!(d.set_bytes(&bytes), "{path} doesn't parse");
    (*d.get()).clone()
}

fn world(t: &fulton::Tuning) -> App {
    let mut app = walking::app();
    friendslop_world::holes::cut(fulton::entrance_hole(t));
    for (at, c) in nyc::city_floor() {
        app.world_mut().spawn((RigidBody::Static, c, at));
    }
    // The city map's boxes too: its sidewalk slabs cover the hole otherwise.
    let map = &nyc::MAP;
    let path = format!("{}/assets/{}", env!("CARGO_MANIFEST_DIR"), map.path());
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("no {path}");
    };
    assert!(map.set_bytes(&bytes), "{path} doesn't parse");
    for (at, c) in nyc::wall_colliders(&nyc::city(&map.get()).walls) {
        app.world_mut().spawn((RigidBody::Static, c, at));
    }
    for b in fulton::entrance_colliders(t) {
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(b.size.x, b.size.y, b.size.z),
            Transform::from_translation(b.centre).with_rotation(b.rotation),
        ));
    }
    walking::settle(&mut app);
    app
}

/// A new walker at `feet` walking facing `yaw` for `ticks` at 60 Hz; the
/// end and the lowest feet on the way. Returns the walker to walk on with.
fn walk(app: &mut App, feet: Vec3, yaw: f32, ticks: usize) -> (Entity, Vec3, f32) {
    let w = walking::spawn(app, feet);
    let (at, lowest) = walking::walk(app, w, yaw, ticks);
    (w, at, lowest)
}

#[test]
fn walks_down_the_entrance_stairs_and_back_up() {
    let t = tuning();
    let mut app = world(&t);
    // On the sidewalk in front of the entrance, walking into it (the open
    // end faces the entrance's yaw).
    let start = fulton::street_at(&t) + Vec3::Y * 0.05;
    let into = t.entrance_yaw.to_radians();
    let (w, feet, lowest) = walk(&mut app, start, into, 200);
    assert!(
        lowest < -2.8,
        "never got down the stairs: lowest {lowest}, at {feet}"
    );
    // At the foot of the stairs, on the landing: the server takes you in from here.
    assert!(feet.y > -3.3, "fell through the landing: {feet}");
    assert!(fulton::entering(&t, feet), "not in the way in: {feet}");
    // Turn round and climb back up to the street.
    let (back, _) = walking::walk(&mut app, w, into + std::f32::consts::PI, 200);
    assert!(back.y > -0.1 && back.y < 0.3, "didn't get back up: {back}");
    assert!(!fulton::entrance_hole(&t).contains(back.xz()));
    // And the street next to the entrance is still solid.
    let (_, beside, low) = walk(
        &mut app,
        fulton::entrance_at(&t) + Vec3::new(0.0, 0.05, 3.0),
        -std::f32::consts::FRAC_PI_2,
        30,
    );
    assert!(low > -0.1, "fell through the street beside it: {beside}");
}

#[test]
fn walks_up_from_the_path_platform_into_the_oculus() {
    let t = tuning();
    let trains = &friendslop_side::features::trains::TUNING;
    let path = format!("{}/assets/{}", env!("CARGO_MANIFEST_DIR"), trains.path());
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("no {path}");
    };
    assert!(trains.set_bytes(&bytes), "{path} doesn't parse");
    use friendslop_side::features::trains as tr;
    let tt = trains.get();
    let Some(wtc) = tt.lines[0].stops.iter().find(|s| s.depth > 0.0) else {
        panic!("no underground stop");
    };
    // The hole the stairs come up through, cut before the city's floor is split.
    friendslop_world::holes::cut(tr_hole(wtc));
    let mut app = world(&t);
    let f = tr::stop_frame(wtc);
    for (c, size, tilt) in tr::station_boxes(wtc) {
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            Transform::from_translation(f.transform_point(c)).with_rotation(f.rotation * tilt),
        ));
    }
    for _ in 0..3 {
        app.update();
    }
    let (bottom, _) = tr::stairs_ends(wtc.depth);
    let start = f.transform_point(bottom - Vec3::X * 1.5 + Vec3::Y * 0.1);
    // Walk along the track's +x (forward is -z turned by the yaw).
    let yaw = wtc.yaw.to_radians() - std::f32::consts::FRAC_PI_2;
    let (_, feet, _) = walk(&mut app, start, yaw, 400);
    assert!(
        feet.y > -0.1 && feet.y < 0.4,
        "didn't get up into the Oculus: {feet} from {start}"
    );
}

fn tr_hole(s: &friendslop_side::features::trains::Stop) -> friendslop_world::holes::Hole {
    let Some(h) = friendslop_side::features::trains::stairs_hole(s) else {
        panic!("no stairs hole");
    };
    h
}
