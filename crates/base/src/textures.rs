//! Model textures get mipmaps and a size cap as they load (#227, WeiYa: "much
//! more fps ... less vram ... mipmaps").
//!
//! The nature kit's PNGs are up to 2048² with one level each, so every tree
//! texel was read at full size however far away it stood: distant leaves
//! shimmered and the GPU held ~16 MB per texture. Each `models/` texture is
//! halved to at most [`MAX_SIDE`] and given its full mip chain (built by
//! `art::mips`), which the glTFs' samplers already ask for. Alpha-tested
//! foliage keeps its coverage, so far trees stay leafy. Purely local.

use bevy::asset::AssetPath;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat};

/// The longest side a model texture keeps. The browser build already ships
/// them at 512 (`tools/web-textures`); on the desktop 2048² textures on
/// low-poly trees cost four times the memory for detail you can't see.
pub const MAX_SIDE: u32 = 1024;

pub fn plugin(app: &mut App) {
    app.init_resource::<Pending>()
        .add_systems(
            PostUpdate,
            // Right after this frame's asset events, before the image's first
            // upload.
            (queue, mipmap, no_anisotropy)
                .chain()
                .after(bevy::asset::AssetEventSystems),
        )
        .add_systems(Update, see_through);
}

/// Which models' glass you can see through (the places you can walk into),
/// and how clear it is. `data/base/glass.ron`.
#[derive(serde::Deserialize, Default)]
pub struct Glass {
    /// Model files (under `assets/`) whose glass materials turn see-through.
    pub models: Vec<String>,
    /// How much of the glass colour stays (0 clear, 1 the old solid glass).
    pub opacity: f32,
}

pub static GLASS: friendslop_data::Data<Glass> = friendslop_data::Data::ron("data/base/glass.ron");

/// Should a material called `material` in `model` be see-through?
pub fn is_window(model: &str, material: &str, glass: &Glass) -> bool {
    material.contains("glass")
        && !material.contains("sill")
        && glass.models.iter().any(|m| m == model)
}

/// Glass in a place you can enter turns see-through as it spawns, so you can
/// look out of (and into) it.
fn see_through(
    added: Query<
        (
            &MeshMaterial3d<StandardMaterial>,
            &bevy::gltf::GltfMaterialName,
        ),
        Added<bevy::gltf::GltfMaterialName>,
    >,
    server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let glass = GLASS.get();
    for (handle, name) in &added {
        let Some(path) = server.get_path(handle.0.id()) else {
            continue;
        };
        let model = path.path().to_string_lossy().replace('\\', "/");
        if !is_window(&model, &name.0, &glass) {
            continue;
        }
        let Some(m) = materials.get(handle.0.id()) else {
            continue;
        };
        if matches!(m.alpha_mode, AlphaMode::Blend) {
            continue;
        }
        let Some(mut m) = materials.get_mut(handle.0.id()) else {
            continue;
        };
        m.alpha_mode = AlphaMode::Blend;
        m.base_color = m.base_color.with_alpha(glass.opacity);
        m.double_sided = true;
        m.cull_mode = None;
    }
}

/// Brushed-metal (anisotropic) materials go wrong under the reflection
/// probes: faces blow out to pure white or black, worst on real Apple GPUs.
/// Plain metal looks the same from a step away, so every material loads
/// without it, and keeps it off when it's loaded again: a model re-exported
/// while the game runs comes back as `Modified`, not `Added`, with the glTF's
/// anisotropy on again (clearing it is itself a change, but then it's zero
/// and nothing more happens).
fn no_anisotropy(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for e in events.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = e else {
            continue;
        };
        if materials
            .get(*id)
            .is_some_and(|m| m.anisotropy_strength > 0.0)
            && let Some(mut m) = materials.get_mut(*id)
        {
            m.anisotropy_strength = 0.0;
        }
    }
}

