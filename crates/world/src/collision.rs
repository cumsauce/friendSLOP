//! Exact colliders for a map's sloped and curved surfaces. A carve with
//! `collider: "mesh"` in the modeler (a smooth tunnel, a ramp) can't be
//! `Block` boxes without stepping; its `.bsn` export writes the triangles to
//! `<map>.collision` instead (format: the modeler's `scene::write_collision`,
//! `MCL1`), which `map.json` names. A feature reads it like any data file:
//!
//! ```ignore
//! pub static COLLISION: Data<CollisionMesh> =
//!     Data::new("maps/fulton.collision", CollisionMesh::parse);
//! // when its area's colliders load:
//! if let Some(c) = COLLISION.get().collider() {
//!     commands.spawn((Transform::from_translation(origin), RigidBody::Static, c, areas::sim(AREA)));
//! }
//! ```
//!
//! Like `Block` markers, it never waits on a model file: it's a data file,
//! fetched before you can host or join in the browser.

use avian3d::prelude::Collider;
use bevy::prelude::*;

/// A map's mesh colliders as one triangle soup, in the map's frame.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct CollisionMesh {
    pub verts: Vec<Vec3>,
    pub tris: Vec<[u32; 3]>,
}

impl CollisionMesh {
    /// Decodes a `.collision` file (`MCL1`); `None` when it isn't one, it's
    /// cut short, or an index points past the vertices.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.get(..4)? != b"MCL1" {
            return None;
        }
        let word = |o: usize| -> Option<[u8; 4]> { bytes.get(o..o + 4)?.try_into().ok() };
        let u = |o| word(o).map(u32::from_le_bytes);
        let f = |o| word(o).map(f32::from_le_bytes);
        let (nv, ni) = (u(4)? as usize, u(8)? as usize);
        if ni % 3 != 0 {
            return None;
        }
        let mut verts = Vec::with_capacity(nv);
        let mut o = 12;
        for _ in 0..nv {
            verts.push(Vec3::new(f(o)?, f(o + 4)?, f(o + 8)?));
            o += 12;
        }
        let mut tris = Vec::with_capacity(ni / 3);
        for _ in 0..ni / 3 {
            let t = [u(o)?, u(o + 4)?, u(o + 8)?];
            if t.iter().any(|&i| i as usize >= nv) {
                return None;
            }
            tris.push(t);
            o += 12;
        }
        Some(Self { verts, tris })
    }

    /// For [`friendslop_data::Data::new`].
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        Self::decode(bytes).ok_or_else(|| "not a collision file".to_string())
    }

    /// The static trimesh collider, or `None` for an empty mesh (the file
    /// is missing: the map has no floor there, as with a missing model).
    pub fn collider(&self) -> Option<Collider> {
        (!self.tris.is_empty()).then(|| Collider::trimesh(self.verts.clone(), self.tris.clone()))
    }

    /// The highest floor (an upward-facing triangle) at (x, z) no higher
    /// than `below`: where something standing there rests. For tests and
    /// placing things on the map.
    pub fn floor_at(&self, x: f32, z: f32, below: f32) -> Option<f32> {
        let p = Vec2::new(x, z);
        let mut best: Option<f32> = None;
        for t in &self.tris {
            let [a, b, c] = t.map(|i| self.verts[i as usize]);
            if (b - a).cross(c - a).y.abs() < 1e-6 {
                continue; // a wall: no floor seen from above
            }
            let (a2, b2, c2) = (a.xz(), b.xz(), c.xz());
            let d = (b2 - a2).perp_dot(c2 - a2);
            let w1 = (p - a2).perp_dot(c2 - a2) / d;
            let w2 = (b2 - a2).perp_dot(p - a2) / d;
            let w0 = 1.0 - w1 - w2;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let y = a.y * w0 + b.y * w1 + c.y * w2;
            if y <= below + 1e-4 && best.is_none_or(|h| y > h) {
                best = Some(y);
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file the way the modeler writes one.
    fn encode(verts: &[[f32; 3]], indices: &[u32]) -> Vec<u8> {
        let mut out = b"MCL1".to_vec();
        out.extend((verts.len() as u32).to_le_bytes());
        out.extend((indices.len() as u32).to_le_bytes());
        for v in verts {
            for c in v {
                out.extend(c.to_le_bytes());
            }
        }
        for i in indices {
            out.extend(i.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_ramp_reads_back_smooth_with_no_steps() {
        // A 10 m ramp falling 2 m toward -x, and a flat landing at its top.
        let verts = [
            [0.0, 0.0, -2.0],
            [0.0, 0.0, 2.0],
            [-10.0, -2.0, -2.0],
            [-10.0, -2.0, 2.0],
            [0.0, 0.0, -2.0],
            [4.0, 0.0, -2.0],
            [4.0, 0.0, 2.0],
            [0.0, 0.0, 2.0],
        ];
        let indices = [0, 2, 1, 1, 2, 3, 4, 5, 6, 4, 6, 7];
        let bytes = encode(&verts, &indices);
        let Some(m) = CollisionMesh::decode(&bytes) else {
            panic!("decodes");
        };
        assert_eq!(m.tris.len(), 4);
        for k in 0..=20 {
            let x = -k as f32 * 0.5;
            let Some(y) = m.floor_at(x, 0.3, 5.0) else {
                panic!("floor at {x}");
            };
            assert!((y - x * 0.2).abs() < 1e-4, "smooth: {x} -> {y}");
        }
        assert_eq!(m.floor_at(2.0, 0.0, 5.0), Some(0.0), "the landing");
        assert_eq!(m.floor_at(20.0, 0.0, 5.0), None, "off the map");
        assert!(m.collider().is_some());
        assert!(CollisionMesh::default().collider().is_none());
        assert!(CollisionMesh::decode(&bytes[..bytes.len() - 3]).is_none());
        assert!(CollisionMesh::decode(&encode(&verts, &[0, 1, 99])).is_none());
    }
}
