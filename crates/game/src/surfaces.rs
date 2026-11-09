//! PBR surfaces for the primitive models (#128, doomy: "rn they're just flat
//! materials can you proc gen, make shaders, or use textures? or all in
//! combination?").
//!
//! Every plain `StandardMaterial` (no textures of its own, opaque, lit, not
//! glossy or glowing) is sorted into a [`Surface`] by its colour and roughness
//! (stone, wood, leaves, cloth, metal, painted) the moment it is added, before
//! the cat palette repaints it. It then gets three shared, code-generated
//! textures for that surface: a greyscale albedo detail map (colour still comes
//! from `base_color`, so the cat palette keeps working), a normal map and a
//! roughness map.
//!
//! Primitive meshes (cubes, spheres, cylinders) are stretched by their
//! `Transform`, so their own UVs would smear one texture over a 10 m wall. Each
//! mesh that wears such a material is re-baked once per scale with box-projected
//! UVs in metres and mikktspace tangents (normal maps need them), so the detail
//! has the same size everywhere. Meshes with vertex colours, skins or no
//! normals are left alone. Purely local visuals: nothing is replicated.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::{PrimitiveTopology, VertexAttributeValues};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::art::{hash, tiled_noise};

/// Texture side in pixels (with a full mip chain).
const SIZE: u32 = 128;

/// What a plain material is made of, judged from its colour and roughness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    /// Greys: rocks, walls, stone rings.
    Stone,
    /// Darker browns and reds: trunks, planks, crates, roofs.
    Wood,
    /// Greens: leaves, bushes, moss.
    Leaves,
    /// Other saturated colours: clothes, flags, banners.
    Cloth,
    /// Anything metallic.
    Metal,
    /// Pale or light warm colours: skin, plaster, hay, toys.
    Painted,
}

impl Surface {
    pub const ALL: [Surface; 6] = [
        Surface::Stone,
        Surface::Wood,
        Surface::Leaves,
        Surface::Cloth,
        Surface::Metal,
        Surface::Painted,
    ];

    /// Metres covered by one repeat of the texture.
    fn tile(self) -> f32 {
        match self {
            Surface::Stone => 2.0,
            Surface::Wood => 1.5,
            Surface::Leaves => 1.0,
            Surface::Cloth => 0.6,
            Surface::Metal => 1.2,
            Surface::Painted => 1.5,
        }
    }

    /// How strongly the height field tilts the normals.
    fn bump(self) -> f32 {
        match self {
            Surface::Stone => 3.0,
            Surface::Wood => 1.6,
            Surface::Leaves => 2.4,
            Surface::Cloth => 1.4,
            Surface::Metal => 0.5,
            Surface::Painted => 0.5,
        }
    }

    /// (lowest, highest) albedo brightness, applied over the height field.
    fn albedo_range(self) -> (f32, f32) {
        match self {
            Surface::Stone => (0.68, 1.0),
            Surface::Wood => (0.66, 1.0),
            Surface::Leaves => (0.7, 1.0),
            Surface::Cloth => (0.8, 1.0),
            Surface::Metal => (0.82, 1.0),
            Surface::Painted => (0.9, 1.0),
        }
    }

    /// (lowest, highest) roughness multiplier on the material's own roughness.
    fn rough_range(self) -> (f32, f32) {
        match self {
            Surface::Stone => (0.8, 1.0),
            Surface::Wood => (0.75, 1.0),
            Surface::Leaves => (0.7, 1.0),
            Surface::Cloth => (0.9, 1.0),
            Surface::Metal => (0.5, 1.0),
            Surface::Painted => (0.85, 1.0),
        }
    }

