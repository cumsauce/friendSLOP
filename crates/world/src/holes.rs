//! Holes in the ground: openings in the terrain or a flat floor (New York's
//! street) that stairs, shafts and pits go down through.
//!
//! A hole is a convex outline in x/z (a rectangle, a convex polygon or a
//! circle) at a spot, turned by a yaw. Declare one in code with [`cut`]
//! (from a protocol function, like `footprints::claim`), with [`provide`]
//! when it comes from a data file that is read later, or as a `Hole` marker
//! on a modeler map (its box is the rectangle, `round` makes it a circle).
//!
//! What a hole does:
//! - the terrain's mesh and collider leave it open ([`crate::terrain`]),
//!   and so do the meshes of any entity tree marked [`Cut`] (a map's floor
//!   glTF), near the hole's `top`;
//! - [`slab`] splits a flat floor (a big box collider) into pieces around it;
//! - a lining ([`lining_mesh`]): walls down its sides `depth` metres, so you
//!   don't see into the void under the ground.
//!
//! The clipping is exact (triangles are cut along the hole's edges), so the
//! edges are clean and nothing else moves. Every peer cuts the same holes.

use std::sync::RwLock;

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use crate::AppState;

/// A hole's outline, in its own frame (before its yaw).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A rectangle: half its size (x, z).
    Rect(Vec2),
    /// A circle of this radius (cut as a 24-sided polygon).
    Circle(f32),
    /// A convex polygon, counter-clockwise seen from above.
    Polygon(Vec<Vec2>),
}

/// An opening in the ground.
#[derive(Clone, Debug, PartialEq)]
pub struct Hole {
    /// Its middle (x, z).
    pub at: Vec2,
    /// Turn about +y, radians.
    pub yaw: f32,
    pub shape: Shape,
    /// The floor it cuts: `None` is the terrain, `Some(y)` a flat floor at y
    /// (meshes marked [`Cut`] and [`slab`]s at that height).
    pub top: Option<f32>,
    /// How far its lining goes down (m); 0 for none.
    pub depth: f32,
    /// How far above `top` the lining starts (a kerb's thickness).
    pub lip: f32,
}

const CIRCLE_SIDES: usize = 24;

impl Hole {
    /// A rectangle `size` (x, z) across at `at`, turned `yaw` radians.
    pub fn rect(at: Vec2, size: Vec2, yaw: f32) -> Self {
        Hole {
            at,
            yaw,
            shape: Shape::Rect(size / 2.0),
            top: None,
            depth: 0.0,
            lip: 0.0,
        }
    }

    /// A circle of `radius` at `at`.
    pub fn circle(at: Vec2, radius: f32) -> Self {
        Hole {
            shape: Shape::Circle(radius),
            ..Hole::rect(at, Vec2::ZERO, 0.0)
        }
    }

    /// On a flat floor at `y` instead of the terrain.
    pub fn on_floor(mut self, y: f32) -> Self {
        self.top = Some(y);
        self
    }

    /// Lined `depth` metres down, starting `lip` above the floor.
    pub fn lined(mut self, depth: f32, lip: f32) -> Self {
        self.depth = depth;
        self.lip = lip;
        self
    }

    /// The outline in world x/z, counter-clockwise seen from above (+y).
    pub fn outline(&self) -> Vec<Vec2> {
        let local: Vec<Vec2> = match &self.shape {
            Shape::Rect(h) => vec![
                Vec2::new(-h.x, -h.y),
                Vec2::new(-h.x, h.y),
                Vec2::new(h.x, h.y),
                Vec2::new(h.x, -h.y),
            ],
            Shape::Circle(r) => (0..CIRCLE_SIDES)
                .map(|k| {
                    let a = -(k as f32) * std::f32::consts::TAU / CIRCLE_SIDES as f32;
                    Vec2::new(a.cos(), a.sin()) * *r
                })
                .collect(),
            Shape::Polygon(p) => p.clone(),
        };
        // A turn about +y by yaw takes local (x, z) to world (x, z).
        let (s, c) = self.yaw.sin_cos();
        let mut out: Vec<Vec2> = local
            .iter()
            .map(|p| self.at + Vec2::new(p.x * c + p.y * s, -p.x * s + p.y * c))
            .collect();
        // Whichever way it was given, keep it counter-clockwise from above:
        // in x/z seen from +y that's a negative signed area.
        if area(&out) > 0.0 {
            out.reverse();
        }
        out
    }

