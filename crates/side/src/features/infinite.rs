//! The infinite hallway. From issue #206 by catherine ("infinite
//! staircase"). It's meant to be a secret, so nothing in the game mentions
//! it.
//!
//! - Every time you walk through a village doorway, or up or down a flight
//!   of stairs or a ramp, there's a small chance ([`CHANCE`]) it doesn't
//!   lead where it should. Instead you're in a dim hallway (gently going
//!   down) or a stairway (going on the way you were going) that never ends.
//! - It never pops: the server moves your body into a copy built high above
//!   the map ([`Space`]) at exactly the same spot relative to the doorway or
//!   step you were on, and as you walk on it quietly moves you back by one
//!   identical stretch, so it looks like one really, really long hallway.
//!   The far end is lost in the dark.
//! - Turn around (face back the way you came) and you're out at once, at
//!   the doorway or step you came in by, facing it. That door or staircase
//!   is normal for you from then on, and it won't happen to you again for a
//!   while ([`COOLDOWN`]).
//! - In there the music cuts out and the sky goes out (local only).
//!
//! The server decides and moves bodies; where the copies are is a pure
//! function of the village layout, so every peer builds the same ones and
//! knows from your position whether you're in one. Nothing is replicated.

use std::collections::{HashMap, HashSet};

use avian3d::prelude::*;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::AppState;
use crate::buildings::{self, Mat, Solid, WALKABLE};
use crate::features::fps::{WalkInput, WalkSet, Walker, teleport};
use crate::player::{LocalPlayer, Player};
use crate::world::MainCamera;

/// Height between copies, so none of them overlap.
pub const STACK: f32 = 160.0;
pub const HALL_SEG: f32 = 4.0;
/// A stretch is always longer than this, so the camera snaps across the
/// step back instead of easing (see `fps_camera`).
pub const MIN_SEG: f32 = 3.6;

/// Tuning numbers: `assets/data/side/infinite.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The chance a doorway or a flight of stairs goes on forever.
    pub chance: f32,
    /// Seconds after getting out before it can happen to you again.
    pub cooldown: f32,
    /// Stretches built behind the way in, and ahead of it.
    pub behind: i32,
    pub ahead: i32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/infinite.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// The lowest copy's floor, far above the map (and its sky islands).
pub const SPACE_Y: f32 = 400.0;
/// The hallway: inside width, floor to ceiling, one stretch, and its slope
/// (it keeps going down).
const HALL_W: f32 = 1.9;
const HALL_H: f32 = 2.7;
pub const HALL_PITCH: f32 = -0.05;
/// Stairways: inside width and headroom over the steps.
const STAIR_W: f32 = 2.2;
const STAIR_H: f32 = 2.8;
/// Wall thickness.
const T: f32 = 0.25;

/// The four ways a doorway or staircase can face (the village is built in
/// quarter turns).
const CARDINALS: [Vec3; 4] = [Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z];

fn cardinal(v: Vec3) -> usize {
    let mut best = 0;
    for (i, c) in CARDINALS.iter().enumerate() {
        if c.dot(v) > CARDINALS[best].dot(v) {
            best = i;
        }
    }
    best
}

/// What the floor of a copy is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Floor {
    /// The hallway: a flat (gently sloping) floor.
    Hall,
    /// A plain ramp.
    Slope,
    /// Steps, each this long.
    Steps { run: f32 },
}

/// One endless copy: a straight hallway or stairway running along `dir`
/// from `origin` (a point on its floor's walking line), rising `pitch`
/// metres per metre, the same every `seg` metres.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Space {
    pub dir: Vec3,
    pub pitch: f32,
    pub floor: Floor,
    pub seg: f32,
    pub width: f32,
    pub height: f32,
    pub origin: Vec3,
}

impl Space {
    fn side(&self) -> Vec3 {
        self.dir.cross(Vec3::Y)
    }

    /// Along the copy from its way in.
    pub fn along(&self, p: Vec3) -> f32 {
        (p - self.origin).dot(self.dir)
    }

