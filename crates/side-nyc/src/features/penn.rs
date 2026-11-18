//! Penn Station. From issue #357 by catherine: "add rose pizza in Penn
//! Station and it's bathroom. when you get to the bathroom someone is always
//! in front of you and you always have to have a really weird nonsensical
//! conversation with them before they go in and then you get 15 seconds of
//! silence as u wait for them to finish and can reflect on the convo and then
//! you can use the bathroom. the mirror is so dirty and small and shitty u can
//! barely see yourself. important detail. make it look like the real place".
//!
//! You get there on the 1 train (a line in `trains.ron`, from its stop in the
//! trees at the city's north end). The interior is the modeler map `penn-map`,
//! exported to `maps/penn.bsn` (the models) and `maps/penn.map.json` (this
//! file's data), laid out in the Penn Station stop's frame and floating high
//! over New York like Miami. Its middle is the 7th Ave exit concourse: low,
//! tiled and buzzing, the 1 train's track along its south side, a TRACK TBA
//! board, Rose Pizza under a red neon sign, the stairs up to 7 Av, and a tiled
//! restroom with a sink, a postcard-sized filthy mirror and a stall.
//!
//! The bathroom queue is yours alone (each player has their own person in
//! front of them): walk up to the door, they talk nonsense at you, go in, and
//! then nothing happens for 15 seconds. Then the stall's yours. Walk away
//! and come back and somebody else is in front of you. Writing and timings in
//! `data/side/penn.ron`.
//!
//! Issue #352 by doomy: "It should be accurate to how it looks externally. it
//! can be a new map load when u go inside. ... there is an NJ Transit side,
//! and a LIRR side. Both should have completely different vibes". From the
//! street it's the Garden's finned drum with the glass 7th Ave entrance
//! (`nyc-penn` on the city map, its `penn` spot): walk in the doors and
//! you're in the concourse; the stairs (EXIT 7 AV) take you back out. West is
//! the NJ Transit side (wide but cramped under a 3.2 m ceiling, cold light,
//! every storefront behind a chained gate, track stairs boarded up, a board of
//! cancellations, trash fires, blood, pee and poop on the floor); north is the
//! LIRR side (7 m tall, white terrazzo, a warm wood-slat ceiling, an on-time
//! board, a lounge, a cafe, charging and a bottle refill). Each side plays its
//! own music (moods `PennNjt` and `PennLirr`, signals `penn_njt` /
//! `penn_lirr`).
//!
//! Nothing here holds positions: the rooms (`room_hall`, `room_njt`,
//! `room_lirr`), the spots (`door`, `in_line`, `toilet`, `mirror`, `counter`,
//! `arrive`, `exit`, `fire_*`) and the colliders (`Block` boxes) are markers
//! in the map. The colliders are read from the data file, so the place works
//! without the model files.

use bevy::prelude::*;
use serde::Deserialize;

use crate::AppState;
use bevy_replicon::prelude::*;

use crate::features::fps::{WalkInput, Walker, teleport};
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
    /// How bright each space's baked light probe is (1 = as baked).
    pub probe: f32,
    /// How many commuters walk the LIRR hall, their walking speed range (m/s)
    /// and how far either side of the hall's middle they keep (m).
    pub commuters: usize,
    pub commuter_speed: (f32, f32),
    pub commuter_spread: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/penn.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------------ the interior's layout

/// The interior as the modeler lays it out: the `penn-map` document, exported to
/// `assets/maps/penn.bsn` (the models) and `penn.map.json` (this data: `Block`
/// collider boxes, `room_*` and the named spots). Everything is in the Penn
/// Station stop's frame (x along the track, +z away from it, y up from the
/// floor). Read at run time on every peer, so the colliders agree everywhere.
/// Change the place in the modeler and re-export; nothing here holds positions.
pub static MAP: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/penn.map.json");

/// On the street, from the city map's `penn` spot (the drum's middle; the
/// entrance faces +x, the avenue): the doors, and where you come out. Offsets
/// inside the `nyc-penn` model, not a choice.
pub const DOORS: Vec3 = Vec3::new(22.6, 0.0, 0.0);
pub const STREET: Vec3 = Vec3::new(26.0, 0.0, 0.0);

