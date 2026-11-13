//! Environment maps (reflection probes), baked from six pictures: build a
//! cubemap from any "what do I see in this direction" function, prefilter it
//! for Bevy's `EnvironmentMapLight` (a diffuse irradiance map and a specular
//! map whose mips go from mirror to fully rough), and write both as KTX2
//! (RGB9E5, uncompressed) for the asset loader. Pure maths, no game code.
//!
//! Directions are world directions (Bevy's: +y up, -z forward). Cubemaps are
//! left-handed, so Bevy samples them with z negated: [`Cube`] stores texels in
//! cube space and converts at the edges ([`world_dir`], [`Cube::sample`]).

use bevy::math::Vec3;

/// A cubemap: six square faces (+X, -X, +Y, -Y, +Z, -Z, cube space), row by row.
#[derive(Clone, Debug)]
pub struct Cube {
    pub size: usize,
    pub faces: [Vec<Vec3>; 6],
}

/// The cube-space direction through texel (s, t) of `face` (s, t in 0..1).
pub fn cube_dir(face: usize, s: f32, t: f32) -> Vec3 {
    let (a, b) = (2.0 * s - 1.0, 2.0 * t - 1.0);
    match face {
        0 => Vec3::new(1.0, -b, -a),
        1 => Vec3::new(-1.0, -b, a),
        2 => Vec3::new(a, 1.0, b),
        3 => Vec3::new(a, -1.0, -b),
        4 => Vec3::new(a, -b, 1.0),
        _ => Vec3::new(-a, -b, -1.0),
    }
    .normalize()
}

/// The world direction a cube-space direction stands for (and back: it's its own inverse).
pub fn world_dir(cube: Vec3) -> Vec3 {
    Vec3::new(cube.x, cube.y, -cube.z)
}

/// Which face and texel coordinates (0..1) a cube-space direction hits.
pub fn face_of(c: Vec3) -> (usize, f32, f32) {
    let a = c.abs();
    let (face, u, v) = if a.x >= a.y && a.x >= a.z {
        if c.x > 0.0 {
            (0, -c.z / a.x, -c.y / a.x)
        } else {
            (1, c.z / a.x, -c.y / a.x)
        }
    } else if a.y >= a.z {
        if c.y > 0.0 {
            (2, c.x / a.y, c.z / a.y)
        } else {
            (3, c.x / a.y, -c.z / a.y)
        }
    } else if c.z > 0.0 {
        (4, c.x / a.z, -c.y / a.z)
    } else {
        (5, -c.x / a.z, -c.y / a.z)
    };
    (face, (u + 1.0) / 2.0, (v + 1.0) / 2.0)
}

impl Cube {
    /// A cube whose texel in each world direction is `f(direction)`.
    pub fn from_fn(size: usize, f: impl Fn(Vec3) -> Vec3) -> Cube {
        let faces = std::array::from_fn(|face| {
            let mut out = Vec::with_capacity(size * size);
            for y in 0..size {
                for x in 0..size {
                    let s = (x as f32 + 0.5) / size as f32;
                    let t = (y as f32 + 0.5) / size as f32;
                    out.push(f(world_dir(cube_dir(face, s, t))));
                }
            }
            out
        });
        Cube { size, faces }
    }

    /// What the cube holds in a world direction (bilinear within the face).
    pub fn sample(&self, world: Vec3) -> Vec3 {
        let (face, s, t) = face_of(world_dir(world));
        let n = self.size;
        let fx = (s * n as f32 - 0.5).clamp(0.0, n as f32 - 1.0);
        let fy = (t * n as f32 - 0.5).clamp(0.0, n as f32 - 1.0);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1));
        let (dx, dy) = (fx - x0 as f32, fy - y0 as f32);
        let p = &self.faces[face];
        let top = p[y0 * n + x0].lerp(p[y0 * n + x1], dx);
        let bottom = p[y1 * n + x0].lerp(p[y1 * n + x1], dx);
        top.lerp(bottom, dy)
    }

    /// Half the size, each texel the average of four.
    pub fn half(&self) -> Cube {
        let n = (self.size / 2).max(1);
        let m = self.size;
        let faces = std::array::from_fn(|f| {
            let p = &self.faces[f];
            let mut out = Vec::with_capacity(n * n);
            for y in 0..n {
                for x in 0..n {
                    let at = |dx: usize, dy: usize| {
                        p[((2 * y + dy).min(m - 1)) * m + (2 * x + dx).min(m - 1)]
                    };
                    out.push((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) / 4.0);
                }
            }
            out
        });
        Cube { size: n, faces }
    }

    /// Every value times `k`.
    pub fn scaled(mut self, k: f32) -> Cube {
        for f in &mut self.faces {
            for v in f.iter_mut() {
                *v *= k;
            }
        }
        self
    }
}

