//! Penn Station. From issue #357 by catherine: "add rose pizza in Penn
//! Station and it's bathroom. when you get to the bathroom someone is always
//! in front of you and you always have to have a really weird nonsensical
//! conversation with them before they go in and then you get 15 seconds of
//! silence as u wait for them to finish and can reflect on the convo and then
//! you can use the bathroom. the mirror is so dirty and small and shitty u can
//! barely see yourself. important detail. make it look like the real place".
//!
//! You get there on the 1 train (a line in `trains.ron`, from its stop in the
//! trees at the city's north end). The concourse (the `penn-station` model,
//! its origin the Penn Station stop's frame) is a low, dingy LIRR-style hall
//! that floats high over New York like Miami: tiled columns, flickering
//! tubes, a departures board, Rose Pizza's counter under a red neon sign and
//! a tiled restroom with a sink, a postcard-sized filthy mirror and a stall.
//!
//! The bathroom queue is yours alone (each player has their own person in
//! front of them): walk up to the door, they talk nonsense at you, go in, and
//! then nothing happens for 15 seconds. Then the stall's yours. Walk away
//! and come back and somebody else is in front of you. Writing and timings in
//! `data/side/penn.ron`; the colliders are mesh-less boxes, so the place
//! works without the model file.
//!
//! Issue #352 by doomy: "It should be accurate to how it looks externally. it
//! can be a new map load when u go inside. ... there is an NJ Transit side,
//! and a LIRR side. Both should have completely different vibes". From the
//! street it's the Garden's finned drum with the glass 7th Ave entrance
//! (`nyc-penn` on the city map, its `penn` spot): walk in the doors and
//! you're in the concourse; the stairs by the tracks (EXIT 7 AV) take you back
//! out. Off the hall's west side is the NJ Transit wing (low, cold blue light,
//! every storefront behind a chained gate, trash fires, blood, pee and poop on
//! the floor, a board of cancellations); through the north wall is the LIRR
//! wing (tall, warm, a wood ceiling, a lounge, a cafe, charging and a bottle
//! refill). Each side plays its own music (moods `PennNjt` and `PennLirr`,
//! signals `penn_njt` / `penn_lirr`).

use bevy::prelude::*;
use serde::Deserialize;

use crate::AppState;
use bevy_replicon::prelude::*;

use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked};
use crate::player::{LocalPlayer, Player};

/// Tuning and writing: `assets/data/side/penn.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The stop in `trains.ron` the concourse is built round.
    pub stop: String,
    /// Stand this close (m) to the bathroom door and the conversation starts.
    pub queue_reach: f32,
    /// Seconds each line of the conversation stays up.
    pub line_secs: f32,
    /// Seconds of silence while they're in there.
    pub silence_secs: f32,
    /// Walk this far (m) from the door once you've been and the next person is in line.
    pub reset_dist: f32,
    /// Shown when you walk up to Rose Pizza.
    pub pizza_line: String,
    /// Shown when you buy a slice.
    pub slice_line: String,
    /// Shown when the person goes in.
    pub gone_in_line: String,
    /// Shown when the silence is over.
    pub free_line: String,
    /// Shown when you try the stall before it's your turn.
    pub occupied_line: String,
    /// Shown when you use it.
    pub used_line: String,
    /// Shown when you look in the mirror.
    pub mirror_line: String,
    /// One conversation per visit, in turn.
    pub conversations: Vec<Vec<String>>,
    /// Stand this close (m) to the street doors or the way-out stairs and you go through.
    pub door_reach: f32,
    /// Shown when you walk into the NJ Transit side, and into the LIRR side.
    pub njt_line: String,
    pub lirr_line: String,
    /// How bright the trash fires burn (lumens) and how much they flicker (0..1).
    pub fire_lumens: f32,
    pub fire_flicker: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/penn.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------------ the concourse's shape
// Offsets inside the `penn-station` model, in the stop's frame (x along the
// track, +z away from it, y up from the floor).

