//! Wonderland, the arcade off stage left of the Michael Jackson Experience:
//! its Skee-Ball lanes and dartboards are playable.
//!
//! The machines are modeler markers on the theatre map (`Arcade`:
//! `skeeball` lane starts, `skeeball_hole`s with their points, `dartboard`s
//! and `oche`s), read from `maps/michael.map.json` ([`super::michael::MAP`]).
//! Click a lane or a board to start. Your hands and eyes go to the machine
//! (like the Kori claw), your own game runs on your own screen (local, like
//! the claw's pit), and `arcade_quit` walks you away.
//!
//! - **Skee-Ball**: nine balls a game. Aim along the lane with the mouse (or
//!   the walk keys), hold `arcade_throw` to charge the throw (the meter swings
//!   up and down) and let go to roll. The ball is a little 2D physics body
//!   ([`skee_step`]) rolling up the lane, over the ramp's lip and onto the
//!   slanted board, where it drops into a hole or rolls back. Holes: 10, 20,
//!   30, 40, 50 and a 100 in each top corner.
//! - **Darts**: 501, three darts a visit, finish on a double ([`outcome`]); a
//!   bust (below zero, one left, or zero without a double) puts the score back
//!   to where the visit started. Look about with the mouse, hold `arcade_steady`
//!   to hold your breath (it steadies the hand for a few seconds), and throw
//!   with `arcade_throw`. Scoring is the real board: sector order, trebles
//!   and doubles, the bull and the outer bull ([`board_hit`]).
//!
//! The numbers (speeds, sway, the lane's profile, the board's radii and
//! sector order) are in `data/side/wonderland.ron`.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use super::michael::{AREA, MAP, theater_at};
use crate::AppState;
use crate::features::fps::HandsBusy;
use crate::features::interact::{Clickable, Clicked};

// ------------------------------------------------------------------ tuning

/// `data/side/wonderland.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub skeeball: SkeeTuning,
    pub darts: DartTuning,
}

/// Skee-Ball's numbers. Lane frame: the lane marker is the origin (the
/// ball's start), `x` runs toward the board, `y` is up, `z` across.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct SkeeTuning {
    /// Balls a game.
    pub balls: u32,
    pub ball_radius: f32,
    /// Roll speed (m/s) at no charge and at full charge.
    pub speed_min: f32,
    pub speed_max: f32,
    /// Seconds for the charge meter to go from empty to full (it swings back).
    pub charge_secs: f32,
    /// Metres either side of the middle the ball may be lined up.
    pub aim_limit: f32,
    /// Metres of aim per pixel of mouse, and per second of a walk key.
    pub aim_per_px: f32,
    pub aim_key_speed: f32,
    pub gravity: f32,
    /// Speed (m/s) a ball loses every second just rolling.
    pub roll_drag: f32,
    /// The lane's side profile `(distance toward the board, height)` from the
    /// start marker: the flat, then the launch ramp up to its lip.
    pub profile: Vec<(f32, f32)>,
    /// Where the ramp's lip is (m toward the board): past it the ball flies.
    pub lip: f32,
    /// The slanted board, bottom and top `(distance, height)`.
    pub board: ((f32, f32), (f32, f32)),
    /// How much of its speed's height the ball keeps landing on the board
    /// (0..1): how far up the board a given launch reaches.
    pub board_energy: f32,
    /// How far (m along the board) from where the ball tops out a hole still
    /// takes it, and how far (m across) from the hole's line.
    pub reach_tolerance: f32,
    pub aim_tolerance: f32,
    /// A ball rolled back past this (m) is home again.
    pub return_x: f32,
    /// Seconds of the flight onto the board, of the ball sinking into a hole,
    /// and of the slide back down when it misses.
    pub flight_secs: f32,
    pub sink_secs: f32,
    pub slide_secs: f32,
    /// How high (m) the ball arcs between the lip and the board.
    pub flight_arc: f32,
    /// Where your eye is while you play, from the start marker: distance
    /// along the lane, height, and where it looks (distance, height).
    pub eye: (f32, f32, f32),
    pub look: (f32, f32),
    /// Seconds between a ball ending and the next one being ready.
    pub rest_secs: f32,
}

/// Darts' numbers.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct DartTuning {
    /// 501 and out on a double.
    pub start: i32,
    pub per_visit: u32,
    /// How far (m) your hand wanders on the board, and how far while holding
    /// your breath.
    pub sway: f32,
    pub steady_sway: f32,
    /// Seconds of breath, and how fast it comes back (seconds per second).
    pub breath_secs: f32,
    pub breath_regain: f32,
    /// How far (degrees) you can look from the board, and radians per pixel.
    pub look_limit_deg: f32,
    pub rad_per_px: f32,
    /// How far (m) the release scatters a dart whatever the hand did.
    pub release_scatter: f32,
    /// Seconds a dart flies, and the arc's height (m).
    pub flight_secs: f32,
    pub flight_arc: f32,
    /// Your eye's height over the oche (m).
    pub eye_height: f32,
    /// Seconds the darts stay on the board at the end of a visit.
    pub clear_secs: f32,
    pub radii: Radii,
    /// The numbers round the board, clockwise from the top.
    pub sector_order: Vec<u32>,
}

/// The board's rings (metres from the bull).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Radii {
    pub bull: f32,
    pub outer_bull: f32,
    pub treble_in: f32,
    pub treble_out: f32,
    pub double_in: f32,
    pub double_out: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/wonderland.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------ Skee-Ball

/// A scoring hole, in the lane frame.
#[derive(Clone, Copy, Debug)]
pub struct Hole {
    pub p: Vec3,
    pub radius: f32,
    pub points: u32,
}

/// The lane's surface height at distance `x` along the profile.
pub fn lane_height(profile: &[(f32, f32)], x: f32) -> Option<f32> {
    profile.windows(2).find_map(|w| {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        ((x0..=x1).contains(&x) && (x1 - x0).abs() > 1e-6)
            .then(|| y0 + (y1 - y0) * (x - x0) / (x1 - x0))
    })
}

