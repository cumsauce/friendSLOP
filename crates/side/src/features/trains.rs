//! Trains. From issue #321 by doomy ("take the path to nyc"): "instead of a
//! sign to click, players board the train headed to the oculus station. it
//! would be cool to have a little transition in the subway when loading
//! between zones with the SAM announcing the stop. can generalize this, we
//! might have more trains". Later: "i want it to actually leave the station
//! before doing the transition scene in the train - like the train should
//! actually go in a tunnel, it transitions, then comes back out the tunnel
//! below the oculus, then you need to go up in the oculus to exit."
//!
//! A line runs between two stops. Each stop is a station: a platform beside
//! the track (the `path-platform` model) and a tunnel mouth at the track's
//! end (`path-tunnel`). In the valley the station stands raised on a plinth
//! (`path-station`) and the track runs off on a viaduct (`path-viaduct`); a
//! stop with a `depth` is underground, in a hall (`path-hall`) under the
//! floor with stairs up through a hole in it (World Trade Center's comes up
//! inside the Oculus).
//!
//! A train (the `path-car` model) waits at both stops. Walk in and stand
//! there a moment and it pulls out with you into the tunnel; once it's in,
//! the server moves you into that direction's ride car, hung deep under the
//! city, where tunnel lights rush past and the announcer (the `samiam`
//! speech synth) says the next stop. Then the other stop's train comes out
//! of its tunnel with you in it and stops at the platform. The cars' motion
//! is a replicated [`Run`] per stop (phase and seconds), so everyone sees
//! the trains come and go. Lines, stops, times and speech are data
//! (`data/side/trains.ron`). Colliders are mesh-less boxes, so the trains
//! work without the model files.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// One stop of a line.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Stop {
    /// The stop's name.
    pub name: String,
    /// The car's middle (x, z) when it stands at the platform.
    pub at: (f32, f32),
    /// Which way the car points, degrees about up: it leaves along its own
    /// +x into the tunnel, and the platform is on its +z side.
    pub yaw: f32,
    /// On the city floor (true) or on the valley's ground (false).
    pub city: bool,
    /// How far under the floor or ground the track is (m): 0 for a station
    /// on the surface, else the hall's depth (the `path-hall` model is
    /// [`HALL_DEPTH`] tall).
    #[serde(default)]
    pub depth: f32,
    /// A ground of its own (its height, m), beating `city`: Miami's (#345)
    /// floats high over the city.
    #[serde(default)]
    pub floor: Option<f32>,
    /// How far above the ground a station on the valley's ground stands (m):
    /// its track runs off on a viaduct, clear of the hills (the `path-station`
    /// model's plinth and ramp are made for [`STATION_RAISE`]).
    #[serde(default)]
    pub raise: f32,
    /// Said on the way to this stop.
    pub announce: String,
}

/// A line: a train between two stops.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Line {
    pub name: String,
    pub stops: [Stop; 2],
    /// What runs on it: a train (the default) or a plane (#345).
    #[serde(default)]
    pub vehicle: Vehicle,
    /// How far this line's car goes before it's gone (m), if not `run_out`.
    #[serde(default)]
    pub run_out: Option<f32>,
    /// How far it climbs per metre once it's past `lift_off` (a plane takes off).
    #[serde(default)]
    pub climb: f32,
    /// How far along (m) it leaves the ground.
    #[serde(default)]
    pub lift_off: f32,
}

/// What runs on a line.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vehicle {
    /// A PATH train out of a tunnel.
    #[default]
    Train,
    /// A plane off a gate (#345): it rolls out and climbs into the sky.
    Plane,
}

impl Vehicle {
    /// The car's model.
    pub fn car(self) -> &'static str {
        match self {
            Vehicle::Train => "path-car.gltf",
            Vehicle::Plane => "plane-car.gltf",
        }
    }
}

/// Tuning and writing: `assets/data/side/trains.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds standing in a waiting train before it leaves with you.
    pub board_secs: f32,
    /// Seconds the train takes to pull out of the station into its tunnel.
    pub depart_secs: f32,
    /// Seconds in the ride car between the stations.
    pub ride_secs: f32,
    /// Seconds the train takes to come out of the tunnel and stop.
    pub arrive_secs: f32,
    /// Seconds a train that left empty-handed stays away before the next one comes in.
    pub away_secs: f32,
    /// How far into the tunnel the train goes (m, its middle from the platform).
    pub run_out: f32,
    /// Where the ride cars hang (x, y, z); one per line and direction, 30 m apart along x.
    pub ride_at: (f32, f32, f32),
    /// The announcer's voice: pitch and speed.
    pub voice: (f32, f32),
    /// Shown while a train waits to leave with you.
    pub boarding_line: String,
    pub lines: Vec<Line>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/trains.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------------ the car's shape

/// Half the car's length (along its own x) and width (z), from the model.
const HALF_LEN: f32 = 8.0;
const HALF_WIDE: f32 = 1.42;
/// The car's floor top above its origin, and its roof.
const FLOOR: f32 = 0.26;
const ROOF: f32 = 3.1;
/// The doors (on the car's +z side): their middles along x and their width.
const DOORS: [f32; 2] = [-3.5, 3.5];
const DOOR_W: f32 = 1.4;

// ------------------------------------------------------------------ the station's shape
// In the stop's frame: x along the track (+x into the tunnel), z across
// (+z the platform side), y up from the track bed. The models match.