/// A room's box in the stop's frame (floor to ceiling).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Room {
    pub min: Vec3,
    pub max: Vec3,
}

impl Room {
    /// Is `l` inside, standing anywhere from just under the floor to the ceiling?
    pub fn contains(&self, l: Vec3) -> bool {
        (self.min.x..self.max.x).contains(&l.x)
            && (self.min.z..self.max.z).contains(&l.z)
            && (self.min.y - 1.0..self.max.y).contains(&l.y)
    }
}

/// One static collider box: centre, size and turn about +y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    pub centre: Vec3,
    pub size: Vec3,
    pub yaw: f32,
}

/// What the game needs from the interior's map.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// The 7th Ave concourse, the NJ Transit side and the LIRR side.
    pub hall: Room,
    pub njt: Room,
    pub lirr: Room,
    /// The bathroom door's middle and where the person in front of you stands.
    pub door: Vec3,
    pub in_line: Vec3,
    /// The toilet, the mirror (its middle, at eye height) and Rose Pizza's counter.
    pub toilet: Vec3,
    pub mirror: Vec3,
    pub counter: Vec3,
    /// Where you come in from the street, and the foot of the way-out stairs (EXIT 7 AV).
    pub arrive: Vec3,
    pub exit: Vec3,
    /// The trash fires' drums (NJ Transit side).
    pub fires: Vec<Vec3>,
    /// The LIRR hall's two ends, where commuters turn round.
    pub commute: (Vec3, Vec3),
    pub walls: Vec<Wall>,
}

/// Read the interior out of its map file. A missing spot is the origin and a
/// missing room is empty, so a half-made map still loads.
pub fn layout(map: &friendslop_world::places::MapFile) -> Layout {
    let find = |name: &str| map.entities.iter().find(|e| e.name == name);
    let at = |name: &str| find(name).map_or(Vec3::ZERO, |e| Vec3::from_array(e.translation));
    let room = |name: &str| {
        find(name).map_or(Room::default(), |e| {
            let c = Vec3::from_array(e.translation);
            let half = e.size.map_or(Vec3::ZERO, Vec3::from_array) / 2.0;
            Room {
                min: c - half,
                max: c + half,
            }
        })
    };
    let mut fires: Vec<(String, Vec3)> = map
        .entities
        .iter()
        .filter(|e| e.kind == "spot" && e.name.starts_with("fire_"))
        .map(|e| (e.name.clone(), Vec3::from_array(e.translation)))
        .collect();
    fires.sort_by(|a, b| a.0.cmp(&b.0));
    Layout {
        hall: room("room_hall"),
        njt: room("room_njt"),
        lirr: room("room_lirr"),
        door: at("door"),
        in_line: at("in_line"),
        toilet: at("toilet"),
        mirror: at("mirror"),
        counter: at("counter"),
        arrive: at("arrive"),
        exit: at("exit"),
        fires: fires.into_iter().map(|(_, p)| p).collect(),
        commute: (at("commute_s"), at("commute_n")),
        walls: map
            .entities
            .iter()
            .filter(|e| e.has("Block"))
            .map(|e| Wall {
                centre: Vec3::from_array(e.translation),
                size: e.size.map_or(Vec3::ONE, Vec3::from_array),
                yaw: e.yaw.to_radians(),
            })
            .collect(),
    }
}

/// The interior as the data file has it now.
pub fn here() -> Layout {
    layout(&MAP.get())
}

/// Each space's look (`data/base/place_looks.ron`): the sky is out in all
/// three; the concourse is fluorescent-dim, the NJ Transit side darker in a
/// cold haze, the LIRR side brighter and warm. Each is also the name its
/// light probe is baked under (`assets/probes/<look>_*.ktx2`).
pub const LOOK_HALL: &str = "penn_hall";
pub const LOOK_NJT: &str = "penn_njt";
pub const LOOK_LIRR: &str = "penn_lirr";

/// Which space's look a point (stop frame) gets, if it's in the station.
pub fn look(lay: &Layout, l: Vec3) -> Option<&'static str> {
    match side(lay, l) {
        Some(Side::NjTransit) => Some(LOOK_NJT),
        Some(Side::Lirr) => Some(LOOK_LIRR),
        None if in_hall(lay, l) => Some(LOOK_HALL),
        None => None,
    }
}

