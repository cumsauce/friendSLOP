//! The whole world is painted in a bright, cheery cartoon-MMO palette, but only in
//! colours cats can see. From issue #12 by just meat and cheese.
//!
//! Cats are dichromats: they see blues and yellows but can't tell red from green.
//! Every `StandardMaterial` is passed through a cat-vision filter once when it is
//! created (red-green collapsed), then "popped" (lighter, more saturated) so it
//! reads like a bright storybook MMO. The sky is a cat-visible bright blue.
//! Purely local visuals, so nothing is replicated.
//!
//! Opt-out: materials made with [`exact_material`] (e.g. team or duel hues that
//! must stay exactly as chosen) skip the filter.

use bevy::prelude::*;

/// Bright storybook sky, before the cat filter.
const SKY: Color = Color::srgb(0.55, 0.8, 1.0);

/// Registered on every peer. Nothing to replicate: colours are local.
pub fn protocol(_app: &mut App) {}

pub fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(cat_palette(SKY)))
        .init_resource::<ExactColors>()
        .add_systems(
            Update,
            paint_new_materials
                .after(friendslop_world::surfaces::SurfaceSet)
                .run_if(on_message::<AssetEvent<StandardMaterial>>),
        );
}

pub use friendslop_art::palette::cat_palette;

/// Materials whose `base_color` must be kept exactly (not cat-filtered).
#[derive(Resource, Default, Debug)]
pub struct ExactColors(pub bevy::platform::collections::HashSet<AssetId<StandardMaterial>>);

/// Add a material that keeps its colours exactly, skipping the cat palette.
pub fn exact_material(
    materials: &mut Assets<StandardMaterial>,
    exact: &mut ExactColors,
    material: impl Into<StandardMaterial>,
) -> Handle<StandardMaterial> {
    let handle = materials.add(material.into());
    exact.0.insert(handle.id());
    handle
}

/// Repaint each material once, right after it is added. Our own edit fires
/// `Modified`, which we ignore, so nothing is filtered twice.
pub(crate) fn paint_new_materials(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    exact: Option<Res<ExactColors>>,
) {
    for event in events.read() {
        let AssetEvent::Added { id } = event else {
            continue;
        };
        if exact.as_ref().is_some_and(|e| e.0.contains(id)) {
            continue;
        }
        let Some(mut mat) = materials.get_mut(*id) else {
            continue;
        };
        mat.base_color = cat_palette(mat.base_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red_green_gap(c: Color) -> f32 {
        let s = c.to_srgba();
        (s.red - s.green).abs()
    }

    #[test]
    fn red_and_green_look_alike_to_cats() {
        for c in [
            Color::srgb(1.0, 0.0, 0.0),
            Color::srgb(0.0, 1.0, 0.0),
            Color::srgb(0.85, 0.12, 0.15),
            Color::srgb(0.35, 0.5, 0.35),
        ] {
            let out = cat_palette(c);
            assert!(red_green_gap(out) < 0.1, "{c:?} -> {out:?}");
        }
    }

    #[test]
    fn blue_stays_blue_and_black_is_lifted() {
        let blue = cat_palette(Color::srgb(0.1, 0.2, 1.0)).to_srgba();
        assert!(blue.blue > blue.red + 0.3, "{blue:?}");
        let black = cat_palette(Color::BLACK).to_srgba();
        assert!(black.red > 0.05, "shadows should be lifted: {black:?}");
    }

    /// The map's surfaces pick a texture from a material's own colour, so
    /// they must see it before the cat repaints it.
    #[test]
    fn surfaces_go_first_then_the_cat_repaints() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<StandardMaterial>()
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .add_plugins((friendslop_world::surfaces::plugin, plugin));
        let brown = Color::srgb(0.45, 0.32, 0.2);
        let wood = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: brown,
                ..default()
            });
        for _ in 0..4 {
            app.update();
        }
        let mats = app.world().resource::<Assets<StandardMaterial>>();
        let Some(m) = mats.get(&wood) else {
            panic!("material missing");
        };
        assert!(m.normal_map_texture.is_some(), "surfaced as wood");
        assert_eq!(m.base_color, cat_palette(brown));
    }

    #[test]
    fn new_materials_get_painted_once() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<StandardMaterial>()
            .add_systems(Update, paint_new_materials);
        let red = Color::srgb(1.0, 0.0, 0.0);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(red);
        for _ in 0..4 {
            app.update();
        }
        let mats = app.world().resource::<Assets<StandardMaterial>>();
        let Some(mat) = mats.get(&handle) else {
            panic!("material missing");
        };
        assert_eq!(mat.base_color, cat_palette(red));
    }

    #[test]
    fn exact_materials_keep_their_colour() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<StandardMaterial>()
            .init_resource::<ExactColors>()
            .add_systems(Update, paint_new_materials);
        let red = Color::srgb(1.0, 0.0, 0.0);
        let handle = app
            .world_mut()
            .resource_scope(|world, mut exact: Mut<ExactColors>| {
                let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
                exact_material(&mut mats, &mut exact, red)
            });
        for _ in 0..4 {
            app.update();
        }
        let mats = app.world().resource::<Assets<StandardMaterial>>();
        let Some(mat) = mats.get(&handle) else {
            panic!("material missing");
        };
        assert_eq!(mat.base_color, red);
    }
}
