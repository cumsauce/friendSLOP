//! Underground passages. From issue #127 by doomy: "sewers, basements,
//! secret passageways, connected! and music to go with it".
//!
//! - Under the map, [`TUNNEL_Y`] down, runs one network of tunnels on a
//!   3 m grid ([`layout`]): brick sewers with a channel of murky water, a
//!   junction chamber under the commune with daylight falling through a
//!   grate, a damp cellar under a village cottage, and a narrow, torch-lit
//!   secret passage. Everything down there joins up.
//! - The network is generated from the [`WorldSeed`] (structures rework,
//!   phase 3): the junction, the foot of every ladder, the cellar and Sewer
//!   Paul's spot are pinned; the chambers, corridors, loops and dead ends
//!   between them change with the seed. Every peer builds [`Tunnels`] from
//!   the same seed, so the server's rats walk the same tunnels players see.
//! - Four ways in ([`entrances`]): a manhole on the road east out of the
//!   commune, a manhole on the road to the ruins, the cellar door beside the
//!   big cottage in the east village, and a loose floorboard in the
//!   woodcutter's hut in the forest clearing (no sign: it's a secret).
//! - Click a way in to climb down its ladder; click the ladder (under any
//!   of them) to climb up there. So you can go down in one place and come up
//!   somewhere else entirely.
//! - The band plays its own tune down there (the `Underground` mood in the
//!   fusion crate: dub-wet minor, a bottle-flute lead, kalimba drips and a
//!   lot of echo), and the sky goes out (local only).
//!
//! The server moves bodies; the rest is local and the same on every peer.

use std::collections::{BTreeMap, VecDeque};

use friendslop_structures::graph::{self, Room, Spec, Use};

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::features::fps::{WalkInput, Walker, teleport};
use crate::features::interact::{SERVER_REACH, deny, within};
use crate::player::PlayerOwner;
pub use friendslop_core::WorldSeed;

pub(crate) mod visuals;

pub use crate::rules::depths::*;
pub use visuals::Below;

/// Floor to ceiling.
pub const TUNNEL_H: f32 = 3.2;
/// Grid cell size: one tunnel is one cell wide.
pub const CELL: f32 = 3.0;

/// What a stretch of underground is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Part {
    /// The junction chamber under the commune.
    Hub,
    /// Brick sewers with water down the middle.
    Sewer,
    /// The narrow, old, torch-lit secret passage.
    Passage,
    /// A cottage's cellar: shelves of jars.
    Cellar,
}

/// What a way in looks like from above.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Manhole,
    CellarDoor,
    /// A loose floorboard. Nothing marks it.
    Trapdoor,
}

/// A way in: where it is on the map, which way you step off it when you
/// come up, and what it's called.
#[derive(Clone, Copy, Debug)]
pub struct Entrance {
    pub at: Vec2,
    pub out: Vec2,
    pub kind: Kind,
    pub name: &'static str,
}

/// Every way in, from the map's `Entrance` markers (`entrance_1`..). All of
/// them join up underground.
pub fn entrances() -> Vec<Entrance> {
    friendslop_world::places::layout()
        .entrances
        .iter()
        .map(|e| {
            let (kind, name) = match e.kind.as_str() {
                "cellar_door" => (Kind::CellarDoor, "a damp cellar"),
                "trapdoor" => (Kind::Trapdoor, "a secret passage"),
                _ => (Kind::Manhole, "the sewers"),
            };
            Entrance {
                at: e.at,
                out: e.out,
                kind,
                name,
            }
        })
        .collect()
}

/// Way in `i` (wrapping round; a manhole at the centre if the map has none).
pub fn entrance(i: usize) -> Entrance {
    let all = entrances();
    all.get(i % all.len().max(1)).copied().unwrap_or(Entrance {
        at: Vec2::ZERO,
        out: Vec2::X,
        kind: Kind::Manhole,
        name: "the sewers",
    })
}

/// Does a way in (plus `margin`) cover this map spot? Scenery keeps out.
pub fn covers(p: Vec2, margin: f32) -> bool {
    entrances()
        .iter()
        .any(|e| p.distance(e.at + e.out * 0.8) < 2.2 + margin)
}

// W1 claims prep: claim the same ground as [`covers`] (world::footprints),
// so scenery/zones can switch to `claimed` and this goes away (follow-up).
fn claim_ground() {
    for e in entrances() {
        friendslop_world::footprints::claim(e.at + e.out * 0.8, 2.2);
    }
}