/// The lane's slope (rise over run) at `x`.
fn lane_slope(profile: &[(f32, f32)], x: f32) -> f32 {
    profile
        .windows(2)
        .find(|w| (w[0].0..=w[1].0).contains(&x) && (w[1].0 - w[0].0).abs() > 1e-6)
        .map_or(0.0, |w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0))
}

/// One step of a ball rolling along the lane (horizontal position `x`, speed
/// `v` toward the board): gravity along the ramp and a little drag. Pure.
pub fn roll_step(x: &mut f32, v: &mut f32, dt: f32, t: &SkeeTuning) {
    let slope = lane_slope(&t.profile, *x);
    let along = -t.gravity * slope / (1.0 + slope * slope);
    let drag = t.roll_drag * v.signum();
    *v += (along - drag) * dt;
    *x += *v * dt;
}

/// Where a ball launched off the lip lands and what it does there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    /// How far up the board (metres along it) the ball tops out.
    pub reach: f32,
    /// The hole it drops into (index into the lane's holes), if any.
    pub hole: Option<usize>,
}

/// The board's direction (unit, in the lane's distance/height plane) and its
/// length.
pub fn board_dir(t: &SkeeTuning) -> (Vec2, f32) {
    let (a, b) = (
        Vec2::new(t.board.0.0, t.board.0.1),
        Vec2::new(t.board.1.0, t.board.1.1),
    );
    ((b - a).normalize_or_zero(), (b - a).length())
}

/// Metres along the board from its bottom to `hole`.
pub fn hole_reach(h: &Hole, t: &SkeeTuning) -> f32 {
    let (dir, _) = board_dir(t);
    (Vec2::new(h.p.x, h.p.y) - Vec2::new(t.board.0.0, t.board.0.1)).dot(dir)
}

/// What a ball that leaves the lip at horizontal speed `v` and lateral
/// position `aim` does: it climbs the board as far as its energy takes it
/// (`board_energy` of the speed's height, over the board's slope) and drops
/// into the hole it tops out at, if there is one. Pure.
pub fn launch(t: &SkeeTuning, holes: &[Hole], aim: f32, v: f32) -> Shot {
    let (dir, len) = board_dir(t);
    let reach = t.board_energy * v * v / (2.0 * t.gravity * dir.y.max(0.05));
    let hole = (reach <= len)
        .then(|| {
            holes
                .iter()
                .enumerate()
                .filter(|(_, h)| (h.p.z - aim).abs() <= t.aim_tolerance)
                .map(|(i, h)| (i, (hole_reach(h, t) - reach).abs()))
                .filter(|&(_, d)| d <= t.reach_tolerance)
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
        })
        .flatten();
    Shot { reach, hole }
}

/// The speed at the lip of a ball rolled from the start at `speed`: `None` if
/// it never gets over (it rolls back). Pure, for tests and the data check.
pub fn lip_speed(t: &SkeeTuning, speed: f32) -> Option<f32> {
    let (mut x, mut v) = (0.0, speed);
    for _ in 0..(10.0 / (1.0 / 240.0)) as usize {
        roll_step(&mut x, &mut v, 1.0 / 240.0, t);
        if x >= t.lip {
            return Some(v);
        }
        if v <= 0.0 || x < t.return_x {
            return None;
        }
    }
    None
}

/// A whole throw's score, for tests: roll at `speed` lined up at `aim`.
pub fn throw_score(t: &SkeeTuning, holes: &[Hole], aim: f32, speed: f32) -> u32 {
    lip_speed(t, speed)
        .and_then(|v| launch(t, holes, aim, v).hole)
        .map_or(0, |i| holes[i].points)
}

// ----------------------------------------------------------------- darts

/// Where a dart landed on the board, and what it's worth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub points: u32,
    /// Counts as a double for finishing (a double ring, or the bull's eye).
    pub double: bool,
    /// 3 for a treble, 2 for a double or the bull's eye, 1 otherwise, 0 for a miss.
    pub mult: u32,
    pub number: u32,
}

impl Hit {
    /// "T20", "D16", "25", "50", "7", "miss".
    pub fn label(&self) -> String {
        match (self.mult, self.number) {
            (0, _) => "miss".into(),
            (2, 25) => "bull".into(),
            (1, 25) => "25".into(),
            (3, n) => format!("T{n}"),
            (2, n) => format!("D{n}"),
            (_, n) => n.to_string(),
        }
    }
}

/// The score of a dart at `(u, v)` metres from the bull (`u` to the right,
/// `v` up, as the thrower sees the board).
pub fn board_hit(u: f32, v: f32, r: &Radii, order: &[u32]) -> Hit {
    let dist = (u * u + v * v).sqrt();
    let miss = Hit {
        points: 0,
        double: false,
        mult: 0,
        number: 0,
    };
    if dist <= r.bull {
        return Hit {
            points: 50,
            double: true,
            mult: 2,
            number: 25,
        };
    }
    if dist <= r.outer_bull {
        return Hit {
            points: 25,
            double: false,
            mult: 1,
            number: 25,
        };
    }
    if dist > r.double_out || order.is_empty() {
        return miss;
    }
    // Clockwise from the top; each number's wedge is centred on its own angle.
    let turn = u.atan2(v).to_degrees().rem_euclid(360.0);
    let wedge = 360.0 / order.len() as f32;
    let number = order[(((turn + wedge / 2.0) / wedge) as usize) % order.len()];
    let mult = if (r.treble_in..=r.treble_out).contains(&dist) {
        3
    } else if dist >= r.double_in {
        2
    } else {
        1
    };
    Hit {
        points: number * mult,
        double: mult == 2,
        mult,
        number,
    }
}

/// What a dart did to a visit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still playing, this much left.
    Left(i32),
    /// Over: below zero, one left, or zero without a double. The visit's
    /// score is void.
    Bust,
    /// Out on a double.
    Out,
}

/// The standard 501 rules: `left` before the dart, the dart's `hit`.
pub fn outcome(left: i32, hit: &Hit) -> Outcome {
    let now = left - hit.points as i32;
    match now {
        n if n < 0 || n == 1 => Outcome::Bust,
        0 if hit.double => Outcome::Out,
        0 => Outcome::Bust,
        n => Outcome::Left(n),
    }
}

// ----------------------------------------------------------------- the map

