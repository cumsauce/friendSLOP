//! The surf map. From issue #306 by doomy: "surf map in one of the buildings
//! in nyc" (no details: surprise me).
//!
//! The brownstone on the avenue's east side has a neon SURF sign over its
//! door. Walk up the stoop into the door and the server puts you on the
//! start pad of a surf hall (bigger on the inside: it hangs high over the
//! city). Five stages of ridge ramps with 59 degree faces (S-bends,
//! zig-zag transfers, a big sweep, wiggles into a speed ramp and a jump, a
//! long carve) wind between glass towers to a neon finish pad, 2.4 km of
//! ramp in all. Each stage starts on a rooftop pad: a checkpoint. Inside the hall the walker's air control is cranked
//! up the way surf servers do it ([`crate::rules::surf::air_at`]): the faces
//! are too steep to stand on, so you slide, and strafing into a face while
//! turning keeps you gliding along it and gains speed. Fall off a stage
//! and you're back on its pad; nothing hangs over anything else, so
//! dropping down skips nothing. Reaching the finish pad shows your time and
//! your best this game (each screen keeps its own). A post on each pad takes
//! you back out to the street.
//!
//! The course lives in `data/side/surf.ron` (the stages' pads and ramp
//! points, and the solid boxes), the same joints the hall's model
//! (`surf-hall`, made in the modeler) was built from; the colliders come
//! from the data, so they never wait on the model file. The course is far
//! bigger than the world, so its edge doesn't hold you in the hall.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::surf::*;

/// The start pad (centre, size), hall coordinates: the first stage's pad.
pub fn start_pad(t: &Tuning) -> (Vec3, Vec3) {
    t.stages
        .first()
        .map_or((Vec3::ZERO, Vec3::ONE), |s| pad(s.pad))
}

/// The finish pad.
pub fn finish_pad(t: &Tuning) -> (Vec3, Vec3) {
    pad(t.finish)
}