/// `sampler` asking for `n`x anisotropic filtering, if it's a plain trilinear
/// one that doesn't already. Without it a floor or wall seen at a slant falls
/// to a blurry mip level a few metres away; with it the texels along the
/// slant stay sharp. (Filtering must be linear everywhere for the GPU to allow
/// it, which the glTFs' and KTX2s' samplers are.)
pub fn with_anisotropy(
    sampler: &bevy::image::ImageSampler,
    n: u16,
) -> Option<bevy::image::ImageSampler> {
    use bevy::image::{ImageFilterMode::Linear, ImageSampler};
    let ImageSampler::Descriptor(d) = sampler else {
        return None;
    };
    let linear = d.min_filter == Linear && d.mag_filter == Linear && d.mipmap_filter == Linear;
    (linear && d.anisotropy_clamp < n).then(|| {
        let mut d = d.clone();
        d.anisotropy_clamp = n;
        ImageSampler::Descriptor(d)
    })
}

/// The anisotropy model textures ask for (`data/base/graphics.ron`); phones
/// in the browser get at most 4.
fn anisotropy() -> u16 {
    let n = crate::graphics::TUNING.get().anisotropy.clamp(1, 16);
    if cfg!(target_arch = "wasm32") {
        n.min(4)
    } else {
        n
    }
}

/// Newly loaded images still to look at.
#[derive(Resource, Default)]
struct Pending(Vec<AssetId<Image>>);

fn queue(mut events: MessageReader<AssetEvent<Image>>, mut pending: ResMut<Pending>) {
    for e in events.read() {
        if let AssetEvent::Added { id } = e {
            pending.0.push(*id);
        }
    }
}

/// Is this a model texture (and not UI art or something generated)? The
/// modeler's shared textures live in `textures/`: KTX2 with their own mips
/// on the desktop (skipped below, they aren't RGBA8), PNGs in the browser.
fn is_model_texture(path: Option<AssetPath>) -> bool {
    path.is_some_and(|p| p.path().starts_with("models") || p.path().starts_with("textures"))
}

/// The alpha-test cutoff of the first masked material using `id` as its
/// base colour, if any.
fn cutoff_for(id: AssetId<Image>, materials: &Assets<StandardMaterial>) -> Option<f32> {
    materials.iter().find_map(|(_, m)| match m.alpha_mode {
        AlphaMode::Mask(c) if m.base_color_texture.as_ref().is_some_and(|t| t.id() == id) => {
            Some(c)
        }
        _ => None,
    })
}