/// The source at every size down to 1 texel (for filtered lookups).
fn pyramid(src: &Cube) -> Vec<Cube> {
    let mut out = vec![src.clone()];
    while out.last().is_some_and(|c| c.size > 1) {
        let next = out[out.len() - 1].half();
        out.push(next);
    }
    out
}

fn hammersley(i: u32, n: u32) -> (f32, f32) {
    (
        i as f32 / n as f32,
        i.reverse_bits() as f32 * 2.328_306_4e-10,
    )
}

/// A tangent frame around `n`.
fn frame(n: Vec3) -> (Vec3, Vec3) {
    let up = if n.y.abs() < 0.999 { Vec3::Y } else { Vec3::X };
    let t = up.cross(n).normalize();
    (t, n.cross(t))
}

/// The specular map: `levels` mips from `size` down, mip i prefiltered for
/// perceptual roughness i / (levels - 1) (GGX, as Bevy samples it).
pub fn specular(src: &Cube, size: usize, levels: usize) -> Vec<Cube> {
    let pyr = pyramid(src);
    let lookup = |dir: Vec3, lod: f32| {
        let i = (lod.max(0.0).round() as usize).min(pyr.len() - 1);
        pyr[i].sample(dir)
    };
    // The pyramid level whose texels match `size` (mirror level).
    let base = pyr.iter().position(|c| c.size <= size).unwrap_or(0);
    let levels = levels.max(1);
    (0..levels)
        .map(|li| {
            let n = (size >> li).max(1);
            let rough = if levels > 1 {
                li as f32 / (levels - 1) as f32
            } else {
                0.0
            };
            if li == 0 || rough <= 0.0 {
                return Cube::from_fn(n, |d| pyr[base].sample(d));
            }
            let alpha = rough * rough;
            let a2 = alpha * alpha;
            let samples = 96u32;
            let src_texel = 4.0 * std::f32::consts::PI / (6.0 * (src.size * src.size) as f32);
            Cube::from_fn(n, |nrm| {
                let (t, b) = frame(nrm);
                let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
                for i in 0..samples {
                    let (u1, u2) = hammersley(i, samples);
                    let phi = std::f32::consts::TAU * u1;
                    let cos_t = ((1.0 - u2) / (1.0 + (a2 - 1.0) * u2)).sqrt();
                    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
                    let h = (t * (phi.cos() * sin_t) + b * (phi.sin() * sin_t) + nrm * cos_t)
                        .normalize();
                    let l = 2.0 * nrm.dot(h) * h - nrm;
                    let ndl = nrm.dot(l);
                    if ndl <= 0.0 {
                        continue;
                    }
                    let ndh = nrm.dot(h).max(0.0);
                    let d = a2 / (std::f32::consts::PI * (ndh * ndh * (a2 - 1.0) + 1.0).powi(2));
                    let pdf = d / 4.0;
                    let sa = 1.0 / (samples as f32 * pdf + 1e-4);
                    let lod = 0.5 * (sa / src_texel).log2() + 1.0;
                    sum += lookup(l, lod) * ndl;
                    weight += ndl;
                }
                if weight > 0.0 {
                    sum / weight
                } else {
                    lookup(nrm, 0.0)
                }
            })
        })
        .collect()
}

