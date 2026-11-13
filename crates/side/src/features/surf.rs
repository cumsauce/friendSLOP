//! The surf map. From issue #306 by doomy: "surf map in one of the buildings
//! in nyc" (no details: surprise me).
//!
//! The brownstone on the avenue's east side has a neon SURF sign over its
//! door. Walk up the stoop into the door and the server puts you on the
//! start pad of a surf hall (bigger on the inside: it hangs high over the
//! city). Two long ridge ramps with 59 degree faces drop away from the pad
//! to a neon finish pad. Inside the hall the walker's air control is cranked
//! up the way surf servers do it ([`crate::rules::surf::air_at`]): the faces
//! are too steep to stand on, so you slide, and strafing into a face while
//! turning keeps you gliding along it and gains speed. Fall into the void and
//! you're back on the start pad. Reaching the finish pad shows your time and
//! your best this game (each screen keeps its own). A post on each pad takes
//! you back out to the street.
//!
//! The ramps and pads below are offsets inside the hall's model
//! (`surf-hall`, made in the modeler); their colliders come from here, so
//! they never wait on the file.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::surf::*;

/// A surf ramp: a ridge running from `z.0` to `z.1` (hall coordinates), its
/// top edge dropping from `top.0` to `top.1`, faces `half_width` out and
/// `height` down to each side.
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    pub z: (f32, f32),
    pub top: (f32, f32),
    pub half_width: f32,
    pub height: f32,
}

/// The ramps, as the model has them.
pub const RAMPS: [Ramp; 2] = [
    Ramp {
        z: (-16.0, -76.0),
        top: (-6.0, -14.0),
        half_width: 6.0,
        height: 10.0,
    },
    Ramp {
        z: (-84.0, -144.0),
        top: (-20.0, -28.0),
        half_width: 6.0,
        height: 10.0,
    },
];

impl Ramp {
    /// The six corners of the ridge (hall coordinates).
    pub fn corners(&self) -> Vec<Vec3> {
        let (w, h) = (self.half_width, self.height);
        let mut out = Vec::new();
        for (z, y) in [(self.z.0, self.top.0), (self.z.1, self.top.1)] {
            out.push(Vec3::new(0.0, y, z));
            out.push(Vec3::new(-w, y - h, z));
            out.push(Vec3::new(w, y - h, z));
        }
        out
    }

    /// How steep its faces are, degrees from level.
    pub fn face_degrees(&self) -> f32 {
        (self.height / self.half_width).atan().to_degrees()
    }
}

/// The start pad (centre, size), hall coordinates.
pub const START_PAD: (Vec3, Vec3) = (Vec3::new(0.0, -0.5, -5.0), Vec3::new(10.0, 1.0, 10.0));
/// The finish pad.
pub const FINISH_PAD: (Vec3, Vec3) = (Vec3::new(0.0, -34.5, -160.0), Vec3::new(16.0, 1.0, 20.0));
/// Every box in the hall: the pads, its four walls and the void's floor.
pub const BOXES: [(Vec3, Vec3); 7] = [
    START_PAD,
    FINISH_PAD,
    (Vec3::new(-22.5, -21.0, -86.0), Vec3::new(1.0, 58.0, 180.0)),
    (Vec3::new(22.5, -21.0, -86.0), Vec3::new(1.0, 58.0, 180.0)),
    (Vec3::new(0.0, -21.0, 4.5), Vec3::new(46.0, 58.0, 1.0)),
    (Vec3::new(0.0, -21.0, -176.5), Vec3::new(46.0, 58.0, 1.0)),
    (Vec3::new(0.0, -50.5, -86.0), Vec3::new(46.0, 1.0, 182.0)),
];

/// Is a hall-coordinate point standing on this pad?
pub fn on_pad(pad: (Vec3, Vec3), local: Vec3) -> bool {
    let (c, s) = pad;
    let top = c.y + s.y / 2.0;
    (local.x - c.x).abs() <= s.x / 2.0
        && (local.z - c.z).abs() <= s.z / 2.0
        && (top - 0.3..top + 2.0).contains(&local.y)
}

/// Where you land on the start pad (world).
pub fn start_at(t: &Tuning) -> Vec3 {
    hall_at(t) + Vec3::new(0.0, 0.05, -2.0)
}

/// Does a body with feet at `feet` walk into the brownstone's door?
pub fn entering(t: &Tuning, feet: Vec3) -> bool {
    (0.5..4.0).contains(&feet.y) && feet.xz().distance(Vec2::new(t.door.0, t.door.1)) < t.door_reach
}

/// Has a body in the hall fallen into the void?
pub fn fallen(t: &Tuning, feet: Vec3) -> bool {
    in_hall(t, feet).is_some_and(|l| l.y < -t.fall_below)
}

/// Where you come back out on the street.
pub fn street_at(t: &Tuning) -> Vec3 {
    Vec3::new(t.street.0, crate::rules::nyc::CITY_Y + 0.05, t.street.1)
}

