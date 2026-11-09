//! The Anarcho-Syndicalist Commune of Dave. From issue #3.
//!
//! Parliament has burned down and Dave's militia runs the place. Pure scenery,
//! spawned locally on every peer (nothing to replicate): a smouldering
//! parliament ruin, the commune's flag, Dave himself, and a banner.
//!
//! The flag is themed on the flag of the UK, but with the Welsh dragon in the
//! middle, because Dave isn't a coward (Freyja Moth's correction in #43).
//! It was then made more anarchist (#57): the field is split diagonally red
//! over black like an anarcho-syndicalist flag, the crosses are black, and a
//! circled A is sprayed on the top hoist corner.

use bevy::prelude::*;

use crate::AppState;

pub fn protocol(_app: &mut App) {}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_commune);
}

/// Where the burnt-out parliament stands (x, z).
pub const PARLIAMENT_AT: Vec2 = Vec2::new(-12.0, -10.0);

/// Where Dave himself stands, 7 m from the parliament; the fart-shame check
/// looks for him here.
pub const DAVE_AT: Vec2 = Vec2::new(PARLIAMENT_AT.x - 1.0, PARLIAMENT_AT.y + 7.0);

/// Flag size (width, height), in metres.
const FLAG_W: f32 = 2.4;
const FLAG_H: f32 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlagColour {
    Black,
    White,
    Red,
    Green,
}

/// The outline of a flag piece.
#[derive(Clone, Copy, Debug)]
enum FlagShape {
    /// A rectangle of this size.
    Rect(Vec2),
    /// A triangle with these corners, relative to the piece's `at`.
    Tri([Vec2; 3]),
}

/// One flat piece of the flag, centred at `at` and rotated by `angle`
/// radians. Later pieces are thicker so they sit on top (both sides).
#[derive(Clone, Copy, Debug)]
struct FlagPart {
    colour: FlagColour,
    at: Vec2,
    shape: FlagShape,
    angle: f32,
    depth: f32,
}

