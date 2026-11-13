//! A small shape grammar for houses. A footprint (`w` x `d`, front on the
//! local south, +z) is refined step by step:
//!
//! 1. **rule set** ([`Rules`]): cottage, townhouse, barn or shop;
//! 2. **floors**: 1 to 3 storeys (barns are one tall storey);
//! 3. **facade**: the front is split into equal bays at least [`BAY`] wide;
//! 4. **modules**: each bay of each floor gets a [`Module`] (door, window,
//!    plain wall, balcony door, shop window);
//! 5. **roof** ([`RoofKind`]): gable, hip or flat with a parapet;
//! 6. **trim**: chimney, shutters, awnings.
//!
//! [`design`] makes those choices from a seed (pure data, [`Design`]) and
//! [`build`] lays the design into a [`Kit`], returning the [`Room`]s inside
//! so a furniture placer can fill them later. Every design keeps doors wide
//! enough for a body, stairs gentle and landing on a floor, every upper floor
//! reachable by stairs, and solid boxes touching but never overlapping (the
//! tests check all of it over many seeds). No game code.

use std::f32::consts::PI;

use bevy::prelude::*;

use crate::{Hole, Kit, Palette, Shape, Solid};

/// Height of one storey (barns are taller, see [`Design::storey`]).
pub const STOREY: f32 = 3.0;
/// Narrowest facade bay: room for a door or window with shutters.
pub const BAY: f32 = 2.2;
/// Run of one flight of stairs (a 3 m storey rises at about 31 degrees).
const FLIGHT: f32 = 5.0;

/// A tiny deterministic random stream (splitmix64), so the grammar needs no
/// dependencies and every peer draws the same numbers from the same seed.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(u64::from(seed) ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// An index below `n` (0 when `n` is 0).
    pub fn index(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            self.next_u32() as usize % n
        }
    }
}

/// The rule sets: each gives a visibly different kind of building.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Rules {
    /// One storey, maybe split into two rooms, chimney, shutters.
    Cottage,
    /// Two or three storeys with stairs, maybe a balcony.
    Townhouse,
    /// One tall storey with a big door and high windows.
    Barn,
    /// A shop window and counter downstairs, an awning, maybe a flat above.
    Shop,
}

impl Rules {
    pub const ALL: [Rules; 4] = [Rules::Cottage, Rules::Townhouse, Rules::Barn, Rules::Shop];

    /// Whether this rule set fits a `w` x `d` footprint.
    pub fn fits(self, w: f32, d: f32) -> bool {
        match self {
            Rules::Cottage => w >= 5.0 && d >= 4.5,
            Rules::Townhouse => w >= 6.5 && d >= 6.0,
            Rules::Barn => w >= 6.0 && d >= 4.5,
            Rules::Shop => w >= 2.0 * BAY && d >= 4.5,
        }
    }

    /// Whether it can have an upper floor.
    pub fn storeys(self, w: f32, d: f32) -> bool {
        match self {
            Rules::Townhouse => true,
            Rules::Shop => Rules::Townhouse.fits(w, d),
            _ => false,
        }
    }

    /// Pick a rule set for a footprint. `upper` forces one with an upper
    /// floor (falls back to a cottage if none fits).
    pub fn pick(w: f32, d: f32, upper: bool, rng: &mut Rng) -> Rules {
        let weights = [
            (Rules::Cottage, 4.0),
            (Rules::Townhouse, 2.0),
            (Rules::Barn, 1.0),
            (Rules::Shop, 2.0),
        ];
        let ok: Vec<_> = weights
            .into_iter()
            .filter(|(r, _)| r.fits(w, d) && (!upper || r.storeys(w, d)))
            .collect();
        let total: f32 = ok.iter().map(|(_, wt)| wt).sum();
        let mut roll = rng.unit() * total;
        for (r, wt) in &ok {
            if roll < *wt {
                return *r;
            }
            roll -= wt;
        }
        ok.last().map_or(Rules::Cottage, |(r, _)| *r)
    }
}

/// What fills one facade bay on one floor.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Module {
    Door,
    Window,
    Plain,
    /// An upper-floor door onto a balcony.
    Balcony,
    /// A wide, low shop window.
    Shopfront,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RoofKind {
    Gable,
    Hip,
    /// A walkable flat roof behind a low parapet.
    Flat,
}

