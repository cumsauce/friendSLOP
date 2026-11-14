//! The city lights up at night: lit windows, shop fronts, neon and lamp heads
//! glow brighter as it gets dark, and street lamps throw real light onto the
//! pavement. Everything follows [`Darkness`]. Purely local.
//!
//! Glow works on the models' materials by name (`data/base/night_lights.ron`),
//! the same way see-through glass does (`textures.rs`). A model's windows
//! share one material, so they all come on together; different buildings
//! have different window materials. Lamp lights are point lights on the
//! nearest few lamps only, so a street of fifty costs no more than a handful.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use crate::rules::daylight::Darkness;
use crate::shared::world::MainCamera;

pub fn plugin(app: &mut App) {
    app.init_resource::<Darkness>()
        .init_resource::<Glowing>()
        .add_systems(
            Update,
            (tag_glow, apply_glow, add_lamps, light_lamps).chain(),
        );
}

/// `data/base/night_lights.ron`.
#[derive(serde::Deserialize, Default)]
pub struct NightLights {
    /// Materials that glow more at night, first match wins.
    pub glow: Vec<Glow>,
    /// Models that get a real light at night.
    pub lamps: Vec<Lamp>,
    /// How many lamps may be lit at once (the nearest ones).
    pub max_lamps: usize,
    /// Lamps further than this from the camera stay dark (m).
    pub lamp_reach: f32,
}

/// A material name pattern and how bright it glows.
#[derive(serde::Deserialize, Default, Clone, Debug)]
pub struct Glow {
    /// Part of the material's name (`*` matches anything).
    pub name: String,
    /// Its own glow by day and at night, as a multiple of the model's.
    pub day: f32,
    pub night: f32,
    /// Extra glow at night (linear RGB) for materials that have none of their own.
    #[serde(default)]
    pub add: [f32; 3],
}

/// A model that holds a lamp.
#[derive(serde::Deserialize, Default, Clone, Debug)]
pub struct Lamp {
    /// The model file under `assets/`.
    pub model: String,
    /// Where the bulb is in the model (m).
    pub offset: [f32; 3],
    /// Brightness at full dark (lumens), its colour (sRGB) and reach (m).
    pub lumens: f32,
    pub color: [f32; 3],
    pub range: f32,
}

pub static NIGHT_LIGHTS: friendslop_data::Data<NightLights> =
    friendslop_data::Data::ron("data/base/night_lights.ron");

/// Does `pattern` (with `*` wildcards) appear in `name`?
pub fn matches(pattern: &str, name: &str) -> bool {
    let mut rest = name;
    for part in pattern.split('*').filter(|p| !p.is_empty()) {
        let Some(at) = rest.find(part) else {
            return false;
        };
        rest = &rest[at + part.len()..];
    }
    true
}

/// The glow multiplier at `darkness`.
pub fn glow_at(darkness: f32, day: f32, night: f32) -> f32 {
    day + (night - day) * darkness.clamp(0.0, 1.0)
}

/// Lamp brightness at `darkness`: off by day, up through dusk.
pub fn lamp_at(darkness: f32, lumens: f32) -> f32 {
    let x = ((darkness - 0.25) / 0.5).clamp(0.0, 1.0);
    lumens * x * x * (3.0 - 2.0 * x)
}

/// Glowing materials: their own emissive and which rule they follow.
#[derive(Resource, Default)]
struct Glowing {
    materials: HashMap<AssetId<StandardMaterial>, (LinearRgba, Glow)>,
    /// The darkness last applied, or `None` to apply again.
    applied: Option<f32>,
}

fn tag_glow(
    added: Query<
        (
            &MeshMaterial3d<StandardMaterial>,
            &bevy::gltf::GltfMaterialName,
        ),
        Added<bevy::gltf::GltfMaterialName>,
    >,
    materials: Res<Assets<StandardMaterial>>,
    mut glowing: ResMut<Glowing>,
) {
    if added.is_empty() {
        return;
    }
    let tuning = NIGHT_LIGHTS.get();
    for (handle, name) in &added {
        let id = handle.0.id();
        if glowing.materials.contains_key(&id) {
            continue;
        }
        let Some(rule) = tuning.glow.iter().find(|g| matches(&g.name, &name.0)) else {
            continue;
        };
        let Some(m) = materials.get(id) else {
            continue;
        };
        glowing.materials.insert(id, (m.emissive, rule.clone()));
        glowing.applied = None;
    }
}