    /// Height field in `0..=1` at `(u, v)` in `[0, 1)`, seamless over the tile.
    fn height(self, u: f32, v: f32) -> f32 {
        match self {
            Surface::Stone => {
                let (f1, f2) = worley(u, v, 5, 11);
                let cracks = smoothstep(0.0, 0.14, f2 - f1);
                let grit = fbm(u, v, 8, 4, 12);
                (cracks * 0.65 + grit * 0.35 + (f1 * 0.15)).clamp(0.0, 1.0)
            }
            Surface::Wood => {
                // Grain runs along v; a little warp makes it wavy.
                let warp = fbm(u, v, 2, 3, 21) - 0.5;
                let x = u * 9.0 + warp * 2.2;
                let ring = 0.5 + 0.5 * (x * std::f32::consts::TAU).sin();
                let fibre = stretched_noise(u, v, 64, 4, 22);
                (ring.powf(0.7) * 0.7 + fibre * 0.3).clamp(0.0, 1.0)
            }
            Surface::Leaves => {
                let (f1, _) = worley(u, v, 7, 31);
                let blob = (1.0 - f1 * 1.4).max(0.0).powf(0.6);
                let vein = fbm(u, v, 16, 2, 32);
                (blob * 0.8 + vein * 0.2).clamp(0.0, 1.0)
            }
            Surface::Cloth => {
                const N: f32 = 24.0;
                let (a, b) = (u * N, v * N);
                let over = (a.floor() as i32 + b.floor() as i32).rem_euclid(2) == 0;
                let thread = if over {
                    (a.fract() * std::f32::consts::PI).sin()
                } else {
                    (b.fract() * std::f32::consts::PI).sin()
                };
                let fuzz = fbm(u, v, 32, 2, 41);
                (thread * 0.8 + fuzz * 0.2).clamp(0.0, 1.0)
            }
            Surface::Metal => {
                let brushed = stretched_noise(u, v, 2, 96, 51);
                let dents = fbm(u, v, 4, 3, 52);
                (brushed * 0.6 + dents * 0.4).clamp(0.0, 1.0)
            }
            Surface::Painted => {
                let soft = fbm(u, v, 4, 4, 61);
                let n = SIZE as i32;
                let grain = hash(
                    ((u * n as f32) as i32).rem_euclid(n),
                    ((v * n as f32) as i32).rem_euclid(n),
                    62,
                );
                (soft * 0.85 + grain * 0.15).clamp(0.0, 1.0)
            }
        }
    }

    /// Sort a plain material by its colour, or `None` if it should stay as is.
    pub fn classify(mat: &StandardMaterial) -> Option<Surface> {
        if mat.unlit
            || mat.alpha_mode != AlphaMode::Opaque
            || mat.base_color_texture.is_some()
            || mat.normal_map_texture.is_some()
            || mat.metallic_roughness_texture.is_some()
            || mat.emissive_texture.is_some()
            || mat.depth_map.is_some()
        {
            return None;
        }
        let glow = mat.emissive;
        if glow.red.max(glow.green).max(glow.blue) > 0.3 {
            return None;
        }
        if mat.metallic >= 0.5 {
            return Some(Surface::Metal);
        }
        if mat.perceptual_roughness < 0.3 {
            // Eyes, gems, water: keep them clean and shiny.
            return None;
        }
        let hsv = Hsva::from(mat.base_color);
        let (h, s, v) = (hsv.hue, hsv.saturation, hsv.value);
        Some(if s < 0.18 {
            if (0.15..0.85).contains(&v) {
                Surface::Stone
            } else {
                Surface::Painted
            }
        } else if (70.0..170.0).contains(&h) {
            Surface::Leaves
        } else if (h < 50.0 || h >= 345.0) && v < 0.65 {
            Surface::Wood
        } else if (15.0..50.0).contains(&h) {
            Surface::Painted
        } else {
            Surface::Cloth
        })
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Seamless value noise with `px` cells across and `py` cells down.
fn stretched_noise(u: f32, v: f32, px: i32, py: i32, seed: u32) -> f32 {
    let (x, y) = (u * px as f32, v * py as f32);
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (
        smoothstep(0.0, 1.0, x - x0 as f32),
        smoothstep(0.0, 1.0, y - y0 as f32),
    );
    let at = |i: i32, j: i32| hash(i.rem_euclid(px), j.rem_euclid(py), seed);
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * fx;
    let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * fx;
    top + (bot - top) * fy
}

/// Seamless fractal noise, `octaves` layers from `period` cells up.
fn fbm(u: f32, v: f32, period: i32, octaves: u32, seed: u32) -> f32 {
    let (mut n, mut amp, mut total) = (0.0, 0.5, 0.0);
    for o in 0..octaves {
        let p = period << o;
        n += amp * tiled_noise(u * p as f32, v * p as f32, p, seed.wrapping_add(o));
        total += amp;
        amp *= 0.5;
    }
    n / total
}

/// Seamless cellular noise: distances to the nearest and second-nearest
/// feature points, in cell units.
fn worley(u: f32, v: f32, period: i32, seed: u32) -> (f32, f32) {
    let (x, y) = (u * period as f32, v * period as f32);
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    let (mut f1, mut f2) = (f32::MAX, f32::MAX);
    for j in -1..=1 {
        for i in -1..=1 {
            let (gx, gy) = (cx + i, cy + j);
            let (wx, wy) = (gx.rem_euclid(period), gy.rem_euclid(period));
            let px = gx as f32 + hash(wx, wy, seed);
            let py = gy as f32 + hash(wx, wy, seed.wrapping_add(1));
            let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2)
}

/// The surface's height field, `size`² values.
fn heights(surface: Surface, size: u32) -> Vec<f32> {
    let mut h = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            h.push(surface.height(x as f32 / size as f32, y as f32 / size as f32));
        }
    }
    h
}

/// Greyscale RGBA8 albedo detail (sRGB).
pub fn albedo_pixels(surface: Surface, size: u32) -> Vec<u8> {
    let (lo, hi) = surface.albedo_range();
    heights(surface, size)
        .into_iter()
        .flat_map(|h| {
            let b = ((lo + (hi - lo) * h) * 255.0) as u8;
            [b, b, b, 255]
        })
        .collect()
}

/// Tangent-space normal map (OpenGL/glTF convention, green up) from the
/// height field, wrapping at the edges so it tiles.
pub fn normal_pixels(surface: Surface, size: u32) -> Vec<u8> {
    let h = heights(surface, size);
    let n = size as i32;
    let at = |x: i32, y: i32| h[(y.rem_euclid(n) * n + x.rem_euclid(n)) as usize];
    let strength = surface.bump() * size as f32 / 32.0;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let dx = (at(x + 1, y) - at(x - 1, y)) * 0.5 * strength;
            // Rows run down the image, so "up" is -y.
            let dy = (at(x, y - 1) - at(x, y + 1)) * 0.5 * strength;
            let normal = Vec3::new(-dx, -dy, 1.0).normalize();
            let enc = |c: f32| ((c * 0.5 + 0.5) * 255.0).round() as u8;
            out.extend_from_slice(&[enc(normal.x), enc(normal.y), enc(normal.z), 255]);
        }
    }
    out
}