/// Every choice the grammar made for one building (local frame, centred,
/// front on +z).
#[derive(Clone, PartialEq, Debug)]
pub struct Design {
    pub rules: Rules,
    pub w: f32,
    pub d: f32,
    pub floors: u32,
    pub storey: f32,
    /// Bay centres along the front (x).
    pub bays: Vec<f32>,
    /// `facade[floor][bay]`.
    pub facade: Vec<Vec<Module>>,
    /// Ground-floor side doors.
    pub east_door: bool,
    pub west_door: bool,
    /// An inner wall at this x splitting the ground floor in two.
    pub split: Option<f32>,
    pub roof: RoofKind,
    pub rise: f32,
    /// Chimney on the east (+1) or west (-1) end.
    pub chimney: Option<f32>,
    pub shutters: bool,
    /// Indices into the [`Style`] lists.
    pub wall: usize,
    pub tiles: usize,
    pub awning: usize,
}

/// The materials a house may be built from. The grammar picks indices.
#[derive(Clone, Debug)]
pub struct Style<M> {
    pub walls: Vec<M>,
    pub barn: M,
    pub roofs: Vec<(M, M)>,
    pub floor: M,
    pub stone: M,
    pub hay: M,
    pub awnings: Vec<M>,
}

/// One room inside a generated building, in world space: the free box
/// between its walls (`min`..`max`, bottom at the floor), the floor's height,
/// the doorways into it (floor-level centres) and a theme tag for a
/// furniture placer ("kitchen", "bedroom", "parlour", "shop", "barn").
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    pub min: Vec3,
    pub max: Vec3,
    pub floor_y: f32,
    pub doors: Vec<Vec3>,
    pub theme: &'static str,
}

/// Bay centres for a front `w` wide.
pub fn bays(w: f32) -> Vec<f32> {
    let n = ((w / BAY).floor() as usize).max(1);
    let bw = w / n as f32;
    (0..n).map(|i| -w / 2.0 + bw * (i as f32 + 0.5)).collect()
}

/// Make every choice for a `w` x `d` building of `rules` from `seed`.
/// `upper` asks for at least two storeys where the rule set allows it.
pub fn design(rules: Rules, w: f32, d: f32, upper: bool, seed: u32) -> Design {
    let mut rng = Rng::new(seed);
    let bays = bays(w);
    let n = bays.len();
    let floors = match rules {
        Rules::Townhouse => {
            if w >= 7.5 && d >= 6.5 && rng.chance(0.4) {
                3
            } else {
                2
            }
        }
        Rules::Shop if rules.storeys(w, d) && (upper || rng.chance(0.5)) => 2,
        _ => 1,
    };
    let storey = if rules == Rules::Barn { 4.5 } else { STOREY };
    let window = |rng: &mut Rng| {
        if rng.chance(0.8) {
            Module::Window
        } else {
            Module::Plain
        }
    };
    let mut facade = Vec::new();
    // Ground floor.
    let door = rng.index(n);
    let mut ground: Vec<Module> = (0..n).map(|_| window(&mut rng)).collect();
    ground[door] = Module::Door;
    if rules == Rules::Shop && n >= 2 {
        let mut shop = rng.index(n - 1);
        if shop >= door {
            shop += 1;
        }
        ground[shop] = Module::Shopfront;
    }
    facade.push(ground);
    for f in 1..floors {
        let mut row: Vec<Module> = (0..n).map(|_| window(&mut rng)).collect();
        if f == 1 && rng.chance(0.6) {
            row[rng.index(n)] = Module::Balcony;
        }
        facade.push(row);
    }
    // At least one side door; barns always open to the west.
    let (east_door, west_door) = match rules {
        Rules::Barn => (false, true),
        _ => match rng.index(3) {
            0 => (true, false),
            1 => (false, true),
            _ => (true, true),
        },
    };
    // Split a cottage on a bay boundary, leaving room for a bed east.
    let split = if rules == Rules::Cottage && n >= 2 && rng.chance(0.5) {
        let bw = w / n as f32;
        let ok: Vec<f32> = (1..n)
            .map(|i| -w / 2.0 + bw * i as f32)
            .filter(|x| w / 2.0 - x >= 2.2 && x + w / 2.0 >= 2.0)
            .collect();
        (!ok.is_empty()).then(|| ok[rng.index(ok.len())])
    } else {
        None
    };
    let roof = match rules {
        Rules::Barn => RoofKind::Gable,
        Rules::Cottage => {
            if rng.chance(0.7) {
                RoofKind::Gable
            } else {
                RoofKind::Hip
            }
        }
        Rules::Townhouse => [
            RoofKind::Gable,
            RoofKind::Gable,
            RoofKind::Hip,
            RoofKind::Flat,
        ][rng.index(4)],
        Rules::Shop => [RoofKind::Gable, RoofKind::Flat, RoofKind::Flat][rng.index(3)],
    };
    let roof = if roof == RoofKind::Hip && w < d {
        RoofKind::Gable
    } else {
        roof
    };
    let rise = match rules {
        Rules::Barn => d * rng.range(0.5, 0.6),
        _ => d * rng.range(0.36, 0.5),
    };
    let chimney = (rules != Rules::Barn && rng.chance(0.75))
        .then(|| if rng.chance(0.5) { 1.0 } else { -1.0 });
    Design {
        rules,
        w,
        d,
        floors,
        storey,
        bays,
        facade,
        east_door,
        west_door,
        split,
        roof,
        rise,
        chimney,
        shutters: matches!(rules, Rules::Cottage | Rules::Townhouse) && rng.chance(0.6),
        wall: rng.next_u32() as usize,
        tiles: rng.next_u32() as usize,
        awning: rng.next_u32() as usize,
    }
}

