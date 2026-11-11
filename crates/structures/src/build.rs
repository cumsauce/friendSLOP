//! Turning kit pieces into meshes and colliders.
//!
//! [`batches`] merges every drawn piece into one mesh per (group, material,
//! small). UVs are in metres, measured in world space on each face's own
//! plane: horizontal faces use world x/z, every other face runs u along the
//! level line of its plane and v down its slope (so roofs read along the
//! slope and walls along their length). Neighbouring pieces in one plane
//! therefore continue one texture. Tangents are generated for normal maps.
//!
//! [`bodies`] turns solid pieces into static colliders: one box per solid
//! box (boxes that line up and touch are merged), a cylinder per cylinder,
//! a convex hull per gable.

use std::f32::consts::TAU;

use avian3d::prelude::Collider;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::{Palette, Piece, PropMark, Shape, Solid};

/// One merged mesh. Its vertices are relative to `origin`.
pub struct Batch<M> {
    pub group: u32,
    pub mat: M,
    pub small: bool,
    pub origin: Vec3,
    pub mesh: Mesh,
}

/// A static collider in world space.
pub struct Body {
    pub centre: Vec3,
    pub rot: Quat,
    pub collider: Collider,
    pub solid: Solid,
}

/// The pieces still to draw from boxes when the props `has_model` names
/// are drawn from models instead (their pieces still collide: pass every
/// piece to [`bodies`]).
pub fn drawn<M: Copy>(
    pieces: &[Piece<M>],
    props: &[PropMark],
    has_model: impl Fn(&str) -> bool,
) -> Vec<Piece<M>> {
    pieces
        .iter()
        .filter(|p| {
            p.prop
                .and_then(|i| props.get(i as usize))
                .is_none_or(|m| !has_model(m.name))
        })
        .copied()
        .collect()
}

/// World-space UV (metres) of a point on a face with normal `n`.
pub fn face_uv(p: Vec3, n: Vec3) -> [f32; 2] {
    if n.y.abs() > 0.99 {
        return [p.x, p.z];
    }
    let t = Vec3::Y.cross(n).normalize();
    let b = n.cross(t);
    [p.dot(t), -p.dot(b)]
}

#[derive(Default)]
struct Builder {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl Builder {
    fn vertex(&mut self, p: Vec3, n: Vec3, uv: [f32; 2]) -> u32 {
        self.pos.push(p.to_array());
        self.nrm.push(n.to_array());
        self.uv.push(uv);
        (self.pos.len() - 1) as u32
    }

    /// A flat convex polygon (world space), facing away from `inside`.
    fn face(&mut self, pts: &[Vec3], inside: Vec3) {
        let mut n = Vec3::ZERO;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            n += a.cross(b);
        }
        let Some(mut n) = n.try_normalize() else {
            return;
        };
        let centroid = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        let flip = n.dot(centroid - inside) < 0.0;
        if flip {
            n = -n;
        }
        let first = self.pos.len() as u32;
        for p in pts {
            self.vertex(*p, n, face_uv(*p, n));
        }
        for i in 1..pts.len() as u32 - 1 {
            let (b, c) = if flip { (i + 1, i) } else { (i, i + 1) };
            self.idx.extend([first, first + b, first + c]);
        }
    }

    fn cube(&mut self, p: &Piece<impl Copy>) {
        let h = p.size / 2.0;
        let w = |x: f32, y: f32, z: f32| p.centre + p.rot * (h * Vec3::new(x, y, z));
        for axis in 0..3 {
            for s in [-1.0, 1.0] {
                let corner = |a: f32, b: f32| {
                    let mut v = [0.0; 3];
                    v[axis] = s;
                    v[(axis + 1) % 3] = a;
                    v[(axis + 2) % 3] = b;
                    w(v[0], v[1], v[2])
                };
                let quad = [
                    corner(-1.0, -1.0),
                    corner(1.0, -1.0),
                    corner(1.0, 1.0),
                    corner(-1.0, 1.0),
                ];
                self.face(&quad, p.centre);
            }
        }
    }