/// The diffuse map: irradiance over pi (cosine-convolved radiance), from
/// the source's order-2 spherical harmonics.
pub fn diffuse(src: &Cube, size: usize) -> Cube {
    let pyr = pyramid(src);
    let low = pyr.iter().find(|c| c.size <= 32).unwrap_or(&pyr[0]);
    let n = low.size;
    let mut sh = [Vec3::ZERO; 9];
    let mut total = 0.0;
    for (face, texels) in low.faces.iter().enumerate() {
        for y in 0..n {
            for x in 0..n {
                let s = (x as f32 + 0.5) / n as f32;
                let t = (y as f32 + 0.5) / n as f32;
                let (a, b) = (2.0 * s - 1.0, 2.0 * t - 1.0);
                // A texel's solid angle on the unit cube.
                let w = 1.0 / (1.0 + a * a + b * b).powf(1.5);
                let d = world_dir(cube_dir(face, s, t));
                let c = texels[y * n + x];
                for (k, basis) in sh_basis(d).iter().enumerate() {
                    sh[k] += c * (*basis * w);
                }
                total += w;
            }
        }
    }
    let norm = 4.0 * std::f32::consts::PI / total;
    for c in &mut sh {
        *c *= norm;
    }
    // Cosine lobe convolution (Ramamoorthi & Hanrahan), then / pi.
    let band = [
        std::f32::consts::PI,
        2.0 * std::f32::consts::PI / 3.0,
        std::f32::consts::PI / 4.0,
    ];
    let order = [0, 1, 1, 1, 2, 2, 2, 2, 2];
    Cube::from_fn(size, |d| {
        let basis = sh_basis(d);
        let mut e = Vec3::ZERO;
        for k in 0..9 {
            e += sh[k] * (basis[k] * band[order[k]]);
        }
        (e / std::f32::consts::PI).max(Vec3::ZERO)
    })
}

fn sh_basis(d: Vec3) -> [f32; 9] {
    let (x, y, z) = (d.x, d.y, d.z);
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}

/// One texel as RGB9E5 (shared exponent; clamps to 0..65408).
pub fn rgb9e5(c: Vec3) -> u32 {
    const MAX: f32 = 65408.0;
    let c = c.clamp(Vec3::ZERO, Vec3::splat(MAX));
    let m = c.max_element();
    if m <= 0.0 {
        return 0;
    }
    let mut exp = (m.log2().floor() as i32).max(-16) + 1 + 15;
    let mut scale = 2f32.powi(exp - 15 - 9);
    if (m / scale + 0.5).floor() >= 512.0 {
        exp += 1;
        scale *= 2.0;
    }
    let q = |v: f32| ((v / scale + 0.5).floor() as u32).min(511);
    q(c.x) | (q(c.y) << 9) | (q(c.z) << 18) | ((exp as u32) << 27)
}

/// Decode one RGB9E5 texel.
pub fn from_rgb9e5(v: u32) -> Vec3 {
    let exp = (v >> 27) as i32;
    let scale = 2f32.powi(exp - 15 - 9);
    Vec3::new(
        (v & 511) as f32,
        ((v >> 9) & 511) as f32,
        ((v >> 18) & 511) as f32,
    ) * scale
}

