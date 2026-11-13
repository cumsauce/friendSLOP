//! New York City. From issue #283 by doomy: "extending the forest a bit if
//! you go far south youll make out WTC one and the path station oculus.
//! right now the train station isnt functional but in the future it might go
//! places. in new york city make sure it really looks like new york and have
//! at least 4 enterable buildings in addition to the train station".
//!
//! Beyond the valley's south rim hills lies a flat city floor
//! ([`crate::rules::nyc`]). The forest carries on over the rim for a stretch,
//! then the city starts: a wide avenue with cross streets, sidewalks, yellow
//! cabs, street lamps, hydrants, brick walk-ups and glass towers, One World
//! Trade Center at the south end (541 m to the top of its spire, so you can
//! make it out over the rim hills from the valley's south edge) and the
//! white ribbed wings of the Oculus beside it, a train station you can walk
//! into but whose trains aren't running yet.
//!
//! The four enterable shops ([`SHOPS`]): a bodega (with a cat), a 99¢ pizza
//! counter, a laundromat and a diner. Step inside one and its line shows.
//!
//! The way there: a trail post at the south edge of the valley ("New York
//! City, this way"). Click it and the server walks you over the rim to the
//! city; click the post at the city's north end to come back. Everything you
//! see is local scenery, the same on every peer; the server only moves
//! bodies.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::nyc::*;

/// Tuning and writing: `assets/data/side/nyc.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The trail post at the valley's south edge (x, z).
    pub trail_post: (f32, f32),
    /// Where you arrive in the city (x, z); the way back post stands beside it.
    pub arrival: (f32, f32),
    /// Where you come back to in the valley (x, z), beside the trail post.
    pub return_to: (f32, f32),
    /// Seed for the skyline (tower heights, cabs, trees).
    pub seed: u32,
    /// The shops' names and the line shown when you step in, in [`SHOPS`] order.
    pub shops: Vec<(String, String)>,
    /// Shown inside the Oculus.
    pub oculus_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/nyc.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

fn v2((x, z): (f32, f32)) -> Vec2 {
    Vec2::new(x, z)
}

/// Client → server: walk over the rim (to the city) or back.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Travel {
    pub to_city: bool,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<Travel>(Channel::Ordered)
        .add_observer(travel);
    // Keep the valley's trees off the trail post.
    friendslop_world::footprints::claim(v2(tuning().trail_post), 4.0);
}

/// Where a traveller from `pos` ends up, or `None` if they aren't at a post.
pub fn destination(pos: Vec3, to_city: bool, t: &Tuning) -> Option<Vec3> {
    let (post, to) = if to_city {
        (v2(t.trail_post), v2(t.arrival))
    } else {
        (v2(t.arrival), v2(t.return_to))
    };
    if !within(pos, post.extend(0.0).xzy(), SERVER_REACH + 1.5) {
        return None;
    }
    let y = if to_city {
        CITY_Y
    } else {
        crate::terrain::height_at(to.x, to.y)
    };
    Some(Vec3::new(to.x, y + 0.05, to.y))
}

