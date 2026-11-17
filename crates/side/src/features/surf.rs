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
use friendslop_base::areas::AreaApp as _;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker, teleport};
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

/// Client → server: send me back (R). `start` goes to the start pad; without
/// it, to the last checkpoint pad reached.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct RestartSurf {
    pub start: bool,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<LeaveSurf>(Channel::Ordered)
        .add_client_event::<RestartSurf>(Channel::Ordered)
        .add_observer(leave)
        .add_observer(restart)
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
    mut input: Option<Mut<WalkInput>>,
    mut kcc: Option<Mut<crate::kcc::Kcc>>,
) {
    let yaw = walker.yaw;
    teleport(walker, input.as_deref_mut(), kcc.as_deref_mut(), to, yaw);
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

/// Where a restart puts you: the start pad, or the stage's pad you reached.
pub fn restart_at(t: &Tuning, start: bool, stage: usize) -> Vec3 {
    if start {
        start_at(t)
    } else {
        stage_spawn(t, stage).unwrap_or_else(|| start_at(t))
    }
}

/// Server: R in the hall sends you to your checkpoint, or the start.
fn restart(
    ev: On<FromClient<RestartSurf>>,
    mut commands: Commands,
    mut bodies: Query<(Entity, Body, Option<&OnStage>)>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((e, (_, mut walker, input, kcc), stage)) =
        bodies.iter_mut().find(|b| b.1.0.0 == owner)
    else {
        return;
    };
    let t = tuning();
    if in_hall(&t, walker.pos).is_none() {
        return;
    }
    let at = restart_at(&t, ev.start, stage.map_or(0, |s| s.0));
    put(at, &mut walker, input, kcc);
    if ev.start {
        commands.entity(e).insert(OnStage(0));
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

/// `FRIENDSLOP_START=surf`: the start pad.
fn starts() -> Vec<(String, Vec3)> {
    vec![("surf".to_owned(), start_at(&tuning()))]
}

pub fn plugin(app: &mut App) {
    crate::features::start::provide(starts);
    app.add_observer(click)
        .spawn_into("surf", build)
        .spawn_into("nyc", build_sign)
        .init_resource::<Run>()
        .add_systems(Update, timer.run_if(in_state(AppState::InGame)))
        .add_systems(Update, restart_input.run_if(in_state(AppState::InGame)))
        .add_systems(
            Update,
            music_signals
                .in_set(crate::music::MusicSignalSet)
                .run_if(in_state(AppState::InGame)),
        );
}

/// For the music: the camera's in the surf hall (`surf`), so the liquid DnB plays.
/// And `airborne` while your own body is in the hall and not standing on
/// anything (in the air or sliding a ramp): the arp follows it.
fn music_signals(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::world::MapCamera>)>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let t = tuning();
    let at = crate::music::camera_at(&camera);
    signals.flag("surf", at.is_some_and(|p| in_hall(&t, p).is_some()));
    let feet = me.and_then(|me| {
        walkers
            .iter()
            .find(|(p, _)| p.id == me.0)
            .map(|(_, w)| w.pos)
    });
    let airborne = feet
        .and_then(|f| in_hall(&t, f))
        .is_some_and(|local| !standing(&t, local));
    signals.flag("airborne", airborne);
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

/// The neon sign over the brownstone's door (faces the avenue, -x): New York's.
fn build_sign(mut commands: Commands, assets: Res<AssetServer>) {
    let t = tuning();
    commands.spawn((
        model(&assets, "surf-door-sign.gltf"),
        Transform::from_xyz(t.sign_at.0, t.sign_at.1, t.sign_at.2)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        DespawnOnExit(AppState::InGame),
    ));
}

fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = tuning();
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

/// The course's dressing: neon, lights, markers. Nothing here collides.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Dressing {
    /// Each stage's neon colour, in order (the last is reused).
    pub stage_colours: Vec<(f32, f32, f32)>,
    /// The finish's colour.
    pub finish_colour: (f32, f32, f32),
    /// Emissive multiplier for the neon.
    pub glow: f32,
    /// Thickness of the neon strip round each pad (m).
    pub strip: f32,
    /// The light over each stage pad: intensity and range (m).
    pub pad_light: f32,
    pub pad_light_range: f32,
    /// Height of each pad's stage marker post (m).
    pub marker_height: f32,
    /// Lane lights: every this many ridge samples, out past the face's foot,
    /// below the ridge, and their size (m).
    pub lane_every: usize,
    pub lane_out: f32,
    pub lane_drop: f32,
    pub lane_size: f32,
    /// The finish arch's height and width (m).
    pub arch_height: f32,
    pub arch_width: f32,
    /// The city's tower facades, one per tower style (the style wraps).
    pub facades: Vec<Skin>,
    /// The towers' roofs and the building sites' concrete.
    pub roof: Skin,
    pub deck: Skin,
}

/// A city surface: a modeler swatch tiled at real size, and the flat
/// colour it keeps if the swatch file is missing.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Skin {
    /// The swatch model (`models/surfaces/...`).
    pub file: String,
    /// How wide (m) one repeat of its texture is.
    pub tile: f32,
    /// Emissive multiplier for its lit windows / neon (0: none).
    pub glow: f32,
    /// Flat colour (r, g, b, 0..1) and roughness without the swatch.
    pub colour: (f32, f32, f32),
    pub rough: f32,
}

pub static DRESSING: friendslop_data::Data<Dressing> =
    friendslop_data::Data::ron("data/side/surf_dressing.ron");

/// Stage `i`'s colour (the last one repeats; pink with none).
pub fn stage_colour(d: &Dressing, i: usize) -> Vec3 {
    let c = d
        .stage_colours
        .get(i)
        .or(d.stage_colours.last())
        .copied()
        .unwrap_or((1.0, 0.3, 0.85));
    Vec3::from(c)
}

/// Thin boxes (centre, size) tracing a pad's top edge.
pub fn edge_strips(pad: (Vec3, Vec3), thick: f32) -> [(Vec3, Vec3); 4] {
    let (c, s) = pad;
    let y = c.y + s.y / 2.0 + thick / 2.0;
    let (hx, hz) = (s.x / 2.0, s.z / 2.0);
    [
        (Vec3::new(c.x, y, c.z - hz), Vec3::new(s.x, thick, thick)),
        (Vec3::new(c.x, y, c.z + hz), Vec3::new(s.x, thick, thick)),
        (Vec3::new(c.x - hx, y, c.z), Vec3::new(thick, thick, s.z)),
        (Vec3::new(c.x + hx, y, c.z), Vec3::new(thick, thick, s.z)),
    ]
}

/// Where a ramp's lane lights float: beside each face, out past its foot.
pub fn lane_lights(t: &Tuning, d: &Dressing, ramp: &[(f32, f32, f32)]) -> Vec<Vec3> {
    let line = ridge(ramp, t.ramp_samples);
    let every = d.lane_every.max(1);
    let mut out = Vec::new();
    for i in (0..line.len()).step_by(every) {
        let a = line[i.saturating_sub(1)];
        let b = line[(i + 1).min(line.len() - 1)];
        let along = Vec3::new(b.x - a.x, 0.0, b.z - a.z).normalize_or_zero();
        let side = Vec3::new(-along.z, 0.0, along.x);
        let p = line[i] - Vec3::Y * d.lane_drop;
        let out_by = t.ramp_half_width * 1.7 + d.lane_out;
        out.push(p + side * out_by);
        out.push(p - side * out_by);
    }
    out
}

/// This screen's run: when it left the start pad, and its best time.
#[derive(Resource, Default)]
struct Run {
    inside: bool,
    started: Option<f32>,
    /// It reached the finish and hasn't left the finish since: R goes again.
    finished: bool,
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
            run.finished = false;
            None
        }
        Some(t0) if on_start => {
            run.started = None;
            run.finished = false;
            Some(RunEvent::WipedOut(now - t0))
        }
        Some(t0) if through_checkpoint(finish, l, tuning().checkpoint_reach) => {
            run.started = None;
            let secs = now - t0;
            let best = run.best.map_or(secs, |b| b.min(secs));
            run.best = Some(best);
            run.finished = true;
            Some(RunEvent::Finished { secs, best })
        }
        _ => None,
    }
}

