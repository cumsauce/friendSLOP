//! The Anarcho-Syndicalist Commune of Dave. From issue #3.
//!
//! Parliament has burned down and Dave's militia runs the place. Pure scenery,
//! spawned locally on every peer (nothing to replicate): a smouldering
//! parliament ruin, the commune's flag, Dave himself, and a banner.
//!
//! The flag is themed on the flag of the UK, but with the Welsh dragon in the
//! middle, because Dave isn't a coward (Freyja Moth's correction in #43).

use bevy::prelude::*;

use crate::AppState;

pub fn protocol(_app: &mut App) {}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_commune);
}

/// Where the burnt-out parliament stands (x, z).
pub const PARLIAMENT_AT: Vec2 = Vec2::new(-12.0, -10.0);

/// Flag size (width, height), in metres.
const FLAG_W: f32 = 2.4;
const FLAG_H: f32 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlagColour {
    Blue,
    White,
    Red,
    Green,
}

/// One flat piece of the flag: a rectangle centred at `at`, rotated by
/// `angle` radians. Later pieces are thicker so they sit on top (both sides).
#[derive(Clone, Copy, Debug)]
struct FlagPart {
    colour: FlagColour,
    at: Vec2,
    size: Vec2,
    angle: f32,
    depth: f32,
}

/// The commune flag: the Union flag's blue field, saltires and cross, with a
/// little Welsh flag (white over green) in the middle carrying a red dragon.
fn commune_flag() -> Vec<FlagPart> {
    use FlagColour::*;
    let mut parts = Vec::new();
    let mut layer = 0.04;
    let mut add = |colour, x: f32, y: f32, w: f32, h: f32, deg: f32| {
        parts.push(FlagPart {
            colour,
            at: Vec2::new(x, y),
            size: Vec2::new(w, h),
            angle: deg.to_radians(),
            depth: layer,
        });
        layer += 0.004;
    };
    let diag = FLAG_W.hypot(FLAG_H) - 0.25;
    let tilt = FLAG_H.atan2(FLAG_W).to_degrees();
    add(Blue, 0.0, 0.0, FLAG_W, FLAG_H, 0.0);
    // Saltires: white, then thinner red on top.
    add(White, 0.0, 0.0, diag, 0.24, tilt);
    add(White, 0.0, 0.0, diag, 0.24, -tilt);
    add(Red, 0.0, 0.0, diag, 0.08, tilt);
    add(Red, 0.0, 0.0, diag, 0.08, -tilt);
    // Cross: white, then red.
    add(White, 0.0, 0.0, FLAG_W, 0.36, 0.0);
    add(White, 0.0, 0.0, 0.36, FLAG_H, 0.0);
    add(Red, 0.0, 0.0, FLAG_W, 0.22, 0.0);
    add(Red, 0.0, 0.0, 0.22, FLAG_H, 0.0);
    // Wales, not cowardly left out: white over green...
    add(White, 0.0, 0.125, 0.7, 0.25, 0.0);
    add(Green, 0.0, -0.125, 0.7, 0.25, 0.0);
    // ...and the red dragon, facing the pole, one claw raised.
    for (x, y, w, h, deg) in [
        (0.0, -0.02, 0.28, 0.1, 0.0),     // body
        (-0.15, 0.05, 0.06, 0.15, -30.0), // neck
        (-0.21, 0.12, 0.1, 0.06, 0.0),    // head
        (0.0, 0.1, 0.06, 0.18, -20.0),    // wing
        (0.07, 0.1, 0.05, 0.15, -35.0),   // other wing
        (0.18, 0.02, 0.16, 0.04, 40.0),   // tail
        (0.25, 0.1, 0.04, 0.08, 0.0),     // tail tip
        (-0.08, -0.12, 0.04, 0.1, 0.0),   // back leg
        (0.08, -0.12, 0.04, 0.1, 0.0),    // back leg
        (-0.17, -0.03, 0.1, 0.035, 20.0), // raised claw
    ] {
        add(Red, x, y, w, h, deg);
    }
    parts
}