/// A Skee-Ball lane on the map (world frame).
#[derive(Component, Clone, Debug)]
pub struct Lane {
    pub id: u32,
    /// The ball's start, world.
    pub start: Vec3,
    /// Along the lane toward the board (horizontal, unit).
    pub forward: Vec3,
    pub holes: Vec<Hole>,
}

impl Lane {
    /// A lane-frame point in the world.
    pub fn world(&self, p: Vec3) -> Vec3 {
        let right = self.forward.cross(Vec3::Y);
        self.start + self.forward * p.x + Vec3::Y * p.y + right * p.z
    }
}

/// A dartboard on the map (world frame).
#[derive(Component, Clone, Debug)]
pub struct Board {
    pub id: u32,
    /// The bull, world.
    pub center: Vec3,
    /// Out of the board's face (toward the thrower), horizontal, unit.
    pub normal: Vec3,
    /// The throw line, world.
    pub oche: Vec3,
}

impl Board {
    /// To the thrower's right as they face the board.
    pub fn right(&self) -> Vec3 {
        (-self.normal).cross(Vec3::Y)
    }
}

/// Every lane and board the map's `Arcade` markers describe.
pub fn machines() -> (Vec<Lane>, Vec<Board>) {
    let map = MAP.get();
    let origin = theater_at();
    let arcade: Vec<_> = map.entities.iter().filter(|e| e.has("Arcade")).collect();
    let at = |e: &friendslop_world::places::MapEntity| origin + Vec3::from_array(e.translation);
    let facing = |e: &friendslop_world::places::MapEntity| {
        Quat::from_rotation_y(e.yaw.to_radians()) * Vec3::Z
    };
    let mut lanes = Vec::new();
    for e in arcade
        .iter()
        .filter(|e| e.text("Arcade", "game") == "skeeball")
    {
        let id = e.num("Arcade", "id") as u32;
        let start = at(e);
        // The lane runs away from the player: the marker faces the player.
        let forward = -facing(e);
        let right = forward.cross(Vec3::Y);
        let holes = arcade
            .iter()
            .filter(|h| {
                h.text("Arcade", "game") == "skeeball_hole" && h.num("Arcade", "id") as u32 == id
            })
            .map(|h| {
                let rel = at(h) - start;
                Hole {
                    p: Vec3::new(rel.dot(forward), rel.y, rel.dot(right)),
                    radius: h.num("Arcade", "radius"),
                    points: h.num("Arcade", "points") as u32,
                }
            })
            .collect();
        lanes.push(Lane {
            id,
            start,
            forward,
            holes,
        });
    }
    let mut boards = Vec::new();
    for e in arcade
        .iter()
        .filter(|e| e.text("Arcade", "game") == "dartboard")
    {
        let id = e.num("Arcade", "id") as u32;
        let oche = arcade
            .iter()
            .find(|o| o.text("Arcade", "game") == "oche" && o.num("Arcade", "id") as u32 == id)
            .map_or_else(|| at(e) + facing(e) * 2.4, |o| at(o));
        boards.push(Board {
            id,
            center: at(e),
            normal: facing(e),
            oche,
        });
    }
    (lanes, boards)
}

// --------------------------------------------------------------- the games

const QUIT_ACTION: &str = "arcade_quit";
const STEADY_ACTION: &str = "arcade_steady";
const BALL_MODEL: &str = "models/prefabs/wl-skeeball-ball.gltf";
const DART_MODEL: &str = "models/prefabs/wl-dart.gltf";

/// Your game, on your own screen (nothing here is replicated).
#[derive(Resource, Default)]
pub struct Playing(Option<Session>);

enum Session {
    Skee(SkeeGame),
    Darts(DartGame),
}

struct SkeeGame {
    lane: Entity,
    /// Balls still to roll, and the score so far.
    left: u32,
    score: u32,
    /// Where the next ball is lined up across the lane (m).
    aim: f32,
    /// Charge meter 0..1 while holding, and its phase.
    charge: Option<f32>,
    ball: Option<Rolling>,
    /// Seconds until the next ball is ready.
    rest: f32,
    /// Each ball's score so far (the scoreboard line).
    rolled: Vec<u32>,
    /// The game's over (all balls rolled): throw to go again.
    over: bool,
}

struct Rolling {
    entity: Entity,
    stage: Stage,
    /// Seconds in this stage.
    secs: f32,
    /// Where it was lined up across the lane.
    aim: f32,
}

/// Where a thrown ball is in its trip.
#[derive(Clone, Copy, Debug)]
enum Stage {
    /// Rolling up the lane: distance along it and speed toward the board.
    Lane { x: f32, v: f32 },
    /// Flying from the lip to the board.
    Air(Shot),
    /// Dropping into a hole.
    Sink(Shot),
    /// Missed: sliding back down the board from `reach` metres up it.
    Slide(f32),
}

/// A point `s` metres up the board at `lat` across, with the ball's centre
/// resting on its surface (lane frame).
fn on_board(t: &SkeeTuning, s: f32, lat: f32) -> Vec3 {
    let (dir, _) = board_dir(t);
    let p = Vec2::new(t.board.0.0, t.board.0.1) + dir * s;
    let up = Vec2::new(-dir.y, dir.x) * t.ball_radius;
    Vec3::new(p.x + up.x, p.y + up.y, lat)
}

struct DartGame {
    board: Entity,
    left: i32,
    /// Where this visit started (a bust goes back to it).
    visit_start: i32,
    /// Darts thrown in this visit and visits so far.
    thrown: u32,
    visits: u32,
    /// Look away from the board's middle (yaw right, pitch up, radians).
    look: Vec2,
    breath: f32,
    steadying: bool,
    /// Seconds into the throw sway (drives the wobble).
    clock: f32,
    /// Darts in the board, with the time they fly to it, and the visit's end.
    flying: Vec<Flying>,
    /// Counting down to clearing the board after a visit.
    clear: Option<f32>,
    last: String,
    over: bool,
}

struct Flying {
    entity: Entity,
    from: Vec3,
    to: Vec3,
    secs: f32,
}

/// Everything a game spawned (balls, darts, the reticle).
#[derive(Component)]
struct Prop;