/// Does a press of the restart button go to the start? A hold does; a tap
/// goes to the checkpoint, or to the start again once you've finished.
pub fn restart_to_start(held: bool, finished: bool) -> bool {
    held || finished
}

/// Local: R (or the pad's button) in the hall. Hold it to go to the start;
/// tap it for your last checkpoint (or the start again after the finish).
fn restart_input(
    time: Res<Time>,
    actions: crate::input::Actions,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    run: Res<Run>,
    ctx: Option<ResMut<crate::input::InputContexts>>,
    mut commands: Commands,
    mut down: Local<Option<f32>>,
    mut sent: Local<bool>,
) {
    const RESET: crate::input::Act = crate::input::Act("surf_reset");
    let t = tuning();
    let inside = me
        .and_then(|me| walkers.iter().find(|(p, _)| p.id == me.0))
        .is_some_and(|(_, w)| in_hall(&t, w.pos).is_some());
    if let Some(mut ctx) = ctx {
        crate::input::switch(&mut ctx, "surf", inside);
    }
    if !inside {
        *down = None;
        return;
    }
    let now = time.elapsed_secs();
    if actions.just_pressed(RESET) {
        *down = Some(now);
        *sent = false;
    }
    let Some(since) = *down else { return };
    if actions.pressed(RESET) {
        if !*sent && now - since >= t.restart_hold {
            *sent = true;
            commands.client_trigger(RestartSurf { start: true });
        }
        return;
    }
    // Let go: a tap.
    *down = None;
    if !*sent {
        commands.client_trigger(RestartSurf {
            start: restart_to_start(false, run.finished),
        });
    }
}