fn mipmap(
    mut pending: ResMut<Pending>,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    materials: Res<Assets<StandardMaterial>>,
) {
    for id in std::mem::take(&mut pending.0) {
        if !is_model_texture(server.get_path(id)) {
            continue;
        }
        // Sharper at a slant, for every model texture, whatever its format.
        let n = anisotropy();
        if let Some(sampler) = images
            .get(id)
            .and_then(|image| with_anisotropy(&image.sampler, n))
            && let Some(mut image) = images.get_mut(id)
        {
            image.sampler = sampler;
        }
        // Peek first: `get_mut` marks the image changed (a re-upload).
        let Some(image) = images.get(id) else {
            continue;
        };
        let d = &image.texture_descriptor;
        let srgb = match d.format {
            TextureFormat::Rgba8UnormSrgb => true,
            TextureFormat::Rgba8Unorm => false,
            _ => continue,
        };
        if d.dimension != TextureDimension::D2
            || d.mip_level_count != 1
            || d.size.depth_or_array_layers != 1
        {
            continue;
        }
        let (w, h) = (d.size.width, d.size.height);
        let Some(data) = image.data.as_deref() else {
            continue;
        };
        if data.len() != (w * h * 4) as usize {
            continue;
        }
        // glTF's default cutoff when no masked material claims it (yet).
        let cutoff = if friendslop_art::mips::has_alpha(data) {
            Some(cutoff_for(id, &materials).unwrap_or(0.5))
        } else {
            None
        };
        let (all, tw, th, levels) = friendslop_art::mips::build(data, w, h, srgb, cutoff, MAX_SIDE);
        let Some(mut image) = images.get_mut(id) else {
            continue;
        };
        image.data = Some(all);
        let d = &mut image.texture_descriptor;
        d.size.width = tw;
        d.size.height = th;
        d.mip_level_count = levels;
        // Browser (#242): nothing reads a model texture's pixels after this,
        // so let the GPU upload take them instead of keeping a second copy
        // in wasm memory (phones kill tabs well under 1 GB).
        if cfg!(target_arch = "wasm32") {
            image.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};

    /// The modeler's KTX2 textures ship their own mip chain; make sure Bevy's
    /// loader keeps every level (so `mipmap` rightly leaves them alone) and
    /// that the sampler reads them. Skipped when LFS data isn't pulled.
    #[test]
    fn prebaked_ktx2_mips_survive_loading() {
        use bevy::image::{CompressedImageFormats, ImageType};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../game/assets/textures");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut checked = 0;
        for path in entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "ktx2"))
            .take(40)
        {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if !bytes.starts_with(b"\xabKTX 20\xbb") {
                continue; // an LFS pointer
            }
            let image = Image::from_buffer(
                &bytes,
                ImageType::Extension("ktx2"),
                CompressedImageFormats::all(),
                true,
                ImageSampler::Descriptor(ImageSamplerDescriptor::linear()),
                bevy::asset::RenderAssetUsages::default(),
            )
            .unwrap_or_else(|e| panic!("{} didn't load: {e}", path.display()));
            let d = &image.texture_descriptor;
            let full = d.size.width.max(d.size.height).ilog2() + 1;
            assert_eq!(d.mip_level_count, full, "{} lost its mips", path.display());
            checked += 1;
        }
        eprintln!("{checked} KTX2 textures keep their full mip chains");
    }

    #[test]
    fn trilinear_samplers_get_anisotropic_filtering_and_others_are_left_alone() {
        let linear = ImageSampler::Descriptor(ImageSamplerDescriptor::linear());
        let Some(ImageSampler::Descriptor(d)) = with_anisotropy(&linear, 16) else {
            panic!("a linear sampler should be sharpened");
        };
        assert_eq!(d.anisotropy_clamp, 16);
        // Already sharp enough: nothing to change (so no re-upload).
        let sharp = ImageSampler::Descriptor(ImageSamplerDescriptor {
            anisotropy_clamp: 16,
            ..ImageSamplerDescriptor::linear()
        });
        assert!(with_anisotropy(&sharp, 16).is_none());
        // Nearest filtering (pixel art) can't use it.
        let nearest = ImageSampler::Descriptor(ImageSamplerDescriptor {
            mag_filter: ImageFilterMode::Nearest,
            ..ImageSamplerDescriptor::linear()
        });
        assert!(with_anisotropy(&nearest, 16).is_none());
        assert!(with_anisotropy(&ImageSampler::Default, 16).is_none());
    }

    #[test]
    fn anisotropy_stays_off_when_a_material_reloads() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<StandardMaterial>()
            .add_systems(
                PostUpdate,
                no_anisotropy.after(bevy::asset::AssetEventSystems),
            );
        // Kept alive for the whole test.
        let handle = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                anisotropy_strength: 0.8,
                ..default()
            });
        let id = handle.id();
        let strength = |app: &App| {
            app.world()
                .resource::<Assets<StandardMaterial>>()
                .get(id)
                .map(|m| m.anisotropy_strength)
        };
        app.update();
        assert_eq!(strength(&app), Some(0.0), "on load");
        // Reloaded (the model re-exported while the game runs): it comes back.
        if let Some(mut m) = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(id)
        {
            m.anisotropy_strength = 0.6;
        }
        app.update();
        app.update();
        assert_eq!(strength(&app), Some(0.0), "on reload");
    }

    #[test]
    fn only_glass_in_listed_models() {
        let g = Glass {
            models: vec!["models/path-car.gltf".into()],
            opacity: 0.2,
        };
        assert!(is_window("models/path-car.gltf", "ns-glass-1", &g));
        assert!(!is_window("models/path-car.gltf", "ns-steel-1", &g));
        assert!(!is_window("models/path-car.gltf", "glass_sill", &g));
        assert!(!is_window("models/prefabs/nyc-wtc.gltf", "sp-glass-0", &g));
    }

    #[test]
    fn only_model_textures() {
        assert!(is_model_texture(Some(AssetPath::from(
            "models/nature/Leaves.png"
        ))));
        assert!(is_model_texture(Some(AssetPath::from(
            "textures/stone-base_color-d43a110146556e10.png"
        ))));
        assert!(!is_model_texture(Some(AssetPath::from("ui/logo.png"))));
        assert!(!is_model_texture(None));
    }
}