/// Pick a rule set and design for a footprint in one go.
pub fn house(w: f32, d: f32, upper: bool, seed: u32) -> Design {
    let mut rng = Rng::new(seed ^ 0xA5A5_5A5A);
    let rules = Rules::pick(w, d, upper, &mut rng);
    design(rules, w, d, upper, seed)
}

fn pick<M: Copy>(list: &[M], i: usize, or: M) -> M {
    if list.is_empty() {
        or
    } else {
        list[i % list.len()]
    }
}

/// Lay `design` into the kit's current local frame (see [`Kit::place`]) and
/// return its rooms (world space).
pub fn build<M: Palette>(k: &mut Kit<M>, design: &Design, style: &Style<M>) -> Vec<Room> {
    let first_door = k.doors.len();
    let Design { w, d, storey, .. } = *design;
    let (hx, hz) = (w / 2.0, d / 2.0);
    let t = k.thick;
    let inner = Vec2::new(hx - t / 2.0, hz - t / 2.0);
    let barn = design.rules == Rules::Barn;
    let wall = if barn {
        style.barn
    } else {
        pick(&style.walls, design.wall, style.stone)
    };
    let (ta, tb) = pick(&style.roofs, design.tiles, (style.stone, style.stone));
    let floors = design.floors;
    let eave = storey * floors as f32;

    // ---- walls, floor by floor
    for f in 0..floors {
        let y0 = storey * f as f32;
        let mut south = Vec::new();
        for (bay, module) in design.bays.iter().zip(&design.facade[f as usize]) {
            match module {
                Module::Door | Module::Balcony => south.push(Hole::door(*bay)),
                Module::Window => south.push(Hole::window(*bay)),
                Module::Shopfront => south.push(Hole {
                    c: *bay,
                    w: 1.8,
                    y0: 0.5,
                    y1: 2.2,
                }),
                Module::Plain => {}
            }
        }
        let high = |c: f32| Hole {
            c,
            w: 0.8,
            y0: 3.2,
            y1: 3.9,
        };
        // North runs east -> west: a hole at x reads at -x.
        let north: Vec<Hole> = if barn {
            vec![high(hx - 1.0), high(-hx + 1.0)]
        } else {
            design
                .bays
                .iter()
                .enumerate()
                .filter(|(i, _)| (i + f as usize).is_multiple_of(2))
                .map(|(_, x)| Hole::window(-x))
                .collect()
        };
        // East runs south -> north (c = -z), west north -> south (c = z).
        let side_door = hz * 0.3;
        let side_window = -hz * 0.35;
        let (mut east, mut west) = (Vec::new(), Vec::new());
        if barn {
            if design.east_door {
                east.push(Hole::door(0.0));
            } else {
                east.push(high(0.0));
            }
            if design.west_door {
                west.push(Hole::door(0.0));
            } else {
                west.push(high(0.0));
            }
        } else if f == 0 {
            if design.east_door {
                east.push(Hole::door(-side_door));
            }
            east.push(Hole::window(-side_window));
            if design.west_door {
                west.push(Hole::door(side_door));
            }
            west.push(Hole::window(side_window));
        } else {
            east.push(Hole::window(0.0));
            west.push(Hole::window(0.0));
        }
        if barn {
            south = vec![
                Hole {
                    c: 0.0,
                    w: 3.0,
                    y0: 0.0,
                    y1: 3.2,
                },
                high(-hx + 0.9),
                high(hx - 0.9),
            ];
        }
        k.walls(w, d, y0, storey, [&south, &north, &east, &west], wall);
        // Trim models in the holes: frames, doors, shop windows, barn doors.
        k.trim(w, d, y0, [&south, &north, &east, &west]);
        // Shutters either side of front windows.
        if design.shutters {
            for hole in south.iter().filter(|h| h.y0 > 0.5 && h.w <= 1.0) {
                for s in [-1.0, 1.0] {
                    let c = Vec3::new(
                        hole.c + s * (hole.w / 2.0 + 0.225),
                        y0 + (hole.y0 + hole.y1) / 2.0,
                        hz + t / 2.0 + 0.04,
                    );
                    let size = Vec3::new(0.35, hole.y1 - hole.y0, 0.04);
                    // The model's hinges face the window.
                    let turn = Quat::from_rotation_y(if s < 0.0 { PI } else { 0.0 });
                    let p = k.begin_prop_turned("shutter", c, turn, Vec3::ONE);
                    k.put(
                        Shape::Cube,
                        c,
                        size,
                        Quat::IDENTITY,
                        M::TIMBER,
                        Solid::None,
                        true,
                    );
                    k.end_prop(p);
                }
            }
        }
    }
    // Ground boards.
    let boards = if barn { style.hay } else { style.floor };
    k.block(
        Vec3::new(-hx, 0.0, -hz),
        Vec3::new(hx, 0.03, hz),
        boards,
        Solid::None,
    );

    // ---- stairs and upper floors
    let lane = |i: f32| -hz + t / 2.0 + 0.7 + 1.25 * i;
    let (x0, x1) = (-hx + 0.5, -hx + 0.5 + FLIGHT);
    for f in 1..floors {
        let (ya, yb) = (storey * (f - 1) as f32, storey * f as f32);
        let z = lane((f - 1) as f32);
        k.stairs(Vec3::new(x0, ya, z), Vec3::new(x1, yb, z), 1.1);
        // The well over this flight (and every flight below in the lanes).
        let well = -hz + 1.4 + 1.3 * (f - 1) as f32;
        k.floor(Vec2::new(-inner.x, well), inner, yb, style.floor);
        k.floor(
            Vec2::new(x1, -inner.y),
            Vec2::new(inner.x, well),
            yb,
            style.floor,
        );
        k.bar(
            Vec2::new(-inner.x, well + 0.05),
            Vec2::new(x1 - 0.3, well + 0.05),
            yb,
            1.0,
            0.08,
            M::TIMBER,
            Solid::Wall,
        );
    }
    // Balconies.
    let bw = w / design.bays.len() as f32;
    for f in 1..floors {
        let y = storey * f as f32;
        for (bay, module) in design.bays.iter().zip(&design.facade[f as usize]) {
            if *module != Module::Balcony {
                continue;
            }
            let bx = (bw / 2.0 - 0.05).min(1.1);
            let bz = hz + 1.5;
            k.floor(
                Vec2::new(bay - bx, hz + t / 2.0),
                Vec2::new(bay + bx, bz),
                y,
                style.floor,
            );
            let v = Vec2::new;
            for (a, b) in [
                (v(bay - bx, bz), v(bay + bx, bz)),
                (v(bay - bx, hz + 0.15), v(bay - bx, bz - 0.05)),
                (v(bay + bx, hz + 0.15), v(bay + bx, bz - 0.05)),
            ] {
                k.bar(a, b, y, 1.0, 0.1, M::TIMBER, Solid::Wall);
            }
            for sx in [bay - bx + 0.1, bay + bx - 0.1] {
                let c = Vec3::new(sx, 0.0, bz - 0.1);
                k.prop(c, Vec3::new(0.16, y - 0.15, 0.16), M::TIMBER);
            }
        }
    }
    // Awning over a shop window.
    for (bay, module) in design.bays.iter().zip(&design.facade[0]) {
        if *module != Module::Shopfront {
            continue;
        }
        let cloth = pick(&style.awnings, design.awning, M::RED);
        let at = Vec3::new(*bay, 2.5, hz + t / 2.0 + 0.6);
        let p = k.begin_prop("awning", at, Vec3::ONE);
        for i in 0..4 {
            let x = bay - 0.9 + 0.6 * (i as f32 + 0.5);
            let mat = if i % 2 == 0 { cloth } else { M::LINEN };
            let c = Vec3::new(x, 2.5, hz + t / 2.0 + 0.6);
            let rot = Quat::from_rotation_x(0.3);
            let size = Vec3::new(0.6, 0.04, 1.2);
            k.put(Shape::Cube, c, size, rot, mat, Solid::None, true);
        }
        k.end_prop(p);
    }
    // Inner wall.
    if let Some(xs) = design.split {
        k.wall(
            Vec2::new(xs, -hz + t / 2.0),
            Vec2::new(xs, hz - t / 2.0),
            0.0,
            storey,
            0.15,
            &[Hole::door(hz * 0.3)],
            wall,
        );
    }

    // ---- furniture (fixed spots that keep doors and stairs clear)
    let ground_x = design.split.map_or(0.0, |xs| (-inner.x + xs) / 2.0);
    match design.rules {
        // Tables, beds and shelves come from `placer` per room (see `Room`).
        Rules::Cottage => {
            k.lamp(Vec3::new(ground_x, 2.3, -hz * 0.3));
        }
        Rules::Townhouse => {
            k.lamp(Vec3::new(0.0, 2.3, -hz * 0.3));
        }
        Rules::Shop => {
            k.lamp(Vec3::new(0.0, 2.3, 0.0));
        }
        Rules::Barn => {
            let bale = Vec3::new(1.2, 0.75, 0.8);
            for i in 0..2 {
                let at = Vec3::new(hx - 1.0, bale.y * i as f32, -hz + 0.8);
                k.prop(at, bale, style.hay);
            }
        }
    }

    // ---- roof and chimney
    let roof_top = match design.roof {
        RoofKind::Gable => {
            k.roof(Vec2::ZERO, w, d, eave, design.rise, ta, tb, wall);
            eave + design.rise
        }
        RoofKind::Hip => {
            hip(k, w, d, eave, design.rise, ta, tb);
            eave + design.rise
        }
        RoofKind::Flat => {
            k.block(
                Vec3::new(-inner.x, eave - 0.15, -inner.y),
                Vec3::new(inner.x, eave, inner.y),
                style.stone,
                Solid::Floor,
            );
            let v = Vec2::new;
            let h = 0.9;
            k.wall(
                v(-hx - t / 2.0, hz),
                v(hx + t / 2.0, hz),
                eave,
                h,
                t,
                &[],
                wall,
            );
            k.wall(
                v(hx + t / 2.0, -hz),
                v(-hx - t / 2.0, -hz),
                eave,
                h,
                t,
                &[],
                wall,
            );
            k.wall(
                v(hx, hz - t / 2.0),
                v(hx, -hz + t / 2.0),
                eave,
                h,
                t,
                &[],
                wall,
            );
            k.wall(
                v(-hx, -hz + t / 2.0),
                v(-hx, hz - t / 2.0),
                eave,
                h,
                t,
                &[],
                wall,
            );
            eave + 0.6
        }
    };
    if let Some(side) = design.chimney {
        let (xa, xb) = (side * (hx - 1.2), side * (hx - 0.6));
        k.block(
            Vec3::new(xa.min(xb), eave, -0.9),
            Vec3::new(xa.max(xb), roof_top + 0.8, -0.3),
            style.stone,
            Solid::None,
        );
        let top = Vec3::new((xa + xb) / 2.0, roof_top + 0.8, -0.6);
        k.mark("chimney_cap", top, Quat::IDENTITY, Vec3::ONE);
    }

    // ---- rooms
    let doors: Vec<Vec3> = k.doors[first_door..].iter().map(|d| d.centre).collect();
    let mut rooms = Vec::new();
    let mut room = |k: &Kit<M>, x0: f32, x1: f32, y: f32, top: f32, theme: &'static str| {
        let a = k.world(Vec3::new(x0, y, -inner.y));
        let b = k.world(Vec3::new(x1, top, inner.y));
        let (min, max) = (a.min(b), a.max(b));
        let floor_y = min.y;
        let near = |p: &Vec3| {
            (p.y - floor_y).abs() < 0.1
                && p.x > min.x - 0.3
                && p.x < max.x + 0.3
                && p.z > min.z - 0.3
                && p.z < max.z + 0.3
        };
        rooms.push(Room {
            min,
            max,
            floor_y,
            doors: doors.iter().copied().filter(near).collect(),
            theme,
        });
    };
    let ground_theme = match design.rules {
        Rules::Cottage => "parlour",
        Rules::Townhouse => "kitchen",
        Rules::Shop => "shop",
        Rules::Barn => "barn",
    };
    if let Some(xs) = design.split {
        room(k, -inner.x, xs - 0.075, 0.0, storey, "kitchen");
        room(k, xs + 0.075, inner.x, 0.0, storey, "bedroom");
    } else {
        room(k, -inner.x, inner.x, 0.0, storey, ground_theme);
    }
    for f in 1..floors {
        let y = storey * f as f32;
        room(k, -inner.x, inner.x, y, y + storey, "bedroom");
    }
    rooms
}

