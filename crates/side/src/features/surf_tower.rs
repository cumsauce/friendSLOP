//! The skyscraper complex on the surf map (#306, doomy: "an area where you
//! go inside a huge skyscraper complex").
//!
//! Stage 4's first ramp runs into a podium of floors with four towers on
//! its roof, through a tall pink atrium (balconies, glass floors, neon
//! signs, escalators, a hanging ring) and out the far side. The ramp stays
//! as it is: [`layout`] carves the building's grid of slabs, walls and
//! columns around a tunnel that keeps `clear_side` metres either side of
//! every ridge and `clear_up` over it ([`in_way`]). The box and sizes are
//! `data/side/surf_tower.ron`; the looks are the modeler prefabs
//! `sf-tower-shell` / `sf-tower-interior` (`models/features/surf_tower_*`),
//! built from this same layout, and the colliders come from here.

use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::rules::surf::{Tuning, hall_at, ridge, tuning};

/// The complex's numbers: `assets/data/side/surf_tower.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tower {
    /// The podium's box (min, max corners; hall coordinates).
    pub min: (f32, f32, f32),
    pub max: (f32, f32, f32),
    /// Floor grid cell and storey height (m).
    pub cell: f32,
    pub storey: f32,
    /// Slab and wall thickness, column width (m).
    pub slab: f32,
    pub wall: f32,
    pub column: f32,
    /// A column every this many cells.
    pub column_every: usize,
    /// The tunnel around the ridges: sideways and above (m).
    pub clear_side: f32,
    pub clear_up: f32,
    /// Railing height; a sign / escalator every this many balcony edges.
    pub rail: f32,
    pub sign_every: usize,
    pub escalator_every: usize,
    /// Towers on the roof: centre (x, z), half size (x, z), roof height.
    pub towers: Vec<((f32, f32), (f32, f32), f32)>,
    /// The hanging ring: centre, radius, tube radius.
    pub sculpture: ((f32, f32, f32), f32, f32),
    /// Lights along the ridge inside: spacing, lift, colour, lumens, range.
    pub light_every: f32,
    pub light_up: f32,
    pub light_colour: (f32, f32, f32),
    pub light_intensity: f32,
    pub light_range: f32,
}

pub static TOWER: friendslop_data::Data<Tower> =
    friendslop_data::Data::ron("data/side/surf_tower.ron");

/// What a box is (picks its look in the model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Solid: floor slab, glass floor at the atrium's edge, facade wall,
    /// column, tower.
    Slab,
    Glass,
    Wall,
    Column,
    Tower,
    /// Looks only: balcony rail, neon strip, sign, escalator (pitched).
    Rail,
    Neon,
    Sign,
    Escalator,
}

impl Kind {
    pub fn solid(self) -> bool {
        matches!(
            self,
            Kind::Slab | Kind::Glass | Kind::Wall | Kind::Column | Kind::Tower
        )
    }
}

/// One box of the complex (hall coordinates, axis-aligned except an
/// escalator's pitch about x).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub kind: Kind,
    pub centre: Vec3,
    pub size: Vec3,
    pub pitch: f32,
}

impl Part {
    fn new(kind: Kind, lo: Vec3, hi: Vec3) -> Self {
        Part {
            kind,
            centre: (lo + hi) / 2.0,
            size: hi - lo,
            pitch: 0.0,
        }
    }
    pub fn lo(&self) -> Vec3 {
        self.centre - self.size / 2.0
    }
    pub fn hi(&self) -> Vec3 {
        self.centre + self.size / 2.0
    }
}

/// Every ridge sample of the course, dense.
pub fn ridges(t: &Tuning) -> Vec<Vec3> {
    t.stages
        .iter()
        .flat_map(|s| s.ramps.iter().flat_map(|r| ridge(r, t.ramp_samples * 8)))
        .collect()
}

/// Is a box (lo, hi) in the tunnel around any ridge sample: within `side`
/// metres of it across the ground and reaching lower than `up` over it?
pub fn in_way(ridges: &[Vec3], lo: Vec3, hi: Vec3, side: f32, up: f32) -> bool {
    ridges.iter().any(|q| {
        let dx = (lo.x - q.x).max(q.x - hi.x).max(0.0);
        let dz = (lo.z - q.z).max(q.z - hi.z).max(0.0);
        dx * dx + dz * dz < side * side && lo.y < q.y + up
    })
}