fn travel(
    ev: On<FromClient<Travel>>,
    mut commands: Commands,
    mut bodies: Query<(
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((_, mut walker, input, kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner) else {
        return;
    };
    let Some(to) = destination(walker.pos, ev.to_city, &tuning()) else {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
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

// ------------------------------------------------------------------ layout

/// What a piece of the city is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Asphalt,
    Sidewalk,
    Paint,
    Brick,
    Brownstone,
    Glass,
    Concrete,
    Steel,
    White,
    Cab,
    Tyre,
    Lamp,
    Hydrant,
    Awning(u8),
    Wood,
    Leaf,
    Bark,
    Sign,
}

/// One box of the city: centre, size, turn, look, and whether you bump into it.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub at: Vec3,
    pub size: Vec3,
    pub rot: Quat,
    pub look: Look,
    pub solid: bool,
}

/// A shop you can walk into: the floor rectangle (x, z) inside its walls.
#[derive(Clone, Copy, Debug)]
pub struct Room {
    pub min: Vec2,
    pub max: Vec2,
}

impl Room {
    pub fn contains(&self, p: Vec2) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
}

/// The whole city, built the same on every peer.
pub struct City {
    pub pieces: Vec<Piece>,
    /// The four shops, in [`SHOPS`] order.
    pub rooms: Vec<Room>,
    /// The Oculus hall's floor.
    pub oculus: Room,
    /// One World Trade Center's foot (x, z).
    pub wtc: Vec2,
}

/// The shops, by kind (names and lines are in the data file).
pub const SHOPS: [&str; 4] = ["bodega", "pizza", "laundromat", "diner"];

/// Avenue half width (curb to curb) and sidewalk width.
const AVE: f32 = 7.0;
const WALK: f32 = 4.0;
/// Where the forest gives way to the city (z).
const CITY_START: f32 = 150.0;
/// Cross streets' centres (z) and their half width.
const STREETS: [f32; 4] = [156.0, 200.0, 244.0, 280.0];
const STREET: f32 = 6.0;
/// Wall thickness.
const WALL: f32 = 0.3;

struct Dice(u32);
impl Dice {
    fn roll(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.roll()
    }
}

fn piece(at: Vec3, size: Vec3, look: Look, solid: bool) -> Piece {
    Piece {
        at,
        size,
        rot: Quat::IDENTITY,
        look,
        solid,
    }
}

/// A box standing on the street, its foot centred on (x, z).
fn block(out: &mut Vec<Piece>, x: f32, z: f32, size: Vec3, look: Look) {
    out.push(piece(
        Vec3::new(x, CITY_Y + size.y / 2.0, z),
        size,
        look,
        true,
    ));
}

/// A walk-up shop whose door faces the avenue (`side` -1 west, +1 east of
/// it): a hollow ground floor with a door gap, an awning, a counter, and
/// floors above. Returns the floor inside.
fn shop(out: &mut Vec<Piece>, side: f32, z0: f32, kind: usize) -> Room {
    let (w, d, h, up) = (12.0, 10.0, 4.2, 9.0);
    // x along the avenue's normal: the front wall is the one nearest it.
    let front = side * (AVE + WALK);
    let back = front + side * w;
    let (x0, x1) = (front.min(back), front.max(back));
    let z1 = z0 + d;
    let mid_x = (x0 + x1) / 2.0;
    let mid_z = (z0 + z1) / 2.0;
    let skin = if kind.is_multiple_of(2) {
        Look::Brick
    } else {
        Look::Brownstone
    };
    let wall = |out: &mut Vec<Piece>, at: Vec3, size: Vec3| out.push(piece(at, size, skin, true));
    let y = CITY_Y + h / 2.0;
    // Back and side walls.
    wall(out, Vec3::new(back, y, mid_z), Vec3::new(WALL, h, d));
    wall(out, Vec3::new(mid_x, y, z0), Vec3::new(w, h, WALL));
    wall(out, Vec3::new(mid_x, y, z1), Vec3::new(w, h, WALL));
    // Front wall with a 2.4 m door in the middle (shop windows either side).
    let door = 2.4;
    let seg = (d - door) / 2.0;
    wall(
        out,
        Vec3::new(front, y, z0 + seg / 2.0),
        Vec3::new(WALL, h, seg),
    );
    wall(
        out,
        Vec3::new(front, y, z1 - seg / 2.0),
        Vec3::new(WALL, h, seg),
    );
    // Over the door.
    out.push(piece(
        Vec3::new(front, CITY_Y + 3.4, mid_z),
        Vec3::new(WALL, 1.6, door),
        skin,
        false,
    ));
    // Shop windows (dark glass in the front wall).
    for z in [z0 + seg / 2.0, z1 - seg / 2.0] {
        out.push(piece(
            Vec3::new(front - side * 0.02, CITY_Y + 1.7, z),
            Vec3::new(WALL + 0.04, 1.8, seg - 0.8),
            Look::Glass,
            false,
        ));
    }
    // The floors above: one solid block (with window rows), and the ceiling.
    out.push(piece(
        Vec3::new(mid_x, CITY_Y + h + up / 2.0, mid_z),
        Vec3::new(w, up, d),
        skin,
        true,
    ));
    for row in 0..3 {
        for col in 0..3 {
            out.push(piece(
                Vec3::new(
                    front - side * 0.03,
                    CITY_Y + h + 1.5 + row as f32 * 2.8,
                    z0 + 1.8 + col as f32 * 3.2,
                ),
                Vec3::new(WALL, 1.6, 1.1),
                Look::Glass,
                false,
            ));
        }
    }
    // A fire escape down the front.
    out.push(piece(
        Vec3::new(front - side * 0.5, CITY_Y + h + up / 2.0, mid_z),
        Vec3::new(0.8, up - 1.0, 2.0),
        Look::Steel,
        false,
    ));
    // The awning over the door.
    out.push(piece(
        Vec3::new(front - side * 0.9, CITY_Y + 3.2, mid_z),
        Vec3::new(1.8, 0.15, d - 1.0),
        Look::Awning(kind as u8),
        false,
    ));
    // A counter at the back, and something of the shop's kind.
    out.push(piece(
        Vec3::new(back - side * 1.4, CITY_Y + 0.5, mid_z),
        Vec3::new(1.0, 1.0, d - 3.0),
        Look::Wood,
        true,
    ));
    match kind {
        // Bodega: shelves down both sides.
        0 => {
            for z in [z0 + 0.6, z1 - 0.6] {
                out.push(piece(
                    Vec3::new(mid_x, CITY_Y + 0.9, z),
                    Vec3::new(w - 4.0, 1.8, 0.5),
                    Look::Awning(4),
                    true,
                ));
            }
        }
        // Pizza: the oven.
        1 => out.push(piece(
            Vec3::new(back - side * 0.6, CITY_Y + 1.6, mid_z + 2.5),
            Vec3::new(0.6, 1.2, 2.0),
            Look::Steel,
            false,
        )),
        // Laundromat: a row of machines.
        2 => {
            for i in 0..5 {
                out.push(piece(
                    Vec3::new(mid_x, CITY_Y + 0.45, z0 + 1.2 + i as f32 * 1.0),
                    Vec3::new(0.8, 0.9, 0.8),
                    Look::White,
                    true,
                ));
            }
        }
        // Diner: booths.
        _ => {
            for i in 0..3 {
                out.push(piece(
                    Vec3::new(mid_x, CITY_Y + 0.4, z0 + 2.0 + i as f32 * 3.0),
                    Vec3::new(2.0, 0.8, 0.9),
                    Look::Awning(0),
                    true,
                ));
            }
        }
    }
    Room {
        min: Vec2::new(x0 + WALL, z0 + WALL),
        max: Vec2::new(x1 - WALL, z1 - WALL),
    }
}

/// The Oculus: a white spine with ribs fanning up and out on both sides,
/// over a hall you can walk into. Centred on (x, z), long along z.
fn oculus(out: &mut Vec<Piece>, x: f32, z: f32) -> Room {
    let (len, half_w) = (44.0, 12.0);
    // The hall's floor and a low white wall around it, open at both ends.
    out.push(piece(
        Vec3::new(x, CITY_Y + 0.02, z),
        Vec3::new(half_w * 2.0, 0.04, len),
        Look::White,
        false,
    ));
    for s in [-1.0, 1.0] {
        out.push(piece(
            Vec3::new(x + s * half_w, CITY_Y + 2.0, z),
            Vec3::new(0.6, 4.0, len),
            Look::White,
            true,
        ));
    }
    // The spine: a skylight down the middle.
    out.push(piece(
        Vec3::new(x, CITY_Y + 14.0, z),
        Vec3::new(1.2, 0.6, len - 4.0),
        Look::Glass,
        false,
    ));
    // Ribs: from the hall's walls up to past the spine and outward like wings.
    let ribs = 22;
    for i in 0..ribs {
        let rz = z - len / 2.0 + 2.0 + i as f32 * (len - 4.0) / (ribs - 1) as f32;
        // Taller in the middle, the wings' shape.
        let t = (rz - z) / (len / 2.0);
        let lift = 1.0 - t * t * 0.45;
        for s in [-1.0f32, 1.0] {
            let length = 26.0 * lift;
            let tilt = s * 0.55;
            // A rib leaning outward from near the spine.
            let foot = Vec3::new(x + s * half_w, CITY_Y + 4.0, rz);
            let dir = Quat::from_rotation_z(-tilt) * Vec3::Y;
            let at = foot + dir * (length / 2.0) - Vec3::X * s * 6.0;
            out.push(Piece {
                at,
                size: Vec3::new(0.5, length, 0.7),
                rot: Quat::from_rotation_z(-tilt),
                look: Look::White,
                solid: false,
            });
        }
    }
    // The tracks below: a platform edge and rails (the trains aren't running).
    for dx in [-3.0, 3.0] {
        out.push(piece(
            Vec3::new(x + dx, CITY_Y + 0.06, z),
            Vec3::new(0.15, 0.08, len - 6.0),
            Look::Steel,
            false,
        ));
    }
    out.push(piece(
        Vec3::new(x, CITY_Y + 1.2, z + len / 2.0 - 6.0),
        Vec3::new(4.0, 2.4, 0.3),
        Look::Sign,
        true,
    ));
    Room {
        min: Vec2::new(x - half_w, z - len / 2.0),
        max: Vec2::new(x + half_w, z + len / 2.0),
    }
}

/// A yellow cab, centred on (x, z), facing along z.
fn cab(out: &mut Vec<Piece>, x: f32, z: f32) {
    block(out, x, z, Vec3::new(1.9, 1.0, 4.6), Look::Cab);
    out.push(piece(
        Vec3::new(x, CITY_Y + 1.35, z + 0.2),
        Vec3::new(1.7, 0.7, 2.4),
        Look::Cab,
        false,
    ));
    out.push(piece(
        Vec3::new(x, CITY_Y + 1.8, z + 0.2),
        Vec3::new(0.6, 0.2, 0.3),
        Look::White,
        false,
    ));
    for (dx, dz) in [(-1.0, -1.5), (1.0, -1.5), (-1.0, 1.5), (1.0, 1.5)] {
        out.push(piece(
            Vec3::new(x + dx * 0.9, CITY_Y + 0.35, z + dz),
            Vec3::new(0.3, 0.7, 0.7),
            Look::Tyre,
            false,
        ));
    }
}

/// A street lamp at (x, z), its arm reaching towards `reach` (x direction).
fn lamp(out: &mut Vec<Piece>, x: f32, z: f32, reach: f32) {
    block(out, x, z, Vec3::new(0.2, 7.0, 0.2), Look::Steel);
    out.push(piece(
        Vec3::new(x + reach * 1.0, CITY_Y + 7.0, z),
        Vec3::new(2.0, 0.15, 0.15),
        Look::Steel,
        false,
    ));
    out.push(piece(
        Vec3::new(x + reach * 2.0, CITY_Y + 6.85, z),
        Vec3::new(0.6, 0.2, 0.35),
        Look::Lamp,
        false,
    ));
}

/// Lay out the city from `seed`.
pub fn city(seed: u32) -> City {
    let mut d = Dice(seed ^ 0x0283_0283);
    let mut out = Vec::new();
    let (x0, x1) = (CITY_MIN.x, CITY_MAX.x);
    let (z0, z1) = (CITY_MIN.y, CITY_MAX.y);
    let mid_z = (z0 + z1) / 2.0;
    // The ground: forest floor up to the city, asphalt beyond.
    out.push(piece(
        Vec3::new(0.0, CITY_Y - 0.5, mid_z),
        Vec3::new(x1 - x0, 1.0, z1 - z0),
        Look::Leaf,
        true,
    ));
    let city_len = z1 - CITY_START;
    out.push(piece(
        Vec3::new(0.0, CITY_Y + 0.01, CITY_START + city_len / 2.0),
        Vec3::new(x1 - x0, 0.02, city_len),
        Look::Sidewalk,
        false,
    ));
    // The avenue, north to south, and the cross streets.
    out.push(piece(
        Vec3::new(0.0, CITY_Y + 0.03, (z0 + z1) / 2.0),
        Vec3::new(AVE * 2.0, 0.02, z1 - z0),
        Look::Asphalt,
        false,
    ));
    for &sz in &STREETS {
        out.push(piece(
            Vec3::new(0.0, CITY_Y + 0.035, sz),
            Vec3::new(x1 - x0, 0.02, STREET * 2.0),
            Look::Asphalt,
            false,
        ));
        // Crosswalk stripes over the avenue.
        for i in 0..7 {
            out.push(piece(
                Vec3::new(
                    -AVE + 1.0 + i as f32 * 2.0,
                    CITY_Y + 0.05,
                    sz - STREET - 1.2,
                ),
                Vec3::new(0.9, 0.02, 2.0),
                Look::Paint,
                false,
            ));
        }
    }
    // The avenue's dashed centre line.
    let mut z = CITY_START;
    while z < z1 - 2.0 {
        out.push(piece(
            Vec3::new(0.0, CITY_Y + 0.05, z),
            Vec3::new(0.2, 0.02, 2.5),
            Look::Paint,
            false,
        ));
        z += 6.0;
    }
    // The forest carrying on over the rim, a path down the middle.
    let mut z = z0 + 2.0;
    while z < CITY_START - 6.0 {
        let mut x = x0 + 3.0;
        while x < x1 - 3.0 {
            let tx = x + d.range(-2.0, 2.0);
            let tz = z + d.range(-2.0, 2.0);
            if tx.abs() > 5.0 && d.roll() < 0.7 {
                let h = d.range(6.0, 11.0);
                block(&mut out, tx, tz, Vec3::new(0.4, h, 0.4), Look::Bark);
                out.push(piece(
                    Vec3::new(tx, CITY_Y + h * 0.75, tz),
                    Vec3::new(3.0, h * 0.5, 3.0),
                    Look::Leaf,
                    false,
                ));
            }
            x += 7.0;
        }
        z += 7.0;
    }
    // Blocks between the streets, on both sides of the avenue: the shops on
    // the first block, then towers; One World Trade Center and the Oculus at
    // the south end.
    let mut rooms = Vec::new();
    let edge = AVE + WALK;
    let wtc = Vec2::new(36.0, 309.0);
    let oc = oculus(&mut out, -36.0, 308.0);
    let bands: Vec<(f32, f32)> = std::iter::once(CITY_START + 1.0)
        .chain(STREETS.iter().map(|s| s + STREET + WALK))
        .zip(
            STREETS
                .iter()
                .map(|s| s - STREET - WALK)
                .chain(std::iter::once(z1 - 1.0)),
        )
        .collect();
    for (b, &(bz0, bz1)) in bands.iter().enumerate() {
        for side in [-1.0f32, 1.0] {
            let mut z = bz0;
            if b == 1 {
                // The shops: two each side, facing the avenue.
                for k in 0..2 {
                    let kind = if side < 0.0 { k } else { 2 + k };
                    rooms.push((kind, shop(&mut out, side, z + 0.5 + k as f32 * 11.5, kind)));
                }
                z += 24.0;
            }
            if b == bands.len() - 1 {
                continue; // the south block: the Oculus and the tower
            }
            while z < bz1 - 6.0 {
                let depth = d.range(10.0, 16.0).min(bz1 - z);
                let width = d.range(14.0, 30.0);
                let tall = d.range(20.0, 95.0) * if b == 0 { 0.5 } else { 1.0 };
                let x = side * (edge + width / 2.0);
                let look = match (d.roll() * 4.0) as u32 {
                    0 => Look::Glass,
                    1 => Look::Concrete,
                    2 => Look::Brownstone,
                    _ => Look::Brick,
                };
                block(
                    &mut out,
                    x,
                    z + depth / 2.0,
                    Vec3::new(width, tall, depth),
                    look,
                );
                // Setback crown on the taller ones, Art Deco style.
                if tall > 55.0 {
                    let crown = Vec3::new(width * 0.6, tall * 0.25, depth * 0.6);
                    out.push(piece(
                        Vec3::new(x, CITY_Y + tall + crown.y / 2.0, z + depth / 2.0),
                        crown,
                        look,
                        false,
                    ));
                }
                // A water tower on the low roofs.
                if tall < 45.0 && d.roll() < 0.5 {
                    out.push(piece(
                        Vec3::new(x, CITY_Y + tall + 2.0, z + depth / 2.0),
                        Vec3::new(2.4, 2.6, 2.4),
                        Look::Wood,
                        false,
                    ));
                }
                // Behind it, more towers further from the avenue.
                let far = d.range(30.0, 130.0);
                let fx = side * (edge + width + 2.0 + 12.0);
                if fx.abs() + 12.0 < x1 {
                    block(
                        &mut out,
                        fx,
                        z + depth / 2.0,
                        Vec3::new(22.0, far, depth),
                        if d.roll() < 0.5 {
                            Look::Glass
                        } else {
                            Look::Concrete
                        },
                    );
                }
                z += depth + 1.0;
            }
        }
    }
    // One World Trade Center's base (the tower itself is its own mesh).
    block(
        &mut out,
        wtc.x,
        wtc.y,
        Vec3::new(40.0, 20.0, 40.0),
        Look::Steel,
    );
    // Cabs, lamps and hydrants.
    let mut z = CITY_START + 6.0;
    while z < z1 - 10.0 {
        if !STREETS.iter().any(|s| (z - s).abs() < STREET + 3.0) {
            if d.roll() < 0.5 {
                cab(&mut out, -AVE + 1.3, z);
            }
            if d.roll() < 0.4 {
                cab(&mut out, AVE - 1.3, z + 3.0);
            }
        }
        z += 9.0;
    }
    let mut z = CITY_START + 4.0;
    while z < z1 - 4.0 {
        lamp(&mut out, -AVE - 0.6, z, 1.0);
        lamp(&mut out, AVE + 0.6, z + 12.0, -1.0);
        block(
            &mut out,
            AVE + 0.8,
            z + 5.0,
            Vec3::new(0.35, 0.8, 0.35),
            Look::Hydrant,
        );
        z += 24.0;
    }
    // Curbs.
    for s in [-1.0, 1.0] {
        out.push(piece(
            Vec3::new(s * AVE, CITY_Y + 0.08, (CITY_START + z1) / 2.0),
            Vec3::new(0.3, 0.16, z1 - CITY_START),
            Look::Concrete,
            false,
        ));
    }
    rooms.sort_by_key(|(k, _)| *k);
    City {
        pieces: out,
        rooms: rooms.into_iter().map(|(_, r)| r).collect(),
        oculus: oc,
        wtc,
    }
}

/// One World Trade Center: a square foot whose corners are cut back as it
/// rises, so it's a square turned 45° at the top (eight long triangles), a
/// parapet and the spire. Local to its foot.
pub fn wtc_mesh() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let (base, half, top_half, h) = (20.0, 21.0, 14.85, 417.0 - 20.0);
    let q = std::f32::consts::FRAC_PI_4;
    let bottom: Vec<Vec3> = (0..4)
        .map(|i| {
            let a = q + i as f32 * 2.0 * q;
            Vec3::new(
                a.cos() * half * std::f32::consts::SQRT_2,
                base,
                a.sin() * half * std::f32::consts::SQRT_2,
            )
        })
        .collect();
    let top: Vec<Vec3> = (0..4)
        .map(|i| {
            let a = i as f32 * 2.0 * q;
            Vec3::new(
                a.cos() * top_half * std::f32::consts::SQRT_2,
                base + h,
                a.sin() * top_half * std::f32::consts::SQRT_2,
            )
        })
        .collect();
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut tri = |a: Vec3, b: Vec3, c: Vec3| {
        pos.extend([a.to_array(), b.to_array(), c.to_array()]);
    };
    for i in 0..4 {
        let b0 = bottom[i];
        let b1 = bottom[(i + 1) % 4];
        let t0 = top[i];
        let t1 = top[(i + 1) % 4];
        // Faces wind counter-clockwise seen from outside.
        tri(b0, t1, b1);
        tri(b0, t0, t1);
    }
    let cap = Vec3::new(0.0, base + h, 0.0);
    for i in 0..4 {
        tri(top[i], cap, top[(i + 1) % 4]);
    }
    let n = pos.len();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_indices(Indices::U32((0..n as u32).collect()));
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

// ------------------------------------------------------------------ client

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build_city)
        .add_systems(Update, where_am_i.run_if(in_state(AppState::InGame)));
}