/// The tunnel mouth's face along x (the `path-tunnel` model's origin).
pub const PORTAL_X: f32 = HALF_LEN + 1.5;
/// The track bed's width (z from -TRACK_W to the platform's edge).
const TRACK_W: f32 = 2.6;
/// The platform's edge and back (z).
const PLATFORM_Z: (f32, f32) = (1.45, 5.05);
/// The far end of the station from the tunnel (x): the platform's start.
const STATION_X: f32 = -10.0;
/// Where the platform's GTA VI ads stand along it (x, between the doors; #359).
const AD_X: [f32; 2] = [-7.0, 0.0];
/// A surface station's ramp from the platform's far end: its run and drop (m).
const RAMP_SLOPE: f32 = 0.32;
const RAMP_DROP: f32 = 1.6;
/// The raise the `path-station` model's plinth and ramp are built for.
pub const STATION_RAISE: f32 = 2.0;
/// An underground hall's depth (m, track bed to the floor above) and how
/// far it runs back from the tunnel, and its back wall (z).
pub const HALL_DEPTH: f32 = 7.0;
const HALL_X: f32 = -34.0;
const HALL_Z: f32 = 12.4;
/// Where the hall's columns stand along the track (x; at z 6.4..7).
const HALL_COLUMNS: [f32; 4] = [-28.0, -20.0, -2.0, 6.0];
/// The hall's stairs up: across (z middle and width) and where they reach
/// the floor above (x); they climb along +x at the stairs' slope.
const STAIRS_Z: f32 = 11.0;
const STAIRS_W: f32 = 2.4;
const STAIRS_TOP_X: f32 = -10.0;
/// The stairs' rise over run (about 34 degrees, like the subway entrance's).
const STAIRS_SLOPE: f32 = 0.675;

/// A stop's frame on the ground: the car's origin at the platform, turned to its yaw.
pub fn stop_frame(s: &Stop) -> Transform {
    let turn = Quat::from_rotation_y(s.yaw.to_radians());
    let (x, z) = s.at;
    let y = if let Some(y) = s.floor {
        y
    } else if s.city {
        crate::rules::nyc::CITY_Y
    } else {
        // On the valley's ground: as high as the ground anywhere under the
        // station, so no hill comes up through it (its plinth goes down).
        let mut top = f32::MIN;
        for i in 0..=8 {
            for j in 0..=4 {
                let l = Vec3::new(
                    STATION_X + (PORTAL_X - STATION_X) * i as f32 / 8.0,
                    0.0,
                    -TRACK_W + (PLATFORM_Z.1 + TRACK_W) * j as f32 / 4.0,
                );
                let p = Vec3::new(x, 0.0, z) + turn * l;
                top = top.max(crate::terrain::height_at(p.x, p.z));
            }
        }
        top + s.raise
    };
    Transform::from_translation(Vec3::new(x, y - s.depth, z)).with_rotation(turn)
}

/// Where a stop's car stands at the platform (its origin).
pub fn stop_origin(s: &Stop) -> Vec3 {
    stop_frame(s).translation
}

/// Where the ride car for line `line` heading to stop `to` hangs.
pub fn ride_origin(t: &Tuning, line: usize, to: usize) -> Vec3 {
    Vec3::from(t.ride_at) + Vec3::X * 30.0 * (line * 2 + to) as f32
}

/// Is `p` (feet) inside a car with this origin and yaw?
pub fn in_car(origin: Vec3, yaw_deg: f32, p: Vec3) -> bool {
    let l = Quat::from_rotation_y(-yaw_deg.to_radians()) * (p - origin);
    l.x.abs() < HALF_LEN - 0.2 && l.z.abs() < HALF_WIDE - 0.1 && (-0.5..2.5).contains(&l.y)
}

/// Which car someone at `p` is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Car {
    /// The train standing at a stop of a line.
    Waiting { line: usize, stop: usize },
    /// A ride car, on the way to stop `to`.
    Riding { line: usize, to: usize },
}

/// Which car someone at `p` is in, the stops' trains standing at their platforms.
pub fn car_at(t: &Tuning, p: Vec3) -> Option<Car> {
    t.lines.iter().enumerate().find_map(|(line, l)| {
        (0..2).find_map(|i| {
            if in_car(stop_origin(&l.stops[i]), l.stops[i].yaw, p) {
                Some(Car::Waiting { line, stop: i })
            } else if in_car(ride_origin(t, line, i), 0.0, p) {
                Some(Car::Riding { line, to: i })
            } else {
                None
            }
        })
    })
}

// ------------------------------------------------------------------ a train's run

/// What a stop's train is doing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// At the platform, doors open.
    #[default]
    Waiting,
    /// Pulling out into the tunnel.
    Leaving,
    /// Gone (inside the tunnel, out of sight).
    Away,
    /// Coming out of the tunnel to stop at the platform.
    Arriving,
}

/// A stop's train (server, replicated): its phase and seconds into it.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Run {
    pub line: u8,
    pub stop: u8,
    pub phase: Phase,
    pub secs: f32,
}

/// How far along the track (+x, into the tunnel) the train is.
pub fn offset(t: &Tuning, phase: Phase, secs: f32) -> f32 {
    let d = t.run_out;
    match phase {
        Phase::Waiting => 0.0,
        // Speeding up out of the station, slowing down into it.
        Phase::Leaving => d * (secs / t.depart_secs.max(0.01)).clamp(0.0, 1.0).powi(2),
        Phase::Away => d,
        Phase::Arriving => d * (1.0 - (secs / t.arrive_secs.max(0.01)).clamp(0.0, 1.0)).powi(2),
    }
}

/// Where a stop's car is now.
pub fn car_frame(s: &Stop, along: f32) -> Transform {
    let f = stop_frame(s);
    f.with_translation(f.translation + f.rotation * Vec3::X * along)
}

/// How far along the track a line's car is (its own `run_out`, if it has one).
pub fn line_offset(t: &Tuning, l: &Line, phase: Phase, secs: f32) -> f32 {
    let out = l.run_out.unwrap_or(t.run_out);
    offset(t, phase, secs) * out / t.run_out.max(0.01)
}

/// Where a line's car is now: along its track, and up in the air for a plane.
pub fn line_frame(l: &Line, s: &Stop, along: f32) -> Transform {
    let mut f = car_frame(s, along);
    f.translation.y += l.climb * (along - l.lift_off).max(0.0);
    f
}