/// Posts out of the hall (world): on the start pad and on the finish pad.
pub fn posts(t: &Tuning) -> [Vec3; 2] {
    let o = hall_at(t);
    [
        o + Vec3::new(3.5, 0.0, -8.5),
        o + Vec3::new(
            FINISH_PAD.0.x + 5.0,
            FINISH_PAD.0.y + 0.5,
            FINISH_PAD.0.z - 6.0,
        ),
    ]
}

// ------------------------------------------------------------------ server

/// Client → server: leave the hall (a post).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct LeaveSurf;

pub fn protocol(app: &mut App) {
    app.add_client_event::<LeaveSurf>(Channel::Ordered)
        .add_observer(leave)
        .add_systems(Update, move_bodies.run_if(in_state(ServerState::Running)));
}

type Body<'a> = (
    &'a PlayerOwner,
    &'a mut Walker,
    Option<&'a mut WalkInput>,
    Option<&'a mut crate::kcc::Kcc>,
);

fn put(
    to: Vec3,
    walker: &mut Walker,
    input: Option<Mut<WalkInput>>,
    kcc: Option<Mut<crate::kcc::Kcc>>,
) {
    walker.pos = to;
    if let Some(mut input) = input {
        input.stick = Vec2::ZERO;
        input.jump = false;
    }
    if let Some(mut kcc) = kcc {
        kcc.velocity = Vec3::ZERO;
    }
}

/// Server: in through the door, and back to the top when you fall.
fn move_bodies(mut bodies: Query<Body>) {
    let t = tuning();
    for (_, mut walker, input, kcc) in &mut bodies {
        if entering(&t, walker.pos) || fallen(&t, walker.pos) {
            put(start_at(&t), &mut walker, input, kcc);
        }
    }
}

fn leave(ev: On<FromClient<LeaveSurf>>, mut commands: Commands, mut bodies: Query<Body>) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((_, mut walker, input, kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner) else {
        return;
    };
    let t = tuning();
    if !posts(&t)
        .iter()
        .any(|p| within(walker.pos, *p, SERVER_REACH + 3.0))
    {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    put(street_at(&t), &mut walker, input, kcc);
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build)
        .add_systems(Update, timer.run_if(in_state(AppState::InGame)));
}

#[derive(Component)]
struct Post;

fn click(ev: On<Clicked>, mut commands: Commands, posts: Query<&Post>) {
    if posts.get(ev.entity).is_ok() {
        commands.client_trigger(LeaveSurf);
    }
}

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    let t = tuning();
    let o = hall_at(&t);
    commands.spawn((
        model(&assets, "surf-hall.gltf"),
        Transform::from_translation(o),
        DespawnOnExit(AppState::InGame),
    ));
    // Colliders never wait on the model.
    for ramp in RAMPS {
        let points: Vec<Vec3> = ramp.corners();
        let Some(hull) = avian3d::prelude::Collider::convex_hull(&points) else {
            continue;
        };
        commands.spawn((
            Transform::from_translation(o),
            avian3d::prelude::RigidBody::Static,
            hull,
            DespawnOnExit(AppState::InGame),
        ));
    }
    for (c, s) in BOXES {
        commands.spawn((
            Transform::from_translation(o + c),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // The hall is roofless under the sky; a light over each pad.
    for at in [START_PAD.0, FINISH_PAD.0] {
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.6, 0.95),
                intensity: 400_000.0,
                range: 30.0,
                ..default()
            },
            Transform::from_translation(o + at + Vec3::Y * 6.0),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // The neon sign over the brownstone's door (faces the avenue, -x).
    commands.spawn((
        model(&assets, "surf-door-sign.gltf"),
        Transform::from_xyz(t.door.0 + 0.4, 4.2, t.door.1)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        DespawnOnExit(AppState::InGame),
    ));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let pole = materials.add(Color::srgb(0.2, 0.2, 0.3));
    let sign = materials.add(Color::srgb(1.0, 0.3, 0.85));
    for at in posts(&t) {
        commands
            .spawn((
                Post,
                Transform::from_translation(at),
                Visibility::default(),
                // Read once a game from the data file.
                Clickable::at_height(1.6)
                    .radius(60.0)
                    .labelled(t.exit_label.clone().leak()),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(pole.clone()),
                    Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(0.12, 2.2, 0.12)),
                ));
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(sign.clone()),
                    Transform::from_xyz(0.0, 1.9, 0.0).with_scale(Vec3::new(1.2, 0.35, 0.06)),
                ));
            });
    }
}

/// This screen's run: when it left the start pad, and its best time.
#[derive(Default)]
struct Run {
    inside: bool,
    started: Option<f32>,
    best: Option<f32>,
}

/// What a run's step did, for the toasts.
#[derive(Debug, PartialEq)]
pub enum RunEvent {
    Entered,
    WipedOut(f32),
    Finished { secs: f32, best: f32 },
}