#[derive(Component)]
struct Post {
    to_city: bool,
}

fn click(ev: On<Clicked>, mut commands: Commands, posts: Query<&Post>) {
    if let Ok(p) = posts.get(ev.entity) {
        commands.client_trigger(Travel { to_city: p.to_city });
    }
}

fn color(look: Look) -> (Color, f32, f32, bool) {
    // (colour, roughness, metallic, glowing)
    match look {
        Look::Asphalt => (Color::srgb(0.12, 0.12, 0.13), 0.95, 0.0, false),
        Look::Sidewalk => (Color::srgb(0.55, 0.55, 0.53), 0.9, 0.0, false),
        Look::Paint => (Color::srgb(0.92, 0.92, 0.88), 0.8, 0.0, false),
        Look::Brick => (Color::srgb(0.55, 0.25, 0.18), 0.9, 0.0, false),
        Look::Brownstone => (Color::srgb(0.42, 0.28, 0.2), 0.9, 0.0, false),
        Look::Glass => (Color::srgb(0.25, 0.38, 0.5), 0.12, 0.6, false),
        Look::Concrete => (Color::srgb(0.68, 0.66, 0.62), 0.85, 0.0, false),
        Look::Steel => (Color::srgb(0.3, 0.32, 0.35), 0.4, 0.8, false),
        Look::White => (Color::srgb(0.95, 0.95, 0.93), 0.5, 0.0, false),
        Look::Cab => (Color::srgb(0.98, 0.76, 0.05), 0.35, 0.2, false),
        Look::Tyre => (Color::srgb(0.05, 0.05, 0.05), 0.9, 0.0, false),
        Look::Lamp => (Color::srgb(1.0, 0.85, 0.55), 0.5, 0.0, true),
        Look::Hydrant => (Color::srgb(0.8, 0.1, 0.08), 0.6, 0.0, false),
        Look::Awning(k) => (
            [
                Color::srgb(0.75, 0.12, 0.12),
                Color::srgb(0.1, 0.45, 0.2),
                Color::srgb(0.15, 0.3, 0.7),
                Color::srgb(0.85, 0.75, 0.2),
                Color::srgb(0.6, 0.5, 0.35),
            ][k as usize % 5],
            0.8,
            0.0,
            false,
        ),
        Look::Wood => (Color::srgb(0.45, 0.3, 0.18), 0.85, 0.0, false),
        Look::Leaf => (Color::srgb(0.2, 0.35, 0.15), 0.95, 0.0, false),
        Look::Bark => (Color::srgb(0.3, 0.2, 0.12), 0.95, 0.0, false),
        Look::Sign => (Color::srgb(0.0, 0.25, 0.55), 0.5, 0.0, false),
    }
}