/// The concourse's corners (x, z) and its ceiling.
const HALL_MIN: Vec2 = Vec2::new(-30.0, -4.0);
const HALL_MAX: Vec2 = Vec2::new(10.0, 34.0);
const HALL_H: f32 = 4.2;
/// The bathroom door's middle (in the restroom's west wall) and where the
/// person in front of you stands (just outside it).
pub const DOOR: Vec3 = Vec3::new(1.1, 0.0, 24.1);
pub const IN_LINE: Vec3 = Vec3::new(0.4, 0.0, 24.1);
/// The toilet, the mirror and Rose Pizza's counter.
pub const TOILET: Vec3 = Vec3::new(8.1, 0.0, 33.2);
pub const MIRROR: Vec3 = Vec3::new(3.0, 1.55, 33.9);
pub const COUNTER: Vec3 = Vec3::new(-21.0, 0.0, 29.3);
/// The NJ Transit wing: a strip off the hall's west side (through a gap at
/// z 16..22) and a room north of it; its ceiling. Corners (x, z).
const NJT_A: (Vec2, Vec2) = (Vec2::new(-48.0, 10.0), Vec2::new(-30.0, 34.0));
const NJT_B: (Vec2, Vec2) = (Vec2::new(-48.0, 34.0), Vec2::new(-19.0, 68.0));
const NJT_H: f32 = 3.6;
/// The LIRR wing, through the hall's north wall (a gap at x -17..-3), and its ceiling.
const LIRR: (Vec2, Vec2) = (Vec2::new(-18.7, 34.0), Vec2::new(10.0, 68.0));
const LIRR_H: f32 = 6.0;
/// The trash fires' barrels (NJ Transit side).
pub const FIRES: [Vec3; 3] = [
    Vec3::new(-40.0, 0.0, 24.0),
    Vec3::new(-34.0, 0.0, 50.0),
    Vec3::new(-26.0, 0.0, 60.0),
];
/// Where you come in from the street, and the foot of the way-out stairs (EXIT 7 AV).
pub const ARRIVE: Vec3 = Vec3::new(-10.0, 0.0, 16.0);
pub const EXIT: Vec3 = Vec3::new(-14.0, 0.0, 0.2);
/// On the street, from the map's `penn` spot (the drum's middle; the
/// entrance faces +x, the avenue): the doors, and where you come out.
pub const DOORS: Vec3 = Vec3::new(20.2, 0.0, 0.0);
pub const STREET: Vec3 = Vec3::new(23.0, 0.0, 0.0);

/// Which side of the station you're on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    NjTransit,
    Lirr,
}

fn inside(r: (Vec2, Vec2), l: Vec3, h: f32) -> bool {
    (r.0.x..r.1.x).contains(&l.x) && (r.0.y..r.1.y).contains(&l.z) && (-1.0..h).contains(&l.y)
}

/// Which wing a point (stop frame) is in, if any.
pub fn side(l: Vec3) -> Option<Side> {
    if inside(NJT_A, l, NJT_H) || inside(NJT_B, l, NJT_H) {
        Some(Side::NjTransit)
    } else if inside(LIRR, l, LIRR_H) {
        Some(Side::Lirr)
    } else {
        None
    }
}

/// Anywhere in the station: the hall or a wing.
pub fn in_station(l: Vec3) -> bool {
    in_hall(l) || side(l).is_some()
}

/// Where the map puts Penn Station's drum (its `penn` spot).
pub fn street_spot(map: &friendslop_world::places::MapFile) -> Option<Vec3> {
    let e = map.entities.iter().find(|e| e.name == "penn")?;
    Some(Vec3::from_array(e.translation))
}

/// Walking through: from the street doors into the concourse, or from the
/// way-out stairs onto the street. `pos` is your feet, `spot` the drum's
/// middle, `f` the concourse's frame. Where you end up, if anywhere.
pub fn through(pos: Vec3, spot: Vec3, f: &Transform, reach: f32) -> Option<Vec3> {
    let flat = |a: Vec3, b: Vec3| Vec2::new(a.x - b.x, a.z - b.z).length();
    if flat(pos, spot + DOORS) < reach && (pos.y - spot.y).abs() < 3.0 {
        return Some(f.transform_point(ARRIVE));
    }
    let l = f.rotation.inverse() * (pos - f.translation);
    if in_hall(l) && flat(l, EXIT) < reach {
        return Some(spot + STREET);
    }
    None
}