/// A hipped roof: four planes of tile rows meeting at the hips, the rows
/// stepping in towards the ridge. Ridge along x (needs `w >= d`). Each plane
/// gets a hidden solid slab.
pub fn hip<M: Palette>(k: &mut Kit<M>, w: f32, d: f32, eave: f32, rise: f32, a: M, b: M) {
    let over = 0.45;
    let hz = d / 2.0;
    let angle = rise.atan2(hz);
    let run = hz + over;
    let slope = run / angle.cos();
    let ridge_half = ((w - d) / 2.0).max(0.0);
    let ridge = Vec3::new(0.0, eave + rise, 0.0);
    let rows = 5;
    let (sin, cos) = angle.sin_cos();
    for side in [-1.0f32, 1.0] {
        // Long planes, facing +-z.
        let rot = Quat::from_rotation_x(angle * side);
        let down = Vec3::new(0.0, -sin, cos * side);
        let n = rot * Vec3::Y;
        for i in 0..rows {
            let s = slope * (i as f32 + 0.5) / rows as f32;
            let u = run * (i as f32 + 0.5) / rows as f32;
            let centre = ridge + down * s + n * (0.08 + 0.03 * (i % 2) as f32);
            let size = Vec3::new(2.0 * (ridge_half + u), 0.12, slope / rows as f32 * 1.12);
            let mat = if i % 2 == 0 { a } else { b };
            k.put(Shape::Cube, centre, size, rot, mat, Solid::None, false);
        }
        let centre = ridge + down * slope / 2.0 + n * 0.08;
        let size = Vec3::new(2.0 * (ridge_half + run / 2.0), 0.12, slope);
        k.put(
            Shape::Cube,
            centre,
            size,
            rot,
            M::HIDDEN,
            Solid::Wall,
            false,
        );
        // Hip ends, facing +-x.
        let rot = Quat::from_rotation_z(-angle * side);
        let down = Vec3::new(cos * side, -sin, 0.0);
        let n = rot * Vec3::Y;
        let apex = ridge + Vec3::X * side * ridge_half;
        for i in 0..rows {
            let s = slope * (i as f32 + 0.5) / rows as f32;
            let u = run * (i as f32 + 0.5) / rows as f32;
            let centre = apex + down * s + n * (0.09 + 0.03 * (i % 2) as f32);
            let size = Vec3::new(slope / rows as f32 * 1.12, 0.12, 2.0 * u);
            let mat = if i % 2 == 0 { b } else { a };
            k.put(Shape::Cube, centre, size, rot, mat, Solid::None, false);
        }
        let centre = apex + down * slope / 2.0 + n * 0.08;
        let size = Vec3::new(slope, 0.12, run);
        k.put(
            Shape::Cube,
            centre,
            size,
            rot,
            M::HIDDEN,
            Solid::Wall,
            false,
        );
    }
    if ridge_half > 0.05 {
        let cap = Vec3::new(2.0 * ridge_half + 0.3, 0.22, 0.32);
        let at = ridge + Vec3::Y * 0.12;
        k.put(Shape::Cube, at, cap, Quat::IDENTITY, b, Solid::None, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::M;
    use crate::{DOOR_H, Piece};

    fn style() -> Style<M> {
        Style {
            walls: vec![M::Wall],
            barn: M::Wall,
            roofs: vec![(M::Wood, M::Timber)],
            floor: M::Wood,
            stone: M::Wall,
            hay: M::Wood,
            awnings: vec![M::Wall],
        }
    }

    /// The village's plot footprints and a few more.
    const FOOTPRINTS: [(f32, f32); 7] = [
        (7.0, 6.0),
        (8.0, 7.0),
        (6.0, 6.0),
        (6.0, 5.0),
        (9.0, 6.0),
        (7.5, 7.0),
        (5.0, 4.5),
    ];

    fn aabb(p: &Piece<M>) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let half = p.size / 2.0;
        let base = if p.shape == Shape::Gable {
            Vec3::new(0.0, half.y, 0.0)
        } else {
            Vec3::ZERO
        };
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let c = p.centre + p.rot * (base + half * Vec3::new(sx, sy, sz));
                    lo = lo.min(c);
                    hi = hi.max(c);
                }
            }
        }
        (lo, hi)
    }

    fn overlaps(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
        a.0.cmplt(b.1).all() && b.0.cmplt(a.1).all()
    }

    fn body_at(feet: Vec3, along: Vec3, depth: f32) -> (Vec3, Vec3) {
        let side = Vec3::Y.cross(along).normalize().abs() * 0.36;
        let fwd = along.normalize().abs() * depth / 2.0;
        (
            feet - side - fwd + Vec3::Y * 0.1,
            feet + side + fwd + Vec3::Y * 1.8,
        )
    }

    fn axis_aligned(p: &Piece<M>) -> bool {
        let m = Mat3::from_quat(p.rot);
        [m.x_axis, m.y_axis, m.z_axis]
            .iter()
            .all(|a| a.abs().max_element() > 0.9999)
    }

    /// Every rule set on every footprint it fits, many seeds, all four turns.
    fn cases() -> Vec<(Design, Kit<M>, Vec<Room>)> {
        let mut out = Vec::new();
        for (i, (w, d)) in FOOTPRINTS.into_iter().enumerate() {
            for rules in Rules::ALL {
                if !rules.fits(w, d) {
                    continue;
                }
                for seed in 0..40u32 {
                    let seed = seed.wrapping_mul(2_654_435_761) ^ i as u32;
                    let mut design = design(rules, w, d, seed.is_multiple_of(2), seed);
                    if seed.is_multiple_of(3) {
                        // Callers may move side doors (a plot against a wall).
                        (design.east_door, design.west_door) = (true, false);
                    }
                    let mut kit = Kit::<M>::default();
                    kit.place(Vec3::new(10.0, 1.0, -5.0), (seed % 4) as i32);
                    let rooms = build(&mut kit, &design, &style());
                    out.push((design, kit, rooms));
                }
            }
        }
        out
    }

    #[test]
    fn same_seed_same_building() {
        for seed in [0, 1, 99, 12345] {
            let (a, b) = (house(8.0, 7.0, true, seed), house(8.0, 7.0, true, seed));
            assert_eq!(a, b);
            let (mut ka, mut kb) = (Kit::<M>::default(), Kit::<M>::default());
            let ra = build(&mut ka, &a, &style());
            let rb = build(&mut kb, &b, &style());
            assert_eq!(ra, rb);
            assert_eq!(ka.pieces.len(), kb.pieces.len());
            for (p, q) in ka.pieces.iter().zip(&kb.pieces) {
                assert_eq!((p.centre, p.size), (q.centre, q.size));
            }
        }
    }

    #[test]
    fn seeds_give_variety() {
        let designs: Vec<_> = (0..60).map(|s| house(8.0, 7.0, false, s)).collect();
        for r in Rules::ALL {
            assert!(designs.iter().any(|d| d.rules == r), "no {r:?}");
        }
        for r in [RoofKind::Gable, RoofKind::Hip, RoofKind::Flat] {
            assert!(designs.iter().any(|d| d.roof == r), "no {r:?} roof");
        }
        assert!(designs.iter().any(|d| d.floors == 3));
        assert!(
            designs
                .iter()
                .any(|d| d.facade.iter().flatten().any(|m| *m == Module::Balcony))
        );
        assert!((0..20).all(|s| house(8.0, 7.0, true, s).floors >= 2));
    }

    #[test]
    fn every_door_lets_a_body_through() {
        for (design, kit, _) in cases() {
            assert!(kit.doors.len() >= 2, "{design:?}");
            for door in &kit.doors {
                assert!(door.width >= 1.2 && door.height >= DOOR_H - 0.2);
                let probe = body_at(door.centre, door.through, 1.6);
                let hit: Vec<_> = kit
                    .pieces
                    .iter()
                    .filter(|p| p.solid == Solid::Wall && overlaps(aabb(p), probe))
                    .collect();
                assert!(
                    hit.is_empty(),
                    "{design:?}\ndoor {door:?} blocked by {hit:?}"
                );
            }
        }
    }

    #[test]
    fn stairs_are_gentle_clear_and_reach_every_upper_floor() {
        for (design, kit, rooms) in cases() {
            let floors: Vec<_> = kit
                .pieces
                .iter()
                .filter(|p| p.solid == Solid::Floor && (p.rot * Vec3::Y).y > 0.999)
                .map(aabb)
                .collect();
            let on_floor = |p: Vec3| {
                p.y < 1.0 + 0.05
                    || floors.iter().any(|(lo, hi)| {
                        (hi.y - p.y).abs() < 0.05
                            && p.x > lo.x - 0.15
                            && p.x < hi.x + 0.15
                            && p.z > lo.z - 0.15
                            && p.z < hi.z + 0.15
                    })
            };
            for ramp in &kit.ramps {
                let d = ramp.to - ramp.from;
                let slope = d.y.atan2(d.with_y(0.0).length()).to_degrees();
                assert!(slope < 36.0, "{ramp:?} too steep ({slope})");
                assert!(on_floor(ramp.from) && on_floor(ramp.to), "{ramp:?} dangles");
                for i in 1..20 {
                    let feet = ramp.from + d * i as f32 / 20.0;
                    let probe = body_at(feet, d.with_y(0.0), 0.7);
                    let hit: Vec<_> = kit
                        .pieces
                        .iter()
                        .filter(|p| p.solid == Solid::Wall && overlaps(aabb(p), probe))
                        .collect();
                    assert!(hit.is_empty(), "{design:?}\n{ramp:?} at {feet} hit {hit:?}");
                }
            }
            assert_eq!(
                rooms.len() as u32,
                design.floors + design.split.is_some() as u32
            );
            for room in rooms.iter().filter(|r| r.floor_y > 1.5) {
                let reached = kit.ramps.iter().any(|r| {
                    (r.to.y - room.floor_y).abs() < 0.05
                        && r.to.x > room.min.x - 0.2
                        && r.to.x < room.max.x + 0.2
                        && r.to.z > room.min.z - 0.2
                        && r.to.z < room.max.z + 0.2
                });
                assert!(reached, "{design:?}\nno stairs up to {room:?}");
            }
            for room in rooms.iter().filter(|r| r.floor_y < 1.5) {
                assert!(!room.doors.is_empty(), "{design:?}\n{room:?} has no door");
            }
        }
    }

    #[test]
    fn solid_boxes_never_overlap_and_faces_never_fight() {
        for (design, kit, _) in cases() {
            let boxes: Vec<_> = kit
                .pieces
                .iter()
                .filter(|p| p.shape == Shape::Cube && p.mat != M::Hidden && axis_aligned(p))
                .map(|p| (p, aabb(p)))
                .collect();
            let span = |lo: f32, hi: f32, lo2: f32, hi2: f32| hi.min(hi2) - lo.max(lo2);
            for (i, (p, a)) in boxes.iter().enumerate() {
                for (q, b) in &boxes[i + 1..] {
                    if p.solid != Solid::None && q.solid != Solid::None {
                        let depth = (a.1.min(b.1) - a.0.max(b.0)).min_element();
                        assert!(depth <= 2e-3, "{design:?}\n{p:?} overlaps {q:?}");
                    }
                    if p.mat == q.mat {
                        continue;
                    }
                    for ax in 0..3 {
                        let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
                        let area = span(a.0[u], a.1[u], b.0[u], b.1[u]) > 1e-3
                            && span(a.0[v], a.1[v], b.0[v], b.1[v]) > 1e-3;
                        let lo = ax != 1 && (a.0[ax] - b.0[ax]).abs() < 2e-3;
                        let same = lo || (a.1[ax] - b.1[ax]).abs() < 2e-3;
                        assert!(
                            !(area && same),
                            "{design:?}\n{p:?} fights {q:?} on axis {ax}"
                        );
                    }
                }
            }
        }
    }
}
