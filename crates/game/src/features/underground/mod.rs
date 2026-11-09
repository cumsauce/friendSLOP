//! Underground passages. From issue #127 by doomy: "sewers, basements,
//! secret passageways, connected! and music to go with it".
//!
//! - Under the map, [`TUNNEL_Y`] down, runs one network of tunnels on a
//!   3 m grid ([`layout`]): brick sewers with a channel of murky water, a
//!   junction chamber under the commune with daylight falling through a
//!   grate, a damp cellar under a village cottage, and a narrow, torch-lit
//!   secret passage. Everything down there joins up.
//! - Four ways in ([`ENTRANCES`]): a manhole on the road east out of the
//!   commune, a manhole on the road to the ruins, the cellar door beside the
//!   big cottage in the east village, and a loose floorboard in the
//!   woodcutter's hut in the forest clearing (no sign: it's a secret).
//! - Click a way in to climb down its ladder; click the ladder (under any
//!   of them) to climb up there. So you can go down in one place and come up
//!   somewhere else entirely.
//! - The band plays its own tune down there (`Mood::Underground` in the
//!   fusion crate: dub-wet minor, a bottle-flute lead, kalimba drips and a
//!   lot of echo), and the sky goes out (local only).
//!
//! The server moves bodies; the rest is local and the same on every peer.

use std::collections::{BTreeMap, VecDeque};

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{SERVER_REACH, deny, within};
use crate::player::PlayerOwner;

mod visuals;

pub use visuals::Below;

/// The tunnel floor, far under the meadow (the warehouse is further down).
pub const TUNNEL_Y: f32 = -30.0;
/// Floor to ceiling.
pub const TUNNEL_H: f32 = 3.2;
/// Grid cell size: one tunnel is one cell wide.
pub const CELL: f32 = 3.0;

/// What a stretch of underground is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// Every way in. All of them join up underground.
pub const ENTRANCES: [Entrance; 4] = [
    Entrance {
        at: Vec2::new(21.0, 1.0),
        out: Vec2::new(0.0, -1.0),
        kind: Kind::Manhole,
        name: "the sewers",
    },
    Entrance {
        at: Vec2::new(-20.0, 22.0),
        out: Vec2::new(1.0, 0.0),
        kind: Kind::Manhole,
        name: "the sewers",
    },
    Entrance {
        at: Vec2::new(72.6, -11.0),
        out: Vec2::new(1.0, 0.0),
        kind: Kind::CellarDoor,
        name: "a damp cellar",
    },
    Entrance {
        at: Vec2::new(-26.8, -62.8),
        out: Vec2::new(1.0, 0.0),
        kind: Kind::Trapdoor,
        name: "a secret passage",
    },
];

/// Is a body with feet at `feet` down in the tunnels?
pub fn below(feet: Vec3) -> bool {
    below_y(feet.y)
}

/// The same, by height alone (the warehouse is below this band).
pub fn below_y(y: f32) -> bool {
    (TUNNEL_Y - 8.0..TUNNEL_Y + 12.0).contains(&y)
}

/// Does a way in (plus `margin`) cover this map spot? Scenery keeps out.
pub fn covers(p: Vec2, margin: f32) -> bool {
    ENTRANCES
        .iter()
        .any(|e| p.distance(e.at + e.out * 0.8) < 2.2 + margin)
}

/// Where you arrive at the bottom of entrance `i`'s ladder.
pub fn down_spot(i: usize) -> Vec3 {
    let e = ENTRANCES[i % ENTRANCES.len()];
    let p = e.at + e.out * 1.0;
    Vec3::new(p.x, TUNNEL_Y, p.y)
}

/// Where you step off at the top of entrance `i`.
pub fn up_spot(i: usize) -> Vec3 {
    let e = ENTRANCES[i % ENTRANCES.len()];
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

    /// Carve a cell (the first part to claim it keeps it).
    fn carve(&mut self, c: Cell, part: Part) {
        self.cells.entry(c).or_insert(part);
    }

    /// Carve an L: along x first, then along z.
    fn tunnel(&mut self, from: Cell, to: Cell, part: Part) {
        let (mut x, z) = from;
        loop {
            self.carve((x, z), part);
            if x == to.0 {
                break;
            }
            x += (to.0 - x).signum();
        }
        let mut z = z;
        while z != to.1 {
            z += (to.1 - z).signum();
            self.carve((x, z), part);
        }
    }

    /// Carve a square room of `r` cells around `c`.
    fn room(&mut self, c: Cell, r: i32, part: Part) {
        for x in -r..=r {
            for z in -r..=r {
                self.carve((c.0 + x, c.1 + z), part);
            }
        }
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

/// The network (pure, the same on every peer and the server).
pub fn layout() -> Layout {
    let mut l = Layout::default();
    let hub = (0, 0);
    let at = |i: usize| cell_of(ENTRANCES[i].at);
    l.room(hub, 1, Part::Hub);
    l.room(at(2), 1, Part::Cellar);
    // The sewers: east under the commune road to the manhole and on to the
    // cottage cellar, and south-west to the manhole on the ruins road (the
    // second one goes south first, then west).
    l.tunnel(hub, at(0), Part::Sewer);
    l.tunnel(at(0), at(2), Part::Sewer);
    let (bx, bz) = at(1);
    l.tunnel((0, bz), (bx, bz), Part::Sewer);
    for z in 1..=bz {
        l.carve((0, z), Part::Sewer);
    }
    // The secret passage: north, a long way, to the woodcutter's hut.
    let (dx, dz) = at(3);
    for z in (dz..0).rev() {
        l.carve((0, z), Part::Passage);
    }
    l.tunnel((0, dz), (dx, dz), Part::Passage);
    l
}

/// Client -> server: I use way in / ladder `id` (down if I'm up top, up if
/// I'm down below).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct UseHatch {
    pub id: u8,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<UseHatch>(Channel::Ordered)
        .add_observer(use_hatch);
}

pub fn plugin(app: &mut App) {
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
    let Some(e) = ENTRANCES.get(id) else { return };
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((_, mut walker, input, kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner) else {
        return;
    };
    let here = walker.pos;
    let underground = below(here);
    let top = !underground && (here.y - crate::terrain::height_at(here.x, here.z)).abs() < 4.0;
    if !(underground || top) || !within(here, e.at.extend(0.0).xzy(), SERVER_REACH) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    walker.pos = if underground {
        up_spot(id)
    } else {
        down_spot(id)
    };
    if let Some(mut input) = input {
        input.stick = Vec2::ZERO;
        input.jump = false;
    }
    if let Some(mut kcc) = kcc {
        kcc.velocity = Vec3::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_down_there_joins_up() {
        let l = layout();
        for (i, a) in ENTRANCES.iter().enumerate() {
            let c = cell_of(a.at);
            assert!(l.open(c), "entrance {i} has no tunnel under it");
            for b in &ENTRANCES {
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
}
