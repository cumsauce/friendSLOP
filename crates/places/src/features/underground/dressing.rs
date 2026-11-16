//! The tunnels' clutter: fallen bricks, moss, leak streaks, glowing
//! mushrooms, old crates, cobwebs and candle stubs along the walls. Rolled
//! from the tunnels' seed (the same on every peer), looks only: no
//! colliders, nothing to do. Odds in `data/places/underground.ron`.

use serde::Deserialize;

use super::*;

/// How likely each bit of clutter is on any one tunnel wall.
#[derive(Deserialize, Default, Clone, Debug)]
pub struct Tuning {
    /// Fallen bricks heaped against a wall (sewers and passage).
    pub rubble: f32,
    /// Green moss creeping up the foot of a wall (sewers).
    pub moss: f32,
    /// Dark wet streaks running down a wall from a leak (sewers and hub).
    pub streaks: f32,
    /// Clumps of faintly glowing mushrooms at a wall's foot (sewers).
    pub mushrooms: f32,
    /// At most this many mushroom clumps get a little light of their own.
    pub mushroom_lights: u32,
    /// Old crates stacked against a wall (cellar and passage).
    pub crates: f32,
    /// Cobwebs in the ceiling corners (passage and cellar).
    pub cobwebs: f32,
    /// Melted candle stubs on the floor by a wall (passage).
    pub candles: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/places/underground.ron");

/// What a bit of clutter is (one material each).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Bit {
    Brick,
    Moss,
    Streak,
    Mushroom,
    Crate,
    Web,
    Candle,
    Flame,
}

/// One box of clutter: what, where (its centre), how big, turned how far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub bit: Bit,
    pub at: Vec3,
    pub size: Vec3,
    pub yaw: f32,
}

/// A cheap, stable roll in 0..1 for a cell, wall side and purpose.
fn roll(seed: u32, c: Cell, side: u32, salt: u32) -> f32 {
    let mut h = seed
        ^ (c.0 as u32).wrapping_mul(0x9E37_79B1)
        ^ (c.1 as u32).wrapping_mul(0x85EB_CA77)
        ^ side.wrapping_mul(0xC2B2_AE3D)
        ^ salt.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_4D2B);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Every bit of clutter for a network (pure). Ladder feet stay clear.