/// The little dot showing where your hand is on the board.
#[derive(Component)]
struct Reticle;

/// The status line.
#[derive(Component)]
struct Status;

fn spawn_machines(mut commands: Commands) {
    let (lanes, boards) = machines();
    for lane in lanes {
        commands.spawn((
            Transform::from_translation(lane.start),
            Visibility::default(),
            Clickable::at_height(0.0)
                .labelled("play Skee-Ball")
                .radius(48.0),
            lane,
            friendslop_base::areas::sim(AREA),
        ));
    }
    for board in boards {
        commands.spawn((
            Transform::from_translation(board.center),
            Visibility::default(),
            Clickable::at_height(0.0)
                .labelled("play darts")
                .radius(48.0),
            board,
            friendslop_base::areas::sim(AREA),
        ));
    }
}

fn model(assets: &AssetServer, file: &str) -> bevy::world_serialization::WorldAssetRoot {
    bevy::world_serialization::WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(file.to_string())),
    )
}

fn click(
    c: On<Clicked>,
    mut commands: Commands,
    mut playing: ResMut<Playing>,
    mut busy: ResMut<HandsBusy>,
    lanes: Query<&Lane>,
    boards: Query<&Board>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if playing.0.is_some() {
        return;
    }
    let t = tuning();
    if lanes.get(c.entity).is_ok() {
        playing.0 = Some(Session::Skee(SkeeGame {
            lane: c.entity,
            left: t.skeeball.balls,
            score: 0,
            aim: 0.0,
            charge: None,
            ball: None,
            rest: 0.0,
            rolled: Vec::new(),
            over: false,
        }));
        busy.0 = true;
    } else if let Ok(board) = boards.get(c.entity) {
        playing.0 = Some(Session::Darts(DartGame {
            board: c.entity,
            left: t.darts.start,
            visit_start: t.darts.start,
            thrown: 0,
            visits: 1,
            look: Vec2::ZERO,
            breath: t.darts.breath_secs,
            steadying: false,
            clock: 0.0,
            flying: Vec::new(),
            clear: None,
            last: String::new(),
            over: false,
        }));
        busy.0 = true;
        // The dot that shows where your hand is.
        commands.spawn((
            Reticle,
            Prop,
            Mesh3d(meshes.add(Sphere::new(0.006))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.1, 0.1),
                emissive: LinearRgba::rgb(8.0, 0.2, 0.2),
                unlit: true,
                ..default()
            })),
            Transform::from_translation(board.center + board.normal * 0.02),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// The ball as it sits ready, or flies.
fn spawn_ball(commands: &mut Commands, assets: &AssetServer, at: Vec3) -> Entity {
    commands
        .spawn((
            Prop,
            Transform::from_translation(at),
            Visibility::default(),
            model(assets, BALL_MODEL),
            DespawnOnExit(AppState::InGame),
        ))
        .id()
}

fn leave(
    commands: &mut Commands,
    playing: &mut Playing,
    busy: &mut HandsBusy,
    props: &Query<Entity, With<Prop>>,
) {
    playing.0 = None;
    busy.0 = false;
    for e in props {
        commands.entity(e).despawn();
    }
}

/// Mouse movement this frame plus the pad/touch look (radians treated as pixels).
fn look_delta(
    motion: &AccumulatedMouseMotion,
    virt: &Option<Res<crate::controls::VirtualInput>>,
) -> Vec2 {
    motion.delta + virt.as_ref().map_or(Vec2::ZERO, |v| v.look * 900.0)
}

fn throw_held(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    keys.pressed(KeyCode::Space) || mouse.pressed(MouseButton::Left)
}

fn throw_pressed(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Left)
}