    fn gable(&mut self, p: &Piece<impl Copy>) {
        let v = gable_points(p.size).map(|q| p.centre + p.rot * q);
        let inside = p.centre + p.rot * Vec3::new(0.0, p.size.y / 3.0, 0.0);
        let [a0, b0, c0, a1, b1, c1] = v;
        self.face(&[a0, b0, c0], inside);
        self.face(&[a1, b1, c1], inside);
        self.face(&[a0, b0, b1, a1], inside);
        self.face(&[b0, c0, c1, b1], inside);
        self.face(&[c0, a0, a1, c1], inside);
    }

    fn cylinder(&mut self, p: &Piece<impl Copy>) {
        const N: usize = 16;
        let (rx, rz, hy) = (p.size.x / 2.0, p.size.z / 2.0, p.size.y / 2.0);
        let ring = |i: usize, y: f32| {
            let a = TAU * i as f32 / N as f32;
            Vec3::new(a.cos() * rx, y, a.sin() * rz)
        };
        let w = |q: Vec3| p.centre + p.rot * q;
        let first = self.pos.len() as u32;
        let radius = (rx + rz) / 2.0;
        for i in 0..=N {
            let a = TAU * i as f32 / N as f32;
            let n = p.rot * Vec3::new(a.cos() / rx.max(1e-3), 0.0, a.sin() / rz.max(1e-3));
            let n = n.normalize_or(Vec3::X);
            for y in [-hy, hy] {
                let q = w(ring(i, y));
                self.vertex(q, n, [a * radius, -q.y]);
            }
        }
        for i in 0..N as u32 {
            let (a, b, c, d) = (
                first + i * 2,
                first + i * 2 + 1,
                first + i * 2 + 2,
                first + i * 2 + 3,
            );
            // Counter-clockwise seen from outside.
            self.idx.extend([a, b, d, a, d, c]);
        }
        for y in [-hy, hy] {
            let pts: Vec<Vec3> = (0..N).map(|i| w(ring(i, y))).collect();
            self.face(&pts, p.centre);
        }
    }

    fn ball(&mut self, p: &Piece<impl Copy>) {
        const LON: usize = 12;
        const LAT: usize = 8;
        let r = p.size / 2.0;
        let first = self.pos.len() as u32;
        for j in 0..=LAT {
            let phi = std::f32::consts::PI * j as f32 / LAT as f32;
            for i in 0..=LON {
                let th = TAU * i as f32 / LON as f32;
                let unit = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
                let q = p.centre + p.rot * (unit * r);
                let n = (p.rot * (unit / r.max(Vec3::splat(1e-3)))).normalize_or(Vec3::Y);
                self.vertex(q, n, [th * r.x, phi * r.y]);
            }
        }
        let row = LON as u32 + 1;
        for j in 0..LAT as u32 {
            for i in 0..LON as u32 {
                let a = first + j * row + i;
                let (b, c, d) = (a + 1, a + row, a + row + 1);
                self.idx.extend([a, b, c, b, d, c]);
            }
        }
    }

    fn into_mesh(self, origin: Vec3) -> Mesh {
        let pos: Vec<[f32; 3]> = self
            .pos
            .iter()
            .map(|p| (Vec3::from_array(*p) - origin).to_array())
            .collect();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
        .with_inserted_indices(Indices::U32(self.idx));
        let _ = mesh.generate_tangents();
        mesh
    }
}

/// The six corners of a gable prism in its own frame: base triangle a, b,
/// apex c at -z, then the same at +z.
fn gable_points(size: Vec3) -> [Vec3; 6] {
    let (hx, y, hz) = (size.x / 2.0, size.y, size.z / 2.0);
    [
        Vec3::new(-hx, 0.0, -hz),
        Vec3::new(hx, 0.0, -hz),
        Vec3::new(0.0, y, -hz),
        Vec3::new(-hx, 0.0, hz),
        Vec3::new(hx, 0.0, hz),
        Vec3::new(0.0, y, hz),
    ]
}