/// Where you arrive at the bottom of entrance `i`'s ladder.
pub fn down_spot(i: usize) -> Vec3 {
    let e = entrance(i);
    let p = e.at + e.out * 1.0;
    Vec3::new(p.x, TUNNEL_Y, p.y)
}

/// Where you step off at the top of entrance `i`.
pub fn up_spot(i: usize) -> Vec3 {
    let e = entrance(i);
    let p = e.at + e.out * 1.4;
    Vec3::new(p.x, crate::terrain::height_at(p.x, p.y), p.y)
}

/// A grid cell.
pub type Cell = (i32, i32);

pub fn cell_of(p: Vec2) -> Cell {
    ((p.x / CELL).round() as i32, (p.y / CELL).round() as i32)
}

pub fn cell_centre(c: Cell) -> Vec2 {
    Vec2::new(c.0 as f32 * CELL, c.1 as f32 * CELL)
}

/// The whole network: which cells are open, and what they are.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub cells: BTreeMap<Cell, Part>,
}

impl Layout {
    pub fn part(&self, c: Cell) -> Option<Part> {
        self.cells.get(&c).copied()
    }

    pub fn open(&self, c: Cell) -> bool {
        self.cells.contains_key(&c)
    }

    /// Can you walk from `a` to `b` through open cells?
    pub fn connected(&self, a: Cell, b: Cell) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        let mut todo = VecDeque::from([a]);
        while let Some(c) = todo.pop_front() {
            if c == b {
                return true;
            }
            if !self.open(c) || !seen.insert(c) {
                continue;
            }
            for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                todo.push_back((c.0 + d.0, c.1 + d.1));
            }
        }
        false
    }
}

/// The nearest way up from `from`, walking the tunnels: which ladder, how
/// many metres away, and the direction of the first step towards it (zero
/// once you're at it). `None` if you're not in a tunnel.
pub fn way_up(l: &Layout, from: Vec2) -> Option<(usize, f32, Vec2)> {
    let start = cell_of(from);
    if !l.open(start) {
        return None;
    }
    let ladders: Vec<Cell> = entrances().iter().map(|e| cell_of(e.at)).collect();
    // Breadth-first from where you stand, remembering each cell's first step.
    let mut first: BTreeMap<Cell, (Cell, u32)> = BTreeMap::new();
    first.insert(start, (start, 0));
    let mut todo = VecDeque::from([start]);
    while let Some(c) = todo.pop_front() {
        let (step, d) = first[&c];
        if let Some(i) = ladders.iter().position(|&lc| lc == c) {
            let dir = if c == start {
                Vec2::ZERO
            } else {
                (cell_centre(step) - cell_centre(start)).normalize_or_zero()
            };
            let at = entrance(i).at;
            let metres = d as f32 * CELL + if d == 0 { from.distance(at) } else { 0.0 };
            return Some((i, metres, dir));
        }
        for dd in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (c.0 + dd.0, c.1 + dd.1);
            if l.open(n) && !first.contains_key(&n) {
                first.insert(n, (if c == start { n } else { step }, d + 1));
                todo.push_back(n);
            }
        }
    }
    None
}

/// Which way `dir` is for someone facing `forward` (both on the ground).
pub fn turn_word(forward: Vec2, dir: Vec2) -> &'static str {
    if dir == Vec2::ZERO {
        return "right here";
    }
    let f = forward.normalize_or_zero();
    let ahead = f.dot(dir);
    // Ground is x/z with -z forward at yaw 0: right of `f` is (-f.y, f.x).
    let right = Vec2::new(-f.y, f.x).dot(dir);
    if ahead > 0.7 {
        "straight ahead"
    } else if ahead < -0.7 {
        "behind you"
    } else if right > 0.0 {
        "to your right"
    } else {
        "to your left"
    }
}

/// The salt of the tunnels' sub-seed ([`WorldSeed::for_`]).
pub const SEED_SALT: u32 = 0x5E3E_0127;

/// Where Sewer Paul's chamber is: halfway along the way to the first manhole.
pub fn paul_cell() -> Cell {
    cell_of(entrance(0).at / 2.0)
}