/// Greedy merge of a grid of kept cells into rectangles (i0, j0, i1, j1),
/// exclusive ends.
fn rects(keep: &[Vec<bool>]) -> Vec<(usize, usize, usize, usize)> {
    let nj = keep.len();
    let ni = keep.first().map_or(0, Vec::len);
    let mut used = vec![vec![false; ni]; nj];
    let mut out = Vec::new();
    for j in 0..nj {
        for i in 0..ni {
            if !keep[j][i] || used[j][i] {
                continue;
            }
            let mut i1 = i;
            while i1 < ni && keep[j][i1] && !used[j][i1] {
                i1 += 1;
            }
            let mut j1 = j + 1;
            while j1 < nj && (i..i1).all(|k| keep[j1][k] && !used[j1][k]) {
                j1 += 1;
            }
            for row in used.iter_mut().take(j1).skip(j) {
                for u in row.iter_mut().take(i1).skip(i) {
                    *u = true;
                }
            }
            out.push((i, j, i1, j1));
        }
    }
    out
}

/// Where every box of the complex goes, and where its lights hang.
pub fn layout(t: &Tuning, d: &Tower) -> (Vec<Part>, Vec<Vec3>) {
    let all = ridges(t);
    let (min, max) = (Vec3::from(d.min), Vec3::from(d.max));
    let (side, up) = (d.clear_side, d.clear_up);
    let cell = d.cell.max(1.0);
    let ni = ((max.x - min.x) / cell).round().max(1.0) as usize;
    let nj = ((max.z - min.z) / cell).round().max(1.0) as usize;
    let nk = ((max.y - min.y) / d.storey.max(1.0)).round().max(1.0) as usize;
    let x = |i: usize| min.x + i as f32 * cell;
    let z = |j: usize| min.z + j as f32 * cell;
    let y = |k: usize| min.y + k as f32 * d.storey;
    let way = |lo: Vec3, hi: Vec3| in_way(&all, lo, hi, side, up);
    let mut parts = Vec::new();
    let mut edges = 0usize;

    // Floor slabs, storey by storey, carved around the tunnel.
    for k in 0..=nk {
        let (y0, y1) = (y(k) - d.slab, y(k));
        let keep: Vec<Vec<bool>> = (0..nj)
            .map(|j| {
                (0..ni)
                    .map(|i| !way(Vec3::new(x(i), y0, z(j)), Vec3::new(x(i + 1), y1, z(j + 1))))
                    .collect()
            })
            .collect();
        let open = |i: isize, j: isize| {
            i >= 0
                && j >= 0
                && (i as usize) < ni
                && (j as usize) < nj
                && !keep[j as usize][i as usize]
        };
        // Cells at the atrium's edge are glass; the rest concrete, merged.
        let mut solid = keep.clone();
        let mut glass = vec![vec![false; ni]; nj];
        for j in 0..nj {
            for i in 0..ni {
                if !keep[j][i] {
                    continue;
                }
                let (ii, jj) = (i as isize, j as isize);
                let sides = [(-1, 0), (1, 0), (0, -1), (0, 1)];
                if !sides.iter().any(|&(a, b)| open(ii + a, jj + b)) {
                    continue;
                }
                solid[j][i] = false;
                glass[j][i] = true;
                let (lo, hi) = (Vec3::new(x(i), y0, z(j)), Vec3::new(x(i + 1), y1, z(j + 1)));
                if k == 0 || k == nk {
                    continue;
                }
                // A balcony rail with a neon strip on each open side.
                for &(a, b) in &sides {
                    if !open(ii + a, jj + b) {
                        continue;
                    }
                    let t = 0.15;
                    let (rlo, rhi) = match (a, b) {
                        (-1, _) => (Vec3::new(lo.x, y1, lo.z), Vec3::new(lo.x + t, y1, hi.z)),
                        (1, _) => (Vec3::new(hi.x - t, y1, lo.z), Vec3::new(hi.x, y1, hi.z)),
                        (_, -1) => (Vec3::new(lo.x, y1, lo.z), Vec3::new(hi.x, y1, lo.z + t)),
                        _ => (Vec3::new(lo.x, y1, hi.z - t), Vec3::new(hi.x, y1, hi.z)),
                    };
                    parts.push(Part::new(Kind::Rail, rlo, rhi + Vec3::Y * d.rail));
                    parts.push(Part::new(
                        Kind::Neon,
                        rlo - Vec3::Y * (d.slab + 0.25),
                        rhi - Vec3::Y * d.slab,
                    ));
                    edges += 1;
                    let mid = (rlo + rhi) / 2.0;
                    if d.sign_every > 0 && edges.is_multiple_of(d.sign_every) {
                        // A sign standing on the balcony behind the rail.
                        let inward = Vec3::new(-a as f32, 0.0, -b as f32);
                        let along = Vec3::new(b.abs() as f32, 0.0, a.abs() as f32);
                        let c = mid + inward * 0.6 + Vec3::Y * (d.rail + 1.9);
                        let s = along * (cell * 0.7) + inward.abs() * 0.3 + Vec3::Y * 2.6;
                        parts.push(Part::new(Kind::Sign, c - s / 2.0, c + s / 2.0));
                    }
                    if d.escalator_every > 0
                        && edges.is_multiple_of(d.escalator_every)
                        && k + 1 < nk
                        && b != 0
                    {
                        // An escalator up to the next storey, along the
                        // balcony's cell (z), in its middle third.
                        let len = (cell * cell + d.storey * d.storey).sqrt();
                        let c = Vec3::new(
                            (lo.x + hi.x) / 2.0,
                            y1 + d.storey / 2.0,
                            (lo.z + hi.z) / 2.0,
                        );
                        parts.push(Part {
                            kind: Kind::Escalator,
                            centre: c,
                            size: Vec3::new(1.6, 0.5, len),
                            pitch: (d.storey / cell).atan(),
                        });
                    }
                }
            }
        }
        for (kind, grid) in [(Kind::Slab, &solid), (Kind::Glass, &glass)] {
            for (i0, j0, i1, j1) in rects(grid) {
                parts.push(Part::new(
                    kind,
                    Vec3::new(x(i0), y0, z(j0)),
                    Vec3::new(x(i1), y1, z(j1)),
                ));
            }
        }
    }
    // Facade walls, storey by storey, with the tunnel's openings.
    let w = d.wall;
    for k in 0..nk {
        let (y0, y1) = (y(k), y(k + 1) - d.slab);
        let faces: [(usize, Box<dyn Fn(usize) -> (Vec3, Vec3)>); 4] = [
            (
                ni,
                Box::new(|i| {
                    (
                        Vec3::new(x(i), y0, min.z),
                        Vec3::new(x(i + 1), y1, min.z + w),
                    )
                }),
            ),
            (
                ni,
                Box::new(|i| {
                    (
                        Vec3::new(x(i), y0, max.z - w),
                        Vec3::new(x(i + 1), y1, max.z),
                    )
                }),
            ),
            (
                nj,
                Box::new(|j| {
                    (
                        Vec3::new(min.x, y0, z(j)),
                        Vec3::new(min.x + w, y1, z(j + 1)),
                    )
                }),
            ),
            (
                nj,
                Box::new(|j| {
                    (
                        Vec3::new(max.x - w, y0, z(j)),
                        Vec3::new(max.x, y1, z(j + 1)),
                    )
                }),
            ),
        ];
        for (n, panel) in &faces {
            let keep: Vec<bool> = (0..*n)
                .map(|i| {
                    let (lo, hi) = panel(i);
                    !way(lo, hi)
                })
                .collect();
            for (i0, _, i1, _) in rects(std::slice::from_ref(&keep)) {
                let (lo, _) = panel(i0);
                let (_, hi) = panel(i1 - 1);
                parts.push(Part::new(Kind::Wall, lo, hi));
            }
            // The openings glow: a neon band along the top of each one.
            for (i, _) in keep.iter().enumerate().filter(|(_, k)| **k) {
                let (lo, hi) = panel(i);
                let h = d.storey;
                let below = k > 0 && way(lo - Vec3::Y * h, hi - Vec3::Y * h);
                if below {
                    parts.push(Part::new(Kind::Neon, lo, Vec3::new(hi.x, lo.y + 0.6, hi.z)));
                }
            }
        }
    }
    // Columns through every storey, where the tunnel isn't.
    let every = d.column_every.max(1);
    let c = d.column / 2.0;
    for j in (every..nj).step_by(every) {
        for i in (every..ni).step_by(every) {
            let (lo, hi) = (
                Vec3::new(x(i) - c, min.y, z(j) - c),
                Vec3::new(x(i) + c, max.y - d.slab, z(j) + c),
            );
            if !way(lo, hi) {
                parts.push(Part::new(Kind::Column, lo, hi));
            }
        }
    }
    // The towers on the roof.
    for &((cx, cz), (hx, hz), top) in &d.towers {
        let (lo, hi) = (
            Vec3::new(cx - hx, max.y, cz - hz),
            Vec3::new(cx + hx, top, cz + hz),
        );
        if !way(lo, hi) {
            parts.push(Part::new(Kind::Tower, lo, hi));
        }
    }
    // Lights over the ridge where it's inside.
    let mut lights = Vec::new();
    let mut since = d.light_every / 2.0;
    for w in all.windows(2) {
        let inside = |p: Vec3| p.cmpge(min).all() && p.cmple(max).all();
        if !inside(w[1]) || !inside(w[0]) {
            continue;
        }
        since += w[0].distance(w[1]);
        if since >= d.light_every {
            since = 0.0;
            lights.push(w[1] + Vec3::Y * d.light_up);
        }
    }
    (parts, lights)
}

