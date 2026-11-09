//! Shared code-generated art (issue #47): procedural textures, soft materials and
//! little helpers like googly eyes, so every feature shares one cute low-poly look.
//!
//! Colour always comes from `StandardMaterial::base_color`, which `cat_palette`
//! filters when the material is added. Textures here are greyscale detail only, so
//! they never sneak un-filtered colours past the cat.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Cheap integer hash to [0, 1).
pub fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x8da6_b343)
        .wrapping_add((y as u32).wrapping_mul(0xd816_3841))
        .wrapping_add(seed.wrapping_mul(0xcb1a_b31f));
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Smooth value noise that tiles every `period` cells.
pub(crate) fn tiled_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let s = |v: f32| v * v * (3.0 - 2.0 * v);
    let at = |i: i32, j: i32| hash(i.rem_euclid(period), j.rem_euclid(period), seed);
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * s(fx);
    let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * s(fx);
    top + (bot - top) * s(fy)
}

/// A seamless greyscale grass texture: soft patches plus speckled blades.
pub fn grass_pixels(size: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (x as f32 / size as f32, y as f32 / size as f32);
            let mut n = 0.0;
            let mut amp = 0.5;
            for octave in 0..4 {
                let period = 4 << octave;
                n += amp * tiled_noise(u * period as f32, v * period as f32, period, octave);
                amp *= 0.5;
            }
            let blade = hash(x as i32, y as i32, 99);
            let speck = if blade > 0.93 {
                0.12
            } else if blade < 0.05 {
                -0.1
            } else {
                0.0
            };
            let l = (0.84 + (n - 0.47) * 0.5 + speck).clamp(0.0, 1.0);
            let b = (l * 255.0) as u8;
            data.extend_from_slice(&[b, b, b, 255]);
        }
    }
    data
}

/// Upload `grass_pixels` as a repeating texture.
pub fn grass_texture(images: &mut Assets<Image>) -> Handle<Image> {
    const SIZE: u32 = 128;
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        grass_pixels(SIZE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    images.add(image)
}

/// A soft, matte, toy-like material. Still a normal `StandardMaterial`, so the
/// cat palette repaints it like any other.
pub fn soft(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.8,
        reflectance: 0.3,
        ..default()
    })
}

/// A shiny material for eyes, noses and gems.
pub fn glossy(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.15,
        reflectance: 0.6,
        ..default()
    })
}

/// A pair of cute eyes (white, dark pupil, sparkle) facing -Z, centred `spread`
/// apart at `at`. `r` is the eyeball radius. They blink now and then.
pub fn eyes(
    p: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
    spread: f32,
    r: f32,
) {
    let white = glossy(materials, Color::srgb(0.98, 0.98, 0.96));
    let pupil = glossy(materials, Color::srgb(0.04, 0.04, 0.06));
    let ball = meshes.add(Sphere::new(r));
    let dot = meshes.add(Sphere::new(r * 0.62));
    let spark = meshes.add(Sphere::new(r * 0.22));
    let spark2 = meshes.add(Sphere::new(r * 0.1));
    let phase = hash(
        (at.x * 100.0) as i32,
        (at.y * 100.0) as i32,
        (r * 1000.0) as u32,
    ) * 5.0;
    for side in [-1.0, 1.0] {
        p.spawn((
            Transform::from_translation(at + Vec3::X * side * spread / 2.0),
            Visibility::default(),
            Blink { phase },
        ))
        .with_children(|e| {
            e.spawn((Mesh3d(ball.clone()), MeshMaterial3d(white.clone())));
            e.spawn((
                Mesh3d(dot.clone()),
                MeshMaterial3d(pupil.clone()),
                Transform::from_xyz(0.0, -r * 0.05, -r * 0.5),
            ));
            // Two sparkles: a big one up-left and a tiny one down-right.
            e.spawn((
                Mesh3d(spark.clone()),
                MeshMaterial3d(white.clone()),
                Transform::from_xyz(r * 0.28, r * 0.3, -r * 0.95),
            ));
            e.spawn((
                Mesh3d(spark2.clone()),
                MeshMaterial3d(white.clone()),
                Transform::from_xyz(-r * 0.25, -r * 0.25, -r * 0.98),
            ));
        });
    }
}

/// Rosy cheek blushes either side of a face (facing -Z).
pub fn blush(
    p: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
    spread: f32,
    r: f32,
) {
    let pink = soft(materials, Color::srgb(1.0, 0.55, 0.6));
    let mesh = meshes.add(Sphere::new(r));
    for side in [-1.0, 1.0] {
        p.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(pink.clone()),
            Transform::from_translation(at + Vec3::X * side * spread / 2.0)
                .with_scale(Vec3::new(1.0, 0.6, 0.3)),
        ));
    }
}