/// The rooms that stay put whatever the seed: the junction, the foot of
/// every ladder (the cellar is a whole room), and Sewer Paul's chamber. The
/// secret passage's end is a leaf: one way in, one way out.
fn pins() -> Vec<Room> {
    let foot = |i: usize| (cell_of(entrance(i).at), cell_of(down_spot(i).xz()));
    let tag = |p: Part| p as u8;
    let (a, b) = foot(0);
    let e0 = Room::covering(a, b, tag(Part::Sewer));
    let (a, b) = foot(1);
    let e1 = Room::covering(a, b, tag(Part::Sewer));
    let (a, b) = foot(2);
    let cellar = Room::around(a, 1, tag(Part::Cellar)).with(b);
    let (a, b) = foot(3);
    let secret = Room::covering(a, b, tag(Part::Passage)).leaf();
    vec![
        Room::around((0, 0), 1, tag(Part::Hub)),
        e0,
        e1,
        cellar,
        secret,
        Room::around(paul_cell(), 0, tag(Part::Sewer)),
    ]
}

/// The pin index of the secret passage's end.
pub const SECRET: usize = 4;

/// The network for a tunnels sub-seed (pure, the same on every peer and the
/// server): pinned rooms joined by sewers, with a few more chambers, loops
/// and dead ends rolled from `seed`. The corridor to the secret passage's
/// end is the passage itself.
pub fn layout(seed: u32) -> Layout {
    let spec = Spec {
        min: (-12, -24),
        max: (27, 10),
        pins: pins(),
        rooms: (4, 7),
        room_size: (2, 4),
        tag: Part::Sewer as u8,
        loops: 2,
        spurs: 7,
        spur_len: (2, 5),
    };
    let g = graph::generate(&spec, seed);
    let part_of_tag = |t: u8| match t {
        0 => Part::Hub,
        2 => Part::Passage,
        3 => Part::Cellar,
        _ => Part::Sewer,
    };
    let cells = g
        .cells
        .iter()
        .map(|(&c, u)| {
            let part = match *u {
                Use::Room(i) => part_of_tag(g.rooms[i].tag),
                Use::Corridor(e) => {
                    let (a, b) = g.edges[e];
                    if a == SECRET || b == SECRET {
                        Part::Passage
                    } else {
                        Part::Sewer
                    }
                }
                Use::Spur => Part::Sewer,
            };
            (c, part)
        })
        .collect();
    Layout { cells }
}

/// The tunnels of this world: [`layout`] of the [`WorldSeed`]'s sub-seed.
/// Every peer (and the server) keeps it in step with the seed.
#[derive(Resource, Clone, Debug, Deref)]
pub struct Tunnels(pub Layout);

fn sync_tunnels(mut commands: Commands, seed: Res<WorldSeed>) {
    commands.insert_resource(Tunnels(layout(seed.for_(SEED_SALT))));
}

fn drop_tunnels(mut commands: Commands) {
    commands.remove_resource::<Tunnels>();
}

/// Client -> server: I use way in / ladder `id` (down if I'm up top, up if
/// I'm down below).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct UseHatch {
    pub id: u8,
}

pub fn protocol(app: &mut App) {
    claim_ground();
    app.add_client_event::<UseHatch>(Channel::Ordered)
        .add_observer(use_hatch)
        .add_systems(
            PreUpdate,
            (
                sync_tunnels.run_if(resource_exists_and_changed::<WorldSeed>),
                drop_tunnels
                    .run_if(not(resource_exists::<WorldSeed>))
                    .run_if(resource_exists::<Tunnels>),
            ),
        );
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
    visuals::plugin(app);
}