    /// The walking line `s` metres along.
    pub fn line(&self, s: f32) -> Vec3 {
        self.origin + self.dir * s + Vec3::Y * self.pitch * s
    }

    /// One stretch further on.
    pub fn period(&self) -> Vec3 {
        self.line(self.seg) - self.origin
    }

    /// Is a body with its feet at `p` in this copy?
    pub fn contains(&self, p: Vec3) -> bool {
        let s = self.along(p);
        let lateral = (p - self.origin).dot(self.side()).abs();
        let range =
            -(tuning().behind + 1) as f32 * self.seg..(tuning().ahead + 1) as f32 * self.seg;
        range.contains(&s) && lateral < self.width + 2.0 && (p.y - self.line(s).y).abs() < 6.0
    }

    /// Two stretches in, step the body back by one: everything around it
    /// is the same, so nothing seems to change.
    pub fn wrap(&self, p: Vec3) -> Option<Vec3> {
        (self.along(p) > 2.0 * self.seg).then(|| p - self.period())
    }

    /// The rotation that lays a box's x along the copy (tilted with it).
    fn tilt(&self) -> Quat {
        Quat::from_rotation_y((-self.dir.z).atan2(self.dir.x))
            * Quat::from_rotation_z(self.pitch.atan())
    }
}

/// Facing back the way you were going (more than a quarter turn round)?
pub fn turned_back(yaw: f32, travel: Vec3) -> bool {
    facing(yaw).dot(travel) < 0.0
}

fn facing(yaw: f32) -> Vec3 {
    Quat::from_rotation_y(yaw) * Vec3::NEG_Z
}

/// What a way in is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Door {
        through: Vec3,
        width: f32,
    },
    Ramp {
        to: Vec3,
        width: f32,
        steps: Option<f32>,
    },
}

/// A doorway (`at` = floor centre) or a flight (`at` = the bottom of its
/// walking line).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Spot {
    pub at: Vec3,
    pub kind: Kind,
}

/// Going through a spot: which way, how steep, the matching point on it,
/// and the copy that goes on from there.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Entry {
    pub travel: Vec3,
    pub at: Vec3,
    pub space: usize,
}

/// Every doorway and flight in the village, and every copy they need.
#[derive(Resource, Clone, Debug)]
pub struct Plan {
    pub spots: Vec<Spot>,
    pub spaces: Vec<Space>,
    /// Per spot: the copy going each way (forwards = through / up, back).
    ways: Vec<[usize; 2]>,
}

impl Plan {
    /// The plan for the default village (`layout(0)`), until a seed arrives.
    pub fn new() -> Self {
        Self::seeded(0)
    }

    /// The plan for the village built from `village` (see
    /// `buildings::village_seed`).
    pub fn seeded(village: u32) -> Self {
        let kit = buildings::layout(village);
        let mut spots = Vec::new();
        for d in &kit.doors {
            spots.push(Spot {
                at: d.centre,
                kind: Kind::Door {
                    through: d.through.with_y(0.0).normalize_or(Vec3::Z),
                    width: d.width,
                },
            });
        }
        for r in &kit.ramps {
            let mid = (r.from + r.to) / 2.0;
            // `stairs` lays an invisible ramp under its steps: a hidden floor.
            let stairs = kit.pieces.iter().any(|p| {
                p.mat == Mat::Hidden && p.solid == Solid::Floor && p.centre.distance(mid) < 0.4
            });
            let flat = (r.to - r.from).with_y(0.0).length();
            if flat < 0.5 || (r.to.y - r.from.y).abs() < 0.3 {
                continue;
            }
            let steps = stairs.then(|| {
                let n = ((r.to.y - r.from.y).abs() / 0.25).ceil().max(1.0);
                flat / n
            });
            spots.push(Spot {
                at: r.from,
                kind: Kind::Ramp {
                    to: r.to,
                    width: r.width,
                    steps,
                },
            });
        }
        Self::from_spots(spots)
    }

