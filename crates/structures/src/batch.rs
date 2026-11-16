//! Merging many placed copies of loaded meshes into one mesh: a level built
//! from a tile kit is a few hundred small model meshes that share a handful of
//! materials, so it draws as one mesh per material instead.
//!
//! [`MeshBatch`] takes meshes with positions, normals and one UV set (a
//! tangent set and a second UV set too, when every mesh in the batch has
//! them), bakes each one's transform into its vertices and builds one
//! triangle list with `u32` indices. Meshes it can't take (other topologies
//! or attributes, data not kept on the CPU) are refused, so the caller can
//! leave them as they are. No game code.

use bevy::asset::RenderAssetUsages;
use bevy::math::Affine3A;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

/// What a mesh carries, for grouping: only meshes with the same layout go
/// into one batch.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Layout {
    pub tangents: bool,
    pub uv1: bool,
}

/// The layout of a mesh a batch can take, or `None` when it can't take it.
pub fn layout(mesh: &Mesh) -> Option<Layout> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let known = [
        Mesh::ATTRIBUTE_POSITION.id,
        Mesh::ATTRIBUTE_NORMAL.id,
        Mesh::ATTRIBUTE_UV_0.id,
        Mesh::ATTRIBUTE_UV_1.id,
        Mesh::ATTRIBUTE_TANGENT.id,
    ];
    let attrs = mesh.try_attributes().ok()?;
    let mut n = 0;
    for (a, _) in attrs {
        if !known.contains(&a.id) {
            return None;
        }
        n += 1;
    }
    let has = |a| mesh.try_attribute(a).ok().is_some();
    if n == 0 || !has(Mesh::ATTRIBUTE_NORMAL) || !has(Mesh::ATTRIBUTE_UV_0) {
        return None;
    }
    Some(Layout {
        tangents: has(Mesh::ATTRIBUTE_TANGENT),
        uv1: has(Mesh::ATTRIBUTE_UV_1),
    })
}

/// Vertices and triangles gathered from many meshes, all in one frame.
#[derive(Default)]
pub struct MeshBatch {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uv0: Vec<[f32; 2]>,
    uv1: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
    layout: Option<Layout>,
}

fn f3(v: Option<&VertexAttributeValues>) -> Option<&[[f32; 3]]> {
    match v? {
        VertexAttributeValues::Float32x3(v) => Some(v),
        _ => None,
    }
}

fn f2(v: Option<&VertexAttributeValues>) -> Option<&[[f32; 2]]> {
    match v? {
        VertexAttributeValues::Float32x2(v) => Some(v),
        _ => None,
    }
}

fn f4(v: Option<&VertexAttributeValues>) -> Option<&[[f32; 4]]> {
    match v? {
        VertexAttributeValues::Float32x4(v) => Some(v),
        _ => None,
    }
}

impl MeshBatch {
    /// Nothing in it yet.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Adds `mesh` placed by `to` (its transform into the batch's frame).
    /// Returns false (and adds nothing) when the mesh can't go in.
    pub fn push(&mut self, mesh: &Mesh, to: Affine3A) -> bool {
        let Some(layout) = layout(mesh) else {
            return false;
        };
        if self.layout.is_some_and(|l| l != layout) {
            return false;
        }
        let get = |a| mesh.try_attribute(a).ok();
        let (Some(pos), Some(nor), Some(uv)) = (
            f3(get(Mesh::ATTRIBUTE_POSITION)),
            f3(get(Mesh::ATTRIBUTE_NORMAL)),
            f2(get(Mesh::ATTRIBUTE_UV_0)),
        ) else {
            return false;
        };
        let uv1 = f2(get(Mesh::ATTRIBUTE_UV_1));
        let tan = f4(get(Mesh::ATTRIBUTE_TANGENT));
        if (layout.uv1 && uv1.is_none()) || (layout.tangents && tan.is_none()) {
            return false;
        }
        let count = pos.len();
        if nor.len() != count || uv.len() != count {
            return false;
        }
        let tris: Vec<u32> = match mesh.try_indices().ok() {
            Some(Indices::U16(i)) => i.iter().map(|&i| i as u32).collect(),
            Some(Indices::U32(i)) => i.clone(),
            None => (0..count as u32).collect(),
        };
        if tris.iter().any(|&i| i as usize >= count) {
            return false;
        }
        self.layout = Some(layout);
        let base = self.positions.len() as u32;
        let linear = to.matrix3;
        let normal_m = linear.inverse().transpose();
        let flip = linear.determinant() < 0.0;
        self.positions.extend(
            pos.iter()
                .map(|&p| to.transform_point3(p.into()).to_array()),
        );
        self.normals.extend(nor.iter().map(|&n| {
            Vec3::from(normal_m * Vec3A::from(n))
                .normalize_or_zero()
                .to_array()
        }));
        self.uv0.extend_from_slice(uv);
        if let Some(uv1) = uv1.filter(|_| layout.uv1) {
            self.uv1.extend_from_slice(uv1);
        }
        if let Some(tan) = tan.filter(|_| layout.tangents) {
            self.tangents.extend(tan.iter().map(|&[x, y, z, w]| {
                let t = Vec3::from(linear * Vec3A::new(x, y, z)).normalize_or_zero();
                [t.x, t.y, t.z, if flip { -w } else { w }]
            }));
        }
        for &[a, b, c] in tris.as_chunks::<3>().0 {
            if flip {
                self.indices.extend([base + a, base + c, base + b]);
            } else {
                self.indices.extend([base + a, base + b, base + c]);
            }
        }
        true
    }

    /// The merged mesh (kept only on the GPU once uploaded).
    pub fn build(self) -> Mesh {
        let layout = self.layout.unwrap_or(Layout {
            tangents: false,
            uv1: false,
        });
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv0)
        .with_inserted_indices(Indices::U32(self.indices));
        if layout.uv1 {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.uv1);
        }
        if layout.tangents {
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, self.tangents);
        }
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> Mesh {
        Mesh::from(Plane3d::default().mesh().size(2.0, 2.0))
    }

    #[test]
    fn copies_land_where_they_were_placed() {
        let mut b = MeshBatch::default();
        let q = quad();
        assert!(b.push(&q, Affine3A::IDENTITY));
        assert!(b.push(&q, Affine3A::from_translation(Vec3::new(6.0, 0.0, 0.0))));
        let m = b.build();
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        assert_eq!(p.len(), 8);
        assert!(p[4..].iter().all(|v| v[0] >= 5.0));
        assert_eq!(m.indices().map(|i| i.len()), Some(12));
    }

    #[test]
    fn turned_and_stretched_normals_stay_unit_and_facing() {
        let mut b = MeshBatch::default();
        let to = Affine3A::from_scale_rotation_translation(
            Vec3::new(1.0, 3.0, 1.0),
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            Vec3::ZERO,
        );
        assert!(b.push(&quad(), to));
        let m = b.build();
        let Some(VertexAttributeValues::Float32x3(n)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else {
            panic!("no normals");
        };
        for v in n {
            let v = Vec3::from(*v);
            assert!((v.length() - 1.0).abs() < 1e-4);
            assert!(v.z > 0.99, "up turned to +z: {v}");
        }
    }

    #[test]
    fn other_topologies_are_refused() {
        let mut b = MeshBatch::default();
        let lines = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 2]);
        assert!(!b.push(&lines, Affine3A::IDENTITY));
        assert!(b.is_empty());
    }
}