/// Metallic-roughness map: green scales roughness, blue keeps metallic as set.
pub fn roughness_pixels(surface: Surface, size: u32) -> Vec<u8> {
    let (lo, hi) = surface.rough_range();
    heights(surface, size)
        .into_iter()
        .flat_map(|h| {
            // Raised bits are worn smoother; crevices stay rough.
            let g = ((hi - (hi - lo) * h) * 255.0) as u8;
            [255, g, 255, 255]
        })
        .collect()
}

/// Box-filtered mip chain appended after `level0`. Returns (data, levels).
fn with_mips(level0: Vec<u8>, size: u32) -> (Vec<u8>, u32) {
    let mut data = level0;
    let (mut prev_start, mut s, mut levels) = (0usize, size, 1);
    while s > 1 {
        let half = s / 2;
        let start = data.len();
        for y in 0..half {
            for x in 0..half {
                for c in 0..4 {
                    let px = |xx: u32, yy: u32| {
                        data[prev_start + ((yy * s + xx) * 4 + c) as usize] as u32
                    };
                    let sum = px(2 * x, 2 * y)
                        + px(2 * x + 1, 2 * y)
                        + px(2 * x, 2 * y + 1)
                        + px(2 * x + 1, 2 * y + 1);
                    data.push(((sum + 2) / 4) as u8);
                }
            }
        }
        prev_start = start;
        s = half;
        levels += 1;
    }
    (data, levels)
}

fn upload(images: &mut Assets<Image>, pixels: Vec<u8>, format: TextureFormat) -> Handle<Image> {
    let (data, levels) = with_mips(pixels, SIZE);
    let mut image = Image::new_uninit(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::linear()
    });
    images.add(image)
}

#[derive(Clone)]
struct SurfaceTextures {
    albedo: Handle<Image>,
    normal: Handle<Image>,
    roughness: Handle<Image>,
}