#[allow(clippy::too_many_arguments)]
fn skee_play(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    actions: crate::input::Actions,
    virt: Option<Res<crate::controls::VirtualInput>>,
    assets: Res<AssetServer>,
    mut playing: ResMut<Playing>,
    mut busy: ResMut<HandsBusy>,
    toast: Option<ResMut<crate::hud::Toast>>,
    lanes: Query<&Lane>,
    props: Query<Entity, With<Prop>>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(Session::Skee(g)) = playing.0.as_mut() else {
        return;
    };
    if actions.just_pressed(crate::input::Act(QUIT_ACTION)) {
        leave(&mut commands, &mut playing, &mut busy, &props);
        return;
    }
    let Ok(lane) = lanes.get(g.lane) else {
        leave(&mut commands, &mut playing, &mut busy, &props);
        return;
    };
    let t = tuning();
    let s = &t.skeeball;
    let dt = time.delta_secs().min(1.0 / 20.0);

    // Line the next ball up.
    let key_aim = (keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight)) as i8
        - (keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft)) as i8;
    let stick = virt.as_ref().map_or(0.0, |v| v.stick.x);
    let aim_d = look_delta(&motion, &virt).x * s.aim_per_px
        + (key_aim as f32 + stick) * s.aim_key_speed * dt;
    if g.ball.is_none() && !g.over {
        g.aim = (g.aim + aim_d).clamp(-s.aim_limit, s.aim_limit);
    }

    // The ball in play.
    if let Some(r) = g.ball.as_mut() {
        r.secs += dt;
        let mut next: Option<Stage> = None;
        let mut done: Option<u32> = None;
        let pos = match r.stage {
            Stage::Lane { mut x, mut v } => {
                let steps = ((dt * 240.0).ceil() as usize).max(1);
                for _ in 0..steps {
                    roll_step(&mut x, &mut v, dt / steps as f32, s);
                }
                if x >= s.lip {
                    next = Some(Stage::Air(launch(s, &lane.holes, r.aim, v)));
                } else if x < s.return_x {
                    done = Some(0);
                } else {
                    r.stage = Stage::Lane { x, v };
                }
                let y = lane_height(&s.profile, x.max(0.0)).unwrap_or(0.0) + s.ball_radius;
                Vec3::new(x, y, r.aim)
            }
            Stage::Air(shot) => {
                let k = (r.secs / s.flight_secs.max(0.05)).min(1.0);
                let len = board_dir(s).1;
                let from = Vec3::new(
                    s.lip,
                    lane_height(&s.profile, s.lip).unwrap_or(0.0) + s.ball_radius,
                    r.aim,
                );
                let to = match shot.hole {
                    Some(i) => lane.holes[i].p + Vec3::new(0.0, s.ball_radius * 0.5, 0.0),
                    None => on_board(s, shot.reach.min(len), r.aim),
                };
                if k >= 1.0 {
                    next = Some(if shot.hole.is_some() {
                        Stage::Sink(shot)
                    } else {
                        Stage::Slide(shot.reach.min(len))
                    });
                }
                from.lerp(to, k) + Vec3::Y * s.flight_arc * 4.0 * k * (1.0 - k)
            }
            Stage::Sink(shot) => {
                let k = (r.secs / s.sink_secs.max(0.05)).min(1.0);
                let hole = shot.hole.map_or(Vec3::ZERO, |i| lane.holes[i].p);
                if k >= 1.0 {
                    done = Some(shot.hole.map_or(0, |i| lane.holes[i].points));
                }
                hole + Vec3::Y * (s.ball_radius * 0.5 - k * s.ball_radius * 2.0)
            }
            Stage::Slide(reach) => {
                let k = (r.secs / s.slide_secs.max(0.05)).min(1.0);
                if k >= 1.0 {
                    done = Some(0);
                }
                on_board(s, reach * (1.0 - k * k), r.aim)
            }
        };
        if let Some(n) = next {
            r.stage = n;
            r.secs = 0.0;
        }
        if let Ok(mut tf) = transforms.get_mut(r.entity) {
            tf.translation = lane.world(pos) - Vec3::Y * s.ball_radius;
        }
        if let Some(points) = done {
            commands.entity(r.entity).despawn();
            g.score += points;
            g.rolled.push(points);
            g.ball = None;
            g.rest = s.rest_secs;
            if g.left == 0 {
                g.over = true;
                if let Some(mut toast) = toast {
                    toast.show(format!("Skee-Ball: {} points", g.score));
                }
            }
        }
        return;
    }
    if g.over {
        if throw_pressed(&keys, &mouse) {
            g.left = s.balls;
            g.score = 0;
            g.rolled.clear();
            g.over = false;
            g.charge = None;
        }
        return;
    }
    if g.rest > 0.0 {
        g.rest -= dt;
        return;
    }

    // Charge and roll.
    if throw_held(&keys, &mouse) {
        let c = g.charge.unwrap_or(0.0) + dt / s.charge_secs.max(0.1);
        g.charge = Some(c);
    } else if let Some(c) = g.charge.take() {
        // The meter swings up and back down.
        let m = 1.0 - (c.rem_euclid(2.0) - 1.0).abs();
        let speed = s.speed_min + (s.speed_max - s.speed_min) * m;
        let y = lane_height(&s.profile, 0.0).unwrap_or(0.0);
        let entity = spawn_ball(&mut commands, &assets, lane.world(Vec3::new(0.0, y, g.aim)));
        g.left = g.left.saturating_sub(1);
        g.ball = Some(Rolling {
            entity,
            stage: Stage::Lane { x: 0.0, v: speed },
            secs: 0.0,
            aim: g.aim,
        });
    }
}

/// The ball waiting in your hand at the lane's start (a model that follows your aim).
fn skee_ready(
    mut commands: Commands,
    assets: Res<AssetServer>,
    playing: Res<Playing>,
    lanes: Query<&Lane>,
    mut ready: Local<Option<Entity>>,
    mut transforms: Query<&mut Transform, (With<Prop>, Without<Reticle>)>,
) {
    let Some(Session::Skee(g)) = playing.0.as_ref() else {
        *ready = None;
        return;
    };
    let Ok(lane) = lanes.get(g.lane) else { return };
    let s = &tuning().skeeball;
    let waiting = g.ball.is_none() && !g.over && g.rest <= 0.0;
    match (*ready, waiting) {
        (None, true) => {
            let y = lane_height(&s.profile, 0.0).unwrap_or(0.0);
            *ready = Some(spawn_ball(
                &mut commands,
                &assets,
                lane.world(Vec3::new(0.0, y, g.aim)),
            ));
        }
        (Some(e), true) => {
            let y = lane_height(&s.profile, 0.0).unwrap_or(0.0);
            if let Ok(mut tf) = transforms.get_mut(e) {
                tf.translation = lane.world(Vec3::new(0.0, y, g.aim));
            }
        }
        (Some(e), false) => {
            // The thrown ball is its own entity; this one was handed over.
            commands.entity(e).despawn();
            *ready = None;
        }
        (None, false) => {}
    }
}

/// The wobble of your hand on the board at `clock` seconds (metres).
fn wobble(clock: f32, size: f32) -> Vec2 {
    Vec2::new(
        (clock * 1.7).sin() + 0.6 * (clock * 3.1 + 1.0).sin(),
        (clock * 1.3 + 0.5).cos() + 0.6 * (clock * 2.7).sin(),
    ) * size
        / 1.6
}

/// The deterministic scatter of a release (so a test can pin it): in -1..1.
fn scatter(seed: f32) -> Vec2 {
    Vec2::new(
        (seed * 12.9898).sin() * 43_758.545,
        (seed * 78.233 + 1.7).sin() * 24_634.635,
    )
    .map(|v| v.fract() * 2.0)
    .clamp(Vec2::splat(-1.0), Vec2::splat(1.0))
}

/// Where the thrower's eye is at a board.
fn eye_at(board: &Board, t: &DartTuning) -> Vec3 {
    board.oche + Vec3::Y * t.eye_height
}

/// Where the aim line (look `look` off the board's middle) meets the board's
/// plane.
fn aim_on_board(board: &Board, eye: Vec3, look: Vec2) -> Vec3 {
    let forward = -board.normal;
    let dir = Quat::from_rotation_y(-look.x)
        * Quat::from_axis_angle(forward.cross(Vec3::Y), look.y)
        * forward;
    let denom = dir.dot(board.normal);
    let d = if denom.abs() < 1e-4 {
        0.0
    } else {
        (board.center - eye).dot(board.normal) / denom
    };
    eye + dir * d
}