    pub fn from_spots(spots: Vec<Spot>) -> Self {
        let mut spaces: Vec<Space> = Vec::new();
        let mut ways = Vec::new();
        let mut find = |dir: Vec3, pitch: f32, floor: Floor| -> usize {
            let dir = CARDINALS[cardinal(dir)];
            if let Some(i) = spaces.iter().position(|s| {
                s.dir == dir && (s.pitch - pitch).abs() < 0.01 && same_floor(s.floor, floor)
            }) {
                return i;
            }
            let (seg, width, height) = match floor {
                Floor::Hall => (HALL_SEG, HALL_W, HALL_H),
                Floor::Slope => (HALL_SEG, STAIR_W, STAIR_H),
                Floor::Steps { run } => ((MIN_SEG / run).ceil() * run, STAIR_W, STAIR_H),
            };
            // Stacked up out of everyone's way, each centred over the map.
            let k = spaces.len() as f32;
            let middle = (tuning().ahead - tuning().behind) as f32 * seg / 2.0;
            let origin = Vec3::new(0.0, SPACE_Y + STACK * k, 0.0) - dir * middle;
            spaces.push(Space {
                dir,
                pitch,
                floor,
                seg,
                width,
                height,
                origin,
            });
            spaces.len() - 1
        };
        for spot in &spots {
            ways.push(match spot.kind {
                Kind::Door { through, .. } => [
                    find(through, HALL_PITCH, Floor::Hall),
                    find(-through, HALL_PITCH, Floor::Hall),
                ],
                Kind::Ramp { to, steps, .. } => {
                    let d = to - spot.at;
                    let flat = d.with_y(0.0);
                    let pitch = d.y / flat.length();
                    let floor = steps.map_or(Floor::Slope, |run| Floor::Steps { run });
                    [find(flat, pitch, floor), find(-flat, -pitch, floor)]
                }
            });
        }
        Self {
            spots,
            spaces,
            ways,
        }
    }

    /// Did a body that moved from `a` to `b` just go through spot `i`?
    pub fn crossed(&self, i: usize, a: Vec3, b: Vec3) -> Option<Entry> {
        let spot = self.spots.get(i)?;
        match spot.kind {
            Kind::Door { through, width } => {
                let (wa, wb) = ((a - spot.at).dot(through), (b - spot.at).dot(through));
                let lateral = (b - spot.at).dot(through.cross(Vec3::Y)).abs();
                let height = b.y - spot.at.y;
                if (wa < 0.0) == (wb < 0.0)
                    || lateral > width / 2.0
                    || !(-0.5..1.0).contains(&height)
                {
                    return None;
                }
                let forwards = wb > wa;
                Some(Entry {
                    travel: if forwards { through } else { -through },
                    at: spot.at,
                    space: self.ways[i][usize::from(!forwards)],
                })
            }
            Kind::Ramp { to, width, .. } => {
                let flat = (to - spot.at).with_y(0.0);
                let len = flat.length();
                let dir = flat / len;
                let t = |p: Vec3| (p - spot.at).dot(dir) / len;
                let (ta, tb) = (t(a), t(b));
                let line = spot.at.lerp(to, tb);
                let lateral = (b - spot.at).dot(dir.cross(Vec3::Y)).abs();
                if (ta < 0.5) == (tb < 0.5)
                    || lateral > width / 2.0 + 0.2
                    || (b.y - line.y).abs() > 0.8
                {
                    return None;
                }
                let up = tb > ta;
                Some(Entry {
                    travel: if up { dir } else { -dir },
                    at: line,
                    space: self.ways[i][usize::from(!up)],
                })
            }
        }
    }

    /// The copy a body at `p` is in, if any.
    pub fn space_of(&self, p: Vec3) -> Option<usize> {
        if p.y < SPACE_Y - 60.0 {
            return None;
        }
        self.spaces.iter().position(|s| s.contains(p))
    }
}

impl Default for Plan {
    fn default() -> Self {
        Self::new()
    }
}

fn same_floor(a: Floor, b: Floor) -> bool {
    match (a, b) {
        (Floor::Steps { run: x }, Floor::Steps { run: y }) => (x - y).abs() < 0.01,
        _ => a == b,
    }
}