/// Local: time each run and say how it went.
fn timer(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut run: ResMut<Run>,
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
        crate::features::surf_map::load_course_for_tests();
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*tuning()).clone()
    }

    #[test]
    fn the_dressing_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            DRESSING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(DRESSING.set_bytes(&bytes), "{path} doesn't parse");
        let d = DRESSING.get();
        assert!(d.stage_colours.len() >= 5 && d.glow > 0.0 && d.strip > 0.0);
        assert!(d.pad_light > 0.0 && d.pad_light_range > 0.0 && d.marker_height > 0.0);
        assert!(d.lane_every > 0 && d.lane_size > 0.0 && d.lane_drop > 0.0);
        assert!(d.arch_height > 2.0 && d.arch_width > 0.0);
        assert!(d.facades.len() >= 4);
        for s in d.facades.iter().chain([&d.roof, &d.deck]) {
            assert!(
                s.tile > 0.0 && s.file.starts_with("models/surfaces/"),
                "{s:?}"
            );
        }
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
    fn the_city_crowds_the_ramps_but_never_blocks_them() {
        let t = t();
        let c = city(&t);
        let count = |f: fn(&Piece) -> bool| c.iter().filter(|b| f(&b.piece)).count();
        let towers = count(|p| matches!(p, Piece::Tower { .. }));
        let sites = count(|p| *p == Piece::Site);
        let kickers = count(|p| *p == Piece::Kicker);
        assert!(towers > 40, "{towers} towers");
        assert!(
            sites > 5 && kickers == sites,
            "{sites} sites, {kickers} kickers"
        );
        assert!(count(|p| *p == Piece::Steel) > 5);
        let ridges: Vec<Vec3> = t
            .stages
            .iter()
            .flat_map(|s| s.ramps.iter().flat_map(|r| ridge(r, t.ramp_samples)))
            .collect();
        // Plenty rise above the ramps beside them: you weave between them.
        let above = c
            .iter()
            .filter(|b| matches!(b.piece, Piece::Tower { .. }))
            .filter(|b| {
                ridges
                    .iter()
                    .any(|q| b.ground_distance(q.xz()) < 30.0 && b.top() > q.y + 5.0)
            })
            .count();
        assert!(
            above > towers / 2,
            "{above} of {towers} rise past the ramps"
        );
        // No tower or site reaches into a ramp's span.
        for b in c
            .iter()
            .filter(|b| matches!(b.piece, Piece::Tower { .. } | Piece::Site))
        {
            for q in &ridges {
                assert!(
                    q.y - t.ramp_height > b.top() || b.ground_distance(q.xz()) > t.ramp_half_width,
                    "{b:?} blocks the ramp at {q}"
                );
            }
        }
        // The same city every time (every peer builds its own).
        assert_eq!(city(&t).len(), c.len());
    }

    #[test]
    fn you_stand_on_pads_and_fly_everywhere_else() {
        let t = t();
        let (c, s) = start_pad(&t);
        let top = c + Vec3::Y * (s.y / 2.0);
        assert!(standing(&t, top));
        assert!(!standing(&t, top + Vec3::Y * 1.5));
        assert!(!standing(&t, top + Vec3::new(s.x, 0.0, 0.0)));
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
    fn restart_goes_to_the_checkpoint_or_the_start() {
        let t = t();
        // A hold is the start; a tap is the checkpoint, until you've finished.
        assert!(restart_to_start(true, false));
        assert!(!restart_to_start(false, false));
        assert!(restart_to_start(false, true));
        assert_eq!(restart_at(&t, true, 3), start_at(&t));
        assert_eq!(
            restart_at(&t, false, 3),
            stage_spawn(&t, 3).unwrap_or_default()
        );
        assert_ne!(restart_at(&t, false, 3), start_at(&t));
        assert_eq!(restart_at(&t, false, 99), start_at(&t));
        assert!(t.restart_hold > 0.0);
    }

    #[test]
    fn finishing_marks_the_run_finished_until_you_set_off_again() {
        let t = t();
        let pads = [start_pad(&t), finish_pad(&t)];
        let mut run = Run::default();
        let start = Vec3::new(0.0, -0.5, -5.0);
        run_step(&mut run, pads, Some(start), 0.0);
        run_step(&mut run, pads, Some(Vec3::new(0.0, -34.0, -60.0)), 2.0);
        assert!(run.started.is_some() && !run.finished);
        let f = finish_pad(&t).0;
        run_step(&mut run, pads, Some(f + Vec3::Y), 30.0);
        assert!(run.finished && run.started.is_none());
        run_step(&mut run, pads, Some(Vec3::new(0.0, -34.0, -60.0)), 31.0);
        assert!(!run.finished || run.started.is_none());
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