/// Shared textures, which materials became which surface, and the re-baked
/// meshes.
#[derive(Resource, Default)]
pub struct Surfaces {
    textures: HashMap<Surface, SurfaceTextures>,
    verdict: HashMap<AssetId<StandardMaterial>, Option<Surface>>,
    baked: HashMap<(AssetId<Mesh>, [i16; 3]), Handle<Mesh>>,
    generated: HashSet<AssetId<Mesh>>,
}

impl Surfaces {
    /// The surface a material was given, if it has been looked at yet.
    pub fn verdict(&self, id: AssetId<StandardMaterial>) -> Option<Option<Surface>> {
        self.verdict.get(&id).copied()
    }

    fn textures(&mut self, surface: Surface, images: &mut Assets<Image>) -> SurfaceTextures {
        self.textures
            .entry(surface)
            .or_insert_with(|| SurfaceTextures {
                albedo: upload(
                    images,
                    albedo_pixels(surface, SIZE),
                    TextureFormat::Rgba8UnormSrgb,
                ),
                normal: upload(
                    images,
                    normal_pixels(surface, SIZE),
                    TextureFormat::Rgba8Unorm,
                ),
                roughness: upload(
                    images,
                    roughness_pixels(surface, SIZE),
                    TextureFormat::Rgba8Unorm,
                ),
            })
            .clone()
    }
}

/// On an entity whose mesh has been looked at (re-baked or left alone).
#[derive(Component)]
pub struct Surfaced;

pub fn plugin(app: &mut App) {
    app.init_resource::<Surfaces>()
        .add_systems(
            Update,
            texture_new_materials
                .before(crate::features::cat_palette::paint_new_materials)
                .run_if(on_message::<AssetEvent<StandardMaterial>>),
        )
        .add_systems(Update, forget_meshes.run_if(on_message::<AssetEvent<Mesh>>))
        .add_systems(PostUpdate, bake_surface_meshes);
}

/// Give each new plain material its surface's textures. Runs before the cat
/// palette so the colour is judged as authored (cats can't tell wood from leaves).
fn texture_new_materials(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: Option<ResMut<Assets<Image>>>,
    mut surfaces: ResMut<Surfaces>,
) {
    for event in events.read() {
        let id = match event {
            AssetEvent::Added { id } => *id,
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                surfaces.verdict.remove(id);
                continue;
            }
            _ => continue,
        };
        let Some(mat) = materials.get(id) else {
            continue;
        };
        let surface = Surface::classify(mat);
        let textures = match (surface, images.as_deref_mut()) {
            (Some(s), Some(images)) => Some((s, surfaces.textures(s, images))),
            _ => None,
        };
        surfaces
            .verdict
            .insert(id, textures.as_ref().map(|(s, _)| *s));
        let Some((s, t)) = textures else {
            continue;
        };
        let Some(mut mat) = materials.get_mut(id) else {
            continue;
        };
        mat.base_color_texture = Some(t.albedo);
        mat.normal_map_texture = Some(t.normal);
        mat.metallic_roughness_texture = Some(t.roughness);
        mat.uv_transform = Affine2::from_scale(Vec2::splat(1.0 / s.tile()));
    }
}

/// Drop cache entries for meshes that are gone.
fn forget_meshes(mut events: MessageReader<AssetEvent<Mesh>>, mut surfaces: ResMut<Surfaces>) {
    for event in events.read() {
        if let AssetEvent::Removed { id } = event {
            surfaces.baked.retain(|(src, _), _| src != id);
            surfaces.generated.remove(id);
        }
    }
}

/// Scale rounded to ~10% steps, so near-identical props share a mesh.
fn quantize(scale: Vec3) -> [i16; 3] {
    scale
        .abs()
        .max(Vec3::splat(1e-3))
        .to_array()
        .map(|s| (s.ln() / 0.1).round() as i16)
}

fn dequantize(q: [i16; 3]) -> Vec3 {
    Vec3::from_array(q.map(|k| (k as f32 * 0.1).exp()))
}