/// Where a body going through a spot lands in its copy: the same place
/// relative to the walking line, so nothing around it seems to move.
/// (A wide flight lands you no further out than the copy's walls allow.)
pub fn landing(space: &Space, entry: &Entry, feet: Vec3) -> Vec3 {
    let off = feet - entry.at;
    let side = space.side();
    let room = (space.width / 2.0 - 0.4).max(0.0);
    let lateral = off.dot(side);
    space.origin + off + side * (lateral.clamp(-room, room) - lateral)
}

// ---------------------------------------------------------------- server

/// Server only: someone lost in a copy, and where they came in.
#[derive(Component, Debug)]
struct Lost {
    space: usize,
    back: Vec3,
}

/// Server only: who was where last tick, who it's spared, and the dice.
#[derive(Resource, Default)]
struct Haunt {
    last: HashMap<u64, Vec3>,
    /// Seconds until it can happen to each player again.
    cooldown: HashMap<u64, f32>,
    /// (player, spot): that way is just a normal one for them now.
    spared: HashSet<(u64, usize)>,
    dice: u32,
}

impl Haunt {
    fn roll(&mut self) -> f32 {
        if self.dice == 0 {
            self.dice = crate::settings::since_epoch().subsec_nanos() | 1;
        }
        self.dice = self
            .dice
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.dice >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub fn protocol(app: &mut App) {
    app.init_resource::<Plan>()
        .init_resource::<Haunt>()
        .add_systems(
            PreUpdate,
            replan.run_if(resource_exists_and_changed::<crate::features::world_seed::WorldSeed>),
        )
        .add_systems(
            Update,
            haunt.after(WalkSet).run_if(in_state(ServerState::Running)),
        );
}

/// Rebuild the plan when the village is regenerated from a new seed.
fn replan(seed: Res<crate::features::world_seed::WorldSeed>, mut plan: ResMut<Plan>) {
    *plan = Plan::seeded(buildings::village_seed(*seed));
}

type Bodies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Player,
        &'static mut Walker,
        Option<&'static mut WalkInput>,
        Option<&'static mut crate::kcc::Kcc>,
        Option<&'static Lost>,
    ),
>;

fn haunt(
    mut commands: Commands,
    time: Res<Time>,
    plan: Res<Plan>,
    mut haunt: ResMut<Haunt>,
    mut bodies: Bodies,
) {
    let dt = time.delta_secs();
    for t in haunt.cooldown.values_mut() {
        *t -= dt;
    }
    haunt.cooldown.retain(|_, t| *t > 0.0);
    for (entity, player, mut walker, input, kcc, lost) in &mut bodies {
        let id = player.id;
        let pos = walker.pos;
        if let Some(lost) = lost {
            let Some(space) = plan.spaces.get(lost.space) else {
                continue;
            };
            if !space.contains(pos) {
                // Something else moved them (a respawn, fate): let go.
                commands.entity(entity).remove::<Lost>();
            } else if turned_back(walker.yaw, space.dir) {
                // Out, ASAP: back where they came in, facing the way out.
                put(&mut walker, input, kcc, lost.back);
                commands.entity(entity).remove::<Lost>();
                haunt.cooldown.insert(id, tuning().cooldown);
            } else if let Some(to) = space.wrap(pos) {
                // Keep the momentum: it's the same hallway.
                let yaw = walker.yaw;
                teleport(&mut walker, None, None, to, yaw);
            }
            haunt.last.insert(id, walker.pos);
            continue;
        }
        let last = haunt.last.insert(id, pos);
        let Some(last) = last.filter(|l| l.distance(pos) < 2.0) else {
            continue; // first sight, or a teleport: not walking through anything
        };
        if haunt.cooldown.contains_key(&id) {
            continue;
        }
        for i in 0..plan.spots.len() {
            let Some(entry) = plan.crossed(i, last, pos) else {
                continue;
            };
            // Only when you're looking where you're going, and only once
            // per way in: try again and it's just a normal one.
            if facing(walker.yaw).dot(entry.travel) < 0.5 || haunt.spared.contains(&(id, i)) {
                break;
            }
            if haunt.roll() >= tuning().chance {
                break;
            }
            let Some(space) = plan.spaces.get(entry.space) else {
                break;
            };
            haunt.spared.insert((id, i));
            let to = landing(space, &entry, pos);
            let yaw = walker.yaw;
            teleport(&mut walker, None, None, to, yaw);
            haunt.last.insert(id, to);
            commands.entity(entity).insert(Lost {
                space: entry.space,
                back: pos,
            });
            break;
        }
    }
}

/// Move a body somewhere else entirely and stop it.
fn put(
    walker: &mut Walker,
    mut input: Option<Mut<WalkInput>>,
    mut kcc: Option<Mut<crate::kcc::Kcc>>,
    to: Vec3,
) {
    let yaw = walker.yaw;
    teleport(walker, input.as_deref_mut(), kcc.as_deref_mut(), to, yaw);
}

// ---------------------------------------------------------------- local

/// Local: which copy I'm in, if any.
#[derive(Resource, Default)]
pub struct Adrift {
    pub inside: Option<usize>,
}

/// For the music: cut it while I'm in there.
pub fn adrift(adrift: &Option<Res<Adrift>>) -> bool {
    adrift.as_ref().is_some_and(|a| a.inside.is_some())
}

/// A copy's visuals (lights included), shown only while I'm in it.
#[derive(Component)]
struct Shown(usize);

pub fn plugin(app: &mut App) {
    app.add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
    app.init_resource::<Adrift>()
        .add_systems(OnEnter(AppState::InGame), (build, reset))
        .add_systems(OnExit(AppState::InGame), reset)
        .add_systems(
            Update,
            darkness
                .in_set(crate::place::PlaceSet)
                .run_if(in_state(AppState::InGame)),
        );
}

fn reset(mut adrift: ResMut<Adrift>) {
    *adrift = Adrift::default();
}

/// One box of a copy: where, how big, how turned, what it looks like and
/// whether it blocks or holds you up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub centre: Vec3,
    pub size: Vec3,
    pub rot: Quat,
    pub look: Look,
    pub solid: Solid,
}

