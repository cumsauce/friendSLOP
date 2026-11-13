//! Reflections: baked environment maps (see `friendslop_base::probes`).
//!
//! Playing: the camera carries the world map ("world", baked outdoors), so
//! shiny things reflect the sky and the commune instead of black. Places that
//! turn the sky out (the warehouse, the sewers, Fulton St, night) dim the
//! ambient light, and the world map dims with it, so nothing indoors mirrors
//! a sunny sky; places with their own probe use that inside its box.
//!
//! Baking: `FRIENDSLOP_BAKE=1 cargo run -- host` waits for the world to load,
//! then for each probe (the world map first, then every
//! `ProbeBakes` a feature added) stands the host there, lets it settle, takes
//! six HDR pictures (one per axis, 90 degrees each), turns them into a
//! prefiltered cubemap (`friendslop_art::envmap`) and writes
//! `assets/probes/<name>_{diffuse,specular}.ktx2`; then it quits. Re-run it
//! when a place changes.

use bevy::camera::{Exposure, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::EnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use friendslop_art::envmap::Cube;
use friendslop_base::probes::{self, ProbeBakes};
use serde::Deserialize;

use crate::world::MainCamera;

/// Tuning: `assets/data/game/probes.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The world map's brightness (1 = as baked).
    pub intensity: f32,
    /// The ambient light the world map was baked under (the outdoor daytime
    /// ambient); it dims in step when a place or the night turns ambient down.
    pub outdoor_ambient: f32,
    /// Where the world map is baked: a spot on the commune map, and how far
    /// above its ground (m).
    pub world_spot: String,
    pub world_height: f32,
    /// The pictures' size (px), and the specular map's size and mip count.
    pub capture: u32,
    pub face: usize,
    pub levels: usize,
    /// The diffuse map's size (px).
    pub diffuse: usize,
    /// Seconds to let a place load and settle before its pictures.
    pub settle_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/game/probes.ron");

pub fn plugin(app: &mut App) {
    app.init_resource::<ProbeBakes>();
    if probes::baking() {
        app.init_resource::<Bake>()
            .add_systems(Update, bake.run_if(in_state(crate::AppState::InGame)));
    } else if !cfg!(target_arch = "wasm32") {
        // Browsers (#242) skip reflections: two more cubemaps to download,
        // keep in a phone's memory and sample in every material.
        app.add_observer(world_map)
            .add_systems(Update, dim_with_ambient);
    }
}

/// The camera gets the world map.
fn world_map(add: On<Add<MainCamera>>, mut commands: Commands, assets: Res<AssetServer>) {
    let t = TUNING.get();
    commands
        .entity(add.entity)
        .insert(probes::env_map(&assets, "world", t.intensity));
}

/// The world map dims with the ambient light (night, dark places).
fn dim_with_ambient(
    ambient: Option<Res<GlobalAmbientLight>>,
    mut maps: Query<&mut EnvironmentMapLight, With<MainCamera>>,
) {
    let Some(ambient) = ambient else { return };
    let t = TUNING.get();
    let k = (ambient.brightness / t.outdoor_ambient.max(1.0)).clamp(0.0, 1.0);
    let want = t.intensity * k;
    for mut m in &mut maps {
        if (m.intensity - want).abs() > 1e-3 {
            m.intensity = want;
        }
    }
}

// ------------------------------------------------------------------ baking

/// The six directions pictures are taken in.
const DIRS: [Vec3; 6] = [
    Vec3::X,
    Vec3::NEG_X,
    Vec3::Y,
    Vec3::NEG_Y,
    Vec3::Z,
    Vec3::NEG_Z,
];

fn up_for(d: Vec3) -> Vec3 {
    if d.y.abs() > 0.5 {
        Vec3::NEG_Z
    } else {
        Vec3::Y
    }
}

#[derive(Default)]
enum Stage {
    #[default]
    Start,
    /// Standing at the probe, waiting for it to settle.
    Settle(f32),
    /// Pictures asked for (frames waited).
    Shoot(u32),
    Wait,
}

#[derive(Resource, Default)]
struct Bake {
    queue: Option<Vec<(String, Vec3)>>,
    stage: Stage,
    targets: Vec<Handle<Image>>,
    cams: Vec<Entity>,
    got: Vec<Option<Image>>,
}

#[allow(clippy::too_many_arguments)]
fn bake(
    mut commands: Commands,
    mut job: ResMut<Bake>,
    bakes: Res<ProbeBakes>,
    ready: Res<crate::loading::WorldReady>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    main: Query<Option<&DistanceFog>, With<MainCamera>>,
    mut walkers: Query<(
        &crate::player::PlayerOwner,
        &mut crate::features::fps::Walker,
    )>,
    mut exit: MessageWriter<AppExit>,
) {
    if !ready.ready {
        return;
    }
    let t = TUNING.get();
    let queue = job.queue.get_or_insert_with(|| {
        let p = friendslop_world::places::at(&t.world_spot);
        let ground = crate::terrain::height_at(p.x, p.y);
        let mut q = vec![(
            "world".to_owned(),
            Vec3::new(p.x, ground + t.world_height, p.y),
        )];
        q.extend(bakes.0.iter().cloned());
        info!("bake: {} probes", q.len());
        q
    });
    let Some((name, at)) = queue.first().cloned() else {
        exit.write(AppExit::Success);
        return;
    };
    match job.stage {
        Stage::Start => {
            // Stand the host there (eye height above the feet).
            if let Some((_, mut w)) = walkers.iter_mut().find(|(o, _)| o.0.is_none()) {
                w.pos = at - Vec3::Y * 1.6;
            }
            info!("bake: {name} at {at}");
            job.stage = Stage::Settle(t.settle_secs);
        }
        Stage::Settle(left) => {
            if let Some((_, mut w)) = walkers.iter_mut().find(|(o, _)| o.0.is_none()) {
                w.pos = at - Vec3::Y * 1.6;
            }
            let left = left - time.delta_secs();
            if left > 0.0 {
                job.stage = Stage::Settle(left);
                return;
            }
            let fog = main.iter().next().flatten().cloned();
            job.targets.clear();
            job.cams.clear();
            job.got = vec![None; 6];
            for (i, d) in DIRS.iter().enumerate() {
                let img = images.add(Image::new_target_texture(
                    t.capture,
                    t.capture,
                    TextureFormat::Rgba16Float,
                    None,
                ));
                let mut cam = commands.spawn((
                    Camera3d::default(),
                    Camera {
                        order: -20 - i as isize,
                        ..default()
                    },
                    RenderTarget::Image(img.clone().into()),
                    Projection::Perspective(PerspectiveProjection {
                        fov: std::f32::consts::FRAC_PI_2,
                        aspect_ratio: 1.0,
                        ..default()
                    }),
                    bevy::camera::Hdr,
                    Tonemapping::None,
                    Exposure::default(),
                    Transform::from_translation(at).looking_to(*d, up_for(*d)),
                ));
                if let Some(f) = fog.clone() {
                    cam.insert(f);
                }
                job.cams.push(cam.id());
                job.targets.push(img);
            }
            job.stage = Stage::Shoot(0);
        }
        Stage::Shoot(frames) => {
            // Let the cameras render a few frames (shadows, streaming) first.
            if frames < 20 {
                job.stage = Stage::Shoot(frames + 1);
                return;
            }
            for (i, img) in job.targets.clone().into_iter().enumerate() {
                commands.spawn(Screenshot::image(img)).observe(
                    move |ev: On<ScreenshotCaptured>, mut job: ResMut<Bake>| {
                        if let Some(slot) = job.got.get_mut(i) {
                            *slot = Some(ev.image.clone());
                        }
                    },
                );
            }
            job.stage = Stage::Wait;
        }
        Stage::Wait => {
            if job.got.iter().any(Option::is_none) {
                return;
            }
            let shots: Vec<Image> = job.got.drain(..).flatten().collect();
            let scale = 1.0 / Exposure::default().exposure();
            let cube = assemble(&shots, (t.face * 2).max(16)).scaled(scale);
            write(&name, &cube, &t);
            for e in job.cams.drain(..) {
                commands.entity(e).despawn();
            }
            if let Some(q) = job.queue.as_mut() {
                q.remove(0);
            }
            job.stage = Stage::Start;
        }
    }
}

/// One texel of an Rgba16Float picture (row-major, `size` px square).
fn texel(img: &Image, size: u32, x: u32, y: u32) -> Vec3 {
    let Some(data) = img.data.as_ref() else {
        return Vec3::ZERO;
    };
    let i = ((y.min(size - 1) * size + x.min(size - 1)) * 8) as usize;
    let h = |o: usize| {
        data.get(i + o..i + o + 2)
            .map_or(0.0, |b| f16_to_f32(u16::from_le_bytes([b[0], b[1]])))
    };
    Vec3::new(h(0), h(2), h(4))
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let man = (h & 0x3ff) as f32;
    let v = match exp {
        0 => man * 2f32.powi(-24),
        31 => 65504.0,
        e => (1.0 + man / 1024.0) * 2f32.powi(e - 15),
    };
    sign * v
}

/// The six pictures as one cube: each direction read from the picture
/// looking most that way.
fn assemble(shots: &[Image], size: usize) -> Cube {
    let rots: Vec<Quat> = DIRS
        .iter()
        .map(|d| Transform::default().looking_to(*d, up_for(*d)).rotation)
        .collect();
    Cube::from_fn(size, |w| {
        let Some(i) = (0..6).max_by(|&a, &b| DIRS[a].dot(w).total_cmp(&DIRS[b].dot(w))) else {
            return Vec3::ZERO;
        };
        let Some(img) = shots.get(i) else {
            return Vec3::ZERO;
        };
        let s = img.width();
        let c = rots[i].inverse() * w;
        if c.z >= 0.0 {
            return Vec3::ZERO;
        }
        let (u, v) = (c.x / -c.z, c.y / -c.z);
        let px = ((u + 1.0) / 2.0 * s as f32) as u32;
        let py = ((1.0 - v) / 2.0 * s as f32) as u32;
        texel(img, s, px, py).max(Vec3::ZERO)
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn write(name: &str, cube: &Cube, t: &Tuning) {
    use friendslop_art::envmap;
    let root = std::env::var("BEVY_ASSET_ROOT").unwrap_or_else(|_| ".".into());
    let dir = std::path::Path::new(&root).join("assets/probes");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        error!("bake: can't make {}: {e}", dir.display());
        return;
    }
    let (d, s) = probes::files(name);
    let spec = envmap::specular(cube, t.face, t.levels);
    let diff = envmap::diffuse(cube, t.diffuse);
    for (file, levels) in [(s, spec), (d, vec![diff])] {
        let path = std::path::Path::new(&root).join("assets").join(&file);
        match std::fs::write(&path, envmap::ktx2(&levels)) {
            Ok(()) => info!("bake: wrote {}", path.display()),
            Err(e) => error!("bake: can't write {}: {e}", path.display()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn write(_: &str, _: &Cube, _: &Tuning) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_decode() {
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0x4000), 2.0);
        assert_eq!(f16_to_f32(0x3800), 0.5);
        assert_eq!(f16_to_f32(0), 0.0);
    }

    #[test]
    fn each_picture_looks_its_way() {
        // A picture's middle is straight down its direction.
        for d in DIRS {
            let r = Transform::default().looking_to(d, up_for(d)).rotation;
            assert!((r * Vec3::NEG_Z).distance(d) < 1e-5);
        }
    }
}