#[allow(clippy::too_many_arguments)]
fn dart_play(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    actions: crate::input::Actions,
    virt: Option<Res<crate::controls::VirtualInput>>,
    assets: Res<AssetServer>,
    mut playing: ResMut<Playing>,
    mut busy: ResMut<HandsBusy>,
    toast: Option<ResMut<crate::hud::Toast>>,
    boards: Query<&Board>,
    props: Query<Entity, With<Prop>>,
    mut transforms: Query<&mut Transform, (With<Prop>, Without<Reticle>)>,
    mut reticle: Query<&mut Transform, With<Reticle>>,
) {
    let Some(Session::Darts(g)) = playing.0.as_mut() else {
        return;
    };
    if actions.just_pressed(crate::input::Act(QUIT_ACTION)) {
        leave(&mut commands, &mut playing, &mut busy, &props);
        return;
    }
    let Ok(board) = boards.get(g.board) else {
        leave(&mut commands, &mut playing, &mut busy, &props);
        return;
    };
    let t = tuning();
    let d = &t.darts;
    let dt = time.delta_secs().min(1.0 / 20.0);
    g.clock += dt;

    // Look about.
    let limit = d.look_limit_deg.to_radians();
    let l = look_delta(&motion, &virt);
    g.look = Vec2::new(
        (g.look.x + l.x * d.rad_per_px).clamp(-limit, limit),
        (g.look.y - l.y * d.rad_per_px).clamp(-limit, limit),
    );

    // Hold your breath: steadier, for a while.
    let want_steady = actions.pressed(crate::input::Act(STEADY_ACTION)) && g.breath > 0.0;
    g.steadying = want_steady;
    g.breath = if want_steady {
        (g.breath - dt).max(0.0)
    } else {
        (g.breath + dt * d.breath_regain).min(d.breath_secs)
    };

    let eye = eye_at(board, d);
    let right = board.right();
    let sway = wobble(g.clock, if g.steadying { d.steady_sway } else { d.sway });
    let aim = aim_on_board(board, eye, g.look) + right * sway.x + Vec3::Y * sway.y;
    if let Ok(mut tf) = reticle.single_mut() {
        tf.translation = aim + board.normal * 0.01;
    }

    // Darts in the air land.
    let mut landed = Vec::new();
    for f in g.flying.iter_mut() {
        f.secs += dt;
        let k = (f.secs / d.flight_secs.max(0.05)).min(1.0);
        if let Ok(mut tf) = transforms.get_mut(f.entity) {
            let dir = (f.to - f.from).normalize_or_zero();
            tf.translation = f.from.lerp(f.to, k) + Vec3::Y * d.flight_arc * 4.0 * k * (1.0 - k);
            tf.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
        }
        if k >= 1.0 {
            landed.push(f.entity);
        }
    }
    g.flying.retain(|f| !landed.contains(&f.entity));

    // The board clears a moment after a visit.
    if let Some(left) = g.clear.as_mut() {
        *left -= dt;
        if *left <= 0.0 {
            g.clear = None;
            g.thrown = 0;
        }
        return;
    }
    if g.over {
        if throw_pressed(&keys, &mouse) {
            g.left = d.start;
            g.visit_start = d.start;
            g.thrown = 0;
            g.visits = 1;
            g.over = false;
            g.last.clear();
        }
        return;
    }

    // Throw.
    if throw_pressed(&keys, &mouse) && g.flying.is_empty() {
        let scatter = scatter(time.elapsed_secs() + g.thrown as f32 * 7.31);
        let hit_point = aim + (right * scatter.x + Vec3::Y * scatter.y) * d.release_scatter;
        let rel = hit_point - board.center;
        let hit = board_hit(rel.dot(right), rel.y, &d.radii, &d.sector_order);
        let from = eye + right * 0.18 - Vec3::Y * 0.2 - board.normal * 0.35;
        let dir = (hit_point - from).normalize_or_zero();
        let stuck = hit_point - dir * 0.0625;
        let dart = commands
            .spawn((
                Prop,
                Transform::from_translation(from)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Z, dir)),
                Visibility::default(),
                model(&assets, DART_MODEL),
                DespawnOnExit(AppState::InGame),
            ))
            .id();
        g.flying.push(Flying {
            entity: dart,
            from,
            to: stuck,
            secs: 0.0,
        });
        g.thrown += 1;
        g.last = format!("{} = {}", hit.label(), hit.points);
        match outcome(g.left, &hit) {
            Outcome::Left(n) => {
                g.left = n;
                if g.thrown >= d.per_visit {
                    g.visits += 1;
                    g.visit_start = g.left;
                    g.clear = Some(d.clear_secs);
                }
            }
            Outcome::Bust => {
                g.left = g.visit_start;
                g.last.push_str("  BUST");
                g.visits += 1;
                g.clear = Some(d.clear_secs);
            }
            Outcome::Out => {
                g.left = 0;
                g.over = true;
                if let Some(mut toast) = toast {
                    toast.show(format!(
                        "Darts: checked out in {} visit{}",
                        g.visits,
                        if g.visits == 1 { "" } else { "s" }
                    ));
                }
            }
        }
    }
}

/// Clear the darts off the board between visits: darts are `Prop`s with a
/// model, and the reticle and ball are meshes.
fn clear_darts(
    mut commands: Commands,
    playing: Res<Playing>,
    darts: Query<Entity, (With<Prop>, With<bevy::world_serialization::WorldAssetRoot>)>,
    mut cleared: Local<bool>,
) {
    let Some(Session::Darts(g)) = playing.0.as_ref() else {
        *cleared = false;
        return;
    };
    // A visit's end: the clear timer ran out and nothing is in the air.
    let idle = g.clear.is_none() && g.flying.is_empty() && g.thrown == 0;
    if idle && !*cleared {
        for e in &darts {
            commands.entity(e).despawn();
        }
        *cleared = true;
    } else if !idle {
        *cleared = false;
    }
}