fn build_city(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = tuning();
    let c = city(t.seed);
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mut mats: Vec<(Look, Handle<StandardMaterial>)> = Vec::new();
    let mut mat = |look: Look, materials: &mut Assets<StandardMaterial>| {
        if let Some((_, m)) = mats.iter().find(|(l, _)| *l == look) {
            return m.clone();
        }
        let (base, roughness, metallic, glow) = color(look);
        let m = materials.add(StandardMaterial {
            base_color: base,
            perceptual_roughness: roughness,
            metallic,
            emissive: if glow {
                base.to_linear() * 8.0
            } else {
                LinearRgba::BLACK
            },
            ..default()
        });
        mats.push((look, m.clone()));
        m
    };
    for p in &c.pieces {
        let mut e = commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(mat(p.look, &mut materials)),
            Transform::from_translation(p.at)
                .with_rotation(p.rot)
                .with_scale(p.size),
            DespawnOnExit(AppState::InGame),
        ));
        if p.solid {
            e.insert((
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
            ));
        }
    }
    // One World Trade Center, and the spire.
    let glass = mat(Look::Glass, &mut materials);
    let foot = Vec3::new(c.wtc.x, CITY_Y, c.wtc.y);
    commands.spawn((
        Mesh3d(meshes.add(wtc_mesh())),
        MeshMaterial3d(glass),
        Transform::from_translation(foot),
        DespawnOnExit(AppState::InGame),
    ));
    let white = mat(Look::White, &mut materials);
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.8, 124.0))),
        MeshMaterial3d(white),
        Transform::from_translation(foot + Vec3::Y * (417.0 + 62.0)),
        DespawnOnExit(AppState::InGame),
    ));
    // The trail posts: one at the valley's south edge, one in the city.
    let sign = mat(Look::Sign, &mut materials);
    let wood = mat(Look::Wood, &mut materials);
    for (at, to_city, label) in [
        (v2(t.trail_post), true, "walk to New York City"),
        (
            v2(t.arrival) + Vec2::new(2.5, -2.0),
            false,
            "walk back to the valley",
        ),
    ] {
        let y = if to_city {
            crate::terrain::height_at(at.x, at.y)
        } else {
            CITY_Y
        };
        commands
            .spawn((
                Post { to_city },
                Transform::from_xyz(at.x, y, at.y),
                Visibility::default(),
                Clickable::at_height(1.6).radius(60.0).labelled(label),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(0.15, 2.2, 0.15)),
                ));
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(sign.clone()),
                    Transform::from_xyz(0.0, 1.9, 0.0).with_scale(Vec3::new(1.4, 0.4, 0.06)),
                ));
            });
    }
    commands.insert_resource(Rooms(c));
}