/// The Penn Station stop's frame (the model's origin), if the line is there.
pub fn frame() -> Option<Transform> {
    let t = tuning();
    let trains = crate::features::trains::tuning();
    trains
        .lines
        .iter()
        .flat_map(|l| l.stops.iter())
        .find(|s| s.name == t.stop)
        .map(crate::features::trains::stop_frame)
}

/// The concourse's static boxes (centre, size) in the stop's frame.
pub fn boxes() -> Vec<(Vec3, Vec3)> {
    let span = |a: Vec3, b: Vec3| ((a + b) / 2.0, (b - a).abs());
    let (lo, hi, h) = (HALL_MIN, HALL_MAX, HALL_H);
    vec![
        // Floor and ceiling.
        span(Vec3::new(lo.x, -0.3, lo.y), Vec3::new(hi.x, 0.0, hi.y)),
        span(Vec3::new(lo.x, h, lo.y), Vec3::new(hi.x, h + 0.3, hi.y)),
        // Walls (the east one leaves the tunnel mouth to the station's own
        // boxes; the north and west ones leave gaps into the wings).
        span(Vec3::new(lo.x, 0.0, hi.y), Vec3::new(-17.0, h, hi.y + 0.3)),
        span(Vec3::new(-3.0, 0.0, hi.y), Vec3::new(hi.x, h, hi.y + 0.3)),
        span(Vec3::new(lo.x, 0.0, lo.y - 0.3), Vec3::new(hi.x, h, lo.y)),
        span(Vec3::new(lo.x - 0.3, 0.0, lo.y), Vec3::new(lo.x, h, 16.0)),
        span(Vec3::new(lo.x - 0.3, 0.0, 22.0), Vec3::new(lo.x, h, hi.y)),
        span(Vec3::new(hi.x, 0.0, 5.1), Vec3::new(hi.x + 0.3, h, hi.y)),
        // The way-out stairs.
        span(Vec3::new(-15.5, 0.0, -4.0), Vec3::new(-12.5, 2.4, -0.6)),
        // The NJ Transit wing: floors, ceilings, walls, the wall to the LIRR.
        span(
            Vec3::new(NJT_A.0.x, -0.3, NJT_A.0.y),
            Vec3::new(NJT_A.1.x, 0.0, NJT_A.1.y),
        ),
        span(
            Vec3::new(NJT_B.0.x, -0.3, NJT_B.0.y),
            Vec3::new(NJT_B.1.x, 0.0, NJT_B.1.y),
        ),
        span(
            Vec3::new(NJT_A.0.x, NJT_H, NJT_A.0.y),
            Vec3::new(NJT_A.1.x, NJT_H + 0.3, NJT_A.1.y),
        ),
        span(
            Vec3::new(NJT_B.0.x, NJT_H, NJT_B.0.y),
            Vec3::new(NJT_B.1.x, NJT_H + 0.3, NJT_B.1.y),
        ),
        span(Vec3::new(-48.3, 0.0, 10.0), Vec3::new(-48.0, NJT_H, 68.0)),
        span(Vec3::new(-48.0, 0.0, 9.7), Vec3::new(-30.0, NJT_H, 10.0)),
        span(Vec3::new(-48.0, 0.0, 68.0), Vec3::new(-19.0, NJT_H, 68.3)),
        span(Vec3::new(-19.0, 0.0, 34.0), Vec3::new(-18.7, LIRR_H, 68.0)),
        // The LIRR wing.
        span(
            Vec3::new(LIRR.0.x, -0.3, LIRR.0.y),
            Vec3::new(LIRR.1.x, 0.0, LIRR.1.y),
        ),
        span(
            Vec3::new(LIRR.0.x, LIRR_H, LIRR.0.y),
            Vec3::new(LIRR.1.x, LIRR_H + 0.3, LIRR.1.y),
        ),
        span(
            Vec3::new(LIRR.0.x, 0.0, 68.0),
            Vec3::new(10.0, LIRR_H, 68.3),
        ),
        span(Vec3::new(10.0, 0.0, 34.0), Vec3::new(10.3, LIRR_H, 68.0)),
        span(Vec3::new(LIRR.0.x, h, 34.0), Vec3::new(10.0, LIRR_H, 34.3)),
        // The cafe's counter and the fire barrels.
        span(Vec3::new(-0.5, 0.0, 64.5), Vec3::new(4.5, 1.1, 65.5)),
        span(
            FIRES[0] - Vec3::new(0.35, 0.0, 0.35),
            FIRES[0] + Vec3::new(0.35, 0.9, 0.35),
        ),
        span(
            FIRES[1] - Vec3::new(0.35, 0.0, 0.35),
            FIRES[1] + Vec3::new(0.35, 0.9, 0.35),
        ),
        span(
            FIRES[2] - Vec3::new(0.35, 0.0, 0.35),
            FIRES[2] + Vec3::new(0.35, 0.9, 0.35),
        ),
        // Rose Pizza's counter and ovens.
        span(Vec3::new(-26.5, 0.0, 29.6), Vec3::new(-15.5, 1.1, 30.4)),
        span(Vec3::new(-27.5, 0.0, 32.8), Vec3::new(-19.0, 1.6, 34.0)),
        // The restroom: its south wall, its west wall either side of the door.
        span(Vec3::new(1.0, 0.0, 21.9), Vec3::new(hi.x, h, 22.1)),
        span(Vec3::new(1.0, 0.0, 22.1), Vec3::new(1.2, h, DOOR.z - 0.6)),
        span(Vec3::new(1.0, 0.0, DOOR.z + 0.6), Vec3::new(1.2, h, hi.y)),
        // The stall's side and front (its door stands open).
        span(Vec3::new(6.47, 0.0, 30.3), Vec3::new(6.53, 2.15, hi.y)),
        span(Vec3::new(6.5, 0.0, 30.27), Vec3::new(8.1, 2.15, 30.33)),
        // The bench.
        span(Vec3::new(-14.0, 0.0, 23.7), Vec3::new(-10.0, 0.45, 24.3)),
    ]
}