/// Where you land on the start pad (world).
pub fn start_at(t: &Tuning) -> Vec3 {
    stage_spawn(t, 0).unwrap_or(hall_at(t)) + Vec3::new(0.0, 0.0, 3.0)
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
    let (s, f) = (start_pad(t), finish_pad(t));
    [
        o + s.0 + Vec3::new(3.5, s.1.y / 2.0, 3.5),
        o + f.0 + Vec3::new(5.0, f.1.y / 2.0, -6.0),
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

/// Server: which stage a body has reached (its last checkpoint pad).
#[derive(Component, Default)]
struct OnStage(usize);

/// Server: in through the door, back to your stage's pad when you fall off
/// it, and back to the top when you fall right out.
fn move_bodies(mut commands: Commands, mut bodies: Query<(Entity, Body, Option<&mut OnStage>)>) {
    let t = tuning();
    for (e, (_, mut walker, input, kcc), stage) in &mut bodies {
        if entering(&t, walker.pos) || fallen(&t, walker.pos) {
            put(start_at(&t), &mut walker, input, kcc);
            commands.entity(e).insert(OnStage(0));
            continue;
        }
        let Some(local) = in_hall(&t, walker.pos) else {
            continue;
        };
        let Some(mut stage) = stage else {
            commands.entity(e).insert(OnStage(0));
            continue;
        };
        let (now, fell) = stage_step(&t, stage.0, local);
        if now != stage.0 {
            stage.0 = now;
        }
        if fell && let Some(at) = stage_spawn(&t, now) {
            put(at, &mut walker, input, kcc);
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
    // Colliders never wait on the model: the ramps' pieces and the boxes
    // come from the data file the model was made from.
    let ramps = t.stages.iter().flat_map(|s| s.ramps.iter());
    for ramp in ramps {
        for piece in pieces(ramp, t.ramp_samples, t.ramp_half_width, t.ramp_height) {
            let Some(hull) = avian3d::prelude::Collider::convex_hull(&piece) else {
                continue;
            };
            commands.spawn((
                Transform::from_translation(o),
                avian3d::prelude::RigidBody::Static,
                hull,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    for (c, s) in t.blocks.iter().map(|&b| pad(b)) {
        commands.spawn((
            Transform::from_translation(o + c),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // Out under the sky; a light over the start and the finish.
    for at in [start_pad(&t).0, finish_pad(&t).0] {
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
        Transform::from_xyz(t.sign_at.0, t.sign_at.1, t.sign_at.2)
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
fn run_step(
    run: &mut Run,
    pads: [(Vec3, Vec3); 2],
    local: Option<Vec3>,
    now: f32,
) -> Option<RunEvent> {
    let [start, finish] = pads;
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
    let on_start = on_pad(start, l);
    match run.started {
        None if !on_start && l.z < start.0.z - start.1.z / 2.0 => {
            run.started = Some(now);
            None
        }
        Some(t0) if on_start => {
            run.started = None;
            Some(RunEvent::WipedOut(now - t0))
        }
        Some(t0) if on_pad(finish, l) => {
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
    let pads = [start_pad(&t), finish_pad(&t)];
    match run_step(&mut run, pads, in_hall(&t, w.pos), time.elapsed_secs()) {
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
        assert!(t.door_reach > 0.0 && t.fall_below > 0.0 && t.sign_at.1 > 3.0);
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
    fn the_ramps_are_too_steep_to_stand_on_and_never_stack() {
        let t = t();
        assert!((t.ramp_height / t.ramp_half_width).atan().to_degrees() > 46.0);
        assert!(t.stages.len() >= 3 && t.stages.iter().all(|s| !s.ramps.is_empty()));
        let mut length = 0.0;
        let mut ridges = Vec::new();
        for (i, s) in t.stages.iter().enumerate() {
            for r in &s.ramps {
                let p = ridge(r, t.ramp_samples);
                length += p.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
                assert!(
                    p.last().is_some_and(|e| e.y < p[0].y),
                    "ramps drop as they go"
                );
                ridges.extend(p.into_iter().map(|q| (i, q)));
                assert!(!pieces(r, t.ramp_samples, t.ramp_half_width, t.ramp_height).is_empty());
            }
        }
        assert!(length > 1500.0, "a long course: {length} m");
        // No ramp of one stage hangs over another's: dropping down skips nothing.
        for (i, (a, p)) in ridges.iter().enumerate() {
            for (b, q) in &ridges[i + 1..] {
                assert!(a == b || p.xz().distance(q.xz()) > 14.0, "{p} over {q}");
            }
        }
    }

    #[test]
    fn the_hall_is_in_the_sky_and_holds_everything() {
        let t = t();
        let o = hall_at(&t);
        assert!(o.y > 560.0, "over One World Trade Center");
        let (min, max) = (Vec3::from(t.hall.0), Vec3::from(t.hall.1));
        // Nowhere near the city, whose edges would hold you in it.
        for c in [
            min,
            max,
            Vec3::new(min.x, 0.0, max.z),
            Vec3::new(max.x, 0.0, min.z),
        ] {
            assert!(!crate::rules::nyc::in_city(o.x + c.x, o.z + c.z));
        }
        assert!(unbounded(start_at(&t)) && !unbounded(street_at(&t)));
        assert!(on_pad(start_pad(&t), start_at(&t) - o));
        for p in posts(&t) {
            assert!(in_hall(&t, p).is_some());
        }
        for (i, s) in t.stages.iter().enumerate() {
            let at = stage_spawn(&t, i).expect("stage");
            assert!(on_pad(pad(s.pad), at - o));
            assert!(!fallen(&t, at));
            // Standing on your pad isn't falling off your stage.
            assert_eq!(stage_step(&t, i, at - o), (i, false));
        }
        assert!(fallen(&t, o + Vec3::new(0.0, -t.fall_below - 1.0, -80.0)));
        assert!(finish_pad(&t).0.y > -t.fall_below && in_hall(&t, o + finish_pad(&t).0).is_some());
    }

    #[test]
    fn stage_pads_are_checkpoints() {
        let t = t();
        let o = hall_at(&t);
        let two = stage_spawn(&t, 1).expect("two stages") - o;
        // Landing on stage 2's pad moves you on to it...
        assert_eq!(stage_step(&t, 0, two), (1, false));
        // ...but not back: stage 1's pad from stage 2 changes nothing.
        let one = stage_spawn(&t, 0).expect("start") - o;
        assert_eq!(stage_step(&t, 1, one).0, 1);
        // Falling well below stage 2's ramps puts you back on its pad.
        let low = stage_low(&t, 1).expect("ramps") - t.stage_fall - 1.0;
        assert_eq!(stage_step(&t, 1, Vec3::new(two.x, low, two.z)), (1, true));
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
        let start = (Vec3::new(0.0, -0.5, -5.0), Vec3::new(10.0, 1.0, 10.0));
        let fin = (Vec3::new(0.0, -34.5, -160.0), Vec3::new(16.0, 1.0, 20.0));
        let pads = [start, fin];
        let pad = Vec3::new(0.0, 0.0, -2.0);
        let finish = Vec3::new(0.0, -34.0, -160.0);
        let away = Vec3::new(0.0, -2.0, -12.0);
        assert_eq!(
            run_step(&mut run, pads, Some(pad), 0.0),
            Some(RunEvent::Entered)
        );
        assert_eq!(run_step(&mut run, pads, Some(away), 1.0), None);
        assert_eq!(
            run_step(&mut run, pads, Some(pad), 3.0),
            Some(RunEvent::WipedOut(2.0))
        );
        run_step(&mut run, pads, Some(away), 4.0);
        assert_eq!(
            run_step(&mut run, pads, Some(finish), 14.0),
            Some(RunEvent::Finished {
                secs: 10.0,
                best: 10.0
            })
        );
        run_step(&mut run, pads, Some(away), 20.0);
        assert_eq!(
            run_step(&mut run, pads, Some(finish), 32.0),
            Some(RunEvent::Finished {
                secs: 12.0,
                best: 10.0
            })
        );
        assert_eq!(run_step(&mut run, pads, None, 40.0), None);
        assert_eq!(run.best, Some(10.0));
    }
}