/// Merge every drawn piece into one mesh per (group, material, small).
pub fn batches<M: Palette>(pieces: &[Piece<M>]) -> Vec<Batch<M>> {
    let mut keys: Vec<(u32, M, bool)> = Vec::new();
    let mut builders: Vec<Builder> = Vec::new();
    for p in pieces {
        if p.mat == M::HIDDEN {
            continue;
        }
        let key = (p.group, p.mat, p.small);
        let i = match keys.iter().position(|k| *k == key) {
            Some(i) => i,
            None => {
                keys.push(key);
                builders.push(Builder::default());
                keys.len() - 1
            }
        };
        let b = &mut builders[i];
        match p.shape {
            Shape::Cube => b.cube(p),
            Shape::Gable => b.gable(p),
            Shape::Cylinder => b.cylinder(p),
            Shape::Ball => b.ball(p),
        }
    }
    keys.into_iter()
        .zip(builders)
        .filter(|(_, b)| !b.pos.is_empty())
        .map(|((group, mat, small), b)| {
            let (lo, hi) = b.pos.iter().fold(
                (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                |(lo, hi), p| (lo.min(Vec3::from_array(*p)), hi.max(Vec3::from_array(*p))),
            );
            let origin = (lo + hi) / 2.0;
            Batch {
                group,
                mat,
                small,
                origin,
                mesh: b.into_mesh(origin),
            }
        })
        .collect()
}

/// A solid box, as merged so far.
#[derive(Clone, Copy, Debug)]
struct Slab {
    centre: Vec3,
    size: Vec3,
    rot: Quat,
    solid: Solid,
}

/// `b` glued onto `a` if they share a rotation, line up on two axes and
/// touch along the third.
fn glue(a: &Slab, b: &Slab) -> Option<Slab> {
    const EPS: f32 = 1e-3;
    if a.solid != b.solid || a.rot.dot(b.rot) < 1.0 - 1e-6 {
        return None;
    }
    let d = a.rot.inverse() * (b.centre - a.centre);
    for k in 0..3 {
        let (i, j) = ((k + 1) % 3, (k + 2) % 3);
        let same = |x: usize| d[x].abs() < EPS && (a.size[x] - b.size[x]).abs() < EPS;
        if !(same(i) && same(j)) {
            continue;
        }
        if (d[k].abs() - (a.size[k] + b.size[k]) / 2.0).abs() > EPS {
            continue;
        }
        let lo = (-a.size[k] / 2.0).min(d[k] - b.size[k] / 2.0);
        let hi = (a.size[k] / 2.0).max(d[k] + b.size[k] / 2.0);
        let mut size = a.size;
        size[k] = hi - lo;
        let mut shift = Vec3::ZERO;
        shift[k] = (lo + hi) / 2.0;
        return Some(Slab {
            centre: a.centre + a.rot * shift,
            size,
            rot: a.rot,
            solid: a.solid,
        });
    }
    None
}

/// Merge boxes that line up and touch, until none do.
fn merge(mut slabs: Vec<Slab>) -> Vec<Slab> {
    loop {
        let mut merged = false;
        let mut i = 0;
        while i < slabs.len() {
            let mut j = i + 1;
            while j < slabs.len() {
                if let Some(s) = glue(&slabs[i], &slabs[j]) {
                    slabs[i] = s;
                    slabs.swap_remove(j);
                    merged = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
        if !merged {
            return slabs;
        }
    }
}

/// Static colliders for every solid piece. Balls don't collide.
pub fn bodies<M: Palette>(pieces: &[Piece<M>]) -> Vec<Body> {
    let mut out = Vec::new();
    let mut slabs = Vec::new();
    for p in pieces.iter().filter(|p| p.solid != Solid::None) {
        match p.shape {
            Shape::Cube => slabs.push(Slab {
                centre: p.centre,
                size: p.size,
                rot: p.rot,
                solid: p.solid,
            }),
            Shape::Cylinder => out.push(Body {
                centre: p.centre,
                rot: p.rot,
                collider: Collider::cylinder(p.size.x / 2.0, p.size.y),
                solid: p.solid,
            }),
            Shape::Gable => {
                let Some(collider) = Collider::convex_hull(&gable_points(p.size)) else {
                    continue;
                };
                out.push(Body {
                    centre: p.centre,
                    rot: p.rot,
                    collider,
                    solid: p.solid,
                });
            }
            Shape::Ball => {}
        }
    }
    out.extend(merge(slabs).into_iter().map(|s| Body {
        centre: s.centre,
        rot: s.rot,
        collider: Collider::cuboid(s.size.x, s.size.y, s.size.z),
        solid: s.solid,
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kit;
    use crate::tests::M;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn uvs_continue_across_neighbouring_boxes() {
        let mut kit = Kit::<M>::default();
        kit.block(Vec3::ZERO, Vec3::new(2.0, 3.0, 0.25), M::Wall, Solid::Wall);
        kit.block(
            Vec3::X * 2.0,
            Vec3::new(5.0, 3.0, 0.25),
            M::Wall,
            Solid::Wall,
        );
        let b = batches(&kit.pieces);
        assert_eq!(b.len(), 1, "one mesh per material");
        let mesh = &b[0].mesh;
        assert!(mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
        let (
            Some(VertexAttributeValues::Float32x3(pos)),
            Some(VertexAttributeValues::Float32x2(uv)),
        ) = (
            mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            mesh.attribute(Mesh::ATTRIBUTE_UV_0),
        )
        else {
            panic!("attributes missing");
        };
        // On the front (+z) face, u is exactly the world x in metres.
        let Some(VertexAttributeValues::Float32x3(nrm)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals missing");
        };
        let mut seen = 0;
        for ((p, n), uv) in pos.iter().zip(nrm).zip(uv) {
            if n[2] > 0.99 {
                let x = p[0] + b[0].origin.x;
                assert!((uv[0].abs() - x.abs()).abs() < 1e-4, "{p:?} {uv:?}");
                seen += 1;
            }
        }
        assert_eq!(seen, 8);
        // And the two boxes become one collider.
        assert_eq!(bodies(&kit.pieces).len(), 1);
    }

    #[test]
    fn roofs_and_gables_collide() {
        let mut kit = Kit::<M>::default();
        kit.roof(Vec2::ZERO, 6.0, 5.0, 3.0, 2.0, M::Wood, M::Timber, M::Wall);
        let bodies = bodies(&kit.pieces);
        assert_eq!(bodies.len(), 4, "two roof planes and two gables");
    }

    #[test]
    fn faces_point_outwards() {
        let mut kit = Kit::<M>::default();
        kit.put(
            Shape::Gable,
            Vec3::ZERO,
            Vec3::new(4.0, 2.0, 0.25),
            Quat::IDENTITY,
            M::Wall,
            Solid::None,
            false,
        );
        kit.block(Vec3::splat(5.0), Vec3::splat(6.0), M::Wall, Solid::None);
        let b = batches(&kit.pieces);
        let mesh = &b[0].mesh;
        let (Some(VertexAttributeValues::Float32x3(pos)), Some(Indices::U32(idx))) =
            (mesh.attribute(Mesh::ATTRIBUTE_POSITION), mesh.indices())
        else {
            panic!("attributes missing");
        };
        let Some(VertexAttributeValues::Float32x3(nrm)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals missing");
        };
        for t in idx.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from_array(pos[i as usize]));
            let n = Vec3::from_array(nrm[t[0] as usize]);
            assert!((b - a).cross(c - a).dot(n) > 0.0, "triangle winds inwards");
        }
    }
}