    /// Is (x, z) inside the hole? (A circle is its 24-sided outline, as cut.)
    pub fn contains(&self, p: Vec2) -> bool {
        let o = self.outline();
        (0..o.len()).all(|i| inside_edge(o[i], o[(i + 1) % o.len()], p) > 0.0)
    }

    /// Its outline's bounds (min, max).
    pub fn bounds(&self) -> (Vec2, Vec2) {
        let o = self.outline();
        let min = o.iter().fold(Vec2::splat(f32::MAX), |a, b| a.min(*b));
        let max = o.iter().fold(Vec2::splat(f32::MIN), |a, b| a.max(*b));
        (min, max)
    }
}

/// Twice the signed area of an x/z polygon (shoelace).
fn area(p: &[Vec2]) -> f32 {
    (0..p.len())
        .map(|i| p[i].perp_dot(p[(i + 1) % p.len()]))
        .sum()
}

/// Positive when `p` is on the hole's side of the outline edge a → b
/// (counter-clockwise from above, so the inside is to its right in x/z).
fn inside_edge(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    -(b - a).perp_dot(p - a)
}

static HOLES: RwLock<Vec<Hole>> = RwLock::new(Vec::new());
type Provider = fn() -> Vec<Hole>;
static PROVIDERS: RwLock<Vec<Provider>> = RwLock::new(Vec::new());

/// Cut a hole in the ground (once; cutting the same hole again does nothing).
pub fn cut(hole: Hole) {
    if let Ok(mut h) = HOLES.write()
        && !h.contains(&hole)
    {
        h.push(hole);
    }
}

/// Cut the holes `f` returns, asked each time the holes are needed (for
/// holes whose places are in a data file read after the protocol runs).
pub fn provide(f: Provider) {
    if let Ok(mut p) = PROVIDERS.write()
        && !p.contains(&f)
    {
        p.push(f);
    }
}

/// Every hole: cut, provided and the commune map's `Hole` markers.
pub fn all() -> Vec<Hole> {
    let mut out = HOLES.read().map(|h| h.clone()).unwrap_or_default();
    let providers = PROVIDERS.read().map(|p| p.clone()).unwrap_or_default();
    for f in providers {
        out.extend(f());
    }
    out.extend(crate::places::layout().holes.iter().cloned());
    out
}

/// The holes in the terrain.
pub fn in_terrain() -> Vec<Hole> {
    all().into_iter().filter(|h| h.top.is_none()).collect()
}

/// The holes in a flat floor at `y`.
pub fn in_floor(y: f32) -> Vec<Hole> {
    all()
        .into_iter()
        .filter(|h| h.top.is_some_and(|t| (t - y).abs() < 0.5))
        .collect()
}

/// Is (x, z) inside any of `holes`?
pub fn inside(holes: &[Hole], p: Vec2) -> bool {
    holes.iter().any(|h| h.contains(p))
}

/// A map's `Hole` markers: the marker's box (x by z) turned by its yaw, or a
/// circle its box's width across (`round`), lined `depth` down. `flat`: the
/// map has its own flat floor at the markers' heights (New York) instead of
/// the terrain.
pub fn from_map(map: &[crate::places::MapEntity], flat: bool) -> Vec<Hole> {
    map.iter()
        .filter(|e| e.has("Hole"))
        .map(|e| {
            let size = e.footprint();
            let base = if e.flag("Hole", "round") {
                Hole::circle(e.at(), size.x / 2.0)
            } else {
                Hole::rect(e.at(), size, e.yaw.to_radians())
            };
            let base = if flat {
                base.on_floor(e.translation[1])
            } else {
                base
            };
            base.lined(e.num("Hole", "depth"), e.num("Hole", "lip"))
        })
        .collect()
}

// ------------------------------------------------------------------ clipping

/// A polygon corner: x/z for the clipping and its weights on the three
/// corners of the triangle it came from (to interpolate everything else).
#[derive(Clone, Copy, Debug)]
struct Corner {
    xz: Vec2,
    w: Vec3,
}