/// The box the complex takes (hall coordinates: centre, half size),
/// towers included, for `surf_props::claim`.
pub fn bounds(d: &Tower) -> (Vec3, Vec3) {
    let (min, mut max) = (Vec3::from(d.min), Vec3::from(d.max));
    for &(_, _, top) in &d.towers {
        max.y = max.y.max(top);
    }
    ((min + max) / 2.0, (max - min) / 2.0)
}

/// The model's origin: the podium's bottom middle (hall coordinates).
pub fn origin(d: &Tower) -> Vec3 {
    let (min, max) = (Vec3::from(d.min), Vec3::from(d.max));
    Vec3::new((min.x + max.x) / 2.0, min.y, (min.z + max.z) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Tower>) {
        crate::features::surf_map::load_course_for_tests();
        for path in [crate::rules::surf::TUNING.path(), TOWER.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == TOWER.path() {
                TOWER.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse (every field is required)");
        }
        (tuning(), TOWER.get())
    }

    #[test]
    fn the_data_parses_and_the_ramp_runs_through_the_complex() {
        let (t, d) = load();
        let (min, max) = (Vec3::from(d.min), Vec3::from(d.max));
        let inside = ridges(&t)
            .into_iter()
            .filter(|p| p.cmpge(min).all() && p.cmple(max).all())
            .count();
        assert!(inside > 100, "only {inside} ridge samples inside");
        let (parts, lights) = layout(&t, &d);
        let n = |k: Kind| parts.iter().filter(|p| p.kind == k).count();
        assert!(n(Kind::Wall) >= 8 && n(Kind::Slab) >= 15 && n(Kind::Column) >= 4);
        assert_eq!(n(Kind::Tower), d.towers.len(), "every tower stands");
        assert!(n(Kind::Glass) > 0 && n(Kind::Rail) > 0 && n(Kind::Sign) > 0);
        assert!(n(Kind::Escalator) > 0);
        assert!(lights.len() >= 3, "{} lights", lights.len());
        // Away from the pads and the speed gaps.
        for s in &t.stages {
            let p = Vec3::from(s.pad.0);
            assert!(!(p.cmpge(min - 10.0).all() && p.cmple(max + 10.0).all()));
        }
        for &(stage, ramp) in &t.speed_gaps {
            let Some(g) = crate::rules::surf::gap(&t, stage, ramp) else {
                continue;
            };
            for p in [g.from, g.to] {
                assert!(
                    !(p.cmpge(min).all() && p.cmple(max).all()),
                    "speed gap inside"
                );
            }
        }
        assert_eq!(layout(&t, &d), (parts, lights));
    }

    #[test]
    fn the_tunnel_around_every_ridge_inside_is_free_of_solids() {
        let (t, d) = load();
        let (parts, _) = layout(&t, &d);
        let (min, max) = (Vec3::from(d.min), Vec3::from(d.max));
        // Faces reach `ramp_half_width`; keep 6 m past them and 8 m over the ridge.
        let side = t.ramp_half_width + 6.0;
        for q in ridges(&t) {
            let near = q.cmpge(min - side).all() && q.cmple(max + side).all();
            if !near {
                continue;
            }
            for p in parts.iter().filter(|p| p.kind.solid()) {
                assert!(
                    !in_way(&[q], p.lo(), p.hi(), side, 8.0),
                    "{:?} at {} blocks the ridge at {q}",
                    p.kind,
                    p.centre
                );
            }
        }
        // The hanging ring too, and the looks-only things (not escalators,
        // whose pitched box is checked by its upright bounds).
        let ((sx, sy, sz), r, tube) = d.sculpture;
        let c = Vec3::new(sx, sy, sz);
        let e = Vec3::new(r + tube, r + tube, tube);
        let all = ridges(&t);
        assert!(!in_way(&all, c - e, c + e, side, 8.0), "ring in the way");
        for p in &parts {
            let h = if p.kind == Kind::Escalator {
                Vec3::new(p.size.x, d.storey, d.cell) / 2.0
            } else {
                p.size / 2.0
            };
            assert!(
                !in_way(&all, p.centre - h, p.centre + h, side, 8.0),
                "{:?} at {} in the way",
                p.kind,
                p.centre
            );
        }
    }

    #[test]
    fn a_box_beside_or_over_the_ridge_is_clear_and_one_on_it_is_not() {
        let q = [Vec3::ZERO];
        let b = |c: Vec3| (c - Vec3::ONE, c + Vec3::ONE);
        let (lo, hi) = b(Vec3::new(0.0, -5.0, 0.0));
        assert!(in_way(&q, lo, hi, 12.0, 8.0));
        let (lo, hi) = b(Vec3::new(20.0, -5.0, 0.0));
        assert!(!in_way(&q, lo, hi, 12.0, 8.0));
        let (lo, hi) = b(Vec3::new(0.0, 10.0, 0.0));
        assert!(!in_way(&q, lo, hi, 12.0, 8.0));
        let (lo, hi) = b(Vec3::new(0.0, -50.0, 0.0));
        assert!(
            in_way(&q, lo, hi, 12.0, 8.0),
            "the tunnel goes all the way down"
        );
    }

    /// `SURF_TOWER_DUMP=<file>` writes the layout (model space) as JSON for
    /// building the modeler prefabs.
    #[test]
    fn dump_for_the_modeler() {
        let Ok(out) = std::env::var("SURF_TOWER_DUMP") else {
            return;
        };
        let (t, d) = load();
        let (parts, lights) = layout(&t, &d);
        let o = origin(&d);
        let mut s = String::from("{\"parts\":[");
        for (i, p) in parts.iter().enumerate() {
            let c = p.centre - o;
            s += &format!(
                "{}{{\"kind\":\"{:?}\",\"c\":[{},{},{}],\"s\":[{},{},{}],\"pitch\":{}}}",
                if i > 0 { "," } else { "" },
                p.kind,
                c.x,
                c.y,
                c.z,
                p.size.x,
                p.size.y,
                p.size.z,
                p.pitch
            );
        }
        let ((sx, sy, sz), r, tube) = d.sculpture;
        let sc = Vec3::new(sx, sy, sz) - o;
        s += &format!(
            "],\"ring\":[{},{},{},{r},{tube}],\"lights\":{}}}",
            sc.x,
            sc.y,
            sc.z,
            lights.len()
        );
        assert!(std::fs::write(out, s).is_ok());
    }
}