/// The three spaces with their look (and probe) names.
fn spaces(lay: &Layout) -> [(&'static str, Room); 3] {
    [
        (LOOK_HALL, lay.hall),
        (LOOK_NJT, lay.njt),
        (LOOK_LIRR, lay.lirr),
    ]
}

/// Which side of the station you're on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    NjTransit,
    Lirr,
}

/// Which side a point (stop frame) is on, if any.
pub fn side(lay: &Layout, l: Vec3) -> Option<Side> {
    if lay.njt.contains(l) {
        Some(Side::NjTransit)
    } else if lay.lirr.contains(l) {
        Some(Side::Lirr)
    } else {
        None
    }
}

/// Anywhere in the station: the hall or a side.
pub fn in_station(lay: &Layout, l: Vec3) -> bool {
    in_hall(lay, l) || side(lay, l).is_some()
}

/// Where the map puts Penn Station's drum (its `penn` spot).
pub fn street_spot(map: &friendslop_world::places::MapFile) -> Option<Vec3> {
    let e = map.entities.iter().find(|e| e.name == "penn")?;
    Some(Vec3::from_array(e.translation))
}

/// Walking through: from the street doors into the concourse, or from the
/// way-out stairs onto the street. `pos` is your feet, `spot` the drum's
/// middle, `f` the concourse's frame. Where you end up, if anywhere.
pub fn through(lay: &Layout, pos: Vec3, spot: Vec3, f: &Transform, reach: f32) -> Option<Vec3> {
    let flat = |a: Vec3, b: Vec3| Vec2::new(a.x - b.x, a.z - b.z).length();
    if flat(pos, spot + DOORS) < reach && (pos.y - spot.y).abs() < 3.0 {
        return Some(f.transform_point(lay.arrive));
    }
    let l = f.rotation.inverse() * (pos - f.translation);
    if in_hall(lay, l) && flat(l, lay.exit) < reach {
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

/// Is a point (stop frame) inside the 7th Ave concourse?
pub fn in_hall(lay: &Layout, l: Vec3) -> bool {
    lay.hall.contains(l)
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
    crate::area::on_sim(app, build_sim);
    crate::area::on_show(app, build);
    crate::features::start::provide(starts);
    app.init_resource::<Line>()
        .add_observer(click)
        .add_systems(OnExit(AppState::InGame), |mut l: ResMut<Line>| {
            *l = Line::default()
        })
        .add_systems(
            Update,
            (
                // Penn Station is under New York: it sleeps with it (#412).
                (queue, pizza_line, side_line, fires, commute).run_if(crate::sites::here("nyc")),
                penn_look.in_set(crate::place::PlaceSet),
                music_signals,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            walk_through
                .run_if(in_state(AppState::InGame))
                .run_if(in_state(ServerState::Running))
                .run_if(crate::sites::awake("nyc")),
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
    let lay = here();
    // The models, all from the modeler map, at the stop's frame on their own
    // flat floor. SALT: not needing `Cargo.lock` for this `.bsn` either; x
    // didn't set us up for success, we need more SME's, we haven't considered
    // the problem space enough, BSN syntax needs more changes, and `Children`
    // sugaring? Ask Sam "Salt" Saltman.
    commands.spawn((
        friendslop_world::map::Map(assets.load("maps/penn.bsn")),
        friendslop_world::map::FlatGround,
        f,
        Visibility::default(),
        crate::area::show(),
    ));
    spawn_stranger(&mut commands, &f, lay.in_line, 0);
    // Commuters up and down the LIRR hall (moved by `commute`).
    for i in 0..tuning().commuters {
        let (at, dir) = commuter_at(&tuning(), lay.commute, i, 0.0);
        commands.spawn((
            Commuter(i),
            crate::people::Gait::default(),
            crate::people::CharacterLook::random(0x0357_1000 + i as u64 * 17),
            Transform::from_translation(f.transform_point(at)).with_rotation(
                f.rotation * Quat::from_rotation_y(crate::people::yaw_toward(dir.xz())),
            ),
            Visibility::default(),
            crate::area::show(),
        ));
    }
    // Each space's baked light: what lights things inside is the room's own
    // light, not the sky's (`data/game/probes.ron`; re-bake when the map changes).
    if !friendslop_base::probes::baking() {
        let t = tuning();
        for (name, r) in spaces(&lay) {
            commands.spawn((
                friendslop_base::probes::probe(
                    &assets,
                    name,
                    f.transform_point((r.min + r.max) / 2.0),
                    r.max - r.min + Vec3::Y * 0.4,
                    t.probe,
                ),
                crate::area::show(),
            ));
        }
    }
    // The trash fires' light (flickered by `fires`).
    for (i, at) in lay.fires.iter().enumerate() {
        commands.spawn((
            Fire(i),
            PointLight {
                color: Color::srgb(1.0, 0.55, 0.2),
                intensity: tuning().fire_lumens,
                range: 9.0,
                ..default()
            },
            Transform::from_translation(f.transform_point(*at + Vec3::Y * 1.5)),
            crate::area::show(),
        ));
    }
    // The ways in and out.
    let out = f.rotation * Vec3::NEG_Z;
    crate::area::doorway(
        &mut commands,
        f.transform_point(lay.exit),
        crate::features::doorway::yaw_towards(Vec2::new(out.x, out.z)),
        crate::features::doorway::Doorway::way_out("7 Av & 33 St").size(3.0, 2.4),
    );
    if let Some(spot) = street_spot(&super::nyc::MAP.get()) {
        crate::area::doorway(
            &mut commands,
            spot + DOORS,
            crate::features::doorway::yaw_towards(Vec2::NEG_X),
            crate::features::doorway::Doorway::way_in("Penn Station").size(3.6, 2.8),
        );
    }
    for (thing, at, label, h) in [
        (Thing::Toilet, lay.toilet, "use the bathroom", 0.5),
        (
            Thing::Mirror,
            lay.mirror - Vec3::Y * 1.2,
            "look in the mirror",
            1.2,
        ),
        (Thing::Slice, lay.counter, "buy a slice", 1.0),
    ] {
        commands.spawn((
            thing,
            Transform::from_translation(f.transform_point(at)),
            Clickable::at_height(h).labelled(label),
            crate::area::show(),
        ));
    }
}

fn build_sim(mut commands: Commands) {
    let Some(f) = frame() else { return };
    // Where each space's probe is baked: its middle, at head height.
    let bakes: Vec<(&'static str, Vec3)> = spaces(&here())
        .into_iter()
        .map(|(name, r)| {
            let mid = (r.min + r.max) / 2.0;
            (name, f.transform_point(mid.with_y(r.min.y + 1.7)))
        })
        .collect();
    commands.queue(move |world: &mut World| {
        if let Some(mut b) = world.get_resource_mut::<friendslop_base::probes::ProbeBakes>() {
            for (name, at) in bakes {
                b.add(name, at);
            }
        }
    });
    for w in here().walls {
        commands.spawn((
            Transform::from_translation(f.transform_point(w.centre))
                .with_rotation(f.rotation * Quat::from_rotation_y(w.yaw)),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(w.size.x, w.size.y, w.size.z),
            crate::area::sim(),
        ));
    }
}

/// `FRIENDSLOP_START=penn` starts you in the concourse, `penn_street` at its doors.
fn starts() -> Vec<(String, Vec3)> {
    let mut out = Vec::new();
    if let Some(f) = frame() {
        out.push((
            "penn".into(),
            f.transform_point(here().arrive + Vec3::Y * 0.05),
        ));
    }
    if let Some(spot) = street_spot(&super::nyc::MAP.get()) {
        out.push(("penn_street".into(), spot + STREET));
    }
    out
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
    let lay = here();
    for (mut walker, mut input, mut kcc) in &mut bodies {
        let Some(to) = through(&lay, walker.pos, spot, &f, reach) else {
            continue;
        };
        let yaw = walker.yaw;
        teleport(
            &mut walker,
            input.as_deref_mut(),
            kcc.as_deref_mut(),
            to + Vec3::Y * 0.05,
            yaw,
        );
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
/// A commuter walking the LIRR hall, by number.
#[derive(Component)]
struct Commuter(usize);

/// A number in 0..1 from a person's number and a salt.
fn unit(i: usize, salt: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Where commuter `i` is `secs` in (the stop's frame) and which way they walk:
/// up and down the hall between its ends in their own lane, at their own pace,
/// turning round at each end.
pub fn commuter_at(t: &Tuning, ends: (Vec3, Vec3), i: usize, secs: f32) -> (Vec3, Vec3) {
    let (a, b) = ends;
    let len = a.distance(b).max(0.1);
    let along = (b - a) / len;
    let across = Vec3::Y.cross(along);
    let (lo, hi) = t.commuter_speed;
    let speed = lo + (hi - lo) * unit(i, 1);
    let lane = (unit(i, 2) * 2.0 - 1.0) * t.commuter_spread;
    // A round trip is 2 * len; each starts somewhere along it.
    let d = (secs * speed + unit(i, 3) * 2.0 * len).rem_euclid(2.0 * len);
    let (x, dir) = if d < len {
        (d, along)
    } else {
        (2.0 * len - d, -along)
    };
    // Keep to the right of the way you're going.
    let side = if d < len { 1.0 } else { -1.0 };
    let off = across * (lane.abs() * side + 1.0 * side);
    (a + along * x + off, dir)
}

fn commute(time: Res<Time>, mut people: Query<(&Commuter, &mut Transform)>) {
    let Some(f) = frame() else { return };
    let t = tuning();
    let ends = here().commute;
    let secs = time.elapsed_secs();
    for (c, mut tf) in &mut people {
        let (at, dir) = commuter_at(&t, ends, c.0, secs);
        tf.translation = f.transform_point(at);
        tf.rotation = f.rotation * Quat::from_rotation_y(crate::people::yaw_toward(dir.xz()));
    }
}

fn fires(time: Res<Time>, mut lights: Query<(&Fire, &mut PointLight)>) {
    let t = tuning();
    for (fire, mut light) in &mut lights {
        light.intensity = fire_at(t.fire_lumens, t.fire_flicker, time.elapsed_secs(), fire.0);
    }
}

fn my_side(me: &Option<Res<LocalPlayer>>, walkers: &Query<(&Player, &Walker)>) -> Option<Side> {
    let f = frame()?;
    let pos = my_pos(me, walkers)?;
    side(&here(), f.rotation.inverse() * (pos - f.translation))
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

/// Local: inside, the station's look replaces the sky's light (`place.rs`).
fn penn_look(here: Res<crate::place::Here>, mut looks: ResMut<crate::place::PlaceLooks>) {
    let Some(f) = frame() else { return };
    let l = f.rotation.inverse() * (here.0 - f.translation);
    if let Some(id) = look(&self::here(), l) {
        looks.push(id);
    }
}

/// For the music: the camera's on the NJ Transit side (`penn_njt`) or the LIRR side (`penn_lirr`).
fn music_signals(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::world::MapCamera>)>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let at = crate::music::camera_at(&camera);
    let lay = here();
    let s = frame()
        .zip(at)
        .and_then(|(f, p)| side(&lay, f.rotation.inverse() * (p - f.translation)));
    signals.flag("penn_njt", s == Some(Side::NjTransit));
    signals.flag("penn_lirr", s == Some(Side::Lirr));
}

fn spawn_stranger(commands: &mut Commands, f: &Transform, in_line: Vec3, visit: usize) {
    // Facing you (-x, out of the door), back to the door.
    let turn = f.rotation * Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
    commands.spawn((
        Stranger,
        crate::people::Gait::default(),
        Transform::from_translation(f.transform_point(in_line)).with_rotation(turn),
        Visibility::default(),
        crate::people::CharacterLook::random(0x0357_0000 + visit as u64 * 7919),
        crate::area::show(),
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
    let lay = here();
    let l = f.rotation.inverse() * (pos - f.translation);
    let d = Vec2::new(l.x - lay.door.x, l.z - lay.door.z).length();
    let near = in_hall(&lay, l) && d < t.queue_reach;
    let away = !in_hall(&lay, l) || d > t.reset_dist;
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
        spawn_stranger(&mut commands, &f, lay.in_line, line.visit);
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
    let lay = here();
    let l = f.rotation.inverse() * (pos - f.translation);
    let near =
        in_hall(&lay, l) && Vec2::new(l.x - lay.counter.x, l.z - lay.counter.z).length() < 6.0;
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
    fn commuters_walk_the_lirr_hall_and_never_through_a_wall() {
        let t = t();
        let l = lay();
        for i in 0..t.commuters {
            for k in 0..200 {
                let (p, dir) = commuter_at(&t, l.commute, i, k as f32 * 1.7);
                assert!(
                    l.lirr.contains(p + Vec3::Y),
                    "commuter {i} left the LIRR side"
                );
                assert!(dir.length() > 0.9);
                assert!(
                    blocked(&l, p + Vec3::Y).is_none(),
                    "commuter {i} walks through a wall at {p}"
                );
            }
        }
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.queue_reach > 0.0 && t.line_secs > 0.0 && t.reset_dist > t.queue_reach);
        assert_eq!(t.silence_secs, 15.0, "catherine: 15 seconds of silence");
        assert!(t.door_reach > 0.0 && t.fire_lumens > 0.0 && (0.0..=1.0).contains(&t.fire_flicker));
        assert!(t.probe > 0.0, "the probes light the inside");
        assert!(
            t.commuters > 0 && t.commuter_speed.0 > 0.0 && t.commuter_speed.1 >= t.commuter_speed.0
        );
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

    fn lay() -> Layout {
        load(&MAP);
        here()
    }

    /// Is `p` inside any of the interior's collider boxes?
    fn blocked(lay: &Layout, p: Vec3) -> Option<Wall> {
        lay.walls.iter().copied().find(|w| {
            let d = Quat::from_rotation_y(-w.yaw) * (p - w.centre);
            d.x.abs() < w.size.x / 2.0 && d.y.abs() < w.size.y / 2.0 && d.z.abs() < w.size.z / 2.0
        })
    }

    #[test]
    fn the_map_has_every_room_and_spot() {
        load(&MAP);
        let map = MAP.get();
        for name in [
            "room_hall",
            "room_njt",
            "room_lirr",
            "door",
            "in_line",
            "toilet",
            "mirror",
            "counter",
            "arrive",
            "exit",
            "fire_0",
            "commute_s",
            "commute_n",
        ] {
            assert!(
                map.entities.iter().any(|e| e.name == name),
                "maps/penn.map.json has no `{name}` spot"
            );
        }
        let l = layout(&map);
        for r in [l.hall, l.njt, l.lirr] {
            assert!(r.max.x > r.min.x && r.max.z > r.min.z && r.max.y > 2.0);
        }
        assert!(l.walls.len() > 10, "the interior has its colliders");
        assert!(l.mirror.y > 1.0, "the mirror hangs at face height");
    }

    #[test]
    fn the_1_train_stops_in_the_concourse_inside_the_city() {
        let t = t();
        let l = lay();
        load(&crate::features::trains::TUNING);
        let Some(f) = frame() else {
            panic!("no stop named {} in trains.ron", t.stop);
        };
        for r in [l.hall, l.njt, l.lirr] {
            for c in [r.min, r.max] {
                let p = f.transform_point(Vec3::new(c.x, 0.0, c.z));
                assert!(
                    crate::rules::nyc::in_city(p.x, p.z),
                    "the city's soft wall would push you home"
                );
            }
        }
        // Clear of Miami's island.
        let miami = Vec3::from(crate::features::miami::TUNING.get().ground);
        assert!((f.translation.y - miami.y).abs() > 100.0 || miami == Vec3::ZERO);
    }

    #[test]
    fn the_track_and_platform_are_clear() {
        let l = lay();
        // The 1 train's track bed and platform (trains.rs), and its tunnel mouth.
        for x in [-9.0, -5.0, 0.0, 5.0, 9.0, 10.2] {
            for z in [-2.0, 0.0, 1.0, 3.0, 4.6] {
                let p = Vec3::new(x, 1.2, z);
                assert!(
                    blocked(&l, p).is_none(),
                    "{:?} blocks the train at {p}",
                    blocked(&l, p)
                );
            }
        }
        assert!(
            l.hall.contains(Vec3::new(0.0, 1.0, 3.0)),
            "the platform is in the hall"
        );
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
        let l = lay();
        for p in [
            l.door,
            l.in_line,
            l.arrive,
            l.exit,
            l.counter,
            // In front of the stall and the sink, inside the restroom.
            l.toilet - Vec3::new(0.4, 0.0, 1.6),
            l.mirror.with_y(0.0) - Vec3::Z * 0.8,
            // Through into the NJ Transit side and the LIRR side, and across them.
            Vec3::new(
                l.hall.min.x,
                0.0,
                (l.njt.min.z.max(l.hall.min.z) + 13.0) / 2.0 + 6.0,
            ),
            Vec3::new(-7.0, 0.0, l.lirr.min.z),
            (l.njt.min + l.njt.max).with_y(0.0) / 2.0 + Vec3::new(0.0, 0.0, 9.0),
            (l.lirr.min + l.lirr.max).with_y(0.0) / 2.0,
        ] {
            let at = p + Vec3::Y;
            assert!(
                blocked(&l, at).is_none(),
                "{:?} blocks {at}",
                blocked(&l, at)
            );
        }
        assert!(
            blocked(&l, Vec3::new(0.0, -0.1, 10.0)).is_some(),
            "the hall has a floor"
        );
    }

    #[test]
    fn two_sides_and_the_hall_between() {
        let l = lay();
        let mid = |r: Room| (r.min + r.max) / 2.0;
        assert_eq!(side(&l, mid(l.njt)), Some(Side::NjTransit));
        assert_eq!(side(&l, mid(l.lirr)), Some(Side::Lirr));
        assert_eq!(side(&l, l.arrive + Vec3::Y), None);
        assert!(in_station(&l, l.arrive + Vec3::Y));
        assert!(in_hall(&l, l.door - Vec3::X * 0.3) && in_hall(&l, l.counter));
        assert!(!l.fires.is_empty());
        for at in &l.fires {
            assert_eq!(
                side(&l, *at),
                Some(Side::NjTransit),
                "fires burn on the NJ Transit side"
            );
        }
    }

    #[test]
    fn each_space_has_its_look() {
        let l = lay();
        let mid = |r: Room| (r.min + r.max) / 2.0;
        assert_eq!(look(&l, mid(l.hall)), Some(LOOK_HALL));
        assert_eq!(look(&l, mid(l.njt)), Some(LOOK_NJT));
        assert_eq!(look(&l, mid(l.lirr)), Some(LOOK_LIRR));
        assert_eq!(look(&l, Vec3::new(0.0, 1.0, -100.0)), None);
    }

    #[test]
    fn in_the_doors_and_out_the_stairs() {
        let l = lay();
        let f = Transform::from_xyz(-40.0, 1000.0, 260.0);
        let spot = Vec3::new(-42.0, 0.0, 281.0);
        let inn = through(&l, spot + DOORS, spot, &f, 1.5);
        assert_eq!(inn, Some(f.transform_point(l.arrive)));
        // Arriving isn't standing at the way out, and coming out isn't at the doors.
        assert_eq!(
            through(&l, f.transform_point(l.arrive), spot, &f, 1.5),
            None
        );
        let out = through(&l, f.transform_point(l.exit), spot, &f, 1.5);
        assert_eq!(out, Some(spot + STREET));
        assert_eq!(through(&l, spot + STREET, spot, &f, 1.5), None);
    }

    #[test]
    fn you_can_walk_up_to_the_street_doors() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/maps/nyc.map.json"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            panic!("no {path}");
        };
        let Ok(map) = serde_json::from_str::<friendslop_world::places::MapFile>(&text) else {
            panic!("{path} doesn't parse");
        };
        let Some(spot) = street_spot(&map) else {
            panic!("no penn spot");
        };
        // Your feet a body's width short of the doors must stand in the open.
        let feet = spot + DOORS + Vec3::new(0.5, 1.0, 0.0);
        for e in map.entities.iter().filter(|e| e.kind == "block") {
            let d = (feet - Vec3::from_array(e.translation)).abs();
            let Some(size) = e.size else { continue };
            let h = Vec3::from_array(size) / 2.0 + Vec3::splat(0.4);
            assert!(
                !(d.x < h.x && d.y < h.y && d.z < h.z),
                "{} is in front of the doors",
                e.name
            );
        }
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