/// Your eyes at the machine.
fn view(
    playing: Res<Playing>,
    lanes: Query<&Lane>,
    boards: Query<&Board>,
    mut camera: Query<&mut Transform, With<crate::world::MainCamera>>,
) {
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    let t = tuning();
    match playing.0.as_ref() {
        Some(Session::Skee(g)) => {
            let Ok(lane) = lanes.get(g.lane) else { return };
            let s = &t.skeeball;
            let eye = lane.world(Vec3::new(s.eye.0, s.eye.1, g.aim * 0.6));
            let look = lane.world(Vec3::new(s.look.0, s.look.1, 0.0));
            *cam = Transform::from_translation(eye).looking_at(look, Vec3::Y);
        }
        Some(Session::Darts(g)) => {
            let Ok(board) = boards.get(g.board) else {
                return;
            };
            let eye = eye_at(board, &t.darts);
            let aim = aim_on_board(board, eye, g.look);
            *cam = Transform::from_translation(eye).looking_at(aim, Vec3::Y);
        }
        None => {}
    }
}

fn arcade_context(playing: Res<Playing>, mut ctx: ResMut<crate::input::InputContexts>) {
    crate::input::switch(&mut ctx, "arcade", playing.0.is_some());
}

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        Status,
        crate::hud::slot(crate::hud::HudSlot::BottomLeft),
        DespawnOnExit(AppState::InGame),
    ));
}

fn meter(v: f32) -> String {
    let n = (v.clamp(0.0, 1.0) * 10.0).round() as usize;
    format!("[{}{}]", "#".repeat(n), "-".repeat(10 - n))
}

fn show_status(playing: Res<Playing>, mut text: Query<&mut Text, With<Status>>) {
    let t = tuning();
    let want = match playing.0.as_ref() {
        Some(Session::Skee(g)) => {
            let rolled: Vec<String> = g.rolled.iter().map(u32::to_string).collect();
            let power = g
                .charge
                .map(|c| format!("  power {}", meter(1.0 - (c.rem_euclid(2.0) - 1.0).abs())))
                .unwrap_or_default();
            if g.over {
                format!(
                    "SKEE-BALL  final score {}  ({})\n{{arcade_throw}} to play again, {{arcade_quit}} to leave",
                    g.score,
                    rolled.join(" ")
                )
            } else {
                format!(
                    "SKEE-BALL  score {}  balls left {}{power}\n{}",
                    g.score,
                    g.left + g.ball.is_some() as u32,
                    rolled.join(" ")
                )
            }
        }
        Some(Session::Darts(g)) => {
            let breath = meter(g.breath / t.darts.breath_secs.max(0.1));
            if g.over {
                format!(
                    "DARTS  checked out in {} visit(s)  ({})\n{{arcade_throw}} for another game, {{arcade_quit}} to leave",
                    g.visits, g.last
                )
            } else {
                format!(
                    "DARTS 501  left {}  visit {}  dart {}/{}\nlast: {}   breath {breath}",
                    g.left,
                    g.visits,
                    (g.thrown + 1).min(t.darts.per_visit),
                    t.darts.per_visit,
                    g.last
                )
            }
        }
        None => String::new(),
    };
    let want = crate::input::fill(&want);
    for mut line in &mut text {
        if line.0 != want {
            line.0.clone_from(&want);
        }
    }
}

fn stop(mut playing: ResMut<Playing>, mut busy: ResMut<HandsBusy>) {
    playing.0 = None;
    busy.0 = false;
}

/// `FRIENDSLOP_START=wonderland` (just inside the door), `wonderland_skeeball`
/// (behind the first lane) and `wonderland_darts` (at the first oche).
fn starts() -> Vec<(String, Vec3)> {
    let (lanes, boards) = machines();
    let mut out = vec![(
        "wonderland".to_string(),
        theater_at() + Vec3::new(26.8, -1.75, -11.0),
    )];
    if let Some(l) = lanes.first() {
        out.push((
            "wonderland_skeeball".into(),
            Vec3::new(l.start.x, l.start.y - 0.5, l.start.z) - l.forward * 0.9,
        ));
    }
    if let Some(b) = boards.first() {
        out.push(("wonderland_darts".into(), b.oche + Vec3::Y * 0.05));
    }
    out
}