/// A low-poly faceted rock/chunk mesh (icosphere, no subdivision).
pub fn chunk(meshes: &mut Assets<Mesh>, r: f32) -> Handle<Mesh> {
    meshes.add(
        Sphere::new(r)
            .mesh()
            .ico(0)
            .unwrap_or_else(|_| Sphere::new(r).into()),
    )
}

/// Squash an eye shut every few seconds.
#[derive(Component, Clone, Copy)]
pub struct Blink {
    pub phase: f32,
}

/// Gentle breathing: the entity's scale swells around `base`.
#[derive(Component, Clone, Copy)]
pub struct Breathe {
    pub base: Vec3,
    pub amount: f32,
    pub rate: f32,
    pub phase: f32,
}

impl Breathe {
    pub fn new(base: Vec3, amount: f32, phase: f32) -> Self {
        Self {
            base,
            amount,
            rate: 1.6,
            phase,
        }
    }
}

/// A sway (or, with `twitch`, an occasional quick flick) around `axis`,
/// on top of the `base` rotation. Ears, tails, wings, flags.
#[derive(Component, Clone, Copy)]
pub struct Wiggle {
    pub base: Quat,
    pub axis: Vec3,
    pub amount: f32,
    pub rate: f32,
    pub phase: f32,
    pub twitch: bool,
}

impl Wiggle {
    pub fn sway(base: Quat, axis: Vec3, amount: f32, rate: f32, phase: f32) -> Self {
        Self {
            base,
            axis,
            amount,
            rate,
            phase,
            twitch: false,
        }
    }
    pub fn twitch(base: Quat, axis: Vec3, amount: f32, phase: f32) -> Self {
        Self {
            base,
            axis,
            amount,
            rate: 1.0,
            phase,
            twitch: true,
        }
    }
}

/// How open an eye is at time `t` (1 open, ~0.1 shut).
pub fn blink_openness(t: f32, phase: f32) -> f32 {
    const PERIOD: f32 = 3.7;
    let k = (t + phase).rem_euclid(PERIOD);
    if k < 0.14 {
        0.1 + 0.9 * ((k / 0.14) * 2.0 - 1.0).abs()
    } else {
        1.0
    }
}

/// Twitch curve: mostly 0, a quick 0→1→0 flick every few seconds.
fn twitch_curve(t: f32, phase: f32) -> f32 {
    let k = (t + phase).rem_euclid(4.3);
    if k < 0.3 {
        (k / 0.3 * std::f32::consts::PI * 2.0).sin()
    } else {
        0.0
    }
}

/// Registers the shared idle animations (blink, breathe, wiggle). Purely local visuals.
pub fn plugin(app: &mut App) {
    app.add_systems(Update, (blink, breathe, wiggle));
}

fn blink(time: Res<Time>, mut q: Query<(Entity, &Blink, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (e, b, mut tr) in &mut q {
        // Everyone is built with the same eye constants: spread the phase by
        // entity so a crowd doesn't blink in unison.
        let spread = (e.to_bits() % 997) as f32 * 0.37;
        tr.scale.y = blink_openness(t, b.phase + spread);
    }
}

fn breathe(time: Res<Time>, mut q: Query<(&Breathe, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (b, mut tr) in &mut q {
        let s = (t * b.rate + b.phase).sin() * b.amount;
        tr.scale = b.base * Vec3::new(1.0 + s * 0.5, 1.0 + s, 1.0 + s * 0.5);
    }
}

fn wiggle(time: Res<Time>, mut q: Query<(&Wiggle, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (w, mut tr) in &mut q {
        let k = if w.twitch {
            twitch_curve(t, w.phase)
        } else {
            (t * w.rate + w.phase).sin()
        };
        let Ok(axis) = Dir3::new(w.axis) else {
            continue;
        };
        tr.rotation = w.base * Quat::from_axis_angle(*axis, k * w.amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grass_is_greyscale_and_varied() {
        let px = grass_pixels(32);
        assert_eq!(px.len(), 32 * 32 * 4);
        assert!(px.chunks(4).all(|c| c[0] == c[1] && c[1] == c[2]));
        let (lo, hi) = px
            .chunks(4)
            .fold((255, 0), |(lo, hi), c| (c[0].min(lo), c[0].max(hi)));
        assert!(hi - lo > 40, "texture too flat: {lo}..{hi}");
    }

    #[test]
    fn eyes_are_mostly_open() {
        let open = (0..1000)
            .filter(|i| blink_openness(*i as f32 * 0.01, 0.3) > 0.99)
            .count();
        assert!(open > 900);
        assert!((0..1000).any(|i| blink_openness(i as f32 * 0.01, 0.3) < 0.5));
    }
}
