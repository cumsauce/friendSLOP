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
/// without it.
fn no_anisotropy(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for e in events.read() {
        let AssetEvent::Added { id } = e else {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
