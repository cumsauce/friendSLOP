//! The Anarcho-Syndicalist Commune of Dave. From issue #3.
//!
//! Parliament has burned down and Dave's militia runs the place. Pure scenery,
//! spawned locally on every peer (nothing to replicate): a smouldering
//! parliament ruin, a red-and-black flag, Dave himself, and a banner.

use bevy::prelude::*;

use crate::AppState;

pub fn protocol(_app: &mut App) {}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_commune);
}

/// Where the burnt-out parliament stands (x, z).
pub const PARLIAMENT_AT: Vec2 = Vec2::new(-12.0, -10.0);

fn spawn_commune(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let char = materials.add(Color::srgb(0.12, 0.1, 0.09));
    let red = materials.add(Color::srgb(0.8, 0.1, 0.1));
    let black = materials.add(Color::srgb(0.05, 0.05, 0.05));
    let pole = materials.add(Color::srgb(0.5, 0.4, 0.3));
    let skin = materials.add(Color::srgb(0.9, 0.7, 0.55));
    let jumper = materials.add(Color::srgb(0.2, 0.3, 0.6));

    let at =
        |x: f32, y: f32, z: f32| Transform::from_xyz(PARLIAMENT_AT.x + x, y, PARLIAMENT_AT.y + z);

    // Charred parliament: broken walls and a fallen tower.
    for (x, z, w, h, d) in [
        (0.0, 0.0, 8.0, 2.0, 0.6),
        (-4.0, 2.0, 0.6, 3.0, 4.0),
        (4.0, 2.0, 0.6, 1.2, 4.0),
        (0.0, 4.0, 3.0, 0.8, 0.6),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(w, h, d))),
            MeshMaterial3d(char.clone()),
            at(x, h / 2.0, z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.2, 5.0, 1.2))),
        MeshMaterial3d(char),
        at(6.0, 0.5, 0.0).with_rotation(Quat::from_rotation_z(1.3)),
        DespawnOnExit(AppState::InGame),
    ));

    // Flag on a pole planted in the ruins: black below, red above (syndicalist).
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.06, 5.0))),
        MeshMaterial3d(pole),
        at(1.0, 2.5, 6.0),
        DespawnOnExit(AppState::InGame),
    ));
    for (y, m) in [(4.6, red), (3.9, black)] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(1.6, 0.7, 0.05))),
            MeshMaterial3d(m),
            at(1.9, y, 6.0),
            DespawnOnExit(AppState::InGame),
        ));
    }

    // Dave: a lad in a jumper, fist raised.
    commands
        .spawn((
            Mesh3d(meshes.add(Capsule3d::new(0.35, 0.8))),
            MeshMaterial3d(jumper),
            at(-1.0, 0.8, 7.0),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Sphere::new(0.28))),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.0, 0.85, 0.0),
            ));
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.15, 0.6, 0.15))),
                MeshMaterial3d(skin),
                Transform::from_xyz(0.45, 0.8, 0.0),
            ));
        });

    commands.spawn((
        Text::new("Commune of Dave: power generates parasites"),
        Node {
            position_type: PositionType::Absolute,
            top: px(8),
            left: percent(35),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}