/// The part of convex `poly` inside (or outside) the hole edge a → b.
fn clip(poly: &[Corner], a: Vec2, b: Vec2, keep_inside: bool) -> Vec<Corner> {
    let s = |p: Vec2| {
        let d = inside_edge(a, b, p);
        if keep_inside { d } else { -d }
    };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (s(p.xz), s(q.xz));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push(Corner {
                xz: p.xz.lerp(q.xz, t),
                w: p.w.lerp(q.w, t),
            });
        }
    }
    out
}

/// Convex `poly` minus convex `outline` (counter-clockwise): convex pieces.
fn subtract(poly: Vec<Corner>, outline: &[Vec2]) -> Vec<Vec<Corner>> {
    let mut left = poly;
    let mut pieces = Vec::new();
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        let outside = clip(&left, a, b, false);
        if outside.len() >= 3 && area_of(&outside) > 1e-7 {
            pieces.push(outside);
        }
        left = clip(&left, a, b, true);
        if left.len() < 3 {
            break;
        }
    }
    pieces
}

fn area_of(p: &[Corner]) -> f32 {
    let xz: Vec<Vec2> = p.iter().map(|c| c.xz).collect();
    area(&xz).abs() / 2.0
}

fn overlaps((amin, amax): (Vec2, Vec2), (bmin, bmax): (Vec2, Vec2)) -> bool {
    amin.x < bmax.x && bmin.x < amax.x && amin.y < bmax.y && bmin.y < amax.y
}

/// A triangle with corners at x/z `t`, minus `holes`: `None` if no hole
/// touches it, else the pieces left (each a fan of weights on its corners).
fn cut_triangle(t: [Vec2; 3], holes: &[(Vec<Vec2>, (Vec2, Vec2))]) -> Option<Vec<[Vec3; 3]>> {
    let bounds = (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]));
    let touching: Vec<_> = holes.iter().filter(|h| overlaps(bounds, h.1)).collect();
    if touching.is_empty() {
        return None;
    }
    let mut pieces = vec![vec![
        Corner {
            xz: t[0],
            w: Vec3::X,
        },
        Corner {
            xz: t[1],
            w: Vec3::Y,
        },
        Corner {
            xz: t[2],
            w: Vec3::Z,
        },
    ]];
    for (outline, _) in touching {
        pieces = pieces
            .into_iter()
            .flat_map(|p| subtract(p, outline))
            .collect();
    }
    Some(
        pieces
            .iter()
            .flat_map(|p| {
                (1..p.len() - 1)
                    // Corners on a hole's edge can leave slivers: drop them.
                    .filter(|&k| (p[k].xz - p[0].xz).perp_dot(p[k + 1].xz - p[0].xz).abs() > 1e-9)
                    .map(move |k| [p[0].w, p[k].w, p[k + 1].w])
            })
            .collect(),
    )
}

fn prepared(holes: &[Hole]) -> Vec<(Vec<Vec2>, (Vec2, Vec2))> {
    holes.iter().map(|h| (h.outline(), h.bounds())).collect()
}

/// Cut `holes` out of an indexed triangle list whose positions are `pos`
/// (world x/z for the cutting is `world(p)`). Untouched triangles keep their
/// indices; cut ones are replaced by new corners, given as weights on an
/// old triangle's three vertices. `eligible` picks the triangles to cut.
/// Returns the new indices and the new corners (old vertex indices and
/// weights), or `None` when no triangle was cut.
#[allow(clippy::type_complexity)]
fn cut_indexed(
    idx: &[u32],
    world: &[Vec3],
    holes: &[Hole],
    eligible: impl Fn([Vec3; 3]) -> bool,
) -> Option<(Vec<u32>, Vec<([u32; 3], Vec3)>)> {
    let holes = prepared(holes);
    if holes.is_empty() {
        return None;
    }
    let base = world.len() as u32;
    let mut out = Vec::with_capacity(idx.len());
    let mut extra: Vec<([u32; 3], Vec3)> = Vec::new();
    let mut changed = false;
    for tri in idx.as_chunks::<3>().0.iter() {
        let v = [tri[0], tri[1], tri[2]];
        let Some(p) = v
            .iter()
            .map(|&i| world.get(i as usize).copied())
            .collect::<Option<Vec<_>>>()
        else {
            out.extend_from_slice(tri);
            continue;
        };
        let p = [p[0], p[1], p[2]];
        let cut = if eligible(p) {
            cut_triangle([p[0].xz(), p[1].xz(), p[2].xz()], &holes)
        } else {
            None
        };
        match cut {
            None => out.extend_from_slice(tri),
            Some(fans) => {
                changed = true;
                for f in fans {
                    for w in f {
                        out.push(base + extra.len() as u32);
                        extra.push((v, w));
                    }
                }
            }
        }
    }
    changed.then_some((out, extra))
}