/// Is this stop out in the open on the valley's ground (a viaduct, not a tunnel)?
pub fn open_air(s: &Stop) -> bool {
    s.depth <= 0.0 && !s.city && s.floor.is_none()
}

/// One server tick of a train: its next phase and seconds, given whether
/// someone in it has stood there long enough (`go`) and whether riders are
/// due in (`due`). `Some(phase)` when the phase just changed from the old one.
pub fn tick(t: &Tuning, r: &mut Run, dt: f32, go: bool, due: bool) -> Option<Phase> {
    r.secs += dt;
    let next = match r.phase {
        // Riders due in: the train standing there goes first, to come back with them.
        Phase::Waiting if go || due => Phase::Leaving,
        Phase::Leaving if r.secs >= t.depart_secs => Phase::Away,
        Phase::Away if due || r.secs >= t.away_secs => Phase::Arriving,
        Phase::Arriving if r.secs >= t.arrive_secs => Phase::Waiting,
        _ => return None,
    };
    let was = r.phase;
    r.phase = next;
    r.secs = 0.0;
    Some(was)
}

/// One rider's progress (server).
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct Rider {
    /// Has been off the trains since they last arrived (so arriving doesn't board you again).
    pub ready: bool,
    /// Seconds stood in a waiting train.
    pub aboard: f32,
    /// Carried along in a stop's car: the line, the stop and where in the car (its frame).
    pub pinned: Option<(usize, usize, Vec3)>,
    /// In a ride car: the line, the stop it goes to, seconds left.
    pub riding: Option<(usize, usize, f32)>,
}

/// Where you stand in a car: its middle, on its floor.
const ABOARD: Vec3 = Vec3::new(0.0, FLOOR + 0.05, 0.0);

pub fn protocol(app: &mut App) {
    app.add_systems(
        Update,
        (spawn_runs, drive).run_if(in_state(ServerState::Running)),
    );
    // The underground stations' stairs come up through holes in the floor.
    friendslop_world::holes::provide(holes);
    // Keep the valley's trees off the stations.
    for l in &tuning().lines {
        for s in &l.stops {
            if !s.city && s.floor.is_none() {
                let f = stop_frame(s);
                let mid = f.transform_point(Vec3::X * 10.0);
                friendslop_world::footprints::claim(mid.xz(), 26.0);
            }
        }
    }
}

/// The trains' replicated state (appended to the end of the protocol order).
pub fn protocol_runs(app: &mut App) {
    app.replicate::<Run>();
}

/// Where along the track (x, stop frame) an underground stop's stairs hole
/// in the floor above runs: from where your head would meet the floor's
/// underside to the stairs' top.
fn hole_span(depth: f32) -> (f32, f32) {
    let (bottom, top) = stairs_ends(depth);
    (bottom.x + (depth - 2.9 - FLOOR) / STAIRS_SLOPE, top.x + 0.2)
}

/// The hole each underground stop's stairs go up through.
pub fn stairs_hole(s: &Stop) -> Option<friendslop_world::holes::Hole> {
    if s.depth <= 0.0 {
        return None;
    }
    let f = stop_frame(s);
    let (from, to) = hole_span(s.depth);
    let mid = f.transform_point(Vec3::new((from + to) / 2.0, 0.0, STAIRS_Z));
    Some(
        friendslop_world::holes::Hole::rect(
            mid.xz(),
            Vec2::new(STAIRS_W + 0.2, to - from),
            s.yaw.to_radians() + std::f32::consts::FRAC_PI_2,
        )
        .on_floor(f.translation.y + s.depth)
        .lined(1.0, 0.1),
    )
}

/// The stairs' bottom and top noses (stop frame).
pub fn stairs_ends(depth: f32) -> (Vec3, Vec3) {
    let rise = depth - FLOOR;
    let top = Vec3::new(STAIRS_TOP_X, depth, STAIRS_Z);
    (top - Vec3::new(rise / STAIRS_SLOPE, rise, 0.0), top)
}

fn holes() -> Vec<friendslop_world::holes::Hole> {
    tuning()
        .lines
        .iter()
        .flat_map(|l| l.stops.iter().filter_map(stairs_hole))
        .collect()
}