fn spawn_commune(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let char = materials.add(Color::srgb(0.12, 0.1, 0.09));
    let pole = materials.add(Color::srgb(0.5, 0.4, 0.3));
    let skin = materials.add(Color::srgb(0.9, 0.7, 0.55));
    let jumper = crate::art::soft(&mut materials, Color::srgb(0.2, 0.3, 0.6));
    let jumper_arm = jumper.clone();
    let trousers = crate::art::soft(&mut materials, Color::srgb(0.25, 0.22, 0.2));
    let rubble = crate::art::soft(&mut materials, Color::srgb(0.5, 0.48, 0.45));
    let red = crate::art::soft(&mut materials, Color::srgb(0.75, 0.1, 0.1));

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
    // Broken columns along the old facade, and rubble piles.
    let column = meshes.add(Cylinder::new(0.3, 1.0));
    let chunk = meshes.add(
        Sphere::new(0.35)
            .mesh()
            .ico(0)
            .unwrap_or_else(|_| Sphere::new(0.35).into()),
    );
    for (i, x) in [-3.0_f32, -1.5, 0.0, 1.5, 3.0].into_iter().enumerate() {
        let h = [2.6, 1.1, 1.8, 0.6, 2.2][i];
        commands.spawn((
            Mesh3d(column.clone()),
            MeshMaterial3d(rubble.clone()),
            at(x, h / 2.0, -0.6).with_scale(Vec3::new(1.0, h, 1.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for i in 0..10 {
        let a = i as f32 * 2.1;
        commands.spawn((
            Mesh3d(chunk.clone()),
            MeshMaterial3d(if i % 2 == 0 {
                rubble.clone()
            } else {
                char.clone()
            }),
            at(a.cos() * 3.0, 0.15, 2.0 + a.sin() * 2.0)
                .with_rotation(Quat::from_rotation_y(a))
                .with_scale(Vec3::splat(0.6 + (i % 3) as f32 * 0.3)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.2, 5.0, 1.2))),
        MeshMaterial3d(char),
        at(6.0, 0.5, 0.0).with_rotation(Quat::from_rotation_z(1.3)),
        DespawnOnExit(AppState::InGame),
    ));

    // Flag on a pole planted in the ruins.
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.06, 5.0))),
        MeshMaterial3d(pole),
        at(1.0, 2.5, 6.0),
        DespawnOnExit(AppState::InGame),
    ));
    let blue = materials.add(Color::srgb(0.0, 0.14, 0.49));
    let white = materials.add(Color::srgb(1.0, 1.0, 1.0));
    let green = materials.add(Color::srgb(0.0, 0.55, 0.25));
    let flag_red = materials.add(Color::srgb(0.8, 0.07, 0.15));
    let flag = commands
        .spawn((
            at(1.0 + 0.06 + FLAG_W / 2.0, 5.0 - FLAG_H / 2.0, 6.0),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    for part in commune_flag() {
        let m = match part.colour {
            FlagColour::Blue => blue.clone(),
            FlagColour::White => white.clone(),
            FlagColour::Red => flag_red.clone(),
            FlagColour::Green => green.clone(),
        };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(part.size.x, part.size.y, part.depth))),
            MeshMaterial3d(m),
            Transform::from_xyz(part.at.x, part.at.y, 0.0)
                .with_rotation(Quat::from_rotation_z(part.angle)),
            ChildOf(flag),
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
            ))
            .with_children(|h| {
                crate::art::eyes(
                    h,
                    &mut meshes,
                    &mut materials,
                    Vec3::new(0.0, 0.04, -0.24),
                    0.18,
                    0.05,
                );
                // Scruffy hair and a determined little beanie.
                h.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.29))),
                    MeshMaterial3d(red.clone()),
                    Transform::from_xyz(0.0, 0.1, 0.03).with_scale(Vec3::new(1.0, 0.7, 1.0)),
                ));
                h.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.08))),
                    MeshMaterial3d(red.clone()),
                    Transform::from_xyz(0.0, 0.33, 0.03),
                ));
            });
            let leg = meshes.add(Capsule3d::new(0.1, 0.35));
            for x in [-0.15, 0.15] {
                p.spawn((
                    Mesh3d(leg.clone()),
                    MeshMaterial3d(trousers.clone()),
                    Transform::from_xyz(x, -0.6, 0.0),
                ));
            }
            p.spawn((
                Mesh3d(meshes.add(Capsule3d::new(0.09, 0.4))),
                MeshMaterial3d(jumper_arm.clone()),
                Transform::from_xyz(-0.42, 0.1, 0.0).with_rotation(Quat::from_rotation_z(-0.3)),
            ));
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.15, 0.6, 0.15))),
                MeshMaterial3d(skin),
                Transform::from_xyz(0.45, 0.8, 0.0),
            ));
        });

    commands.spawn((
        Text::new("Commune of Dave: power generates parasites"),
        crate::hud::small(),
        crate::hud::slot(crate::hud::HudSlot::TopCenter),
        DespawnOnExit(AppState::InGame),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_is_union_flag_with_a_welsh_dragon() {
        let parts = commune_flag();
        assert_eq!(parts[0].colour, FlagColour::Blue, "blue field");
        assert!(parts.iter().any(|p| p.colour == FlagColour::Green), "wales");
        // The dragon (red, on top of the Welsh patch) is there.
        let wales = parts
            .iter()
            .position(|p| p.colour == FlagColour::Green)
            .unwrap_or(usize::MAX);
        assert!(
            parts[wales + 1..]
                .iter()
                .all(|p| p.colour == FlagColour::Red)
        );
        assert!(parts.len() > wales + 5);
        // Each piece is layered above the last so nothing z-fights.
        assert!(parts.windows(2).all(|w| w[1].depth > w[0].depth));
        // Everything stays roughly on the flag.
        assert!(
            parts
                .iter()
                .all(|p| p.at.x.abs() <= FLAG_W / 2.0 && p.at.y.abs() <= FLAG_H / 2.0)
        );
    }
}
