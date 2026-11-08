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

/// Seconds between leftover gusts.
pub const GUST_EVERY: f32 = 25.0;
/// How long a gust lasts.
pub const GUST_SECS: f32 = 4.0;

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
        let phase = self.t % GUST_EVERY;
        let into = phase - (GUST_EVERY - GUST_SECS);
        if into <= 0.0 {
            0.0
        } else {
            (into / GUST_SECS * std::f32::consts::PI).sin()
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

fn spawn_wreckage(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let bark = materials.add(Color::srgb(0.45, 0.3, 0.18));
    let leaves = crate::art::soft(&mut materials, Color::srgb(0.3, 0.55, 0.25));
    let slate = materials.add(Color::srgb(0.35, 0.37, 0.42));
    let iron = materials.add(Color::srgb(0.2, 0.2, 0.22));
    let stone = materials.add(Color::srgb(0.75, 0.72, 0.65));
    let trunk = meshes.add(Cylinder::new(0.25, 4.0));
    let crown = meshes.add(
        Sphere::new(1.0)
            .mesh()
            .ico(1)
            .unwrap_or_else(|_| Sphere::new(1.0).into()),
    );
    let tile = meshes.add(Cuboid::new(0.5, 0.05, 0.35));
    let debris = |commands: &mut Commands, mesh, mat, rest: Transform, wobble: f32| {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            rest,
            Debris { rest, wobble },
            DespawnOnExit(AppState::InGame),
        ));
    };

    // Fallen trees: trunk lying on its side, crown at the top end.
    for (at, yaw) in [(Vec2::new(10.0, -6.0), 0.4), (Vec2::new(-6.0, 11.0), 2.1)] {
        let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let dir = Quat::from_rotation_y(yaw) * Vec3::X;
        let base = Vec3::new(at.x, 0.25, at.y);
        debris(
            &mut commands,
            trunk.clone(),
            bark.clone(),
            Transform::from_translation(base).with_rotation(rot),
            0.02,
        );
        debris(
            &mut commands,
            crown.clone(),
            leaves.clone(),
            Transform::from_translation(base + dir * 2.4 + Vec3::Y * 0.6),
            0.08,
        );
        // Extra leafy clumps and an upturned root ball so the trees read as trees.
        let side = Quat::from_rotation_y(yaw) * Vec3::Z;
        for (along, off, up, s) in [
            (1.9, 0.7, 0.4, 0.7),
            (2.9, -0.6, 0.5, 0.75),
            (3.2, 0.3, 0.9, 0.6),
        ] {
            debris(
                &mut commands,
                crown.clone(),
                leaves.clone(),
                Transform::from_translation(base + dir * along + side * off + Vec3::Y * up)
                    .with_scale(Vec3::splat(s)),
                0.1,
            );
        }
        debris(
            &mut commands,
            crown.clone(),
            bark.clone(),
            Transform::from_translation(base - dir * 2.0 + Vec3::Y * 0.2)
                .with_scale(Vec3::new(0.25, 0.55, 0.55)),
            0.01,
        );
    }

    // A toppled lamppost.
    debris(
        &mut commands,
        meshes.add(Cylinder::new(0.06, 3.0)),
        iron,
        Transform::from_xyz(14.0, 0.06, 6.0).with_rotation(
            Quat::from_rotation_y(-0.7) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        ),
        0.03,
    );

    // Roof slates scattered everywhere.
    for i in 0..18 {
        let a = i as f32 * 2.399;
        let r = 5.0 + (i * 7 % 11) as f32;
        debris(
            &mut commands,
            tile.clone(),
            slate.clone(),
            Transform::from_xyz(a.cos() * r, 0.03, a.sin() * r)
                .with_rotation(Quat::from_rotation_y(a * 3.0)),
            0.3,
        );
    }

    // The memorial plaque: a little stone stele.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.2, 1.0, 0.2))),
        MeshMaterial3d(stone),
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
        tf.translation = d.rest.translation + Vec3::new(shake, 0.0, shake * 0.5);
        tf.rotation = d.rest.rotation * Quat::from_rotation_y(shake);
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
        assert!(at(GUST_EVERY - GUST_SECS / 2.0) > 0.9);
        assert_eq!(at(GUST_EVERY + 1.0), 0.0);
    }
}