/// Is a point (stop frame) inside the concourse?
pub fn in_hall(l: Vec3) -> bool {
    (HALL_MIN.x..HALL_MAX.x).contains(&l.x)
        && (HALL_MIN.y..HALL_MAX.y).contains(&l.z)
        && (-1.0..HALL_H).contains(&l.y)
}

// ------------------------------------------------------------------ the queue

/// Where you are in the bathroom line.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Queue {
    /// Someone's in front of you, waiting.
    #[default]
    Waiting,
    /// They're talking at you: line `line`, `secs` into it.
    Talking { line: usize, secs: f32 },
    /// They're in there. Nothing happens.
    Silence { secs: f32 },
    /// It's yours.
    Free,
    /// You've been.
    Used,
}

/// What the HUD should say after a step.
#[derive(Clone, Debug, PartialEq)]
pub enum Say {
    Line(usize),
    GoneIn,
    Free,
}

/// One step of the queue: `near` is whether you're at the door, `away`
/// whether you've walked off far enough for the next person, `lines` how long
/// this visit's conversation is.
pub fn step(
    q: Queue,
    dt: f32,
    near: bool,
    away: bool,
    lines: usize,
    t: &Tuning,
) -> (Queue, Option<Say>) {
    match q {
        Queue::Waiting if near => {
            if lines == 0 {
                (Queue::Silence { secs: 0.0 }, Some(Say::GoneIn))
            } else {
                (Queue::Talking { line: 0, secs: 0.0 }, Some(Say::Line(0)))
            }
        }
        Queue::Talking { line, secs } => {
            let secs = secs + dt;
            if secs < t.line_secs {
                (Queue::Talking { line, secs }, None)
            } else if line + 1 < lines {
                (
                    Queue::Talking {
                        line: line + 1,
                        secs: 0.0,
                    },
                    Some(Say::Line(line + 1)),
                )
            } else {
                (Queue::Silence { secs: 0.0 }, Some(Say::GoneIn))
            }
        }
        Queue::Silence { secs } => {
            let secs = secs + dt;
            if secs >= t.silence_secs {
                (Queue::Free, Some(Say::Free))
            } else {
                (Queue::Silence { secs }, None)
            }
        }
        Queue::Used if away => (Queue::Waiting, None),
        q => (q, None),
    }
}

