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
    app.init_resource::<Pending>().add_systems(
        PostUpdate,
        // Right after this frame's asset events, before the image's first
        // upload.
        (queue, mipmap)
            .chain()
            .after(bevy::asset::AssetEventSystems),
    );
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
