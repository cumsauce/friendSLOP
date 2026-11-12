//! The great storm of '21. From issue #13 by Freyja Moth.
//!
//! In 1921 a great storm (to the British: "the storm of it's a bit windy innit")
//! did untold damage and toppled several governments; the world is still
//! recovering. This is world history, so it shows up as wreckage: fallen
//! trees, a toppled lamppost, scattered roof slates and a memorial plaque.
//! Every so often a leftover gust blows through, rattling the debris, and the
//! plaque's British translation pops up. Local-only scenery on every peer;
//! gusts pause during the Stillness.

use bevy::prelude::*;

use crate::AppState;
use crate::features::stillness::{Stillness, is_frozen};

/// Tuning numbers: `assets/data/base/great_storm.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds between leftover gusts.
    pub gust_every: f32,
    /// How long a gust lasts.
    pub gust_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/base/great_storm.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Gusts>()
        .add_systems(OnEnter(AppState::InGame), (spawn_wreckage, reset_gusts))
        .add_systems(Update, blow.run_if(in_state(AppState::InGame)));
}

/// Local wind clock.
#[derive(Resource, Default)]
pub struct Gusts {
    pub t: f32,
}

impl Gusts {
    /// Strength of the current gust, 0 when calm.
    pub fn strength(&self) -> f32 {
        let phase = self.t % tuning().gust_every;
        let into = phase - (tuning().gust_every - tuning().gust_secs);
        if into <= 0.0 {
            0.0
        } else {
            (into / tuning().gust_secs * std::f32::consts::PI).sin()
        }
    }
}

fn reset_gusts(mut gusts: ResMut<Gusts>) {
    gusts.t = 0.0;
}

/// A piece of debris that rattles in the wind, around its resting transform.
#[derive(Component)]
struct Debris {
    rest: Transform,
    wobble: f32,
}

#[derive(Component)]
struct StormText;

const PLAQUE: &str = "In memory of the great storm of '21, which toppled governments. The world is still recovering.";
const WINDY: &str = "it's a bit windy innit";

fn spawn_wreckage(mut commands: Commands, assets: Res<AssetServer>) {
    // Modeler-made debris (sources in `.modeler/`, prefabs `sp-fallen-tree`,
    // `sp-lamppost-fallen`, `sp-roof-slate`, `sp-memorial-stele`).
    let model = |file: &str| -> WorldAssetRoot {
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
        )
    };
    let debris = |commands: &mut Commands, m: WorldAssetRoot, rest: Transform, wobble: f32| {
        commands.spawn((
            m,
            rest,
            Debris { rest, wobble },
            DespawnOnExit(AppState::InGame),
        ));
    };

    // Fallen trees: trunk lying on its side (its axis 0.25 m up), crown at the
    // top end, upturned root plate at the other.
    for (at, yaw) in [(Vec2::new(10.0, -6.0), 0.4), (Vec2::new(-6.0, 11.0), 2.1)] {
        debris(
            &mut commands,
            model("fallen_tree.gltf"),
            Transform::from_xyz(at.x, 0.25, at.y).with_rotation(Quat::from_rotation_y(yaw)),
            0.03,
        );
    }

    // A toppled lamppost (the model lies along its local z).
    debris(
        &mut commands,
        model("lamppost_fallen.gltf"),
        Transform::from_xyz(14.0, 0.06, 6.0).with_rotation(Quat::from_rotation_y(-0.7)),
        0.03,
    );

    // Roof slates scattered everywhere.
    let slate = model("roof_slate.gltf");
    for i in 0..18 {
        let a = i as f32 * 2.399;
        let r = 5.0 + (i * 7 % 11) as f32;
        debris(
            &mut commands,
            slate.clone(),
            Transform::from_xyz(a.cos() * r, 0.03, a.sin() * r)
                .with_rotation(Quat::from_rotation_y(a * 3.0)),
            0.3,
        );
    }

    // The memorial plaque: a little stone stele.
    commands.spawn((
        model("memorial_stele.gltf"),
        Transform::from_xyz(4.0, 0.5, 8.0),
        DespawnOnExit(AppState::InGame),
    ));

    commands.spawn((
        Text::new(""),
        StormText,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(format!("memorial plaque: {PLAQUE}")),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

fn blow(
    time: Res<Time>,
    mut gusts: ResMut<Gusts>,
    still: Query<&Stillness>,
    mut debris: Query<(&Debris, &mut Transform)>,
    mut text: Query<&mut Text, With<StormText>>,
) {
    if !is_frozen(&still) {
        gusts.t += time.delta_secs();
    }
    let s = gusts.strength();
    for (d, mut tf) in &mut debris {
        let shake = (gusts.t * 9.0 + d.rest.translation.x).sin() * s * d.wobble;
        // Calm most of the time: leave resting debris untouched (#151).
        tf.set_if_neq(Transform {
            translation: d.rest.translation + Vec3::new(shake, 0.0, shake * 0.5),
            rotation: d.rest.rotation * Quat::from_rotation_y(shake),
            ..*tf
        });
    }
    let want = if s > 0.0 { WINDY } else { "" };
    for mut t in &mut text {
        if t.0 != want {
            t.0 = want.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gusts_come_and_go() {
        let at = |t| Gusts { t }.strength();
        assert_eq!(at(0.0), 0.0);
        assert!(at(tuning().gust_every - tuning().gust_secs / 2.0) > 0.9);
        assert_eq!(at(tuning().gust_every + 1.0), 0.0);
    }
}