/// Server: climb down or up.
fn use_hatch(
    ev: On<FromClient<UseHatch>>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut bodies: Query<(
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let id = ev.id as usize;
    let Some(e) = entrances().get(id).copied() else {
        return;
    };
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((_, mut walker, mut input, mut kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner)
    else {
        return;
    };
    let here = walker.pos;
    let underground = below(here);
    let top = !underground && (here.y - crate::terrain::height_at(here.x, here.z)).abs() < 4.0;
    if !(underground || top) || !within(here, e.at.extend(0.0).xzy(), SERVER_REACH) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let to = if underground {
        up_spot(id)
    } else {
        down_spot(id)
    };
    let yaw = walker.yaw;
    teleport(
        &mut walker,
        input.as_deref_mut(),
        kcc.as_deref_mut(),
        to,
        yaw,
    );
}

/// For the music: the camera's down in the tunnels.
fn music_signals(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let at = crate::music::camera_at(&camera);
    signals.flag(
        "underground",
        at.is_some_and(|at| below(at - Vec3::Y * 1.6)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_way_up_follows_the_tunnels() {
        let l = layout(WorldSeed(1).for_(SEED_SALT));
        // From the hub there's always a ladder, some way off.
        let Some((_, m, dir)) = way_up(&l, Vec2::ZERO) else {
            panic!("no way up from the hub")
        };
        assert!(m > 0.0 && dir != Vec2::ZERO);
        // Standing at a ladder: it's right here.
        let at = entrance(0).at;
        let Some((i, m, dir)) = way_up(&l, cell_centre(cell_of(at))) else {
            panic!("no way up at a ladder")
        };
        assert_eq!((i, dir), (0, Vec2::ZERO));
        assert!(m < CELL);
        // Nowhere near a tunnel: nothing.
        assert!(way_up(&l, Vec2::new(9_999.0, 9_999.0)).is_none());
        // Turning words.
        let fwd = Vec2::new(0.0, -1.0);
        assert_eq!(turn_word(fwd, Vec2::new(0.0, -1.0)), "straight ahead");
        assert_eq!(turn_word(fwd, Vec2::new(0.0, 1.0)), "behind you");
        assert_eq!(turn_word(fwd, Vec2::new(1.0, 0.0)), "to your right");
        assert_eq!(turn_word(fwd, Vec2::new(-1.0, 0.0)), "to your left");
    }

    #[test]
    fn everything_down_there_joins_up() {
        for seed in 0..40 {
            everything_joins_up(&layout(WorldSeed(seed).for_(SEED_SALT)));
        }
    }

    #[test]
    fn the_seed_changes_the_tunnels_but_not_the_ways_in() {
        let a = layout(WorldSeed(1).for_(SEED_SALT));
        let b = layout(WorldSeed(2).for_(SEED_SALT));
        assert_ne!(a.cells, b.cells);
        assert_eq!(a.cells, layout(WorldSeed(1).for_(SEED_SALT)).cells);
        for l in [&a, &b] {
            assert_eq!(l.part((0, 0)), Some(Part::Hub));
            assert_eq!(l.part(cell_of(entrance(2).at)), Some(Part::Cellar));
            assert_eq!(l.part(cell_of(entrance(3).at)), Some(Part::Passage));
            assert_eq!(l.part(paul_cell()), Some(Part::Sewer));
        }
    }

    fn everything_joins_up(l: &Layout) {
        for (i, a) in entrances().iter().enumerate() {
            let c = cell_of(a.at);
            assert!(l.open(c), "entrance {i} has no tunnel under it");
            for b in &entrances() {
                assert!(l.connected(c, cell_of(b.at)), "{} to {}", a.name, b.name);
            }
            // You arrive inside the tunnel, at the foot of its ladder.
            assert!(l.open(cell_of(down_spot(i).xz())), "{i}");
        }
        assert!(l.cells.values().any(|p| *p == Part::Passage));
        assert!(l.cells.values().any(|p| *p == Part::Sewer));
    }

    #[test]
    fn the_tunnels_stay_out_of_the_warehouse() {
        assert!(below(Vec3::new(0.0, TUNNEL_Y, 0.0)));
        assert!(below(Vec3::new(0.0, TUNNEL_Y + 1.0, 0.0)));
        assert!(!below(Vec3::new(0.0, 0.0, 0.0)));
        let wh = crate::features::warehouse::INSIDE_Y;
        assert!(!below(Vec3::new(0.0, wh, 0.0)));
        assert!(!crate::features::warehouse::inside(Vec3::new(
            0.0, TUNNEL_Y, 0.0
        )));
        // The tunnel ceiling is far below the lowest ground on the map.
        for x in (-90..=90).step_by(6) {
            for z in (-90..=90).step_by(6) {
                let h = crate::terrain::height_at(x as f32, z as f32);
                assert!(h > TUNNEL_Y + TUNNEL_H + 10.0, "{x},{z}");
            }
        }
    }

    #[test]
    fn claims_cover_the_sample_points_covers_does() {
        claim_ground();
        for e in entrances() {
            let c = e.at + e.out * 0.8;
            for margin in [0.0, 1.0] {
                for d in [Vec2::ZERO, Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y] {
                    let p = c + d * 2.1;
                    if covers(p, margin) {
                        assert!(
                            friendslop_world::footprints::claimed(p, margin),
                            "covers but unclaimed at {p}"
                        );
                    }
                }
            }
        }
    }
}