pub fn dress(l: &Layout, seed: u32, t: &Tuning) -> Vec<Piece> {
    let keep_clear: Vec<Cell> = entrances()
        .iter()
        .enumerate()
        .flat_map(|(i, e)| [cell_of(e.at), cell_of(down_spot(i).xz())])
        .collect();
    let (y, top) = (TUNNEL_Y, TUNNEL_Y + TUNNEL_H);
    // The wall's inner face, from the cell's centre.
    let face = CELL * 0.5 - 0.2;
    let mut out = Vec::new();
    for (&c, &part) in &l.cells {
        if keep_clear.contains(&c) {
            continue;
        }
        let centre = cell_centre(c);
        for (side, d) in [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().enumerate() {
            let side = side as u32;
            if l.open((c.0 + d.0, c.1 + d.1)) {
                continue;
            }
            let n = Vec2::new(d.0 as f32, d.1 as f32);
            let along = Vec2::new(-n.y, n.x);
            let yaw = (-n.x).atan2(-n.y);
            let r = |salt| roll(seed, c, side, salt);
            // Somewhere along this wall, off its middle.
            let slide = |salt| (r(salt) - 0.5) * (CELL - 0.8);
            let at = |inset: f32, s: f32, h: f32| {
                let p = centre + n * (face - inset) + along * s;
                Vec3::new(p.x, h, p.y)
            };
            let wet = matches!(part, Part::Sewer | Part::Hub);
            let old = matches!(part, Part::Passage | Part::Cellar);
            if matches!(part, Part::Sewer | Part::Passage) && r(1) < t.rubble {
                let s = slide(2);
                for k in 0..4 {
                    let k = k as f32;
                    let size = Vec3::new(0.24, 0.08, 0.12);
                    out.push(Piece {
                        bit: Bit::Brick,
                        at: at(0.1 + 0.12 * (k % 2.0), s + 0.18 * k - 0.27, y + 0.04)
                            + Vec3::Y * if k == 3.0 { 0.08 } else { 0.0 },
                        size,
                        yaw: yaw + r(10 + k as u32) * 1.4,
                    });
                }
            }
            if part == Part::Sewer && r(3) < t.moss {
                let w = 0.8 + r(4) * 1.4;
                let h = 0.25 + r(5) * 0.5;
                out.push(Piece {
                    bit: Bit::Moss,
                    at: at(-0.02, slide(6) * 0.4, y + h * 0.5),
                    size: Vec3::new(w, h, 0.04),
                    yaw,
                });
            }
            if wet && r(7) < t.streaks {
                let s = slide(8);
                let len = 1.2 + r(9) * 1.6;
                for k in 0..2 {
                    out.push(Piece {
                        bit: Bit::Streak,
                        at: at(-0.03, s + k as f32 * 0.22, top - len * 0.5 - k as f32 * 0.3),
                        size: Vec3::new(0.12 + 0.06 * k as f32, len, 0.03),
                        yaw,
                    });
                }
            }
            if part == Part::Sewer && r(20) < t.mushrooms {
                let s = slide(21);
                for k in 0..5 {
                    let h = 0.05 + r(22 + k) * 0.12;
                    out.push(Piece {
                        bit: Bit::Mushroom,
                        at: at(
                            0.06 + r(30 + k) * 0.15,
                            s + (k as f32 - 2.0) * 0.09,
                            y + h * 0.5,
                        ),
                        size: Vec3::new(0.06, h, 0.06),
                        yaw,
                    });
                }
            }
            if old && r(40) < t.crates {
                let s = slide(41);
                let big = 0.55 + r(42) * 0.2;
                out.push(Piece {
                    bit: Bit::Crate,
                    at: at(big * 0.5, s, y + big * 0.5),
                    size: Vec3::splat(big),
                    yaw: yaw + (r(43) - 0.5) * 0.3,
                });
                if r(44) < 0.5 {
                    let small = big * 0.7;
                    out.push(Piece {
                        bit: Bit::Crate,
                        at: at(big * 0.5, s + 0.05, y + big + small * 0.5),
                        size: Vec3::splat(small),
                        yaw: yaw + (r(45) - 0.5) * 0.8,
                    });
                }
            }
            if old && r(50) < t.cobwebs {
                // Across the corner where this wall meets the ceiling.
                let s = if r(51) < 0.5 { -1.0 } else { 1.0 } * (CELL * 0.5 - 0.5);
                out.push(Piece {
                    bit: Bit::Web,
                    at: at(0.25, s, top - 0.3),
                    size: Vec3::new(0.7, 0.01, 0.55),
                    yaw,
                });
            }
            if part == Part::Passage && r(60) < t.candles {
                let s = slide(61);
                for k in 0..3 {
                    let h = 0.06 + r(62 + k) * 0.1;
                    let p = at(
                        0.1 + 0.07 * (k % 2) as f32,
                        s + k as f32 * 0.08,
                        y + h * 0.5,
                    );
                    out.push(Piece {
                        bit: Bit::Candle,
                        at: p,
                        size: Vec3::new(0.045, h, 0.045),
                        yaw,
                    });
                    if k == 0 {
                        out.push(Piece {
                            bit: Bit::Flame,
                            at: p + Vec3::Y * (h * 0.5 + 0.02),
                            size: Vec3::new(0.02, 0.04, 0.02),
                            yaw,
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tunnels_get_cluttered_the_same_way_every_time() {
        let t = Tuning {
            rubble: 0.5,
            moss: 0.5,
            streaks: 0.5,
            mushrooms: 0.5,
            mushroom_lights: 2,
            crates: 0.5,
            cobwebs: 0.5,
            candles: 0.5,
        };
        let l = layout(WorldSeed(1).for_(SEED_SALT));
        let a = dress(&l, 1, &t);
        assert!(!a.is_empty());
        assert_eq!(a, dress(&l, 1, &t));
        assert!(dress(&l, 1, &Tuning::default()).is_empty());
        // Everything stays inside the tunnels.
        for p in &a {
            assert!(l.open(cell_of(p.at.xz())), "{p:?}");
            assert!(p.at.y >= TUNNEL_Y && p.at.y <= TUNNEL_Y + TUNNEL_H);
        }
    }
}