/// What a box looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Hidden,
    Wall,
    Floor,
    Ceiling,
    Timber,
    Lamp,
}

/// Every box of a copy (pure, the same on every peer), and its lamps.
pub fn blocks(space: &Space) -> (Vec<Block>, Vec<Vec3>) {
    let mut out = Vec::new();
    let mut lamps = Vec::new();
    let rot = space.tilt();
    let up = rot * Vec3::Y;
    let side = space.side();
    let stretch = (1.0 + space.pitch * space.pitch).sqrt();
    let (s0, s1) = (
        -tuning().behind as f32 * space.seg,
        tuning().ahead as f32 * space.seg,
    );
    let mid = space.line((s0 + s1) / 2.0);
    let len = (s1 - s0) * stretch;
    let (w, h) = (space.width, space.height);
    let mut put = |centre: Vec3, size: Vec3, rot: Quat, look: Look, solid: Solid| {
        out.push(Block {
            centre,
            size,
            rot,
            look,
            solid,
        });
    };
    // The floor you walk on: one long slab under the walking line.
    let floor_look = match space.floor {
        Floor::Steps { .. } => Look::Hidden,
        _ => Look::Floor,
    };
    let slab = Vec3::new(len, 0.2, w + 2.0 * T);
    put(mid - up * 0.1, slab, rot, floor_look, Solid::Floor);
    // Ceiling and walls, tilted with it; the walls reach well below the
    // floor so steps never show a gap.
    put(mid + up * (h + 0.1), slab, rot, Look::Ceiling, Solid::Wall);
    for k in [-1.0f32, 1.0] {
        let c = mid + side * k * (w + T) / 2.0 + up * (h / 2.0 - 0.5);
        put(c, Vec3::new(len, h + 1.0, T), rot, Look::Wall, Solid::Wall);
    }
    // Dead ends, far out of sight in the dark.
    for (s, deep) in [(s0, -1.0f32), (s1, 1.0)] {
        let c = space.line(s) + space.dir * deep * T / 2.0 + Vec3::Y * (h / 2.0 - 0.5);
        let size = Vec3::new(T, h + 1.0, w + 2.0 * T);
        put(c, size, rot, Look::Wall, Solid::Wall);
    }
    // Every stretch: a timber frame, a lamp, and its steps.
    for k in -tuning().behind..tuning().ahead {
        let s = k as f32 * space.seg;
        let at = space.line(s);
        for side_k in [-1.0f32, 1.0] {
            let c = at + side * side_k * (w / 2.0 - 0.05) + up * (h / 2.0);
            put(c, Vec3::new(0.2, h, 0.14), rot, Look::Timber, Solid::None);
        }
        put(
            at + up * (h - 0.08),
            Vec3::new(0.2, 0.16, w),
            rot,
            Look::Timber,
            Solid::None,
        );
        let lamp = space.line(s + space.seg / 2.0) + up * (h - 0.04);
        put(
            lamp,
            Vec3::new(0.5, 0.06, 0.24),
            rot,
            Look::Lamp,
            Solid::None,
        );
        lamps.push(lamp - Vec3::Y * 0.35);
        if let Floor::Steps { run } = space.floor {
            let n = (space.seg / run).round() as i32;
            let flat = Quat::from_rotation_y((-space.dir.z).atan2(space.dir.x));
            for j in 0..n {
                let (a, b) = (s + j as f32 * run, s + (j + 1) as f32 * run);
                // Each step's top meets the walking line at its high end.
                let top = space.line(a).y.max(space.line(b).y);
                let foot = space.line((a + b) / 2.0).with_y(top - 0.4);
                let size = Vec3::new(run + 0.01, 0.8, w);
                put(foot, size, flat, Look::Floor, Solid::None);
            }
        }
    }
    // The way in: a door frame, for doorways.
    if space.floor == Floor::Hall {
        let at = space.line(0.0);
        let (dw, dh) = (1.4, 2.3);
        let jamb = (w - dw) / 2.0;
        for k in [-1.0f32, 1.0] {
            let c = at + side * k * (dw + jamb) / 2.0 + Vec3::Y * h / 2.0;
            put(c, Vec3::new(T, h, jamb), rot, Look::Wall, Solid::Wall);
        }
        let c = at + Vec3::Y * (dh + h) / 2.0;
        put(c, Vec3::new(T, h - dh, dw), rot, Look::Wall, Solid::Wall);
    }
    (out, lamps)
}