/// One step of a screen's run timer for feet at `local` (hall coordinates,
/// `None` outside the hall) at time `now`. Pure, so it's tested.
fn run_step(run: &mut Run, local: Option<Vec3>, now: f32) -> Option<RunEvent> {
    let Some(l) = local else {
        *run = Run {
            best: run.best,
            ..default()
        };
        return None;
    };
    if !run.inside {
        run.inside = true;
        return Some(RunEvent::Entered);
    }
    let on_start = on_pad(START_PAD, l);
    match run.started {
        None if !on_start && l.z < START_PAD.0.z - START_PAD.1.z / 2.0 => {
            run.started = Some(now);
            None
        }
        Some(t0) if on_start => {
            run.started = None;
            Some(RunEvent::WipedOut(now - t0))
        }
        Some(t0) if on_pad(FINISH_PAD, l) => {
            run.started = None;
            let secs = now - t0;
            let best = run.best.map_or(secs, |b| b.min(secs));
            run.best = Some(best);
            Some(RunEvent::Finished { secs, best })
        }
        _ => None,
    }
}

/// Local: time each run and say how it went.
fn timer(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut run: Local<Run>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let t = tuning();
    let secs = |s: f32| format!("{s:.2}");
    match run_step(&mut run, in_hall(&t, w.pos), time.elapsed_secs()) {
        Some(RunEvent::Entered) => toast.show(t.enter_line.clone()),
        Some(RunEvent::WipedOut(s)) => toast.show(t.wipeout_line.replace("{secs}", &secs(s))),
        Some(RunEvent::Finished { secs: s, best }) => toast.show(
            t.finish_line
                .replace("{secs}", &secs(s))
                .replace("{best}", &secs(best)),
        ),
        None => {}
    }
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
        assert!(t.door_reach > 0.0 && t.fall_below > 0.0);
        assert!(
            t.air_accel_hz > 10.0,
            "surf needs more air control than walking"
        );
        assert!(t.max_speed > 30.0 && t.air_max_wish > 0.0);
        assert!(!t.enter_line.is_empty() && !t.exit_label.is_empty());
        assert!(t.wipeout_line.contains("{secs}"));
        assert!(t.finish_line.contains("{secs}") && t.finish_line.contains("{best}"));
    }

    #[test]
    fn the_ramps_are_too_steep_to_stand_on() {
        for r in RAMPS {
            assert!(r.face_degrees() > 46.0, "{r:?}");
            assert!(r.top.1 < r.top.0, "ramps drop as they go");
            assert_eq!(r.corners().len(), 6);
        }
    }

    #[test]
    fn the_hall_is_over_the_city_and_holds_everything() {
        let t = t();
        let o = hall_at(&t);
        assert!(o.y > 560.0, "over One World Trade Center");
        for c in [HALL_MIN, HALL_MAX] {
            assert!(crate::rules::nyc::in_city(o.x + c.x, o.z + c.z));
        }
        assert!(in_hall(&t, start_at(&t)).is_some());
        assert!(on_pad(START_PAD, start_at(&t) - o));
        for p in posts(&t) {
            assert!(in_hall(&t, p).is_some());
        }
        assert!(fallen(&t, o + Vec3::new(0.0, -t.fall_below - 1.0, -80.0)));
        assert!(!fallen(&t, start_at(&t)));
        // The finish pad is above where you fall out.
        assert!(FINISH_PAD.0.y > -t.fall_below);
    }

    #[test]
    fn the_door_lets_you_in_and_the_street_doesnt() {
        let t = t();
        let door = Vec3::new(t.door.0, 1.55, t.door.1);
        assert!(entering(&t, door));
        assert!(!entering(&t, street_at(&t)));
        assert!(air_at(street_at(&t)) == crate::kcc::Air::NORMAL);
        assert!(air_at(start_at(&t)).accel_hz > crate::kcc::Air::NORMAL.accel_hz);
    }

    #[test]
    fn a_run_is_timed_from_the_pad_to_the_finish() {
        let mut run = Run::default();
        let pad = Vec3::new(0.0, 0.0, -2.0);
        let finish = Vec3::new(0.0, -34.0, -160.0);
        assert_eq!(run_step(&mut run, Some(pad), 0.0), Some(RunEvent::Entered));
        assert_eq!(
            run_step(&mut run, Some(Vec3::new(0.0, -2.0, -12.0)), 1.0),
            None
        );
        assert_eq!(
            run_step(&mut run, Some(pad), 3.0),
            Some(RunEvent::WipedOut(2.0))
        );
        run_step(&mut run, Some(Vec3::new(0.0, -2.0, -12.0)), 4.0);
        assert_eq!(
            run_step(&mut run, Some(finish), 14.0),
            Some(RunEvent::Finished {
                secs: 10.0,
                best: 10.0
            })
        );
        run_step(&mut run, Some(Vec3::new(0.0, -2.0, -12.0)), 20.0);
        assert_eq!(
            run_step(&mut run, Some(finish), 32.0),
            Some(RunEvent::Finished {
                secs: 12.0,
                best: 10.0
            })
        );
        assert_eq!(run_step(&mut run, None, 40.0), None);
        assert_eq!(run.best, Some(10.0));
    }
}