pub fn plugin(app: &mut App) {
    crate::features::start::provide(starts);
    friendslop_base::areas::on_sim(app, AREA, spawn_machines);
    app.init_resource::<Playing>()
        .add_observer(click)
        .add_hud(spawn_ui)
        .add_systems(
            Update,
            (
                (skee_play, skee_ready, dart_play, clear_darts, view).chain(),
                arcade_context,
                show_status,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::InGame), stop);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> std::sync::Arc<Tuning> {
        tuning()
    }

    #[test]
    fn the_data_parses_and_is_complete() {
        let t = t();
        assert!(t.skeeball.balls > 0 && t.skeeball.profile.len() >= 3);
        assert_eq!(t.darts.sector_order.len(), 20);
        assert_eq!(t.darts.start, 501);
    }

    #[test]
    fn the_board_scores_like_a_dartboard() {
        let t = t();
        let (r, o) = (&t.darts.radii, &t.darts.sector_order);
        // Dead centre is the bull's eye (50, a double); just off is the outer bull.
        let bull = board_hit(0.0, 0.0, r, o);
        assert_eq!((bull.points, bull.double), (50, true));
        assert_eq!(board_hit(0.0, 0.01, r, o).points, 25);
        // Straight up in the single band is the 20; its treble is 60 and its double 40.
        assert_eq!(board_hit(0.0, 0.13, r, o).points, 20);
        assert_eq!(board_hit(0.0, 0.103, r, o).points, 60);
        let d20 = board_hit(0.0, 0.166, r, o);
        assert_eq!((d20.points, d20.double), (40, true));
        // A quarter turn clockwise is the 6, straight down the 3, anticlockwise the 11.
        assert_eq!(board_hit(0.13, 0.0, r, o).number, 6);
        assert_eq!(board_hit(0.0, -0.13, r, o).number, 3);
        assert_eq!(board_hit(-0.13, 0.0, r, o).number, 11);
        // Off the double ring is a miss.
        assert_eq!(board_hit(0.0, 0.2, r, o).points, 0);
        assert_eq!(board_hit(0.0, 0.2, r, o).label(), "miss");
        assert_eq!(d20.label(), "D20");
    }

    #[test]
    fn the_501_rules() {
        let hit = |points, double| Hit {
            points,
            double,
            mult: if double { 2 } else { 1 },
            number: points,
        };
        assert_eq!(outcome(501, &hit(60, false)), Outcome::Left(441));
        assert_eq!(outcome(40, &hit(40, true)), Outcome::Out);
        assert_eq!(outcome(40, &hit(40, false)), Outcome::Bust);
        assert_eq!(outcome(40, &hit(41, false)), Outcome::Bust);
        assert_eq!(outcome(40, &hit(39, false)), Outcome::Bust);
        assert_eq!(outcome(2, &hit(2, true)), Outcome::Out);
        assert_eq!(outcome(50, &hit(50, true)), Outcome::Out);
    }

    #[test]
    fn the_map_has_every_machine() {
        let (lanes, boards) = machines();
        assert_eq!(lanes.len(), 6);
        assert_eq!(boards.len(), 3);
        for l in &lanes {
            let points: Vec<u32> = l.holes.iter().map(|h| h.points).collect();
            for p in [10, 20, 30, 40, 50, 100] {
                assert!(points.contains(&p), "lane {} lacks the {p}", l.id);
            }
            assert!((l.forward.length() - 1.0).abs() < 1e-3);
        }
        for b in &boards {
            // The oche is the regulation distance from the board.
            let d = (b.oche - b.center).xz().length();
            assert!((2.3..2.5).contains(&d), "board {} oche at {d}", b.id);
        }
    }

    #[test]
    fn a_skee_ball_throw_can_score_every_hole() {
        let t = t();
        let (lanes, _) = machines();
        let holes = &lanes[0].holes;
        let mut windows: std::collections::BTreeMap<u32, (f32, f32)> = Default::default();
        let n = 400;
        for si in 0..=n {
            let speed = t.skeeball.speed_min
                + (t.skeeball.speed_max - t.skeeball.speed_min) * si as f32 / n as f32;
            for ai in -5..=5 {
                let aim = ai as f32 * 0.04;
                let p = throw_score(&t.skeeball, holes, aim, speed);
                if p > 0 {
                    let w = windows.entry(p).or_insert((speed, speed));
                    w.0 = w.0.min(speed);
                    w.1 = w.1.max(speed);
                }
            }
        }
        for p in [10, 20, 30, 40, 50, 100] {
            let (lo, hi) = windows
                .get(&p)
                .copied()
                .unwrap_or_else(|| panic!("no throw scores {p}: {windows:?}"));
            // A window of at least 3% of the meter, so it can be hit.
            let range = t.skeeball.speed_max - t.skeeball.speed_min;
            assert!((hi - lo) / range > 0.03, "the {p} window is {lo}..{hi}");
        }
    }

    #[test]
    fn a_gentle_roll_never_gets_over_the_ramp() {
        let t = t();
        assert_eq!(lip_speed(&t.skeeball, 1.0), None);
    }

    /// A headless app with the arcade's systems, a lane and a board.
    fn arcade() -> (App, Entity, Entity) {
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .init_asset::<StandardMaterial>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_micros(16_667),
            ))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<HandsBusy>()
            .init_resource::<Playing>()
            .add_observer(click)
            .add_systems(
                Update,
                (skee_play, skee_ready, dart_play, clear_darts).chain(),
            );
        let (lanes, boards) = machines();
        let lane = app.world_mut().spawn(lanes[0].clone()).id();
        let board = app.world_mut().spawn(boards[0].clone()).id();
        app.update();
        (app, lane, board)
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
    }

    fn release(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
        }
    }

    #[test]
    fn clicking_a_lane_plays_skee_ball_and_a_good_throw_scores() {
        let (mut app, lane, _) = arcade();
        app.world_mut().trigger(Clicked { entity: lane });
        assert!(
            app.world().resource::<HandsBusy>().0,
            "hands on the machine"
        );
        assert!(matches!(
            app.world().resource::<Playing>().0,
            Some(Session::Skee(_))
        ));
        // Find the charge that scores the 50 at the middle of the lane.
        let t = tuning();
        let (lanes, _) = machines();
        let power = (0..=400)
            .map(|i| i as f32 / 400.0)
            .find(|m| {
                let speed =
                    t.skeeball.speed_min + (t.skeeball.speed_max - t.skeeball.speed_min) * m;
                throw_score(&t.skeeball, &lanes[0].holes, 0.0, speed) == 50
            })
            .unwrap_or_else(|| panic!("no power scores the 50"));
        // Hold the throw key for that part of the meter's first sweep, then let go.
        // (Two extra frames: the first press registers a frame late.)
        press(&mut app, KeyCode::Space);
        let held = (power * t.skeeball.charge_secs * 60.0).round() as usize;
        frames(&mut app, held + 1);
        release(&mut app, KeyCode::Space);
        frames(&mut app, 60 * 12);
        let Some(Session::Skee(g)) = &app.world().resource::<Playing>().0 else {
            panic!("the game ended");
        };
        assert_eq!(g.rolled, vec![50], "one ball rolled, into the 50");
        assert_eq!(g.score, 50);
        assert_eq!(g.left, t.skeeball.balls - 1);
        // Walk away: the hands are free again.
        press(&mut app, KeyCode::Enter);
        frames(&mut app, 2);
        assert!(!app.world().resource::<HandsBusy>().0);
        assert!(app.world().resource::<Playing>().0.is_none());
    }

    #[test]
    fn clicking_a_board_plays_darts_and_a_dart_lands() {
        let (mut app, _, board) = arcade();
        app.world_mut().trigger(Clicked { entity: board });
        assert!(app.world().resource::<HandsBusy>().0);
        let t = tuning();
        frames(&mut app, 5);
        press(&mut app, KeyCode::Space);
        frames(&mut app, 2);
        release(&mut app, KeyCode::Space);
        frames(&mut app, 60);
        let Some(Session::Darts(g)) = &app.world().resource::<Playing>().0 else {
            panic!("the game ended");
        };
        assert_eq!(g.thrown, 1, "one dart thrown");
        assert!(g.flying.is_empty(), "and it has landed");
        assert!(
            g.left <= t.darts.start && g.left > t.darts.start - 61,
            "left {}",
            g.left
        );
        assert!(!g.last.is_empty());
    }
}