/// Is the person in front of you still standing outside?
pub fn stranger_outside(q: Queue) -> bool {
    matches!(q, Queue::Waiting | Queue::Talking { .. })
}

// ------------------------------------------------------------------ every peer (all local)

pub fn plugin(app: &mut App) {
    app.init_resource::<Line>()
        .add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build)
        .add_systems(OnExit(AppState::InGame), |mut l: ResMut<Line>| {
            *l = Line::default()
        })
        .add_systems(
            Update,
            (queue, pizza_line, side_line, fires, music_signals).run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            walk_through
                .run_if(in_state(AppState::InGame))
                .run_if(in_state(ServerState::Running)),
        );
}

/// Local: your place in the bathroom line and which visit this is.
#[derive(Resource, Default)]
struct Line {
    queue: Queue,
    visit: usize,
}

#[derive(Component)]
struct Stranger;

#[derive(Component, Clone, Copy, PartialEq)]
enum Thing {
    Toilet,
    Mirror,
    Slice,
}

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let Some(f) = frame() else { return };
    commands.spawn((
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset("models/penn-station.gltf")),
        ),
        f,
        DespawnOnExit(AppState::InGame),
    ));
    for (c, s) in boxes() {
        commands.spawn((
            Transform::from_translation(f.transform_point(c)).with_rotation(f.rotation),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    spawn_stranger(&mut commands, &f, 0);
    // The trash fires' light (flickered by `fires`).
    for (i, at) in FIRES.iter().enumerate() {
        commands.spawn((
            Fire(i),
            PointLight {
                color: Color::srgb(1.0, 0.55, 0.2),
                intensity: tuning().fire_lumens,
                range: 9.0,
                ..default()
            },
            Transform::from_translation(f.transform_point(*at + Vec3::Y * 1.5)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // The ways in and out.
    let out = f.rotation * Vec3::NEG_Z;
    crate::features::doorway::spawn(
        &mut commands,
        f.transform_point(EXIT),
        crate::features::doorway::yaw_towards(Vec2::new(out.x, out.z)),
        crate::features::doorway::Doorway::way_out("7 Av & 33 St").size(3.0, 2.4),
    );
    if let Some(spot) = street_spot(&super::nyc::MAP.get()) {
        crate::features::doorway::spawn(
            &mut commands,
            spot + DOORS,
            crate::features::doorway::yaw_towards(Vec2::NEG_X),
            crate::features::doorway::Doorway::way_in("Penn Station").size(3.6, 2.8),
        );
        let street = spot + STREET;
        commands.queue(move |world: &mut World| {
            if let Some(mut p) = world.get_resource_mut::<crate::features::start::StartPoints>() {
                p.add("penn_street", street);
            }
        });
    }
    for (thing, at, label, h) in [
        (Thing::Toilet, TOILET, "use the bathroom", 0.5),
        (
            Thing::Mirror,
            MIRROR - Vec3::Y * 1.2,
            "look in the mirror",
            1.2,
        ),
        (Thing::Slice, COUNTER, "buy a slice", 1.0),
    ] {
        commands.spawn((
            thing,
            Transform::from_translation(f.transform_point(at)),
            Clickable::at_height(h).labelled(label),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // `FRIENDSLOP_START=penn` starts you in the concourse.
    let start = f.transform_point(Vec3::new(-10.0, 0.05, 16.0));
    commands.queue(move |world: &mut World| {
        if let Some(mut p) = world.get_resource_mut::<crate::features::start::StartPoints>() {
            p.add("penn", start);
        }
    });
}

#[derive(Component)]
struct Fire(usize);

/// Server: through the street doors or up the way-out stairs.
fn walk_through(
    mut bodies: Query<(
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    let (Some(f), Some(spot)) = (frame(), street_spot(&super::nyc::MAP.get())) else {
        return;
    };
    let reach = tuning().door_reach;
    for (mut walker, input, kcc) in &mut bodies {
        let Some(to) = through(walker.pos, spot, &f, reach) else {
            continue;
        };
        walker.pos = to + Vec3::Y * 0.05;
        if let Some(mut input) = input {
            input.stick = Vec2::ZERO;
            input.jump = false;
        }
        if let Some(mut kcc) = kcc {
            kcc.velocity = Vec3::ZERO;
        }
    }
}

/// How bright a trash fire is at `secs` (a cheap flicker, different per fire).
pub fn fire_at(lumens: f32, flicker: f32, secs: f32, i: usize) -> f32 {
    let p = i as f32 * 1.7;
    let w = (secs * 11.0 + p).sin() * 0.5
        + (secs * 23.0 + p * 2.3).sin() * 0.3
        + (secs * 5.0).sin() * 0.2;
    lumens * (1.0 + flicker * w).max(0.0)
}

/// Local: the fires flicker.
fn fires(time: Res<Time>, mut lights: Query<(&Fire, &mut PointLight)>) {
    let t = tuning();
    for (fire, mut light) in &mut lights {
        light.intensity = fire_at(t.fire_lumens, t.fire_flicker, time.elapsed_secs(), fire.0);
    }
}

fn my_side(me: &Option<Res<LocalPlayer>>, walkers: &Query<(&Player, &Walker)>) -> Option<Side> {
    let f = frame()?;
    let pos = my_pos(me, walkers)?;
    side(f.rotation.inverse() * (pos - f.translation))
}

/// Local: a line when you walk into either side.
fn side_line(
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<Option<Side>>,
) {
    let now = my_side(&me, &walkers);
    if now != *was
        && let (Some(s), Some(mut toast)) = (now, toast)
    {
        let t = tuning();
        toast.show(match s {
            Side::NjTransit => t.njt_line.clone(),
            Side::Lirr => t.lirr_line.clone(),
        });
    }
    *was = now;
}

/// For the music: the camera's on the NJ Transit side (`penn_njt`) or the LIRR side (`penn_lirr`).
fn music_signals(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let at = crate::music::camera_at(&camera);
    let s = frame()
        .zip(at)
        .and_then(|(f, p)| side(f.rotation.inverse() * (p - f.translation)));
    signals.flag("penn_njt", s == Some(Side::NjTransit));
    signals.flag("penn_lirr", s == Some(Side::Lirr));
}

fn spawn_stranger(commands: &mut Commands, f: &Transform, visit: usize) {
    // Facing you (-x, out of the door), back to the door.
    let turn = f.rotation * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
    commands.spawn((
        Stranger,
        crate::people::Gait::default(),
        Transform::from_translation(f.transform_point(IN_LINE)).with_rotation(turn),
        Visibility::default(),
        crate::people::CharacterLook::random(0x0357_0000 + visit as u64 * 7919),
        DespawnOnExit(AppState::InGame),
    ));
}

fn my_pos(me: &Option<Res<LocalPlayer>>, walkers: &Query<(&Player, &Walker)>) -> Option<Vec3> {
    let me = me.as_ref()?;
    walkers
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map(|(_, w)| w.pos)
}

/// Local: the line moves (or doesn't).
fn queue(
    mut commands: Commands,
    time: Res<Time>,
    mut line: ResMut<Line>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    mut strangers: Query<(Entity, &mut Visibility), With<Stranger>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (Some(f), Some(pos)) = (frame(), my_pos(&me, &walkers)) else {
        return;
    };
    let t = tuning();
    let l = f.rotation.inverse() * (pos - f.translation);
    let d = Vec2::new(l.x - DOOR.x, l.z - DOOR.z).length();
    let near = in_hall(l) && d < t.queue_reach;
    let away = !in_hall(l) || d > t.reset_dist;
    let convo = if t.conversations.is_empty() {
        None
    } else {
        Some(&t.conversations[line.visit % t.conversations.len()])
    };
    let lines = convo.map_or(0, |c| c.len());
    let was = line.queue;
    let (next, say) = step(was, time.delta_secs(), near, away, lines, &t);
    line.queue = next;
    if was == Queue::Used && next == Queue::Waiting {
        // Somebody new is in front of you.
        line.visit += 1;
        for (e, _) in &strangers {
            commands.entity(e).despawn();
        }
        spawn_stranger(&mut commands, &f, line.visit);
    }
    let outside = stranger_outside(next);
    for (_, mut v) in &mut strangers {
        let want = if outside {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        v.set_if_neq(want);
    }
    let (Some(say), Some(mut toast)) = (say, toast) else {
        return;
    };
    let text = match say {
        Say::Line(i) => convo.and_then(|c| c.get(i)).cloned().unwrap_or_default(),
        Say::GoneIn => t.gone_in_line.clone(),
        Say::Free => t.free_line.clone(),
    };
    toast.show(text);
}

fn click(
    click: On<Clicked>,
    things: Query<&Thing>,
    mut line: ResMut<Line>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (Ok(thing), Some(mut toast)) = (things.get(click.entity), toast) else {
        return;
    };
    let t = tuning();
    let text = match thing {
        Thing::Mirror => t.mirror_line.clone(),
        Thing::Slice => t.slice_line.clone(),
        Thing::Toilet => match line.queue {
            Queue::Free => {
                line.queue = Queue::Used;
                t.used_line.clone()
            }
            Queue::Used => t.used_line.clone(),
            _ => t.occupied_line.clone(),
        },
    };
    toast.show(text);
}

/// Local: Rose Pizza's line when you walk up to the counter.
fn pizza_line(
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<bool>,
) {
    let (Some(f), Some(pos), Some(mut toast)) = (frame(), my_pos(&me, &walkers), toast) else {
        return;
    };
    let l = f.rotation.inverse() * (pos - f.translation);
    let near = in_hall(l) && Vec2::new(l.x - COUNTER.x, l.z - COUNTER.z).length() < 6.0;
    if near && !*was {
        toast.show(tuning().pizza_line.clone());
    }
    *was = near;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load<T: Send + Sync + 'static>(d: &'static friendslop_data::Data<T>) {
        let path = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), d.path());
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(d.set_bytes(&bytes), "{path} doesn't parse");
    }

    fn t() -> Tuning {
        load(&TUNING);
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.queue_reach > 0.0 && t.line_secs > 0.0 && t.reset_dist > t.queue_reach);
        assert_eq!(t.silence_secs, 15.0, "catherine: 15 seconds of silence");
        assert!(t.door_reach > 0.0 && t.fire_lumens > 0.0 && (0.0..=1.0).contains(&t.fire_flicker));
        assert!(!t.njt_line.is_empty() && !t.lirr_line.is_empty());
        assert!(!t.conversations.is_empty() && t.conversations.iter().all(|c| !c.is_empty()));
        for s in [
            &t.pizza_line,
            &t.slice_line,
            &t.gone_in_line,
            &t.free_line,
            &t.occupied_line,
            &t.used_line,
            &t.mirror_line,
        ] {
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn the_1_train_stops_in_the_concourse_inside_the_city() {
        let t = t();
        load(&crate::features::trains::TUNING);
        let Some(f) = frame() else {
            panic!("no stop named {} in trains.ron", t.stop);
        };
        for c in [HALL_MIN, HALL_MAX, NJT_A.0, NJT_B.1, LIRR.1] {
            let p = f.transform_point(Vec3::new(c.x, 0.0, c.y));
            assert!(
                crate::rules::nyc::in_city(p.x, p.z),
                "the city's soft wall would push you home"
            );
        }
        // Clear of Miami's island.
        let miami = Vec3::from(crate::features::miami::TUNING.get().ground);
        assert!((f.translation.y - miami.y).abs() > 100.0 || miami == Vec3::ZERO);
    }

    #[test]
    fn someone_is_always_in_front_then_silence_then_its_yours() {
        let t = t();
        let n = 3;
        let mut q = Queue::Waiting;
        // Not at the door: nothing.
        q = step(q, 1.0, false, false, n, &t).0;
        assert_eq!(q, Queue::Waiting);
        let (q2, say) = step(q, 0.0, true, false, n, &t);
        assert_eq!(say, Some(Say::Line(0)));
        q = q2;
        let mut said = vec![];
        let mut secs = 0.0;
        while !matches!(q, Queue::Free) {
            let (nq, say) = step(q, 0.1, true, false, n, &t);
            if let Some(s) = say {
                said.push(s);
            }
            if matches!(nq, Queue::Silence { .. }) {
                secs += 0.1;
            }
            q = nq;
            assert!(secs < 100.0);
        }
        assert_eq!(
            said,
            vec![Say::Line(1), Say::Line(2), Say::GoneIn, Say::Free]
        );
        assert!(
            (secs - t.silence_secs).abs() < 0.3,
            "about 15 s of silence, got {secs}"
        );
        assert!(!stranger_outside(Queue::Silence { secs: 0.0 }));
        // Used, walked off: someone new is in line.
        assert_eq!(step(Queue::Used, 0.1, false, true, n, &t).0, Queue::Waiting);
        assert_eq!(step(Queue::Used, 0.1, true, false, n, &t).0, Queue::Used);
    }

    #[test]
    fn the_doors_are_open_and_the_line_spot_is_clear() {
        let b = boxes();
        let inside = |p: Vec3| {
            b.iter().any(|(c, s)| {
                let d = (p - *c).abs();
                d.x < s.x / 2.0 && d.y < s.y / 2.0 && d.z < s.z / 2.0
            })
        };
        for p in [
            DOOR,
            IN_LINE,
            Vec3::new(4.0, 0.0, 27.0),
            Vec3::new(8.0, 0.0, 31.0),
            Vec3::new(-10.0, 0.0, 16.0),
            // Into the NJ Transit wing and the LIRR wing, and the way out.
            Vec3::new(-30.0, 0.0, 19.0),
            Vec3::new(-10.0, 0.0, 34.0),
            Vec3::new(-35.0, 0.0, 40.0),
            Vec3::new(-5.0, 0.0, 50.0),
            ARRIVE,
            EXIT,
        ] {
            assert!(!inside(p + Vec3::Y), "blocked at {p}");
        }
        assert!(inside(Vec3::new(0.0, -0.1, 10.0)), "a floor");
    }

    #[test]
    fn two_sides_and_the_hall_between() {
        assert_eq!(side(Vec3::new(-40.0, 1.0, 20.0)), Some(Side::NjTransit));
        assert_eq!(side(Vec3::new(-30.0, 1.0, 60.0)), Some(Side::NjTransit));
        assert_eq!(side(Vec3::new(0.0, 1.0, 50.0)), Some(Side::Lirr));
        assert_eq!(side(Vec3::new(-10.0, 1.0, 16.0)), None);
        assert!(in_station(Vec3::new(-10.0, 1.0, 16.0)));
        for at in FIRES {
            assert_eq!(
                side(at),
                Some(Side::NjTransit),
                "fires burn on the NJ Transit side"
            );
        }
    }

    #[test]
    fn in_the_doors_and_out_the_stairs() {
        let f = Transform::from_xyz(-40.0, 1000.0, 260.0);
        let spot = Vec3::new(-42.0, 0.0, 281.0);
        let inn = through(spot + DOORS, spot, &f, 1.5);
        assert_eq!(inn, Some(f.transform_point(ARRIVE)));
        // Arriving isn't standing at the way out, and coming out isn't at the doors.
        assert_eq!(through(f.transform_point(ARRIVE), spot, &f, 1.5), None);
        let out = through(f.transform_point(EXIT), spot, &f, 1.5);
        assert_eq!(out, Some(spot + STREET));
        assert_eq!(through(spot + STREET, spot, &f, 1.5), None);
    }

    #[test]
    fn the_fires_flicker_but_never_go_negative() {
        let a = fire_at(800.0, 0.6, 0.0, 0);
        let b = fire_at(800.0, 0.6, 0.37, 0);
        assert_ne!(a, b);
        for i in 0..200 {
            assert!(fire_at(800.0, 1.0, i as f32 * 0.05, 2) >= 0.0);
        }
    }
}