fn apply_glow(
    darkness: Res<Darkness>,
    mut glowing: ResMut<Glowing>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let d = darkness.0;
    if glowing.applied.is_some_and(|a| (a - d).abs() < 0.02) {
        return;
    }
    glowing.applied = Some(d);
    for (id, (base, rule)) in &glowing.materials {
        let Some(mut m) = materials.get_mut(*id) else {
            continue;
        };
        let k = glow_at(d, rule.day, rule.night);
        let a = d.clamp(0.0, 1.0);
        m.emissive = LinearRgba::new(
            base.red * k + rule.add[0] * a,
            base.green * k + rule.add[1] * a,
            base.blue * k + rule.add[2] * a,
            base.alpha,
        );
    }
}

/// A street lamp's light, a child of its model.
#[derive(Component)]
struct LampLight {
    lumens: f32,
}

fn add_lamps(
    mut commands: Commands,
    added: Query<(Entity, &WorldAssetRoot), Added<WorldAssetRoot>>,
    server: Res<AssetServer>,
) {
    if added.is_empty() {
        return;
    }
    let tuning = NIGHT_LIGHTS.get();
    for (entity, root) in &added {
        let Some(path) = server.get_path(root.0.id()) else {
            continue;
        };
        let model = path.path().to_string_lossy().replace('\\', "/");
        let Some(lamp) = tuning.lamps.iter().find(|l| l.model == model) else {
            continue;
        };
        commands.entity(entity).with_child((
            PointLight {
                intensity: 0.0,
                range: lamp.range,
                color: Color::srgb(lamp.color[0], lamp.color[1], lamp.color[2]),
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(Vec3::from_array(lamp.offset)),
            Visibility::Hidden,
            LampLight {
                lumens: lamp.lumens,
            },
        ));
    }
}

/// Light the nearest lamps as it gets dark; the rest stay hidden (free).
fn light_lamps(
    darkness: Res<Darkness>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut lamps: Query<(
        Entity,
        &GlobalTransform,
        &LampLight,
        &mut PointLight,
        &mut Visibility,
    )>,
    mut near: Local<Vec<(f32, Entity)>>,
) {
    let Ok(eye) = camera.single() else {
        return;
    };
    let tuning = NIGHT_LIGHTS.get();
    let eye = eye.translation();
    near.clear();
    if darkness.0 > 0.25 {
        for (e, tf, ..) in &lamps {
            let d = tf.translation().distance(eye);
            if d < tuning.lamp_reach {
                near.push((d, e));
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(tuning.max_lamps);
    }
    for (e, _, lamp, mut light, mut vis) in &mut lamps {
        let on = near.iter().any(|(_, n)| *n == e);
        let want = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
        let i = if on {
            lamp_at(darkness.0, lamp.lumens)
        } else {
            0.0
        };
        if (light.intensity - i).abs() > lamp.lumens * 0.02 {
            light.intensity = i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_inside_names() {
        assert!(matches("-win-", "nyc-brick-win-1"));
        assert!(matches("neon", "ns-neon-pink-9"));
        assert!(matches("nyc-*-lamp", "nyc-red-lamp-5"));
        assert!(!matches("-win-", "nyc-brick-0"));
        assert!(!matches("nyc-*-lamp", "de-lamp-0"));
    }

    #[test]
    fn glow_and_lamps_follow_the_dark() {
        assert_eq!(glow_at(0.0, 0.2, 2.0), 0.2);
        assert_eq!(glow_at(1.0, 0.2, 2.0), 2.0);
        assert_eq!(lamp_at(0.0, 100.0), 0.0);
        assert_eq!(lamp_at(1.0, 100.0), 100.0);
        assert!(lamp_at(0.5, 100.0) > 0.0 && lamp_at(0.5, 100.0) < 100.0);
    }

    #[test]
    fn the_data_file_parses() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/base/night_lights.ron"
        );
        let Ok(bytes) = std::fs::read(path) else {
            panic!("no {path}");
        };
        assert!(NIGHT_LIGHTS.set_bytes(&bytes), "{path} doesn't parse");
        let t = NIGHT_LIGHTS.get();
        assert!(!t.glow.is_empty() && !t.lamps.is_empty());
        assert!(t.max_lamps > 0 && t.lamp_reach > 0.0);
        for l in &t.lamps {
            let model = concat!(env!("CARGO_MANIFEST_DIR"), "/../game/assets/");
            assert!(
                std::path::Path::new(&format!("{model}{}", l.model)).exists(),
                "no lamp model {}",
                l.model
            );
        }
    }
}