fn build(
    mut commands: Commands,
    plan: Res<Plan>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mut mat = |c: Color, glow: f32| {
        materials.add(StandardMaterial {
            base_color: c,
            emissive: LinearRgba::from(c) * glow,
            perceptual_roughness: 0.9,
            ..default()
        })
    };
    let wall = mat(Color::srgb(0.82, 0.76, 0.62), 0.0);
    let floor = mat(Color::srgb(0.45, 0.32, 0.21), 0.0);
    let ceiling = mat(Color::srgb(0.7, 0.66, 0.56), 0.0);
    let timber = mat(Color::srgb(0.3, 0.2, 0.13), 0.0);
    let lamp = mat(Color::srgb(1.0, 0.86, 0.6), 6.0);
    for (i, space) in plan.spaces.iter().enumerate() {
        let root = commands
            .spawn((
                Shown(i),
                Transform::default(),
                Visibility::Hidden,
                DespawnOnExit(AppState::InGame),
            ))
            .id();
        let (pieces, lamps) = blocks(space);
        for b in pieces {
            let tf = Transform::from_translation(b.centre)
                .with_rotation(b.rot)
                .with_scale(b.size);
            let mut e = commands.spawn((tf, ChildOf(root)));
            let look = match b.look {
                Look::Hidden => None,
                Look::Wall => Some(&wall),
                Look::Floor => Some(&floor),
                Look::Ceiling => Some(&ceiling),
                Look::Timber => Some(&timber),
                Look::Lamp => Some(&lamp),
            };
            if let Some(m) = look {
                e.insert((Mesh3d(cube.clone()), MeshMaterial3d(m.clone())));
                e.insert(NotShadowCaster);
            }
            match b.solid {
                Solid::Wall => {
                    e.insert((
                        RigidBody::Static,
                        Collider::cuboid(1.0, 1.0, 1.0),
                        avian_rerecast::ExcludeColliderFromNavmesh,
                    ));
                }
                Solid::Floor => {
                    e.insert((
                        RigidBody::Static,
                        Collider::cuboid(1.0, 1.0, 1.0),
                        CollisionLayers::new(WALKABLE, LayerMask::ALL),
                        avian_rerecast::ExcludeColliderFromNavmesh,
                    ));
                }
                Solid::None => {}
            }
        }
        for at in lamps {
            commands.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.82, 0.55),
                    intensity: 22_000.0,
                    range: 7.0,
                    ..default()
                },
                Transform::from_translation(at),
                ChildOf(root),
            ));
        }
    }
}

