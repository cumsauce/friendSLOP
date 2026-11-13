//! Miami Music Week. From issue #345 by doomy: "in new york you can go to JFK
//! and from there u can fly like the train into miami and go to factory town
//! for miami music week. should be EDM music and stuff and people and you can
//! also do ketamine".
//!
//! The flight is a line in `trains.rs` (a plane instead of a train: board it
//! at JFK's gate at the city's north end, it rolls out, takes off and you
//! land at Miami's gate). Miami is a little island of ground floating far
//! over New York (like the surf hall, so it's inside the city's soft wall):
//! a street with palm trees and sand around Factory Town, a warehouse club
//! with a stage, a DJ booth, speaker stacks, lasers and a dancing crowd. The
//! music goes EDM (the `Miami` mood) while you're there. A guy by the bar
//! sells ketamine: click him and you're in a k-hole for a while, the world
//! going wide and purple and slow (yours only; your friends just see you
//! standing there). Fall off the island and you're back at the gate.
//!
//! The model is `miami-factory-town` (modeler), its origin the ground's middle;
//! the colliders are mesh-less boxes from the offsets below, so gameplay never
//! waits on the file. Places and writing in `data/side/miami.ron`.

use bevy::camera::Projection;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked, in_reach};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Tuning and writing: `assets/data/side/miami.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The middle of Miami's ground, on top (x, y, z): the model's origin.
    pub ground: (f32, f32, f32),
    /// Where you're put back when you fall off (x, z, on the ground): the gate.
    pub back_to: (f32, f32),
    /// Fall this far (m) below the ground and you're back at the gate.
    pub fall_below: f32,
    /// How many people dance in Factory Town.
    pub dancers: u32,
    /// The dancers' beat (beats per minute; the Miami song's tempo).
    pub bpm: f32,
    /// Seconds a bump of ketamine lasts.
    pub k_secs: f32,
    /// Shown when you walk into Factory Town.
    pub venue_line: String,
    /// Shown when you take a bump.
    pub bump_line: String,
    /// Shown one after another while you're in the k-hole.
    pub k_lines: Vec<String>,
    /// Shown when it wears off.
    pub sober_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/miami.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------------ the island's shape
// Offsets inside the `miami-factory-town` model (x, z from the ground's middle, y up).

/// The ground's half size (x, z): the island's edge.
pub const HALF: Vec2 = Vec2::new(60.0, 45.0);
/// Factory Town: its middle, half size (x, z) and height. Its open front faces -z.
pub const VENUE: Vec2 = Vec2::new(10.0, 20.0);
pub const VENUE_HALF: Vec2 = Vec2::new(18.0, 13.0);
pub const VENUE_H: f32 = 9.0;
/// The front's opening (half width, from the venue's middle along x).
pub const DOOR_HALF: f32 = 7.0;
/// The stage at the back: its depth (z) and height.
pub const STAGE_D: f32 = 6.0;
pub const STAGE_H: f32 = 1.2;
/// The bar along the west wall: its middle (x, z) and half size.
pub const BAR: Vec2 = Vec2::new(-5.0, 20.0);
pub const BAR_HALF: Vec2 = Vec2::new(0.6, 6.0);
/// Where the dealer stands (beside the bar).
pub const DEALER: Vec2 = Vec2::new(-6.5, 12.0);
const WALL: f32 = 0.4;

/// The ground's middle (world).
pub fn ground() -> Vec3 {
    Vec3::from(tuning().ground)
}

/// Is a body with feet at `p` on (or over, or just off) Miami's island?
pub fn in_miami(p: Vec3) -> bool {
    let l = p - ground();
    l.x.abs() < HALF.x + 20.0 && l.z.abs() < HALF.y + 20.0 && (-200.0..60.0).contains(&l.y)
}

/// Is `p` inside Factory Town?
pub fn in_venue(p: Vec3) -> bool {
    let l = p - ground();
    (l.x - VENUE.x).abs() < VENUE_HALF.x
        && (l.z - VENUE.y).abs() < VENUE_HALF.y
        && (-1.0..VENUE_H).contains(&l.y)
}

/// Has a body at `p` fallen off the island?
pub fn fallen(t: &Tuning, p: Vec3) -> bool {
    let g = Vec3::from(t.ground);
    let l = p - g;
    l.x.abs() < HALF.x + 400.0
        && l.z.abs() < HALF.y + 400.0
        && (-300.0..-t.fall_below).contains(&l.y)
}

/// The island's static boxes (centre, size), relative to the ground's middle.
pub fn boxes() -> Vec<(Vec3, Vec3)> {
    let mut b = vec![
        // The ground slab.
        (Vec3::Y * -1.0, Vec3::new(HALF.x * 2.0, 2.0, HALF.y * 2.0)),
    ];
    // A low sea wall round the edge.
    for s in [-1.0, 1.0] {
        b.push((
            Vec3::new(s * HALF.x, 0.6, 0.0),
            Vec3::new(WALL, 1.2, HALF.y * 2.0),
        ));
        b.push((
            Vec3::new(0.0, 0.6, s * HALF.y),
            Vec3::new(HALF.x * 2.0, 1.2, WALL),
        ));
    }
    let (v, h, y) = (VENUE, VENUE_HALF, VENUE_H / 2.0);
    // Back and side walls.
    b.push((
        Vec3::new(v.x, y, v.y + h.y),
        Vec3::new(h.x * 2.0, VENUE_H, WALL),
    ));
    for s in [-1.0, 1.0] {
        b.push((
            Vec3::new(v.x + s * h.x, y, v.y),
            Vec3::new(WALL, VENUE_H, h.y * 2.0),
        ));
    }
    // The front, either side of the doors, and over them.
    let side = h.x - DOOR_HALF;
    for s in [-1.0, 1.0] {
        b.push((
            Vec3::new(v.x + s * (DOOR_HALF + side / 2.0), y, v.y - h.y),
            Vec3::new(side, VENUE_H, WALL),
        ));
    }
    b.push((
        Vec3::new(v.x, VENUE_H - 1.5, v.y - h.y),
        Vec3::new(DOOR_HALF * 2.0, 3.0, WALL),
    ));
    // The roof.
    b.push((
        Vec3::new(v.x, VENUE_H, v.y),
        Vec3::new(h.x * 2.0, WALL, h.y * 2.0),
    ));
    // The stage and the bar.
    b.push((
        Vec3::new(v.x, STAGE_H / 2.0, v.y + h.y - STAGE_D / 2.0),
        Vec3::new(h.x * 2.0, STAGE_H, STAGE_D),
    ));
    b.push((
        Vec3::new(BAR.x, 0.55, BAR.y),
        Vec3::new(BAR_HALF.x * 2.0, 1.1, BAR_HALF.y * 2.0),
    ));
    b
}

/// Where dancer `i` of `n` stands (relative to the ground's middle): rows on
/// the dance floor in front of the stage, a little jumbled.
pub fn dancer_spot(i: u32, n: u32) -> Vec3 {
    let cols = (n as f32).sqrt().ceil().max(1.0) as u32;
    let (c, r) = (i % cols, i / cols);
    let jig = |k: u32| ((k.wrapping_mul(2_654_435_761) >> 8) % 1000) as f32 / 1000.0 - 0.5;
    let x = VENUE.x - 12.0 + 24.0 * (c as f32 + 0.5) / cols as f32 + jig(i * 2) * 1.6;
    let z = VENUE.y + 5.0 - 13.0 * (r as f32 + 0.5) / cols as f32 + jig(i * 2 + 1) * 1.4;
    Vec3::new(x, 0.0, z)
}

/// A dancer's hop at `secs` (m up): on the beat, each a little off.
pub fn hop(bpm: f32, secs: f32, i: u32) -> f32 {
    let beat = secs * bpm / 60.0 + (i % 4) as f32 * 0.05;
    let f = beat.fract();
    0.18 * (1.0 - (2.0 * f - 1.0).powi(2)).max(0.0)
}

/// How wide the view is in the k-hole (radians, from the normal `fov`) at
/// `secs` into it, of `total`: it swells, sways and settles back.
pub fn k_fov(fov: f32, secs: f32, total: f32) -> f32 {
    let ramp = (secs / 3.0).min(1.0) * ((total - secs) / 3.0).clamp(0.0, 1.0);
    fov * (1.0 + ramp * (0.45 + 0.12 * (secs * 0.7).sin()))
}

// ------------------------------------------------------------------ server

/// Client -> server: "I'll take a bump."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakeK;

/// Server -> the taker only: you're in a k-hole for `secs`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct KHole {
    pub secs: f32,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<TakeK>(Channel::Ordered)
        .add_server_event::<KHole>(Channel::Ordered)
        .add_observer(take_k)
        .add_systems(Update, catch_fallers.run_if(in_state(ServerState::Running)));
}

fn take_k(
    take: On<FromClient<TakeK>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
) {
    let d = ground() + Vec3::new(DEALER.x, 0.0, DEALER.y);
    if !in_reach(&mut commands, take.client_id, &walkers, d) {
        return;
    }
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(take.client_id),
        message: KHole {
            secs: tuning().k_secs,
        },
    });
}

/// Server: anyone who fell off the island is back at the gate.
fn catch_fallers(
    mut bodies: Query<
        (
            &mut Walker,
            Option<&mut WalkInput>,
            Option<&mut crate::kcc::Kcc>,
        ),
        With<PlayerOwner>,
    >,
) {
    let t = tuning();
    for (mut w, input, kcc) in &mut bodies {
        if !fallen(&t, w.pos) {
            continue;
        }
        w.pos = Vec3::new(t.back_to.0, t.ground.1 + 0.3, t.back_to.1);
        if let Some(mut input) = input {
            input.stick = Vec2::ZERO;
        }
        if let Some(mut kcc) = kcc {
            kcc.velocity = Vec3::ZERO;
        }
    }
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.init_resource::<Trip>()
        .add_observer(feel_k)
        .add_observer(click_dealer)
        .add_systems(OnEnter(AppState::InGame), build)
        .add_systems(OnExit(AppState::InGame), sober_up)
        .add_systems(
            Update,
            (dance, lasers, trip, venue_line).run_if(in_state(AppState::InGame)),
        )
        .add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
}

#[derive(Component)]
struct Dancer {
    i: u32,
    at: Vec3,
}

#[derive(Component)]
struct Laser {
    i: u32,
    from: Vec3,
}

#[derive(Component)]
struct Dealer;

#[derive(Component)]
struct TripTint;

/// Local: the k-hole you're in, if any.
#[derive(Resource, Default)]
struct Trip {
    secs: f32,
    total: f32,
    /// The camera's own field of view, put back when it wears off.
    fov: Option<f32>,
    line: usize,
}

fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    let t = tuning();
    let g = Vec3::from(t.ground);
    commands.spawn((
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset("models/miami-factory-town.gltf")),
        ),
        Transform::from_translation(g),
        DespawnOnExit(AppState::InGame),
    ));
    for (c, s) in boxes() {
        commands.spawn((
            Transform::from_translation(g + c),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // The crowd.
    for i in 0..t.dancers {
        let at = g + dancer_spot(i, t.dancers);
        commands.spawn((
            Dancer { i, at },
            crate::people::Gait::default(),
            Transform::from_translation(at)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
            Visibility::default(),
            crate::people::CharacterLook::random(0x0345_0000 + i as u64 * 11),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // The dealer by the bar.
    commands.spawn((
        Dealer,
        crate::people::Gait::default(),
        Transform::from_translation(g + Vec3::new(DEALER.x, 0.0, DEALER.y))
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        Visibility::default(),
        Clickable::at_height(1.0).labelled("buy a bump of ketamine"),
        crate::people::CharacterLook::random(0x0345_dea1),
        DespawnOnExit(AppState::InGame),
    ));
    // Lasers fanning out over the crowd from the DJ booth, and coloured lights.
    let beam = meshes.add(Cuboid::new(0.05, 0.05, 30.0));
    let booth = g + Vec3::new(VENUE.x, STAGE_H + 2.5, VENUE.y + VENUE_HALF.y - 2.0);
    for i in 0..8 {
        let hue = i as f32 * 45.0;
        let c = Color::hsl(hue, 1.0, 0.5).to_linear();
        commands.spawn((
            Laser { i, from: booth },
            Mesh3d(beam.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: c * 30.0,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(booth),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for (i, x) in [-10.0, 0.0, 10.0].into_iter().enumerate() {
        commands.spawn((
            PointLight {
                color: Color::hsl(280.0 + i as f32 * 60.0, 1.0, 0.55),
                intensity: 400_000.0,
                range: 22.0,
                ..default()
            },
            Transform::from_translation(g + Vec3::new(VENUE.x + x, VENUE_H - 1.5, VENUE.y)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // `FRIENDSLOP_START=factory_town` starts you on the dance floor.
    let floor = g + Vec3::new(VENUE.x, 0.05, VENUE.y - 8.0);
    commands.queue(move |world: &mut World| {
        if let Some(mut p) = world.get_resource_mut::<crate::features::start::StartPoints>() {
            p.add("factory_town", floor);
        }
    });
}

fn dance(time: Res<Time>, mut dancers: Query<(&Dancer, &mut Transform)>) {
    let t = tuning();
    let secs = time.elapsed_secs();
    for (d, mut tf) in &mut dancers {
        let y = hop(t.bpm, secs, d.i);
        tf.translation = d.at + Vec3::Y * y;
        let sway = (secs * t.bpm / 120.0 * std::f32::consts::PI + d.i as f32).sin() * 0.4;
        tf.rotation = Quat::from_rotation_y(std::f32::consts::PI + sway);
    }
}

fn lasers(time: Res<Time>, mut beams: Query<(&Laser, &mut Transform)>) {
    let secs = time.elapsed_secs();
    for (l, mut tf) in &mut beams {
        let k = l.i as f32;
        let yaw = (secs * 0.9 + k * 0.8).sin() * 0.9;
        let pitch = -0.15 - 0.12 * (secs * 1.3 + k).sin().abs();
        let dir = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        // The beam's mesh runs 30 m along z: aim it from the booth out over the crowd (-z).
        let fwd = dir * Vec3::NEG_Z;
        *tf = Transform::from_translation(l.from + fwd * 15.0).looking_to(fwd, Vec3::Y);
    }
}

/// The unified click landed on the dealer.
fn click_dealer(click: On<Clicked>, mut commands: Commands, dealers: Query<(), With<Dealer>>) {
    if dealers.contains(click.entity) {
        commands.client_trigger(TakeK);
    }
}

/// The server says you've taken it: here it comes.
fn feel_k(k: On<KHole>, mut trip: ResMut<Trip>, toast: Option<ResMut<crate::hud::Toast>>) {
    trip.total = k.secs;
    trip.secs = 0.0;
    trip.line = 0;
    if let Some(mut toast) = toast {
        toast.show(tuning().bump_line.clone());
    }
}

/// Local: the k-hole widens your view and tints the world, then wears off.
fn trip(
    mut commands: Commands,
    time: Res<Time>,
    mut trip: ResMut<Trip>,
    mut cameras: Query<&mut Projection, With<Camera3d>>,
    mut tints: Query<(Entity, &mut BackgroundColor), With<TripTint>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    if trip.total <= 0.0 {
        return;
    }
    let t = tuning();
    trip.secs += time.delta_secs();
    let done = trip.secs >= trip.total;
    for mut p in &mut cameras {
        let Projection::Perspective(p) = p.as_mut() else {
            continue;
        };
        let base = *trip.fov.get_or_insert(p.fov);
        p.fov = if done {
            base
        } else {
            k_fov(base, trip.secs, trip.total)
        };
    }
    let alpha = if done {
        0.0
    } else {
        let ramp = (trip.secs / 3.0).min(1.0) * ((trip.total - trip.secs) / 3.0).clamp(0.0, 1.0);
        ramp * (0.22 + 0.08 * (trip.secs * 1.7).sin())
    };
    let color = Color::srgba(0.55, 0.2, 0.85, alpha);
    if tints.is_empty() && !done {
        commands.spawn((
            TripTint,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(color),
            Pickable::IGNORE,
            DespawnOnExit(AppState::InGame),
        ));
    }
    for (e, mut bg) in &mut tints {
        if done {
            commands.entity(e).despawn();
        } else {
            bg.0 = color;
        }
    }
    let Some(mut toast) = toast else { return };
    if done {
        toast.show(t.sober_line.clone());
        *trip = Trip::default();
        return;
    }
    // A new thought every few seconds.
    let n = t.k_lines.len().max(1);
    let next = ((trip.secs / (trip.total / (n as f32 + 1.0))) as usize).min(n);
    if next > trip.line && next <= t.k_lines.len() {
        trip.line = next;
        toast.show(t.k_lines[next - 1].clone());
    }
}

fn sober_up(mut trip: ResMut<Trip>) {
    *trip = Trip::default();
}

/// Local: the venue's line when you walk into Factory Town.
fn venue_line(
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<bool>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let inside = in_venue(w.pos);
    if inside && !*was {
        toast.show(tuning().venue_line.clone());
    }
    *was = inside;
}

/// For the music: the camera's in Miami (`miami`) or in Factory Town (`factory_town`).
fn music_signals(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let at = crate::music::camera_at(&camera);
    signals.flag("miami", at.is_some_and(in_miami));
    signals.flag("factory_town", at.is_some_and(in_venue));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.ground.1 > 500.0, "Miami floats high over the city");
        assert!(t.fall_below > 0.0 && t.dancers > 0 && t.bpm > 0.0 && t.k_secs > 0.0);
        assert!(!t.venue_line.is_empty() && !t.bump_line.is_empty());
        assert!(!t.k_lines.is_empty() && !t.sober_line.is_empty());
        // Inside the city's soft wall, so it doesn't push you back to the valley.
        let g = Vec3::from(t.ground);
        for (x, z) in [(-HALF.x, -HALF.y), (HALF.x, HALF.y)] {
            assert!(crate::rules::nyc::in_city(g.x + x, g.z + z));
        }
        // The gate you come back to is on the island.
        let back = Vec3::new(t.back_to.0, g.y, t.back_to.1);
        assert!(in_miami(back) && !fallen(&t, back));
        assert!(fallen(&t, back - Vec3::Y * (t.fall_below + 1.0)));
    }

    #[test]
    fn the_flight_lands_on_the_island() {
        let _ = t();
        let trains = crate::features::trains::TUNING.get();
        let Some(flight) = trains
            .lines
            .iter()
            .find(|l| l.vehicle == crate::features::trains::Vehicle::Plane)
        else {
            // The trains' file isn't loaded in this test: its own tests check it.
            return;
        };
        let m = &flight.stops[1];
        let at = crate::features::trains::stop_origin(m);
        assert!(in_miami(at), "Miami's gate is on the island: {at}");
    }

    #[test]
    fn the_crowd_is_on_the_dance_floor_and_hops_on_the_beat() {
        let g = Vec3::new(0.0, 1500.0, 220.0);
        for i in 0..40 {
            let p = g + dancer_spot(i, 40) + Vec3::Y * 0.1;
            let l = p - g;
            assert!(in_venue_at(l), "dancer {i} at {l}");
            assert!(l.z < VENUE.y + VENUE_HALF.y - STAGE_D, "not on the stage");
            assert!(Vec2::new(l.x, l.z).distance(BAR) > 1.0);
        }
        assert!(hop(120.0, 0.0, 0) < 0.01);
        assert!(hop(120.0, 0.25, 0) > 0.15);
    }

    fn in_venue_at(l: Vec3) -> bool {
        (l.x - VENUE.x).abs() < VENUE_HALF.x && (l.z - VENUE.y).abs() < VENUE_HALF.y
    }

    #[test]
    fn the_k_hole_widens_then_settles() {
        let f = 1.0;
        assert!((k_fov(f, 0.0, 30.0) - f).abs() < 1e-4);
        assert!(k_fov(f, 15.0, 30.0) > 1.3);
        assert!((k_fov(f, 30.0, 30.0) - f).abs() < 1e-4);
    }

    #[test]
    fn the_island_is_walled_and_floored() {
        let b = boxes();
        // A slab under the ground's middle.
        assert!(b.iter().any(|(c, s)| c.y < 0.0 && s.x >= HALF.x * 2.0));
        // A way in through the front.
        let door = Vec3::new(VENUE.x, 1.0, VENUE.y - VENUE_HALF.y);
        assert!(!b.iter().any(|(c, s)| {
            let d = (door - *c).abs();
            d.x < s.x / 2.0 && d.y < s.y / 2.0 && d.z < s.z / 2.0
        }));
    }
}