/// Swap textured primitives onto meshes with metre-sized UVs and tangents.
fn bake_surface_meshes(
    mut commands: Commands,
    mut surfaces: ResMut<Surfaces>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    q: Query<
        (
            Entity,
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            Option<&Transform>,
        ),
        Without<Surfaced>,
    >,
) {
    let Some(mut meshes) = meshes else {
        return;
    };
    for (e, mesh, mat, tr) in &q {
        let src = mesh.0.id();
        if surfaces.generated.contains(&src) {
            commands.entity(e).insert(Surfaced);
            continue;
        }
        match surfaces.verdict(mat.0.id()) {
            // Not looked at yet (added this frame): try again next frame.
            None => continue,
            Some(None) => {
                commands.entity(e).insert(Surfaced);
                continue;
            }
            Some(Some(_)) => {}
        }
        let key = (src, quantize(tr.map_or(Vec3::ONE, |t| t.scale)));
        if let Some(baked) = surfaces.baked.get(&key) {
            commands.entity(e).insert((Mesh3d(baked.clone()), Surfaced));
            continue;
        }
        let Some(source) = meshes.get(src) else {
            // Still loading: wait for it.
            continue;
        };
        let Some(baked) = bake(source, dequantize(key.1)) else {
            commands.entity(e).insert(Surfaced);
            continue;
        };
        let handle = meshes.add(baked);
        surfaces.generated.insert(handle.id());
        surfaces.baked.insert(key, handle.clone());
        commands.entity(e).insert((Mesh3d(handle), Surfaced));
    }
}