fn lerp_attr(
    vals: &VertexAttributeValues,
    extra: &[([u32; 3], Vec3)],
) -> Option<VertexAttributeValues> {
    fn mix<const N: usize>(src: &[[f32; N]], extra: &[([u32; 3], Vec3)]) -> Option<Vec<[f32; N]>> {
        let mut out = src.to_vec();
        for (v, w) in extra {
            let mut c = [0.0; N];
            for (k, wk) in [w.x, w.y, w.z].into_iter().enumerate() {
                let s = src.get(v[k] as usize)?;
                for n in 0..N {
                    c[n] += s[n] * wk;
                }
            }
            out.push(c);
        }
        Some(out)
    }
    Some(match vals {
        VertexAttributeValues::Float32x2(s) => VertexAttributeValues::Float32x2(mix(s, extra)?),
        VertexAttributeValues::Float32x3(s) => VertexAttributeValues::Float32x3(mix(s, extra)?),
        VertexAttributeValues::Float32x4(s) => VertexAttributeValues::Float32x4(mix(s, extra)?),
        _ => return None,
    })
}

/// `mesh` with `holes` cut out of it, where `to_world` places it; only
/// triangles `eligible` (given world corners) are cut. `None` when nothing
/// was cut (or the mesh isn't a plain float triangle list).
pub fn cut_mesh(
    mesh: &Mesh,
    to_world: impl Fn(Vec3) -> Vec3,
    holes: &[Hole],
    eligible: impl Fn([Vec3; 3]) -> bool,
) -> Option<Mesh> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };
    let idx: Vec<u32> = match mesh.indices() {
        Some(i) => i.iter().map(|i| i as u32).collect(),
        None => (0..pos.len() as u32).collect(),
    };
    let world: Vec<Vec3> = pos.iter().map(|p| to_world(Vec3::from_array(*p))).collect();
    let (new_idx, extra) = cut_indexed(&idx, &world, holes, eligible)?;
    let mut out = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    for (attr, vals) in mesh.attributes() {
        if let Some(v) = lerp_attr(vals, &extra) {
            out.insert_attribute(*attr, v);
        }
    }
    Some(out.with_inserted_indices(Indices::U32(new_idx)))
}

/// Positions and indices (a triangle list) with `holes` cut out; every
/// triangle is eligible (the terrain's mesh and collider use this).
pub fn cut_triangles(pos: &[Vec3], idx: &[u32], holes: &[Hole]) -> Option<(Vec<Vec3>, Vec<u32>)> {
    let (new_idx, extra) = cut_indexed(idx, pos, holes, |_| true)?;
    let mut out = pos.to_vec();
    out.extend(extra.iter().map(|(v, w)| {
        pos[v[0] as usize] * w.x + pos[v[1] as usize] * w.y + pos[v[2] as usize] * w.z
    }));
    Some((out, new_idx))
}

// ------------------------------------------------------------------ slabs

/// A flat floor rectangle (min, max in x/z) split into convex pieces that
/// cover all of it except `holes` (outlines, counter-clockwise from above).
pub fn slab(min: Vec2, max: Vec2, holes: &[Hole]) -> Vec<Vec<Vec2>> {
    let holes: Vec<_> = prepared(holes)
        .into_iter()
        .filter(|h| overlaps((min, max), h.1))
        .collect();
    // Cut lines at every hole's bounds; cells clear of every hole's bounds
    // stay whole boxes, the rest are clipped exactly.
    let mut xs = vec![min.x, max.x];
    let mut zs = vec![min.y, max.y];
    for (_, (lo, hi)) in &holes {
        xs.extend([lo.x.clamp(min.x, max.x), hi.x.clamp(min.x, max.x)]);
        zs.extend([lo.y.clamp(min.y, max.y), hi.y.clamp(min.y, max.y)]);
    }
    let sorted = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        v
    };
    let (xs, zs) = (sorted(xs), sorted(zs));
    let mut out = Vec::new();
    for j in 0..zs.len() - 1 {
        for i in 0..xs.len() - 1 {
            let (lo, hi) = (Vec2::new(xs[i], zs[j]), Vec2::new(xs[i + 1], zs[j + 1]));
            let corners = [lo, Vec2::new(lo.x, hi.y), hi, Vec2::new(hi.x, lo.y)];
            let touching: Vec<_> = holes.iter().filter(|h| overlaps((lo, hi), h.1)).collect();
            if touching.is_empty() {
                out.push(corners.to_vec());
                continue;
            }
            let mut pieces = vec![
                corners
                    .iter()
                    .map(|&xz| Corner { xz, w: Vec3::ZERO })
                    .collect::<Vec<_>>(),
            ];
            for (outline, _) in touching {
                pieces = pieces
                    .into_iter()
                    .flat_map(|p| subtract(p, outline))
                    .collect();
            }
            out.extend(pieces.into_iter().map(|p| p.iter().map(|c| c.xz).collect()));
        }
    }
    out
}