/// The commune flag: the Union flag's saltires and cross on an
/// anarcho-syndicalist red-and-black field, with black crosses, a circled A,
/// and a little Welsh flag (white over green) in the middle carrying a red
/// dragon.
fn commune_flag() -> Vec<FlagPart> {
    use FlagColour::*;
    let mut parts = Vec::new();
    let mut layer = 0.04;
    let mut add_shape = |colour, at: Vec2, shape, deg: f32| {
        parts.push(FlagPart {
            colour,
            at,
            shape,
            angle: deg.to_radians(),
            depth: layer,
        });
        layer += 0.004;
    };
    let diag = FLAG_W.hypot(FLAG_H) - 0.25;
    let tilt = FLAG_H.atan2(FLAG_W).to_degrees();
    // Field: red above, black below, split corner to corner (hidden under a
    // saltire, like the Union flag's own diagonals).
    let (hw, hh) = (FLAG_W / 2.0, FLAG_H / 2.0);
    add_shape(
        Red,
        Vec2::ZERO,
        FlagShape::Rect(Vec2::new(FLAG_W, FLAG_H)),
        0.0,
    );
    let corners = [Vec2::new(-hw, -hh), Vec2::new(hw, -hh), Vec2::new(hw, hh)];
    let centroid = (corners[0] + corners[1] + corners[2]) / 3.0;
    add_shape(
        Black,
        centroid,
        FlagShape::Tri(corners.map(|c| c - centroid)),
        0.0,
    );
    let mut add = |colour, x: f32, y: f32, w: f32, h: f32, deg: f32| {
        add_shape(
            colour,
            Vec2::new(x, y),
            FlagShape::Rect(Vec2::new(w, h)),
            deg,
        );
    };
    // Saltires: white, then thinner black on top.
    add(White, 0.0, 0.0, diag, 0.24, tilt);
    add(White, 0.0, 0.0, diag, 0.24, -tilt);
    add(Black, 0.0, 0.0, diag, 0.08, tilt);
    add(Black, 0.0, 0.0, diag, 0.08, -tilt);
    // Cross: white, then black.
    add(White, 0.0, 0.0, FLAG_W, 0.36, 0.0);
    add(White, 0.0, 0.0, 0.36, FLAG_H, 0.0);
    add(Black, 0.0, 0.0, FLAG_W, 0.22, 0.0);
    add(Black, 0.0, 0.0, 0.22, FLAG_H, 0.0);
    // A circled A, sprayed in black on the red top hoist corner.
    let (ax, ay, r) = (-0.4, 0.46, 0.11);
    for i in 0..12 {
        let a = i as f32 * std::f32::consts::TAU / 12.0;
        add(
            Black,
            ax + a.cos() * r,
            ay + a.sin() * r,
            0.025,
            0.062,
            a.to_degrees(),
        );
    }
    add(Black, ax - 0.035, ay, 0.022, 0.21, -18.0);
    add(Black, ax + 0.035, ay, 0.022, 0.21, 18.0);
    add(Black, ax, ay - 0.03, 0.13, 0.022, 0.0);
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
    let trousers = crate::art::soft(&mut materials, Color::srgb(0.25, 0.22, 0.2));
    let rubble = crate::art::soft(&mut materials, Color::srgb(0.5, 0.48, 0.45));
    let red = crate::art::soft(&mut materials, Color::srgb(0.75, 0.1, 0.1));

    let at =
        |x: f32, y: f32, z: f32| Transform::from_xyz(PARLIAMENT_AT.x + x, y, PARLIAMENT_AT.y + z);

    // Charred parliament: a stepped plinth, a broken colonnade with capitals,
    // a cracked arch, scorched walls with jagged tops, and a toppled tower.
    let stone = crate::art::soft(&mut materials, Color::srgb(0.72, 0.68, 0.6));
    let scorch = materials.add(StandardMaterial {
        base_color: Color::srgba(0.05, 0.04, 0.04, 0.75),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 1.0,
        ..default()
    });
    let ember = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.45, 0.1),
        emissive: LinearRgba::rgb(2.0, 0.6, 0.1),
        ..default()
    });
    let block = |commands: &mut Commands,
                 meshes: &mut Assets<Mesh>,
                 t: Transform,
                 size: Vec3,
                 m: &Handle<StandardMaterial>| {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(m.clone()),
            t,
            DespawnOnExit(AppState::InGame),
        ));
    };
    // Steps up to the old facade.
    for (i, (w, d)) in [(9.0, 2.2), (8.4, 1.8), (7.8, 1.4)].into_iter().enumerate() {
        let y = 0.1 + i as f32 * 0.2;
        block(
            &mut commands,
            &mut meshes,
            at(0.0, y, -0.6 + i as f32 * 0.1),
            Vec3::new(w, 0.2, d),
            &stone,
        );
    }
    // Back and side walls, with jagged broken tops.
    for (x, z, w, h, d) in [
        (-3.0, 0.4, 2.2, 2.4, 0.5),
        (0.0, 0.4, 1.4, 1.6, 0.5),
        (3.0, 0.4, 2.2, 1.9, 0.5),
        (-4.0, 2.4, 0.5, 3.0, 3.6),
        (4.0, 2.4, 0.5, 1.2, 3.6),
        (0.0, 4.0, 3.0, 0.8, 0.5),
    ] {
        block(
            &mut commands,
            &mut meshes,
            at(x, h / 2.0, z),
            Vec3::new(w, h, d),
            &char,
        );
        for k in 0..3 {
            let off = (k as f32 - 1.0) * w.max(d) * 0.3;
            let th = 0.2 + crate::art::hash(k, (x * 10.0) as i32, 7) * 0.5;
            let (tx, tz) = if w > d { (x + off, z) } else { (x, z + off) };
            block(
                &mut commands,
                &mut meshes,
                at(tx, h + th / 2.0, tz).with_rotation(Quat::from_rotation_y(off)),
                Vec3::new(w.min(0.5), th, d.min(0.5)),
                &char,
            );
        }
    }
    // Lintel stones over the gaps.
    for x in [-1.5, 1.5] {
        block(
            &mut commands,
            &mut meshes,
            at(x, 1.7, 0.4),
            Vec3::new(0.9, 0.25, 0.55),
            &stone,
        );
    }
    // Broken colonnade: base, shaft, capital on the tall ones.
    let shaft = meshes.add(Cylinder::new(0.28, 1.0).mesh().resolution(10));
    let base = meshes.add(Cuboid::new(0.75, 0.2, 0.75));
    let capital = meshes.add(Cylinder::new(0.4, 0.22).mesh().resolution(10));
    for (i, x) in [-3.0_f32, -1.5, 0.0, 1.5, 3.0].into_iter().enumerate() {
        let h = [2.6, 1.1, 2.6, 0.6, 2.2][i];
        let y0 = 0.6;
        commands.spawn((
            Mesh3d(base.clone()),
            MeshMaterial3d(stone.clone()),
            at(x, y0 + 0.1, -1.0),
            DespawnOnExit(AppState::InGame),
        ));
        commands.spawn((
            Mesh3d(shaft.clone()),
            MeshMaterial3d(stone.clone()),
            at(x, y0 + 0.2 + h / 2.0, -1.0).with_scale(Vec3::new(1.0, h, 1.0)),
            DespawnOnExit(AppState::InGame),
        ));
        if h > 2.0 {
            commands.spawn((
                Mesh3d(capital.clone()),
                MeshMaterial3d(stone.clone()),
                at(x, y0 + 0.3 + h, -1.0),
                DespawnOnExit(AppState::InGame),
            ));
        } else {
            // Snapped column: its fallen drum lies in front.
            commands.spawn((
                Mesh3d(shaft.clone()),
                MeshMaterial3d(stone.clone()),
                at(x + 0.5, 0.28, -2.3)
                    .with_rotation(
                        Quat::from_rotation_y(0.6)
                            * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    )
                    .with_scale(Vec3::new(1.0, 1.2, 1.0)),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    // A cracked arch over the doorway: only part of it still standing.
    for k in 0..6 {
        let ang = k as f32 / 8.0 * std::f32::consts::PI;
        block(
            &mut commands,
            &mut meshes,
            at(-ang.cos(), 1.6 + ang.sin(), 0.4)
                .with_rotation(Quat::from_rotation_z(ang - std::f32::consts::FRAC_PI_2)),
            Vec3::new(0.35, 0.4, 0.55),
            &stone,
        );
    }
    // Scorch marks on the ground and walls, and a few glowing embers.
    let splat = meshes.add(Circle::new(1.0));
    for (x, z, r) in [
        (-2.0, 1.8, 1.4),
        (1.5, 2.6, 1.0),
        (3.2, 1.0, 0.8),
        (-0.5, 3.4, 1.2),
    ] {
        commands.spawn((
            Mesh3d(splat.clone()),
            MeshMaterial3d(scorch.clone()),
            at(x, 0.03, z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::new(r, r * 0.8, 1.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for (x, y) in [(-3.0, 1.4), (3.2, 1.0)] {
        commands.spawn((
            Mesh3d(splat.clone()),
            MeshMaterial3d(scorch.clone()),
            at(x, y, 0.14)
                .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_scale(Vec3::new(0.9, 1.2, 1.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    let ember_mesh = crate::art::chunk(&mut meshes, 0.07);
    for i in 0..7 {
        let a = i as f32 * 2.7;
        commands.spawn((
            Mesh3d(ember_mesh.clone()),
            MeshMaterial3d(ember.clone()),
            at(-2.0 + a.cos() * 0.9, 0.05, 1.8 + a.sin() * 0.7),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // Rubble piles: faceted chunks in mixed tones, plus loose bricks.
    let chunk = crate::art::chunk(&mut meshes, 0.35);
    for i in 0..18 {
        let a = i as f32 * 2.1;
        let r = 2.0 + (i % 4) as f32 * 0.6;
        commands.spawn((
            Mesh3d(chunk.clone()),
            MeshMaterial3d(match i % 3 {
                0 => rubble.clone(),
                1 => stone.clone(),
                _ => char.clone(),
            }),
            at(a.cos() * r, 0.12, 2.0 + a.sin() * 1.8)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, a, a * 0.7, a * 1.3))
                .with_scale(Vec3::splat(0.4 + (i % 3) as f32 * 0.3)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for i in 0..10 {
        let a = i as f32 * 1.37 + 0.4;
        block(
            &mut commands,
            &mut meshes,
            at(a.cos() * 3.4, 0.08, 1.5 + a.sin() * 2.2).with_rotation(Quat::from_euler(
                EulerRot::YXZ,
                a * 3.0,
                0.2 * a.sin(),
                0.0,
            )),
            Vec3::new(0.4, 0.16, 0.2),
            &stone,
        );
    }
    // The toppled tower, its pointed roof knocked off beside it, clock face up.
    block(
        &mut commands,
        &mut meshes,
        at(6.0, 0.5, 0.0).with_rotation(Quat::from_rotation_z(1.3)),
        Vec3::new(1.2, 5.0, 1.2),
        &char,
    );
    commands.spawn((
        Mesh3d(meshes.add(Cone::new(0.95, 1.4).mesh().resolution(4))),
        MeshMaterial3d(rubble.clone()),
        at(8.7, 0.6, 0.4).with_rotation(Quat::from_rotation_z(-1.9)),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(0.4))),
        MeshMaterial3d(materials.add(Color::srgb(0.95, 0.9, 0.75))),
        at(6.6, 1.0, -0.62).with_rotation(
            Quat::from_rotation_y(std::f32::consts::PI) * Quat::from_rotation_z(1.3),
        ),
        DespawnOnExit(AppState::InGame),
    ));

    // Flag on a pole planted in the ruins.
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.06, 5.0))),
        MeshMaterial3d(pole),
        at(1.0, 2.5, 6.0),
        DespawnOnExit(AppState::InGame),
    ));
    let black = materials.add(Color::srgb(0.03, 0.03, 0.03));
    let white = materials.add(Color::srgb(1.0, 1.0, 1.0));
    let green = materials.add(Color::srgb(0.0, 0.55, 0.25));
    let flag_red = materials.add(Color::srgb(0.8, 0.07, 0.15));
    // The flag hangs from a hinge at the pole and swings on it in the breeze
    // (cheap waving that can't come off the pole).
    let hinge = commands
        .spawn((
            at(1.0 + 0.06, 5.0 - FLAG_H / 2.0, 6.0),
            Visibility::default(),
            crate::art::Wiggle::sway(Quat::IDENTITY, Vec3::Y, 0.28, 1.3, 0.0),
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    let flag = commands
        .spawn((
            Transform::from_xyz(FLAG_W / 2.0, 0.0, 0.0),
            Visibility::default(),
            crate::art::Wiggle::sway(Quat::IDENTITY, Vec3::X, 0.08, 2.1, 1.0),
            ChildOf(hinge),
        ))
        .id();
    for part in commune_flag() {
        let m = match part.colour {
            FlagColour::Black => black.clone(),
            FlagColour::White => white.clone(),
            FlagColour::Red => flag_red.clone(),
            FlagColour::Green => green.clone(),
        };
        commands.spawn((
            Mesh3d(match part.shape {
                FlagShape::Rect(size) => meshes.add(Cuboid::new(size.x, size.y, part.depth)),
                FlagShape::Tri([a, b, c]) => {
                    meshes.add(Extrusion::new(Triangle2d::new(a, b, c), part.depth))
                }
            }),
            MeshMaterial3d(m),
            Transform::from_xyz(part.at.x, part.at.y, 0.0)
                .with_rotation(Quat::from_rotation_z(part.angle)),
            ChildOf(flag),
        ));
    }
    // Dave: a little lad in a jumper and beanie, fist raised for the commune.
    let shoe = crate::art::soft(&mut materials, Color::srgb(0.15, 0.1, 0.08));
    let hair = crate::art::soft(&mut materials, Color::srgb(0.35, 0.2, 0.1));
    let cream = crate::art::soft(&mut materials, Color::srgb(0.95, 0.92, 0.85));
    let dark = crate::art::glossy(&mut materials, Color::srgb(0.08, 0.05, 0.05));
    commands
        .spawn((
            at(-1.0, 0.0, 7.0),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|root| {
            // Legs and chunky shoes.
            let leg = meshes.add(Capsule3d::new(0.1, 0.3));
            let shoe_mesh = meshes.add(Sphere::new(0.13));
            for x in [-0.14, 0.14] {
                root.spawn((
                    Mesh3d(leg.clone()),
                    MeshMaterial3d(trousers.clone()),
                    Transform::from_xyz(x, 0.3, 0.0),
                ));
                root.spawn((
                    Mesh3d(shoe_mesh.clone()),
                    MeshMaterial3d(shoe.clone()),
                    Transform::from_xyz(x, 0.07, -0.05).with_scale(Vec3::new(0.9, 0.6, 1.4)),
                ));
            }
            // Breathing torso carries the arms and the head.
            root.spawn((
                Transform::from_xyz(0.0, 0.55, 0.0),
                Visibility::default(),
                crate::art::Breathe::new(Vec3::ONE, 0.03, 1.0),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Capsule3d::new(0.3, 0.35))),
                    MeshMaterial3d(jumper.clone()),
                    Transform::from_xyz(0.0, 0.32, 0.0).with_scale(Vec3::new(1.0, 1.0, 0.85)),
                ));
                // Jumper stripe and hem.
                p.spawn((
                    Mesh3d(meshes.add(Torus::new(0.28, 0.32))),
                    MeshMaterial3d(cream.clone()),
                    Transform::from_xyz(0.0, 0.42, 0.0).with_scale(Vec3::new(1.0, 1.0, 0.85)),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Torus::new(0.27, 0.32))),
                    MeshMaterial3d(trousers.clone()),
                    Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(1.0, 1.0, 0.85)),
                ));
                let hand = meshes.add(Sphere::new(0.09));
                let arm = meshes.add(Capsule3d::new(0.085, 0.32));
                // Relaxed arm, swinging a touch.
                p.spawn((
                    Transform::from_xyz(-0.36, 0.55, 0.0),
                    Visibility::default(),
                    crate::art::Wiggle::sway(Quat::from_rotation_z(-0.25), Vec3::X, 0.1, 1.6, 0.0),
                ))
                .with_children(|a| {
                    a.spawn((
                        Mesh3d(arm.clone()),
                        MeshMaterial3d(jumper.clone()),
                        Transform::from_xyz(0.0, -0.25, 0.0),
                    ));
                    a.spawn((
                        Mesh3d(hand.clone()),
                        MeshMaterial3d(skin.clone()),
                        Transform::from_xyz(0.0, -0.5, 0.0),
                    ));
                });
                // Raised fist, pumping gently.
                p.spawn((
                    Transform::from_xyz(0.36, 0.55, 0.0),
                    Visibility::default(),
                    crate::art::Wiggle::sway(Quat::from_rotation_z(2.9), Vec3::Z, 0.12, 2.2, 0.5),
                ))
                .with_children(|a| {
                    a.spawn((
                        Mesh3d(arm.clone()),
                        MeshMaterial3d(jumper.clone()),
                        Transform::from_xyz(0.0, -0.25, 0.0),
                    ));
                    a.spawn((
                        Mesh3d(hand.clone()),
                        MeshMaterial3d(skin.clone()),
                        Transform::from_xyz(0.0, -0.52, 0.0).with_scale(Vec3::new(1.2, 1.1, 1.1)),
                    ));
                });
                // Big round head with a slight determined tilt.
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.3))),
                    MeshMaterial3d(skin.clone()),
                    Transform::from_xyz(0.0, 0.95, 0.0),
                    crate::art::Wiggle::sway(Quat::IDENTITY, Vec3::Z, 0.06, 0.9, 2.0),
                ))
                .with_children(|h| {
                    crate::art::eyes(
                        h,
                        &mut meshes,
                        &mut materials,
                        Vec3::new(0.0, 0.02, -0.26),
                        0.2,
                        0.055,
                    );
                    crate::art::blush(
                        h,
                        &mut meshes,
                        &mut materials,
                        Vec3::new(0.0, -0.09, -0.24),
                        0.36,
                        0.05,
                    );
                    // Button nose, ears, a cheeky grin.
                    h.spawn((
                        Mesh3d(meshes.add(Sphere::new(0.045))),
                        MeshMaterial3d(skin.clone()),
                        Transform::from_xyz(0.0, -0.06, -0.3),
                    ));
                    let ear = meshes.add(Sphere::new(0.07));
                    for x in [-0.29, 0.29] {
                        h.spawn((
                            Mesh3d(ear.clone()),
                            MeshMaterial3d(skin.clone()),
                            Transform::from_xyz(x, -0.02, 0.0).with_scale(Vec3::new(0.5, 1.0, 0.8)),
                        ));
                    }
                    h.spawn((
                        Mesh3d(meshes.add(Capsule3d::new(0.018, 0.08))),
                        MeshMaterial3d(dark.clone()),
                        Transform::from_xyz(0.0, -0.15, -0.27)
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                    ));
                    // Scruffy fringe under a beanie with a bobble.
                    let tuft = meshes.add(Sphere::new(0.07));
                    for x in [-0.15, -0.05, 0.06, 0.16] {
                        h.spawn((
                            Mesh3d(tuft.clone()),
                            MeshMaterial3d(hair.clone()),
                            Transform::from_xyz(x, 0.15, -0.22),
                        ));
                    }
                    h.spawn((
                        Mesh3d(meshes.add(Sphere::new(0.31))),
                        MeshMaterial3d(red.clone()),
                        Transform::from_xyz(0.0, 0.12, 0.03).with_scale(Vec3::new(1.0, 0.7, 1.0)),
                    ));
                    h.spawn((
                        Mesh3d(meshes.add(Torus::new(0.27, 0.33))),
                        MeshMaterial3d(cream.clone()),
                        Transform::from_xyz(0.0, 0.1, 0.03),
                    ));
                    // Pivot on the beanie so the bobble swings, not just spins.
                    h.spawn((
                        Transform::from_xyz(0.0, 0.24, 0.03),
                        Visibility::default(),
                        crate::art::Wiggle::sway(Quat::IDENTITY, Vec3::Z, 0.3, 2.2, 0.0),
                    ))
                    .with_children(|pivot| {
                        pivot.spawn((
                            Mesh3d(meshes.add(Sphere::new(0.09))),
                            MeshMaterial3d(cream.clone()),
                            Transform::from_xyz(0.0, 0.12, 0.0),
                        ));
                    });
                });
            });
        });

    commands.spawn((
        Text::new("Commune of Dave: power generates parasites"),
        crate::hud::title(),
        crate::hud::slot(crate::hud::HudSlot::TopCenter),
        DespawnOnExit(AppState::InGame),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_is_anarchist_union_flag_with_a_welsh_dragon() {
        let parts = commune_flag();
        assert_eq!(parts[0].colour, FlagColour::Red, "red field...");
        assert_eq!(parts[1].colour, FlagColour::Black, "...split with black");
        assert!(matches!(parts[1].shape, FlagShape::Tri(_)));
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
        // No blue left: the crosses are black now.
        assert!(
            parts[2..]
                .iter()
                .filter(|p| p.colour == FlagColour::Black)
                .count()
                > 15,
            "black crosses and a circled A"
        );
        // Everything stays roughly on the flag.
        assert!(
            parts
                .iter()
                .all(|p| p.at.x.abs() <= FLAG_W / 2.0 && p.at.y.abs() <= FLAG_H / 2.0)
        );
    }
}