/// A copy of `mesh` with box-projected UVs in metres (after `scale`) and
/// tangents, or `None` if the mesh shouldn't be touched.
pub fn bake(mesh: &Mesh, scale: Vec3) -> Option<Mesh> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    for skip in [
        Mesh::ATTRIBUTE_COLOR,
        Mesh::ATTRIBUTE_JOINT_INDEX,
        Mesh::ATTRIBUTE_JOINT_WEIGHT,
    ] {
        if mesh.try_contains_attribute(skip).ok()? {
            return None;
        }
    }
    if !mesh.try_contains_attribute(Mesh::ATTRIBUTE_NORMAL).ok()? {
        return None;
    }
    let mut out = mesh.clone();
    // One projection per triangle, so no triangle smears across two.
    out.try_duplicate_vertices().ok()?;
    let Some(VertexAttributeValues::Float32x3(pos)) =
        out.try_attribute_option(Mesh::ATTRIBUTE_POSITION).ok()?
    else {
        return None;
    };
    if pos.is_empty() || pos.len() % 3 != 0 {
        return None;
    }
    let mut uvs = Vec::with_capacity(pos.len());
    for tri in pos.as_chunks::<3>().0 {
        let q = tri.map(|p| Vec3::from_array(p) * scale);
        let face = (q[1] - q[0]).cross(q[2] - q[0]).abs();
        for p in q {
            uvs.push(if face.x >= face.y && face.x >= face.z {
                [p.z, -p.y]
            } else if face.y >= face.z {
                [p.x, p.z]
            } else {
                [p.x, -p.y]
            });
        }
    }
    out.try_insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs).ok()?;
    let _ = out.try_remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    out.generate_tangents().ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mat(color: Color) -> StandardMaterial {
        StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.8,
            ..default()
        }
    }

    #[test]
    fn colours_sort_into_surfaces() {
        let cases = [
            (Color::srgb(0.58, 0.56, 0.54), Surface::Stone),
            (Color::srgb(0.45, 0.32, 0.2), Surface::Wood),
            (Color::srgb(0.55, 0.25, 0.2), Surface::Wood),
            (Color::srgb(0.3, 0.6, 0.25), Surface::Leaves),
            (Color::srgb(0.2, 0.3, 0.8), Surface::Cloth),
            (Color::srgb(0.95, 0.78, 0.65), Surface::Painted),
            (Color::srgb(0.95, 0.95, 0.95), Surface::Painted),
        ];
        for (c, want) in cases {
            assert_eq!(Surface::classify(&mat(c)), Some(want), "{c:?}");
        }
        let metal = StandardMaterial {
            metallic: 1.0,
            ..mat(Color::WHITE)
        };
        assert_eq!(Surface::classify(&metal), Some(Surface::Metal));
    }

    #[test]
    fn special_materials_stay_plain() {
        let glossy = StandardMaterial {
            perceptual_roughness: 0.15,
            ..mat(Color::WHITE)
        };
        let glass = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..mat(Color::WHITE)
        };
        let lamp = StandardMaterial {
            emissive: LinearRgba::rgb(4.0, 2.8, 1.4),
            ..mat(Color::WHITE)
        };
        let unlit = StandardMaterial {
            unlit: true,
            ..mat(Color::WHITE)
        };
        let textured = StandardMaterial {
            base_color_texture: Some(Handle::default()),
            ..mat(Color::WHITE)
        };
        for m in [glossy, glass, lamp, unlit, textured] {
            assert_eq!(Surface::classify(&m), None);
        }
    }

    #[test]
    fn textures_are_varied_greyscale_and_tile() {
        for s in Surface::ALL {
            let px = albedo_pixels(s, 32);
            assert_eq!(px.len(), 32 * 32 * 4);
            assert!(px.chunks(4).all(|c| c[0] == c[1] && c[1] == c[2]), "{s:?}");
            let (lo, hi) = px
                .chunks(4)
                .fold((255, 0), |(lo, hi), c| (c[0].min(lo), c[0].max(hi)));
            assert!(hi - lo > 10, "{s:?} too flat: {lo}..{hi}");
            // Seamless: the field just past the right/bottom edge matches the left/top.
            for t in [0.1, 0.37, 0.8] {
                let a = s.height(0.0, t);
                let b = s.height(1.0, t);
                assert!((a - b).abs() < 1e-3, "{s:?} u seam {a} vs {b}");
                let a = s.height(t, 0.0);
                let b = s.height(t, 1.0);
                assert!((a - b).abs() < 1e-3, "{s:?} v seam {a} vs {b}");
            }
        }
    }

    #[test]
    fn normals_point_out_of_the_surface() {
        for s in Surface::ALL {
            let px = normal_pixels(s, 32);
            assert!(px.chunks(4).all(|c| c[2] > 128), "{s:?}");
        }
    }

    #[test]
    fn mips_go_down_to_one_pixel() {
        let (data, levels) = with_mips(vec![200; 8 * 8 * 4], 8);
        assert_eq!(levels, 4);
        assert_eq!(data.len(), (64 + 16 + 4 + 1) * 4);
        assert!(data.iter().all(|b| *b == 200));
    }

    #[test]
    fn baked_cube_has_metre_uvs_and_tangents() {
        let cube = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let Some(baked) = bake(&cube, Vec3::new(10.0, 3.0, 0.5)) else {
            panic!("cube should bake");
        };
        assert!(baked.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
        let Some(VertexAttributeValues::Float32x2(uvs)) = baked.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("no uvs");
        };
        let span = uvs.iter().fold(0.0f32, |m, uv| m.max(uv[0].abs()));
        assert!(
            (span - 5.0).abs() < 1e-3,
            "a 10 m wall spans -5..5 m of uv: {span}"
        );
    }

    #[test]
    fn vertex_coloured_meshes_are_left_alone() {
        let mut m = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let n = m.count_vertices();
        m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; n]);
        assert!(bake(&m, Vec3::ONE).is_none());
    }

    #[test]
    fn materials_and_meshes_get_surfaced_before_the_cat_repaints() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<StandardMaterial>()
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .add_plugins((plugin, crate::features::cat_palette::plugin));
        let brown = Color::srgb(0.45, 0.32, 0.2);
        let (wood, cube) = {
            let world = app.world_mut();
            let wood = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(mat(brown));
            let cube = world
                .resource_mut::<Assets<Mesh>>()
                .add(Cuboid::new(1.0, 1.0, 1.0));
            (wood, cube)
        };
        let e = app
            .world_mut()
            .spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(wood.clone()),
                Transform::from_scale(Vec3::new(4.0, 2.0, 1.0)),
            ))
            .id();
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<Surfaces>().verdict(wood.id()),
            Some(Some(Surface::Wood))
        );
        let mats = app.world().resource::<Assets<StandardMaterial>>();
        let Some(m) = mats.get(&wood) else {
            panic!("material missing");
        };
        assert!(m.normal_map_texture.is_some());
        assert_eq!(
            m.base_color,
            crate::features::cat_palette::cat_palette(brown)
        );
        let Some(mesh) = app.world().get::<Mesh3d>(e) else {
            panic!("mesh missing");
        };
        assert_ne!(mesh.0.id(), cube.id(), "mesh should be re-baked");
        assert!(app.world().get::<Surfaced>(e).is_some());
    }
}