/// Colliders for a flat floor `thickness` thick with its top at `top`, over
/// (min, max), with `holes` left open: boxes where it's whole, convex
/// prisms around the holes.
pub fn slab_colliders(
    min: Vec2,
    max: Vec2,
    top: f32,
    thickness: f32,
    holes: &[Hole],
) -> Vec<(Transform, Collider)> {
    slab(min, max, holes)
        .into_iter()
        .filter_map(|p| {
            let lo = p.iter().fold(Vec2::splat(f32::MAX), |a, b| a.min(*b));
            let hi = p.iter().fold(Vec2::splat(f32::MIN), |a, b| a.max(*b));
            let mid = (lo + hi) / 2.0;
            let boxy = p.len() == 4
                && p.iter().all(|c| {
                    ((c.x - lo.x).abs() < 1e-4 || (c.x - hi.x).abs() < 1e-4)
                        && ((c.y - lo.y).abs() < 1e-4 || (c.y - hi.y).abs() < 1e-4)
                });
            let at = Transform::from_xyz(mid.x, top - thickness / 2.0, mid.y);
            if boxy {
                let s = hi - lo;
                return Some((at, Collider::cuboid(s.x, thickness, s.y)));
            }
            let points: Vec<Vec3> = p
                .iter()
                .flat_map(|c| {
                    let c = *c - mid;
                    [
                        Vec3::new(c.x, thickness / 2.0, c.y),
                        Vec3::new(c.x, -thickness / 2.0, c.y),
                    ]
                })
                .collect();
            Collider::convex_hull(&points).map(|c| (at, c))
        })
        .collect()
}

// ------------------------------------------------------------------ lining

/// Walls down a hole's sides, from `lip` above its floor to `depth` below
/// (the floor's height at each corner is `ground(x, z)`), facing inwards.
pub fn lining_mesh(hole: &Hole, ground: impl Fn(Vec2) -> f32) -> Mesh {
    let o = hole.outline();
    let mut pos = Vec::new();
    let mut nrm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();
    let mut run = 0.0;
    for i in 0..o.len() {
        let (a, b) = (o[i], o[(i + 1) % o.len()]);
        let (ta, tb) = (ground(a) + hole.lip, ground(b) + hole.lip);
        let (ba, bb) = (ta - hole.lip - hole.depth, tb - hole.lip - hole.depth);
        let edge = b - a;
        // Inwards: to the edge's right in x/z (the outline is counter-clockwise from above).
        let inward = Vec2::new(-edge.y, edge.x).normalize_or_zero() * -1.0;
        let n = [inward.x, 0.0, inward.y];
        let k = pos.len() as u32;
        pos.extend([
            [a.x, ta, a.y],
            [b.x, tb, b.y],
            [a.x, ba, a.y],
            [b.x, bb, b.y],
        ]);
        nrm.extend([n; 4]);
        let len = edge.length();
        uv.extend([
            [run, 0.0],
            [run + len, 0.0],
            [run, hole.depth + hole.lip],
            [run + len, hole.depth + hole.lip],
        ]);
        run += len;
        // Wind so the inside face is the front.
        let tri = [k, k + 1, k + 2, k + 1, k + 3, k + 2];
        let p = |i: u32| Vec3::from_array(pos[i as usize]);
        let face = (p(k + 1) - p(k)).cross(p(k + 2) - p(k));
        if face.dot(Vec3::from_array(n)) >= 0.0 {
            idx.extend(tri);
        } else {
            idx.extend([k, k + 2, k + 1, k + 1, k + 2, k + 3]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(idx))
}

// ------------------------------------------------------------------ plugin

/// Put on an entity tree (a map with its own floor) whose meshes should have
/// the holes in that floor cut out of them as they load.
#[derive(Component, Clone, Copy, Debug)]
pub struct Cut {
    /// The floor's height: holes on it are cut from triangles within a
    /// metre below and half a metre above it.
    pub floor: f32,
}

#[derive(Component)]
struct Checked;

pub struct HolesPlugin;

impl Plugin for HolesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_linings)
            .add_systems(
                PostUpdate,
                cut_loaded_meshes.after(bevy::transform::TransformSystems::Propagate),
            );
    }
}