#[derive(Resource)]
struct Rooms(City);

/// Local: step into a shop (or the Oculus) and its line shows.
fn where_am_i(
    me: Option<Res<LocalPlayer>>,
    rooms: Option<Res<Rooms>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut inside: Local<Option<usize>>,
) {
    let (Some(me), Some(rooms), Some(mut toast)) = (me, rooms, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let here = room_at(&rooms.0, w.pos.xz());
    if here != *inside {
        *inside = here;
        let t = tuning();
        if let Some(i) = here {
            let line = if i < SHOPS.len() {
                t.shops.get(i).map(|(n, l)| format!("{n}: {l}"))
            } else {
                Some(t.oculus_line.clone())
            };
            if let Some(line) = line {
                toast.show(line);
            }
        }
    }
}

/// Which shop (index into [`SHOPS`]) or the Oculus (`SHOPS.len()`) `p` is in.
pub fn room_at(c: &City, p: Vec2) -> Option<usize> {
    c.rooms
        .iter()
        .position(|r| r.contains(p))
        .or_else(|| c.oculus.contains(p).then_some(SHOPS.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        Tuning {
            trail_post: (10.0, 86.0),
            arrival: (0.0, 125.0),
            return_to: (10.0, 83.0),
            ..default()
        }
    }

    #[test]
    fn the_posts_take_you_there_and_back() {
        let t = t();
        let there = destination(Vec3::new(10.0, 0.0, 86.5), true, &t).unwrap_or_default();
        assert!(in_city(there.x, there.z));
        let back = destination(there, false, &t).unwrap_or_default();
        assert!(!in_city(back.x, back.z));
        assert!(destination(Vec3::ZERO, true, &t).is_none());
    }

    #[test]
    fn four_shops_and_a_station_all_in_the_city() {
        let c = city(7);
        assert_eq!(c.rooms.len(), 4);
        for r in c.rooms.iter().chain([&c.oculus]) {
            assert!(in_city(r.min.x, r.min.y) && in_city(r.max.x, r.max.y));
            assert!(r.max.x - r.min.x > 4.0 && r.max.y - r.min.y > 4.0);
        }
        for (i, r) in c.rooms.iter().enumerate() {
            assert_eq!(room_at(&c, (r.min + r.max) / 2.0), Some(i));
        }
        assert!(in_city(c.wtc.x, c.wtc.y));
    }

    #[test]
    fn the_avenue_is_clear_of_buildings() {
        let c = city(7);
        for p in c.pieces.iter().filter(|p| p.solid && p.size.y > 3.0) {
            let near = p.at.x.abs() - p.size.x / 2.0;
            assert!(near >= AVE - 0.5 || p.at.z < CITY_START, "{p:?}");
        }
    }
}