/// The hallway's look (`data/base/place_looks.ron`): no sky, a brown dark.
pub const LOOK: &str = "infinite";

/// Local: in a copy, the sky goes out and the dark closes in (`place.rs`),
/// and only that copy shows.
fn darkness(
    plan: Res<Plan>,
    here: Res<crate::place::Here>,
    mut adrift: ResMut<Adrift>,
    mut looks: ResMut<crate::place::PlaceLooks>,
    mut shown: Query<(&Shown, &mut Visibility)>,
) {
    let space = plan.space_of(here.0);
    if space.is_some() {
        looks.push(LOOK);
    }
    if space == adrift.inside {
        return;
    }
    adrift.inside = space;
    for (s, mut v) in &mut shown {
        let want = if Some(s.0) == space {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

/// For the music: I'm in the endless hallway (it cuts the band dead).
fn music_signals(
    adrift_now: Option<Res<Adrift>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    signals.flag("adrift", adrift(&adrift_now));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn door_plan() -> Plan {
        Plan::from_spots(vec![
            Spot {
                at: Vec3::new(10.0, 0.0, 5.0),
                kind: Kind::Door {
                    through: Vec3::Z,
                    width: 1.4,
                },
            },
            Spot {
                at: Vec3::new(0.0, 0.0, 0.0),
                kind: Kind::Ramp {
                    to: Vec3::new(4.0, 3.0, 0.0),
                    width: 1.1,
                    steps: Some(4.0 / 12.0),
                },
            },
        ])
    }

    #[test]
    fn the_village_has_ways_in() {
        let plan = Plan::new();
        let doors = plan
            .spots
            .iter()
            .filter(|s| matches!(s.kind, Kind::Door { .. }))
            .count();
        let stairs = plan
            .spots
            .iter()
            .filter(|s| matches!(s.kind, Kind::Ramp { steps: Some(_), .. }))
            .count();
        assert!(doors >= 20, "{doors} doors");
        assert!(stairs >= 2, "{stairs} staircases");
        // Copies are shared, not one per doorway.
        assert!(plan.spaces.len() < plan.spots.len());
    }

    #[test]
    fn copies_never_overlap_and_stay_on_the_map() {
        let plan = Plan::new();
        for (i, a) in plan.spaces.iter().enumerate() {
            for s in [
                -(tuning().behind as f32) * a.seg,
                tuning().ahead as f32 * a.seg,
            ] {
                let p = a.line(s);
                assert!(p.x.abs() < 85.0 && p.z.abs() < 85.0, "{i} {p}");
                assert!(p.y > SPACE_Y - 60.0, "{i} {p}");
                for (j, b) in plan.spaces.iter().enumerate() {
                    if i != j {
                        assert!(!b.contains(p), "{i} reaches into {j}");
                    }
                }
            }
        }
    }

    #[test]
    fn walking_through_a_door_either_way() {
        let plan = door_plan();
        let a = Vec3::new(10.2, 0.0, 4.9);
        let b = Vec3::new(10.2, 0.0, 5.1);
        let e = plan.crossed(0, a, b).expect("went through");
        assert_eq!(e.travel, Vec3::Z);
        let back = plan.crossed(0, b, a).expect("came back through");
        assert_eq!(back.travel, Vec3::NEG_Z);
        assert_ne!(e.space, back.space);
        // Beside the door (through the wall) or not crossing: nothing.
        assert!(
            plan.crossed(0, a + Vec3::X * 2.0, b + Vec3::X * 2.0)
                .is_none()
        );
        assert!(plan.crossed(0, a, a + Vec3::X * 0.1).is_none());
    }

    #[test]
    fn landing_keeps_you_where_you_were() {
        let plan = door_plan();
        let b = Vec3::new(10.3, 0.0, 5.15);
        let e = plan
            .crossed(0, Vec3::new(10.3, 0.0, 4.9), b)
            .expect("went in");
        let space = plan.spaces[e.space];
        let to = landing(&space, &e, b);
        assert!((to - space.origin - Vec3::new(0.3, 0.0, 0.15)).length() < 1e-4);
        assert!(space.contains(to));
        assert_eq!(plan.space_of(to), Some(e.space));
        // Far out at the side of a wide flight: still between the walls.
        let wide = landing(&space, &e, b + Vec3::X * 3.0);
        assert!((wide - space.origin).dot(space.side()).abs() < space.width / 2.0);
        assert_eq!(plan.space_of(b), None);
    }

    #[test]
    fn stairs_go_on_the_way_you_were_going() {
        let plan = door_plan();
        let line = |t: f32| Vec3::new(4.0 * t, 3.0 * t, 0.0);
        let up = plan.crossed(1, line(0.48), line(0.52)).expect("climbing");
        assert_eq!(up.travel, Vec3::X);
        assert!(plan.spaces[up.space].pitch > 0.7);
        let down = plan.crossed(1, line(0.52), line(0.48)).expect("going down");
        assert_eq!(down.travel, Vec3::NEG_X);
        assert!(plan.spaces[down.space].pitch < -0.7);
        // Jumping off the side doesn't count.
        assert!(
            plan.crossed(1, line(0.48) + Vec3::Z * 2.0, line(0.52) + Vec3::Z * 2.0)
                .is_none()
        );
    }

    #[test]
    fn the_hallway_goes_on_forever_and_down() {
        let plan = door_plan();
        let space = plan.spaces[plan.ways[0][0]];
        assert!(space.pitch < 0.0, "it keeps going down");
        assert!(space.period().length() > 3.0, "the camera snaps across");
        let mut p = space.origin + Vec3::Z * 0.2;
        let start = p;
        for _ in 0..1000 {
            p += space.dir * 0.1 + Vec3::Y * space.pitch * 0.1;
            if let Some(to) = space.wrap(p) {
                p = to;
            }
            assert!(space.contains(p));
        }
        assert!(p.distance(start) <= 2.0 * space.period().length() + 0.2);
    }

    #[test]
    fn copies_repeat_exactly() {
        // Shift every box by one stretch: away from the ends, it lands on
        // another box just like it, so a step back changes nothing you see.
        let plan = door_plan();
        for space in &plan.spaces {
            let (pieces, lamps) = blocks(space);
            let near =
                |p: Vec3| space.along(p) > 0.5 * space.seg && space.along(p) < 8.0 * space.seg;
            for b in pieces.iter().filter(|b| near(b.centre)) {
                if b.size.x > space.seg * 2.0 {
                    continue; // the long floor, walls and ceiling
                }
                let moved = b.centre + space.period();
                assert!(
                    pieces.iter().any(|o| o.look == b.look
                        && o.size == b.size
                        && o.centre.distance(moved) < 1e-3),
                    "{b:?} has no twin"
                );
            }
            for l in lamps.iter().filter(|l| near(**l)) {
                let moved = *l + space.period();
                assert!(lamps.iter().any(|o| o.distance(moved) < 1e-3));
            }
        }
    }

    #[test]
    fn turning_round_is_the_way_out() {
        // Yaw 0 faces -z.
        assert!(!turned_back(0.0, Vec3::NEG_Z));
        assert!(!turned_back(1.4, Vec3::NEG_Z));
        assert!(turned_back(std::f32::consts::PI, Vec3::NEG_Z));
        assert!(turned_back(1.7, Vec3::NEG_Z));
    }
}