fn spawn_linings(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let holes: Vec<Hole> = all().into_iter().filter(|h| h.depth > 0.0).collect();
    if holes.is_empty() {
        return;
    }
    let wall = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.21, 0.2),
        perceptual_roughness: 0.95,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    for h in holes {
        let top = h.top;
        let mesh = lining_mesh(&h, |p| {
            top.unwrap_or_else(|| crate::terrain::height_at(p.x, p.y))
        });
        commands.spawn((
            Name::new("Hole lining"),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(wall.clone()),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Cut the floor's holes out of meshes under a [`Cut`] as they appear.
fn cut_loaded_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut added: Query<(Entity, &mut Mesh3d, &GlobalTransform), Without<Checked>>,
    parents: Query<&ChildOf>,
    cuts: Query<&Cut>,
) {
    let mut holes: Option<Vec<Hole>> = None;
    for (e, mut mesh, at) in &mut added {
        let Some(cut) = parents.iter_ancestors(e).find_map(|a| cuts.get(a).ok()) else {
            commands.entity(e).insert(Checked);
            continue;
        };
        let Some(src) = meshes.get(&mesh.0) else {
            continue; // not loaded yet: try again next frame
        };
        commands.entity(e).insert(Checked);
        let holes = holes.get_or_insert_with(|| in_floor(cut.floor));
        let y = cut.floor;
        let affine = at.affine();
        let eligible = |t: [Vec3; 3]| t.iter().all(|p| p.y > y - 1.0 && p.y < y + 0.5);
        if let Some(m) = cut_mesh(src, |p| affine.transform_point3(p), holes, eligible) {
            mesh.0 = meshes.add(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: u32, size: f32) -> (Vec<Vec3>, Vec<u32>) {
        let step = size / n as f32;
        let mut pos = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                pos.push(Vec3::new(
                    i as f32 * step - size / 2.0,
                    0.0,
                    j as f32 * step - size / 2.0,
                ));
            }
        }
        let mut idx = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let a = j * (n + 1) + i;
                idx.extend([a, a + n + 1, a + 1, a + 1, a + n + 1, a + n + 2]);
            }
        }
        (pos, idx)
    }

    fn area_xz(pos: &[Vec3], idx: &[u32]) -> f32 {
        idx.as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| pos[t[k] as usize].xz());
                (b - a).perp_dot(c - a).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn points_in_holes() {
        let r = Hole::rect(
            Vec2::new(10.0, 5.0),
            Vec2::new(2.0, 6.0),
            90f32.to_radians(),
        );
        // Turned a quarter, it's 6 m along x and 2 m along z.
        assert!(r.contains(Vec2::new(12.9, 5.0)));
        assert!(!r.contains(Vec2::new(10.0, 6.5)));
        assert!(!r.contains(Vec2::new(13.1, 5.0)));
        let c = Hole::circle(Vec2::ZERO, 1.0);
        assert!(c.contains(Vec2::new(0.5, 0.5)));
        assert!(!c.contains(Vec2::new(0.8, 0.8)));
        let p = Hole {
            shape: Shape::Polygon(vec![Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(0.0, 2.0)]),
            ..Hole::rect(Vec2::ZERO, Vec2::ZERO, 0.0)
        };
        assert!(p.contains(Vec2::new(0.5, 0.5)));
        assert!(!p.contains(Vec2::new(1.5, 1.5)));
    }

    #[test]
    fn the_mesh_has_no_triangles_inside_a_hole() {
        let (pos, idx) = grid(10, 20.0);
        let holes = [
            Hole::rect(Vec2::new(1.3, -2.1), Vec2::new(2.2, 6.2), 0.4),
            Hole::circle(Vec2::new(-5.0, 5.0), 2.0),
        ];
        let Some((p, i)) = cut_triangles(&pos, &idx, &holes) else {
            panic!("nothing cut");
        };
        for t in i.as_chunks::<3>().0.iter() {
            let mid = (p[t[0] as usize] + p[t[1] as usize] + p[t[2] as usize]) / 3.0;
            assert!(!inside(&holes, mid.xz()), "a triangle in a hole at {mid}");
            // Winding kept: every triangle still faces up.
            let [a, b, c] = [0, 1, 2].map(|k| p[t[k] as usize]);
            assert!((b - a).cross(c - a).y > 0.0);
        }
        // Exactly the holes' area is gone (the circle is a 24-gon).
        let gone = 400.0 - area_xz(&p, &i);
        let want = 2.2 * 6.2 + 0.5 * 24.0 * 4.0 * (std::f32::consts::TAU / 24.0).sin();
        assert!((gone - want).abs() < 0.01, "{gone} vs {want}");
        // No hole near: untouched.
        assert!(cut_triangles(&pos, &idx, &[Hole::circle(Vec2::splat(50.0), 1.0)]).is_none());
    }

    #[test]
    fn the_floor_split_covers_everything_but_the_holes() {
        let (min, max) = (Vec2::new(-90.0, 116.0), Vec2::new(90.0, 330.0));
        let holes = [
            Hole::rect(
                Vec2::new(-14.0, 228.0),
                Vec2::new(2.6, 6.2),
                90f32.to_radians(),
            ),
            Hole::rect(Vec2::new(20.0, 200.0), Vec2::new(3.0, 3.0), 0.6),
            Hole::circle(Vec2::new(21.0, 201.0), 2.0),
        ];
        let pieces = slab(min, max, &holes);
        let in_piece = |q: Vec2| {
            pieces
                .iter()
                .any(|p| (0..p.len()).all(|i| inside_edge(p[i], p[(i + 1) % p.len()], q) >= -1e-4))
        };
        let mut k = 0u32;
        for j in 0..400 {
            for i in 0..300 {
                // A fine grid, denser round the holes.
                let q = if k.is_multiple_of(2) {
                    Vec2::new(
                        -90.0 + i as f32 * 0.6 + 0.013,
                        116.0 + j as f32 * 0.535 + 0.017,
                    )
                } else {
                    Vec2::new(
                        -20.0 + i as f32 * 0.15 + 0.011,
                        195.0 + j as f32 * 0.09 + 0.007,
                    )
                };
                k += 1;
                assert_eq!(in_piece(q), !inside(&holes, q), "at {q}");
            }
        }
        // Areas add up too.
        let total: f32 = pieces.iter().map(|p| area(p).abs() / 2.0).sum();
        assert!(total < 180.0 * 214.0 && total > 180.0 * 214.0 - 40.0);
        // And the colliders: one per piece.
        assert_eq!(
            slab_colliders(min, max, 0.0, 1.0, &holes).len(),
            pieces.len()
        );
        // No holes: one box.
        assert_eq!(slab(min, max, &[]).len(), 1);
    }

    #[test]
    fn a_lining_goes_down_every_side() {
        let h = Hole::rect(Vec2::ZERO, Vec2::new(2.0, 4.0), 0.3).lined(3.0, 0.15);
        let m = lining_mesh(&h, |_| 0.0);
        assert_eq!(m.count_vertices(), 16);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let ys: Vec<f32> = p.iter().map(|p| p[1]).collect();
        assert!(
            ys.iter()
                .all(|y| (*y - 0.15).abs() < 1e-5 || (*y + 3.0).abs() < 1e-5)
        );
    }

    #[test]
    fn map_markers_make_holes() {
        let e: crate::places::MapEntity = serde_json::from_str(
            r#"{"name":"h","translation":[3,0.5,4],"yaw":90,"size":[2,1,6],
                "components":{"Hole":{"depth":3,"lip":0.1}}}"#,
        )
        .unwrap_or_default();
        let h = from_map(std::slice::from_ref(&e), true);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].top, Some(0.5));
        assert_eq!(h[0].depth, 3.0);
        assert!(h[0].contains(Vec2::new(5.9, 4.0)));
        assert_eq!(from_map(&[e], false)[0].top, None);
    }
}