fn spawn_runs(
    mut commands: Commands,
    runs: Query<(), With<Run>>,
    state: Option<Res<State<AppState>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) || !runs.is_empty() {
        return;
    }
    for (line, l) in tuning().lines.iter().enumerate() {
        for stop in 0..l.stops.len() {
            commands.spawn((
                Run {
                    line: line as u8,
                    stop: stop as u8,
                    ..default()
                },
                Replicated,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
}

/// Server: board, carry, ride and set down every player, and run the trains.
fn drive(
    time: Res<Time>,
    mut riders: Local<HashMap<Entity, Rider>>,
    mut runs: Query<&mut Run>,
    mut bodies: Query<
        (
            Entity,
            &mut Walker,
            Option<&mut WalkInput>,
            Option<&mut crate::kcc::Kcc>,
        ),
        With<PlayerOwner>,
    >,
) {
    let t = tuning();
    let dt = time.delta_secs();
    riders.retain(|e, _| bodies.contains(*e));
    let mut state: HashMap<(usize, usize), Run> = runs
        .iter()
        .map(|r| ((r.line as usize, r.stop as usize), *r))
        .collect();
    let positions: Vec<(Entity, Vec3)> = bodies.iter().map(|b| (b.0, b.1.pos)).collect();
    let mut moves = step_all(&t, &mut state, &mut riders, &positions, dt);
    for mut r in &mut runs {
        if let Some(n) = state.get(&(r.line as usize, r.stop as usize))
            && *n != *r
        {
            *r = *n;
        }
    }
    for (e, mut walker, input, kcc) in &mut bodies {
        let Some(to) = moves.remove(&e) else {
            continue;
        };
        walker.pos = to;
        if let Some(mut input) = input {
            input.stick = Vec2::ZERO;
            input.jump = false;
        }
        if let Some(mut kcc) = kcc {
            kcc.velocity = Vec3::ZERO;
        }
    }
}

/// One server tick of every train and rider: where to put each body that moves.
pub fn step_all<K: Copy + Eq + std::hash::Hash>(
    t: &Tuning,
    runs: &mut HashMap<(usize, usize), Run>,
    riders: &mut HashMap<K, Rider>,
    bodies: &[(K, Vec3)],
    dt: f32,
) -> HashMap<K, Vec3> {
    let mut out = HashMap::new();
    let stop = |line: usize, i: usize| t.lines.get(line).and_then(|l| l.stops.get(i));
    let line_of = |line: usize| t.lines.get(line);
    // Who's due to arrive at each stop, and who's stood long enough in a waiting train.
    let mut go = HashMap::new();
    let mut due = HashMap::new();
    for &(k, p) in bodies {
        let r = riders.entry(k).or_default();
        if let Some((line, to, left)) = r.riding {
            if !matches!(car_at(t, p), Some(Car::Riding { .. })) {
                r.riding = None; // moved off the ride some other way
            } else if left - dt <= 0.0 {
                due.insert((line, to), ());
            }
            continue;
        }
        if r.pinned.is_some() {
            continue;
        }
        match car_at(t, p) {
            Some(Car::Waiting { line, stop: s })
                if r.ready
                    && runs
                        .get(&(line, s))
                        .is_some_and(|run| run.phase == Phase::Waiting) =>
            {
                r.aboard += dt;
                if r.aboard >= t.board_secs {
                    go.insert((line, s), ());
                }
            }
            Some(_) => {}
            None => {
                r.ready = true;
                r.aboard = 0.0;
            }
        }
    }
    for (&key, run) in runs.iter_mut() {
        let Some(was) = tick(t, run, dt, go.contains_key(&key), due.contains_key(&key)) else {
            continue;
        };
        let Some(s) = stop(key.0, key.1) else {
            continue;
        };
        match (was, run.phase) {
            // Pulling out: everyone in the car goes with it.
            (Phase::Waiting, Phase::Leaving) => {
                let f = stop_frame(s);
                for &(k, p) in bodies {
                    let r = riders.entry(k).or_default();
                    if r.riding.is_none() && in_car(f.translation, s.yaw, p) {
                        r.pinned = Some((
                            key.0,
                            key.1,
                            f.compute_affine().inverse().transform_point3(p),
                        ));
                        r.aboard = 0.0;
                    }
                }
            }
            // In the tunnel: on to the ride car.
            (Phase::Leaving, Phase::Away) => {
                for r in riders.values_mut() {
                    if r.pinned.is_some_and(|(l, i, _)| (l, i) == key) {
                        r.pinned = None;
                        r.riding = Some((key.0, 1 - key.1, t.ride_secs));
                    }
                }
            }
            // Coming in: riders due here are in it.
            (Phase::Away, Phase::Arriving) => {
                for r in riders.values_mut() {
                    if r.riding
                        .is_some_and(|(l, to, left)| (l, to) == key && left - dt <= 0.0)
                    {
                        r.riding = None;
                        r.ready = false;
                        r.pinned = Some((key.0, key.1, ABOARD));
                    }
                }
            }
            // At the platform: off you get.
            (Phase::Arriving, Phase::Waiting) => {
                for r in riders.values_mut() {
                    if r.pinned.is_some_and(|(l, i, _)| (l, i) == key) {
                        r.pinned = None;
                        r.ready = false;
                    }
                }
            }
            _ => {}
        }
    }
    // Put everyone where their car is.
    for &(k, _) in bodies {
        let Some(r) = riders.get_mut(&k) else {
            continue;
        };
        if let Some((line, to, left)) = r.riding {
            // Just on: into the ride car.
            if left >= t.ride_secs {
                out.insert(k, ride_origin(t, line, to) + ABOARD);
            }
            // At zero, wait in the ride car until the far end's train can come in.
            r.riding = Some((line, to, (left - dt).max(0.0)));
            continue;
        }
        if let Some((line, i, local)) = r.pinned
            && let (Some(l), Some(s), Some(run)) =
                (line_of(line), stop(line, i), runs.get(&(line, i)))
        {
            out.insert(
                k,
                line_frame(l, s, line_offset(t, l, run.phase, run.secs)).transform_point(local),
            );
        }
    }
    out
}
// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), build)
        .add_systems(
            Update,
            (announce, tunnel_lights, move_cars).run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            look.in_set(crate::place::PlaceSet)
                .run_if(in_state(AppState::InGame)),
        );
}

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{file}"))))
}

/// The static boxes of a car (centre, size in its own frame); `closed` shuts the doors.
pub fn car_boxes(closed: bool) -> Vec<(Vec3, Vec3)> {
    let h = ROOF / 2.0;
    let mut b = vec![
        (Vec3::Y * FLOOR / 2.0, Vec3::new(16.0, FLOOR, 2.8)),
        (Vec3::new(0.0, h, -HALF_WIDE), Vec3::new(16.0, ROOF, 0.08)),
        (Vec3::new(-HALF_LEN, h, 0.0), Vec3::new(0.08, ROOF, 2.8)),
        (Vec3::new(HALF_LEN, h, 0.0), Vec3::new(0.08, ROOF, 2.8)),
        (Vec3::Y * ROOF, Vec3::new(16.0, 0.08, 2.8)),
    ];
    // The door side, in pieces around the doors.
    let mut x0 = -HALF_LEN;
    for d in DOORS {
        let x1 = d - DOOR_W / 2.0;
        b.push((
            Vec3::new((x0 + x1) / 2.0, h, HALF_WIDE),
            Vec3::new(x1 - x0, ROOF, 0.08),
        ));
        if closed {
            b.push((Vec3::new(d, h, HALF_WIDE), Vec3::new(DOOR_W, ROOF, 0.08)));
        }
        x0 = d + DOOR_W / 2.0;
    }
    b.push((
        Vec3::new((x0 + HALF_LEN) / 2.0, h, HALF_WIDE),
        Vec3::new(HALF_LEN - x0, ROOF, 0.08),
    ));
    b
}

/// A box between two corners (stop frame): centre and size.
fn span(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    ((a + b) / 2.0, (b - a).abs())
}

/// A station's static boxes (centre, size and tilt, in the stop's frame): platform, plinth or hall, the barrier at the tunnel mouth, and an
/// underground hall's stairs up.
pub fn station_boxes(s: &Stop) -> Vec<(Vec3, Vec3, Quat)> {
    let flat = |(c, size): (Vec3, Vec3)| (c, size, Quat::IDENTITY);
    let (pz0, pz1) = PLATFORM_Z;
    let mut out = Vec::new();
    // Nobody walks into the tunnel or off along the viaduct (the train goes
    // through: it isn't a body).
    out.push(flat(span(
        Vec3::new(PORTAL_X, 0.0, -TRACK_W),
        Vec3::new(PORTAL_X + 0.3, if open_air(s) { 1.2 } else { 4.5 }, pz0),
    )));
    if s.depth <= 0.0 {
        // On a plinth: track bed and platform, the plinth going down into the ground.
        out.push(flat(span(
            Vec3::new(STATION_X, -3.0 - s.raise, -TRACK_W),
            Vec3::new(PORTAL_X, 0.0, pz1),
        )));
        out.push(flat(span(
            Vec3::new(STATION_X, 0.0, pz0),
            Vec3::new(PORTAL_X, FLOOR, pz1),
        )));
        // A ramp down from the platform's far end to wherever the ground is.
        let (top, bottom) = (
            Vec3::new(STATION_X, FLOOR, 0.0),
            Vec3::new(
                STATION_X - (RAMP_DROP + s.raise) / RAMP_SLOPE,
                FLOOR - RAMP_DROP - s.raise,
                0.0,
            ),
        );
        let along = top - bottom;
        let tilt = Quat::from_rotation_arc(Vec3::NEG_X, -along.normalize());
        let mid = (top + bottom) / 2.0 + tilt * Vec3::new(0.0, -0.15, (pz0 + pz1) / 2.0);
        out.push((mid, Vec3::new(along.length(), 0.3, pz1 - pz0), tilt));
        // Raised: a railing along the platform's back edge.
        if s.raise > 0.0 {
            out.push(flat(span(
                Vec3::new(STATION_X, FLOOR, pz1 - 0.1),
                Vec3::new(PORTAL_X, FLOOR + 1.1, pz1),
            )));
        }
        // The platform ends at the tunnel mouth, or a railing.
        out.push(flat(span(
            Vec3::new(PORTAL_X, 0.0, pz0),
            Vec3::new(
                PORTAL_X + 0.3,
                if open_air(s) { 1.3 } else { 4.5 },
                pz1 + 0.5,
            ),
        )));
        return out;
    }
    let d = s.depth;
    // The hall: its floor and platform, walls and the end wall.
    out.push(flat(span(
        Vec3::new(HALL_X, -1.0, -TRACK_W),
        Vec3::new(PORTAL_X, 0.0, HALL_Z),
    )));
    out.push(flat(span(
        Vec3::new(HALL_X, 0.0, pz0),
        Vec3::new(PORTAL_X, FLOOR, HALL_Z),
    )));
    for z in [-TRACK_W - 0.3, HALL_Z] {
        out.push(flat(span(
            Vec3::new(HALL_X - 0.3, 0.0, z),
            Vec3::new(PORTAL_X + 0.3, d, z + 0.3),
        )));
    }
    out.push(flat(span(
        Vec3::new(HALL_X - 0.3, 0.0, -TRACK_W),
        Vec3::new(HALL_X, d, HALL_Z),
    )));
    out.push(flat(span(
        Vec3::new(PORTAL_X, 0.0, pz0),
        Vec3::new(PORTAL_X + 0.3, d, HALL_Z),
    )));
    // The hall's columns (the `path-hall` model's).
    for x in HALL_COLUMNS {
        out.push(flat(span(
            Vec3::new(x - 0.3, FLOOR, 6.4),
            Vec3::new(x + 0.3, d, 7.0),
        )));
    }
    // The stairs: a ramp up to the floor above, walled both sides up past
    // the floor (a railing round the hole), and a railing across its low end.
    let (bottom, top) = stairs_ends(d);
    let along = top - bottom;
    let tilt = Quat::from_rotation_arc(Vec3::X, along.normalize());
    let mid = (top + bottom) / 2.0 + tilt * Vec3::new(0.0, -0.15, 0.0);
    out.push((mid, Vec3::new(along.length(), 0.3, STAIRS_W), tilt));
    let half = STAIRS_W / 2.0 + 0.15;
    for z in [STAIRS_Z - half, STAIRS_Z + half] {
        out.push(flat(span(
            Vec3::new(bottom.x, FLOOR, z - 0.1),
            Vec3::new(top.x, d + 1.1, z + 0.1),
        )));
    }
    let (from, _) = hole_span(d);
    out.push(flat(span(
        Vec3::new(from - 0.2, d - 1.0, STAIRS_Z - half),
        Vec3::new(from, d + 1.1, STAIRS_Z + half),
    )));
    out
}

fn spawn_boxes(commands: &mut Commands, at: Transform, boxes: &[(Vec3, Vec3, Quat)]) {
    for (c, s, tilt) in boxes {
        commands.spawn((
            Transform::from_translation(at.transform_point(*c)).with_rotation(at.rotation * *tilt),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// A light rushing past a ride car's windows.
#[derive(Component)]
struct TunnelLight {
    origin: Vec3,
}

/// A stop's car (every peer): moved along its track from its [`Run`].
#[derive(Component)]
struct CarOf {
    line: u8,
    stop: u8,
}

/// The name a stop's start goes by (`FRIENDSLOP_START`): its name in snake case.
pub fn start_name(s: &Stop) -> String {
    s.name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    let t = tuning();
    let shut = car_boxes(true);
    let open = car_boxes(false);
    for (li, l) in t.lines.iter().enumerate() {
        for (si, s) in l.stops.iter().enumerate() {
            let at = stop_frame(s);
            // Both doors glow as a way in, named for where the train goes.
            let to = &l.stops[1 - si].name;
            for x in DOORS {
                crate::features::doorway::spawn(
                    &mut commands,
                    at.transform_point(Vec3::new(x, FLOOR, HALF_WIDE + 0.1)),
                    s.yaw.to_radians(),
                    crate::features::doorway::Doorway::way_in(format!("{} to {to}", l.name))
                        .size(DOOR_W, 2.1),
                );
            }
            // The car: a moving body with its walls as children.
            commands
                .spawn((
                    CarOf {
                        line: li as u8,
                        stop: si as u8,
                    },
                    at,
                    Visibility::default(),
                    avian3d::prelude::RigidBody::Kinematic,
                    DespawnOnExit(AppState::InGame),
                ))
                .with_children(|car| {
                    car.spawn((model(&assets, l.vehicle.car()), Transform::default()));
                    for (c, size) in &open {
                        car.spawn((
                            Transform::from_translation(*c),
                            avian3d::prelude::Collider::cuboid(size.x, size.y, size.z),
                        ));
                    }
                });
            // The station.
            let mut files = vec!["path-platform.gltf"];
            if l.vehicle == Vehicle::Plane {
                // A gate: its platform and canopy, no tunnel (the plane flies off).
                files = vec!["plane-gate.gltf"];
            } else if s.depth > 0.0 {
                files.push("path-hall.gltf");
            } else {
                files.push("path-station.gltf");
            }
            for file in files {
                commands.spawn((model(&assets, file), at, DespawnOnExit(AppState::InGame)));
            }
            // GTA VI ads at the back of the platform, facing the track (#359).
            if l.vehicle == Vehicle::Train {
                for x in AD_X {
                    commands.spawn((
                        model(&assets, "features/gta6-ad.gltf"),
                        Transform::from_translation(at.transform_point(Vec3::new(
                            x,
                            0.0,
                            PLATFORM_Z.1 - 0.4,
                        )))
                        .with_rotation(at.rotation * Quat::from_rotation_y(std::f32::consts::PI)),
                        DespawnOnExit(AppState::InGame),
                    ));
                }
            }
            let portal = at.with_translation(at.transform_point(Vec3::X * PORTAL_X));
            if l.vehicle == Vehicle::Train {
                // Out in the open the track runs off on a viaduct; else into a tunnel.
                let file = if open_air(s) {
                    "path-viaduct.gltf"
                } else {
                    "path-tunnel.gltf"
                };
                commands.spawn((
                    model(&assets, file),
                    portal,
                    DespawnOnExit(AppState::InGame),
                ));
            }
            spawn_boxes(&mut commands, at, &station_boxes(s));
            if s.depth > 0.0 {
                // The hall's lights along the platform, and the way up.
                for i in 0..4 {
                    commands.spawn((
                        PointLight {
                            intensity: 120_000.0,
                            range: 14.0,
                            ..default()
                        },
                        Transform::from_translation(at.transform_point(Vec3::new(
                            HALL_X + 4.0 + i as f32 * ((PORTAL_X - HALL_X - 8.0) / 3.0),
                            s.depth - 1.0,
                            3.5,
                        ))),
                        DespawnOnExit(AppState::InGame),
                    ));
                }
                let (bottom, _) = stairs_ends(s.depth);
                crate::features::doorway::spawn(
                    &mut commands,
                    at.transform_point(bottom - Vec3::X * 0.4),
                    s.yaw.to_radians() - std::f32::consts::FRAC_PI_2,
                    crate::features::doorway::Doorway::new(
                        crate::features::doorway::Way::Up,
                        s.name.clone(),
                    )
                    .size(STAIRS_W, 2.4),
                );
            }
            // `FRIENDSLOP_START=<stop>` starts you on its platform.
            let name = start_name(s);
            let feet = at.transform_point(Vec3::new(0.0, FLOOR + 0.05, 3.25));
            commands.queue(move |world: &mut World| {
                if let Some(mut p) = world.get_resource_mut::<crate::features::start::StartPoints>()
                {
                    p.add(&name, feet);
                }
            });
        }
        for to in 0..2 {
            let o = ride_origin(&t, li, to);
            let at = Transform::from_translation(o);
            commands.spawn((
                model(&assets, l.vehicle.car()),
                at,
                DespawnOnExit(AppState::InGame),
            ));
            let boxes: Vec<_> = shut.iter().map(|(c, s)| (*c, *s, Quat::IDENTITY)).collect();
            spawn_boxes(&mut commands, at, &boxes);
            commands.spawn((
                PointLight {
                    intensity: 60_000.0,
                    range: 12.0,
                    ..default()
                },
                Transform::from_translation(o + Vec3::Y * 2.6),
                DespawnOnExit(AppState::InGame),
            ));
            // The tunnel: lights streaming past both sides (a plane: clouds).
            let (mesh, glow) = if l.vehicle == Vehicle::Plane {
                (
                    meshes.add(Sphere::new(1.4)),
                    materials.add(StandardMaterial {
                        base_color: Color::WHITE,
                        emissive: LinearRgba::rgb(1.5, 1.5, 1.6),
                        ..default()
                    }),
                )
            } else {
                (
                    meshes.add(Cuboid::new(1.2, 0.15, 0.15)),
                    materials.add(StandardMaterial {
                        base_color: Color::BLACK,
                        emissive: LinearRgba::rgb(6.0, 5.0, 3.0),
                        ..default()
                    }),
                )
            };
            for i in 0..8 {
                for side in [-3.0, 3.0] {
                    commands.spawn((
                        TunnelLight { origin: o },
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(glow.clone()),
                        Transform::from_translation(
                            o + Vec3::new(-40.0 + i as f32 * 10.0, 2.0, side),
                        ),
                        DespawnOnExit(AppState::InGame),
                    ));
                }
            }
        }
    }
}

/// Every peer: each stop's car where its run has it.
fn move_cars(runs: Query<&Run>, mut cars: Query<(&CarOf, &mut Transform)>) {
    let t = tuning();
    for (c, mut tf) in &mut cars {
        let Some(run) = runs.iter().find(|r| r.line == c.line && r.stop == c.stop) else {
            continue;
        };
        let Some(l) = t.lines.get(c.line as usize) else {
            continue;
        };
        let Some(s) = l.stops.get(c.stop as usize) else {
            continue;
        };
        let want = line_frame(l, s, line_offset(&t, l, run.phase, run.secs));
        if *tf != want {
            *tf = want;
        }
    }
}

/// The looks (`data/base/place_looks.ron`) of a ride car and an underground hall.
pub const RIDE_LOOK: &str = "train_ride";
pub const HALL_LOOK: &str = "train_hall";

/// Which look `p` is in, if it's somewhere dark: a ride car (deep under
/// the map) or an underground station's hall.
pub fn dark_at(t: &Tuning, p: Vec3) -> Option<&'static str> {
    if p.y < t.ride_at.1 + 30.0 {
        return Some(RIDE_LOOK);
    }
    let under = t.lines.iter().flat_map(|l| l.stops.iter()).any(|s| {
        if s.depth <= 0.0 {
            return false;
        }
        let l = stop_frame(s).compute_affine().inverse().transform_point3(p);
        (HALL_X - 1.0..PORTAL_X + 120.0).contains(&l.x)
            && (-TRACK_W - 1.0..HALL_Z + 1.0).contains(&l.z)
            && (-1.0..s.depth - 0.9).contains(&l.y)
    });
    under.then_some(HALL_LOOK)
}

/// Local: a ride car or an underground hall puts the sky out (`place.rs`).
fn look(here: Res<crate::place::Here>, mut looks: ResMut<crate::place::PlaceLooks>) {
    if let Some(id) = dark_at(&tuning(), here.0) {
        looks.push(id);
    }
}

/// Speed of the tunnel lights going by (m/s).
const TUNNEL_SPEED: f32 = 25.0;

fn tunnel_lights(time: Res<Time>, mut lights: Query<(&TunnelLight, &mut Transform)>) {
    let dx = TUNNEL_SPEED * time.delta_secs();
    for (l, mut tf) in &mut lights {
        tf.translation.x -= dx;
        if tf.translation.x < l.origin.x - 40.0 {
            tf.translation.x += 80.0;
        }
    }
}

/// The announcer's voice saying `text`.
pub fn announcer_says(text: &str, (pitch, speed): (f32, f32)) -> (Vec<f32>, u32) {
    let mut voice = samiam::Voice::new();
    voice.pitch = pitch;
    voice.speed = speed;
    (voice.speak(text), voice.sample_rate)
}

/// Local: the doors line when you step into a waiting train, and the
/// announcement when the ride starts.
fn announce(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<Option<Car>>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let t = tuning();
    let car = car_at(&t, w.pos);
    if car == *was {
        return;
    }
    let before = *was;
    *was = car;
    match car {
        Some(Car::Waiting { .. }) if !matches!(before, Some(Car::Riding { .. })) => {
            toast.show(t.boarding_line.clone());
        }
        Some(Car::Riding { line, to }) => {
            let Some(stop) = t.lines.get(line).map(|l| &l.stops[to]) else {
                return;
            };
            toast.show(stop.announce.clone());
            let (samples, rate) = announcer_says(&stop.announce, t.voice);
            crate::spatial::play_wav_at(
                &mut commands,
                w.pos + Vec3::Y * 2.5,
                1.0,
                crate::spatial::Carry::NEARBY,
                move || crate::features::bear::wav_bytes(&samples, rate),
            );
        }
        _ => {}
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
        assert!(t.board_secs > 0.0 && t.ride_secs > t.board_secs);
        assert!(t.depart_secs > 0.0 && t.arrive_secs > 0.0 && t.away_secs > 0.0);
        assert!(t.voice.0 > 0.0 && t.voice.1 > 0.0 && !t.boarding_line.is_empty());
        assert!(!t.lines.is_empty());
        for l in &t.lines {
            assert!(!l.name.is_empty());
            for s in &l.stops {
                assert!(!s.name.is_empty() && !s.announce.is_empty());
            }
        }
        // The PATH goes to the Oculus.
        let path = &t.lines[0];
        let city = path.stops.iter().find(|s| s.city).expect("a city stop");
        let oculus = Vec2::new(-55.0, 230.0);
        assert!(Vec2::new(city.at.0, city.at.1).distance(oculus) < 15.0);
    }

    /// Run the trains until `done` says so (or give up): the body's position each tick.
    fn run_until(
        t: &Tuning,
        runs: &mut HashMap<(usize, usize), Run>,
        r: &mut HashMap<u8, Rider>,
        p: &mut Vec3,
        secs: f32,
        mut done: impl FnMut(Vec3, &HashMap<(usize, usize), Run>) -> bool,
    ) -> bool {
        for _ in 0..(secs * 20.0) as usize {
            if let Some(to) = step_all(t, runs, r, &[(0, *p)], 0.05).get(&0) {
                *p = *to;
            }
            if done(*p, runs) {
                return true;
            }
        }
        false
    }

    #[test]
    fn the_train_leaves_with_you_rides_and_pulls_in_at_the_other_end() {
        let mut t = t();
        t.lines.truncate(1);
        let mut runs: HashMap<(usize, usize), Run> = (0..2)
            .map(|i| {
                (
                    (0, i),
                    Run {
                        line: 0,
                        stop: i as u8,
                        ..default()
                    },
                )
            })
            .collect();
        let mut r = HashMap::new();
        let stops = t.lines[0].stops.clone();
        // Off the train first, then into the one at the valley.
        let mut p = stop_origin(&stops[0]) + Vec3::new(0.0, 0.3, 20.0);
        step_all(&t, &mut runs, &mut r, &[(0, p)], 0.05);
        p = stop_origin(&stops[0]) + ABOARD;
        // It pulls out with you: carried along the track into the tunnel.
        let start = p;
        assert!(run_until(
            &t,
            &mut runs,
            &mut r,
            &mut p,
            t.board_secs + 1.0,
            |_, runs| { runs[&(0, 0)].phase == Phase::Leaving }
        ));
        let mut furthest: f32 = 0.0;
        assert!(run_until(
            &t,
            &mut runs,
            &mut r,
            &mut p,
            t.depart_secs + 1.0,
            |p, _| {
                furthest = furthest.max(p.distance(start));
                car_at(&t, p) == Some(Car::Riding { line: 0, to: 1 })
            }
        ));
        assert!(
            furthest > PORTAL_X + HALF_LEN,
            "never got into the tunnel: {furthest}"
        );
        // The ride, then out of the far tunnel and stopping at the platform there.
        let mut arrived_moving = false;
        assert!(run_until(
            &t,
            &mut runs,
            &mut r,
            &mut p,
            t.ride_secs + 30.0,
            |p, runs| {
                if runs[&(0, 1)].phase == Phase::Arriving
                    && !matches!(car_at(&t, p), Some(Car::Riding { .. }))
                {
                    arrived_moving = true;
                }
                runs[&(0, 1)].phase == Phase::Waiting && arrived_moving
            }
        ));
        assert_eq!(car_at(&t, p), Some(Car::Waiting { line: 0, stop: 1 }));
        assert!(crate::rules::nyc::in_city(p.x, p.z));
        assert!(
            p.y < crate::rules::nyc::CITY_Y - 5.0,
            "the platform is under the Oculus: {p}"
        );
        // Standing in the train you came on doesn't send you straight back.
        assert!(!run_until(
            &t,
            &mut runs,
            &mut r,
            &mut p,
            10.0,
            |_, runs| { runs[&(0, 1)].phase != Phase::Waiting }
        ));
        // Step off and back on: you ride home.
        let on = p;
        p += Vec3::Z * 5.0;
        step_all(&t, &mut runs, &mut r, &[(0, p)], 0.05);
        p = on;
        assert!(run_until(
            &t,
            &mut runs,
            &mut r,
            &mut p,
            t.board_secs + 1.0,
            |_, runs| { runs[&(0, 1)].phase == Phase::Leaving }
        ));
    }

    #[test]
    fn the_train_slows_into_the_station() {
        let t = t();
        assert_eq!(offset(&t, Phase::Waiting, 3.0), 0.0);
        assert_eq!(offset(&t, Phase::Leaving, 0.0), 0.0);
        assert!((offset(&t, Phase::Leaving, t.depart_secs) - t.run_out).abs() < 1e-4);
        assert!((offset(&t, Phase::Arriving, 0.0) - t.run_out).abs() < 1e-4);
        assert!(offset(&t, Phase::Arriving, t.arrive_secs).abs() < 1e-4);
        // Out of sight in the tunnel when away.
        assert!(t.run_out - HALF_LEN > PORTAL_X);
    }

    #[test]
    fn underground_stairs_come_up_through_a_hole() {
        let t = t();
        let wtc = t.lines[0]
            .stops
            .iter()
            .find(|s| s.depth > 0.0)
            .expect("an underground stop");
        let h = stairs_hole(wtc).expect("a hole");
        let (_, top) = stairs_ends(wtc.depth);
        let f = stop_frame(wtc);
        let top = f.transform_point(top - Vec3::X * 0.5);
        assert!(h.contains(top.xz()));
        assert_eq!(h.top, Some(crate::rules::nyc::CITY_Y));
        // Inside the Oculus.
        assert!(top.xz().distance(Vec2::new(-55.0, 230.0)) < 25.0);
        assert_eq!(start_name(wtc), "world_trade_center");
    }

    #[test]
    fn the_cars_are_boxed_in() {
        assert_eq!(car_boxes(true).len(), car_boxes(false).len() + DOORS.len());
        let o = Vec3::new(0.0, 0.0, 0.0);
        assert!(in_car(o, 0.0, Vec3::new(0.0, FLOOR, 0.0)));
        assert!(!in_car(o, 0.0, Vec3::new(0.0, FLOOR, 3.0)));
        // Turned 90 degrees, the car runs along z.
        assert!(in_car(o, 90.0, Vec3::new(0.0, FLOOR, 6.0)));
    }

    #[test]
    fn the_ride_and_the_hall_are_dark_and_the_valley_isnt() {
        let t = t();
        let ride = ride_origin(&t, 0, 1) + ABOARD;
        assert_eq!(dark_at(&t, ride), Some(RIDE_LOOK));
        let path = &t.lines[0];
        let wtc = path
            .stops
            .iter()
            .find(|s| s.depth > 0.0)
            .expect("underground");
        let platform = stop_frame(wtc).transform_point(Vec3::new(0.0, 1.8, 3.0));
        assert_eq!(dark_at(&t, platform), Some(HALL_LOOK));
        // Up in the Oculus it's daylight.
        assert_eq!(dark_at(&t, platform.with_y(1.8)), None);
        let valley = path
            .stops
            .iter()
            .find(|s| open_air(s))
            .expect("a valley stop");
        assert_eq!(dark_at(&t, stop_origin(valley) + Vec3::Y * 1.8), None);
        assert!(valley.raise > 0.0);
    }

    #[test]
    fn the_announcer_speaks() {
        let (s, rate) = announcer_says("Next stop: World Trade Center.", (80.0, 0.9));
        assert!(rate > 0 && s.len() > rate as usize / 2);
    }
}
