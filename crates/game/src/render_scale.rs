//! Render scale (#393): with the "Fast" resolution on desktop, the 3D world is
//! drawn into a smaller picture and stretched over the window, so a weak GPU
//! (a Steam Deck, an old laptop) shades far fewer pixels. The UI stays sharp:
//! it's drawn on top at the window's own size by a small 2D camera.
//!
//! The picture's scale factor is set so its *logical* size matches the
//! window's, so cursor rays and world-to-screen labels keep working unchanged.
//! "Sharp" (High) draws straight to the window as before. In the browser the
//! page picks its pixel ratio instead (`graphics::RESOLUTION_LIVE` is false).
//! How small "Fast" draws is `assets/data/game/render_scale.ron`.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraUpdateSystems, ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::window::{PrimaryWindow, WindowRef};
use serde::Deserialize;

use crate::AppState;
use crate::graphics::{GraphicsQuality, RESOLUTION_LIVE};
use crate::world::MainCamera;

/// Tuning: `assets/data/game/render_scale.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The share of the window's width and height the 3D world is drawn at
    /// with the "Fast" resolution (1 = full size).
    pub fast: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/game/render_scale.ron");

/// Only the presenting camera sees this layer.
const PRESENT_LAYER: usize = 31;

/// The 2D camera that shows the picture (and draws the UI over it).
#[derive(Component)]
struct Present {
    image: Handle<Image>,
    size: UVec2,
}

/// The full-window sprite showing the picture.
#[derive(Component)]
struct Picture;

pub fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, apply.before(CameraUpdateSystems));
}

/// The scale to draw the 3D world at: 1 unless the Resolution is "Fast" on
/// a platform that applies it live. Kept within sane bounds whatever the file
/// says (0, before the file has loaded, means full size).
pub fn scale(sharp: bool, live: bool, fast: f32) -> f32 {
    if sharp || !live || !fast.is_finite() || fast <= 0.0 {
        1.0
    } else {
        fast.clamp(0.25, 1.0)
    }
}

/// The picture's size in pixels for a window this many physical pixels big.
pub fn picture_size(physical: UVec2, scale: f32) -> UVec2 {
    (physical.as_vec2() * scale)
        .round()
        .as_uvec2()
        .max(UVec2::ONE)
}

#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    q: Res<GraphicsQuality>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut RenderTarget, With<MainCamera>>,
    mut present: Query<(Entity, &mut Present)>,
    mut pictures: Query<(Entity, &mut Sprite), With<Picture>>,
    mut images: ResMut<Assets<Image>>,
) {
    let s = scale(q.sharp, RESOLUTION_LIVE, TUNING.get().fast);
    let (Ok(window), Ok(mut target)) = (window.single(), cameras.single_mut()) else {
        // No game camera (the menu): nothing to present.
        for (e, _) in &present {
            commands.entity(e).despawn();
        }
        for (e, _) in &pictures {
            commands.entity(e).despawn();
        }
        return;
    };
    let physical = window.physical_size();
    if s >= 1.0 || physical.min_element() == 0 {
        if !matches!(*target, RenderTarget::Window(_)) {
            *target = RenderTarget::Window(WindowRef::Primary);
        }
        for (e, _) in &present {
            commands.entity(e).despawn();
        }
        for (e, _) in &pictures {
            commands.entity(e).despawn();
        }
        return;
    }
    let size = picture_size(physical, s);
    // Logical size of the picture = the window's, so cursor maths line up.
    let factor = window.scale_factor() * size.x as f32 / physical.x as f32;
    let logical = window.size();
    let current = present.single_mut().ok();
    let fresh = current.as_ref().is_none_or(|(_, p)| p.size != size);
    let image = match current {
        Some((_, ref p)) if !fresh => p.image.clone(),
        _ => {
            let mut img =
                Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
            img.sampler = ImageSampler::linear();
            images.add(img)
        }
    };
    let want = RenderTarget::Image(ImageRenderTarget {
        handle: image.clone(),
        scale_factor: factor,
    });
    let same = match &*target {
        RenderTarget::Image(t) => t.handle == image && t.scale_factor == factor,
        _ => false,
    };
    if !same {
        *target = want;
    }
    match current {
        Some((_, mut p)) if fresh => {
            p.image = image.clone();
            p.size = size;
        }
        Some(_) => {}
        None => {
            commands.spawn((
                Camera2d,
                Camera {
                    order: 50,
                    clear_color: ClearColorConfig::Custom(Color::BLACK),
                    ..default()
                },
                Msaa::Off,
                Tonemapping::None,
                DebandDither::Disabled,
                RenderLayers::layer(PRESENT_LAYER),
                Present {
                    image: image.clone(),
                    size,
                },
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    if let Ok((_, mut sprite)) = pictures.single_mut() {
        if sprite.image != image {
            sprite.image = image;
        }
        if sprite.custom_size != Some(logical) {
            sprite.custom_size = Some(logical);
        }
    } else {
        commands.spawn((
            Sprite {
                image,
                custom_size: Some(logical),
                ..default()
            },
            Transform::default(),
            RenderLayers::layer(PRESENT_LAYER),
            Picture,
            DespawnOnExit(AppState::InGame),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_draws_smaller_only_where_it_applies_live() {
        assert_eq!(scale(true, true, 0.7), 1.0);
        assert_eq!(scale(false, false, 0.7), 1.0);
        assert_eq!(scale(false, true, 0.7), 0.7);
        assert_eq!(scale(false, true, 0.0), 1.0);
        assert_eq!(scale(false, true, 0.1), 0.25);
        assert_eq!(scale(false, true, f32::NAN), 1.0);
        assert_eq!(
            picture_size(UVec2::new(1280, 800), 0.7),
            UVec2::new(896, 560)
        );
        assert_eq!(picture_size(UVec2::new(1, 1), 0.25), UVec2::ONE);
    }
}