/// A cubemap with its mips (largest first) as a KTX2 file: RGB9E5, six faces.
pub fn ktx2(levels: &[Cube]) -> Vec<u8> {
    let n = levels.len();
    let size = levels.first().map_or(1, |c| c.size) as u32;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[
        0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
    ]);
    let u32s = |out: &mut Vec<u8>, v: &[u32]| v.iter().for_each(|x| out.extend(x.to_le_bytes()));
    // vkFormat E5B9G9R9_UFLOAT_PACK32, typeSize, w, h, depth, layers, faces, levels, no supercompression.
    u32s(&mut out, &[123, 4, size, size, 0, 0, 6, n as u32, 0]);
    let level_index = 80;
    let dfd_at = level_index + 24 * n;
    let data_at = dfd_at + 4;
    // DFD (just its size), no key/value data, no supercompression data.
    u32s(&mut out, &[dfd_at as u32, 4, 0, 0]);
    out.extend(0u64.to_le_bytes());
    out.extend(0u64.to_le_bytes());
    // Data: smallest mip first.
    let bytes: Vec<Vec<u8>> = levels
        .iter()
        .map(|c| {
            c.faces
                .iter()
                .flat_map(|f| f.iter().flat_map(|v| rgb9e5(*v).to_le_bytes()))
                .collect()
        })
        .collect();
    let mut offsets = vec![0usize; n];
    let mut at = data_at;
    for i in (0..n).rev() {
        offsets[i] = at;
        at += bytes[i].len();
    }
    for i in 0..n {
        let len = bytes[i].len() as u64;
        out.extend((offsets[i] as u64).to_le_bytes());
        out.extend(len.to_le_bytes());
        out.extend(len.to_le_bytes());
    }
    out.extend(4u32.to_le_bytes());
    for i in (0..n).rev() {
        out.extend(&bytes[i]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_round_trip() {
        for face in 0..6 {
            for (s, t) in [(0.1, 0.2), (0.5, 0.5), (0.9, 0.7)] {
                let d = cube_dir(face, s, t);
                let (f, s2, t2) = face_of(d);
                assert_eq!(f, face);
                assert!(
                    (s - s2).abs() < 1e-4 && (t - t2).abs() < 1e-4,
                    "{face} {s} {t}"
                );
            }
        }
        // Up is up, and world -z (forward) is cube +z.
        assert!(world_dir(cube_dir(2, 0.5, 0.5)).distance(Vec3::Y) < 1e-5);
        assert!(world_dir(cube_dir(4, 0.5, 0.5)).distance(Vec3::NEG_Z) < 1e-5);
    }

    #[test]
    fn a_cube_samples_what_it_was_built_from() {
        let c = Cube::from_fn(16, |d| Vec3::new(d.x.max(0.0), d.y.max(0.0), d.z.max(0.0)));
        for d in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            Vec3::new(0.3, 0.8, -0.5).normalize(),
        ] {
            let want = Vec3::new(d.x.max(0.0), d.y.max(0.0), d.z.max(0.0));
            assert!(c.sample(d).distance(want) < 0.12, "{d}");
        }
        assert_eq!(c.half().size, 8);
    }

    #[test]
    fn a_flat_white_world_stays_white() {
        let src = Cube::from_fn(16, |_| Vec3::ONE);
        // Irradiance / pi of a uniform white world is 1.
        let d = diffuse(&src, 8);
        assert!(
            d.sample(Vec3::Y).distance(Vec3::ONE) < 0.03,
            "{}",
            d.sample(Vec3::Y)
        );
        let s = specular(&src, 16, 5);
        assert_eq!(s.len(), 5);
        assert_eq!(s[4].size, 1);
        for c in &s {
            assert!(c.sample(Vec3::X).distance(Vec3::ONE) < 0.02);
        }
    }

    #[test]
    fn a_bright_sky_lights_upward_faces_more() {
        let src = Cube::from_fn(16, |d| {
            if d.y > 0.0 {
                Vec3::splat(4.0)
            } else {
                Vec3::ZERO
            }
        });
        let d = diffuse(&src, 8);
        assert!(d.sample(Vec3::Y).x > 3.0 * d.sample(Vec3::NEG_Y).x.max(0.1));
    }

    #[test]
    fn rgb9e5_round_trips() {
        for c in [
            Vec3::ZERO,
            Vec3::new(0.25, 0.5, 1.0),
            Vec3::new(1000.0, 30.0, 0.1),
            Vec3::splat(1e-3),
        ] {
            let back = from_rgb9e5(rgb9e5(c));
            assert!(
                back.distance(c) <= c.max_element() / 200.0 + 1e-6,
                "{c} {back}"
            );
        }
    }

    #[test]
    fn the_ktx2_header_reads_back() {
        let lv = specular(&Cube::from_fn(8, |_| Vec3::ONE), 8, 4);
        let bytes = ktx2(&lv);
        // Format, size, faces, levels.
        let u = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap_or([0; 4]));
        assert_eq!((u(12), u(20), u(36), u(40)), (123, 8, 6, 4));
        // Level 0's data is 6 faces of 8x8 texels.
        let len0 = u64::from_le_bytes(bytes[88..96].try_into().unwrap_or([0; 8]));
        assert_eq!(len0, 6 * 64 * 4);
        let total: usize = lv.iter().map(|c| 6 * c.size * c.size * 4).sum();
        assert_eq!(bytes.len(), 80 + 24 * 4 + 4 + total);
    }

    #[test]
    fn bevy_loads_it_as_a_cubemap() {
        use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
        let lv = specular(&Cube::from_fn(16, |d| d.abs()), 16, 5);
        let img = Image::from_buffer(
            &ktx2(&lv),
            ImageType::Extension("ktx2"),
            CompressedImageFormats::NONE,
            false,
            ImageSampler::Default,
            bevy::asset::RenderAssetUsages::default(),
        );
        let Ok(img) = img else {
            panic!("bevy can't read it: {:?}", img.err());
        };
        let d = img.texture_descriptor;
        assert_eq!(d.size.depth_or_array_layers, 6);
        assert_eq!(d.mip_level_count, 5);
        assert_eq!(
            d.format,
            bevy::render::render_resource::TextureFormat::Rgb9e5Ufloat
        );
    }
}
