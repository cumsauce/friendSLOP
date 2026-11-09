//! Every peer: the ways in up top, the tunnels below (built once from the
//! pure [`layout`]), and the dark while you're down there.

use avian3d::prelude::{Collider, RigidBody};
use bevy::light::NotShadowCaster;

use super::*;
use crate::AppState;
use crate::features::interact::{Clickable, Clicked};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player};
use crate::world::MainCamera;

pub fn plugin(app: &mut App) {
    app.init_resource::<Below>()
        .add_observer(click)
        .add_systems(OnEnter(AppState::InGame), (build, reset))
        .add_systems(OnExit(AppState::InGame), reset)
        .add_systems(Update, darkness.run_if(in_state(AppState::InGame)));
}

/// Local: is my body in the tunnels, and what the dark replaced.
#[derive(Resource, Default)]
pub struct Below {
    pub inside: bool,
    saved: Option<Saved>,
}

struct Saved {
    fog: Option<DistanceFog>,
    ambient: f32,
    suns: Vec<(Entity, f32)>,
}

fn reset(mut below: ResMut<Below>) {
    *below = Below::default();
}

/// A way in (up top) or its ladder (down below): clicking either uses it.
#[derive(Component)]
struct Hatch(u8);

/// Part of the underground: hidden while you're up top.
#[derive(Component)]
struct Deep;

struct Mats {
    cube: Handle<Mesh>,
    disc: Handle<Mesh>,
}

fn mat(materials: &mut Assets<StandardMaterial>, c: Color, rough: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        perceptual_roughness: rough,
        ..default()
    })
}

fn glow(
    materials: &mut Assets<StandardMaterial>,
    c: Color,
    power: f32,
) -> Handle<StandardMaterial> {
    let l = c.to_linear();
    materials.add(StandardMaterial {
        base_color: c,
        emissive: LinearRgba::rgb(l.red * power, l.green * power, l.blue * power),
        ..default()
    })
}

/// A box. Solid ones block bodies (and stay out of the villagers' navmesh).
fn block(
    commands: &mut Commands,
    m: &Mats,
    mat: &Handle<StandardMaterial>,
    tf: Transform,
    solid: bool,
    deep: bool,
) {
    let mut e = commands.spawn((
        Mesh3d(m.cube.clone()),
        MeshMaterial3d(mat.clone()),
        tf,
        DespawnOnExit(AppState::InGame),
    ));
    if deep {
        e.insert((Deep, NotShadowCaster, Visibility::Hidden));
    }
    if solid {
        e.insert((
            RigidBody::Static,
            Collider::cuboid(1.0, 1.0, 1.0),
            avian_rerecast::ExcludeColliderFromNavmesh,
        ));
    }
}

fn boxed(at: Vec3, size: Vec3) -> Transform {
    Transform::from_translation(at).with_scale(size)
}

fn light(commands: &mut Commands, at: Vec3, color: Color, intensity: f32, range: f32) {
    commands.spawn((
        Deep,
        PointLight {
            color,
            intensity,
            range,
            ..default()
        },
        Transform::from_translation(at),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
}

fn build(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let m = Mats {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        disc: meshes.add(Cylinder::new(1.0, 1.0)),
    };
    build_tops(&mut commands, &m, &mut materials);
    build_tunnels(&mut commands, &m, &mut materials);
}

/// The ways in, up top.
fn build_tops(commands: &mut Commands, m: &Mats, materials: &mut Assets<StandardMaterial>) {
    let iron = mat(materials, Color::srgb(0.16, 0.16, 0.17), 0.55);
    let rim = mat(materials, Color::srgb(0.32, 0.31, 0.3), 0.7);
    let slot = mat(materials, Color::srgb(0.03, 0.03, 0.03), 1.0);
    let door = mat(materials, Color::srgb(0.36, 0.25, 0.16), 0.85);
    let stone = mat(materials, Color::srgb(0.45, 0.44, 0.42), 0.95);
    let board = mat(materials, Color::srgb(0.4, 0.3, 0.2), 0.9);
    for (i, e) in ENTRANCES.iter().enumerate() {
        let h = crate::terrain::height_at(e.at.x, e.at.y);
        let at = Vec3::new(e.at.x, h, e.at.y);
        match e.kind {
            Kind::Manhole => {
                for (r, y, mat) in [(0.68, 0.03, &rim), (0.56, 0.05, &iron)] {
                    commands.spawn((
                        Mesh3d(m.disc.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_translation(at + Vec3::Y * y / 2.0)
                            .with_scale(Vec3::new(r, y, r)),
                        DespawnOnExit(AppState::InGame),
                    ));
                }
                for k in -2..=2 {
                    block(
                        commands,
                        m,
                        &slot,
                        boxed(
                            at + Vec3::new(k as f32 * 0.18, 0.052, 0.0),
                            Vec3::new(0.05, 0.01, 0.7 - 0.1 * (k as f32).abs()),
                        ),
                        false,
                        false,
                    );
                }
            }
            Kind::CellarDoor => {
                // A stone curb and two sloping wooden leaves against the
                // cottage wall (to the -x side).
                block(
                    commands,
                    m,
                    &stone,
                    boxed(at + Vec3::new(0.0, 0.12, 0.0), Vec3::new(1.5, 0.24, 1.9)),
                    false,
                    false,
                );
                for z in [-0.42, 0.42] {
                    block(
                        commands,
                        m,
                        &door,
                        Transform::from_translation(at + Vec3::new(0.0, 0.55, z))
                            .with_rotation(Quat::from_rotation_z(-0.42))
                            .with_scale(Vec3::new(1.5, 0.08, 0.8)),
                        false,
                        false,
                    );
                }
            }
            Kind::Trapdoor => {
                // Floorboards, one a shade darker and a little proud.
                for k in 0..3 {
                    block(
                        commands,
                        m,
                        &board,
                        boxed(
                            at + Vec3::new(0.0, 0.035 + 0.006 * k as f32, (k as f32 - 1.0) * 0.32),
                            Vec3::new(1.0, 0.04, 0.28),
                        ),
                        false,
                        false,
                    );
                }
            }
        }
        commands.spawn((
            Hatch(i as u8),
            Transform::from_translation(at),
            Visibility::default(),
            Clickable::at_height(0.3).radius(80.0),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// The network below, from [`layout`].
fn build_tunnels(commands: &mut Commands, m: &Mats, materials: &mut Assets<StandardMaterial>) {
    let l = layout();
    let looks = |materials: &mut Assets<StandardMaterial>, p: Part| match p {
        Part::Hub | Part::Sewer => (
            mat(materials, Color::srgb(0.2, 0.21, 0.19), 0.9),
            mat(materials, Color::srgb(0.27, 0.3, 0.25), 0.95),
        ),
        Part::Passage => (
            mat(materials, Color::srgb(0.24, 0.2, 0.17), 1.0),
            mat(materials, Color::srgb(0.38, 0.22, 0.16), 0.95),
        ),
        Part::Cellar => (
            mat(materials, Color::srgb(0.3, 0.22, 0.14), 0.9),
            mat(materials, Color::srgb(0.5, 0.47, 0.4), 0.95),
        ),
    };
    let parts = [Part::Hub, Part::Sewer, Part::Passage, Part::Cellar];
    let skins: Vec<_> = parts.iter().map(|p| looks(materials, *p)).collect();
    let skin = |p: Part| &skins[parts.iter().position(|q| *q == p).unwrap_or(0)];
    let (y, top) = (TUNNEL_Y, TUNNEL_Y + TUNNEL_H);
    let (min, max) = l
        .cells
        .keys()
        .fold(((i32::MAX, i32::MAX), (i32::MIN, i32::MIN)), |(a, b), c| {
            ((a.0.min(c.0), a.1.min(c.1)), (b.0.max(c.0), b.1.max(c.1)))
        });

    // Floors and ceilings: one slab per run of same-part cells in a row.
    for z in min.1..=max.1 {
        let mut x = min.0;
        while x <= max.0 {
            let Some(p) = l.part((x, z)) else {
                x += 1;
                continue;
            };
            let start = x;
            while l.part((x + 1, z)) == Some(p) {
                x += 1;
            }
            let a = cell_centre((start, z));
            let b = cell_centre((x, z));
            let mid = (a + b) / 2.0;
            let len = b.x - a.x + CELL;
            for (yy, mat) in [(y - 0.25, &skin(p).0), (top + 0.25, &skin(p).1)] {
                block(
                    commands,
                    m,
                    mat,
                    boxed(Vec3::new(mid.x, yy, mid.y), Vec3::new(len, 0.5, CELL)),
                    true,
                    true,
                );
            }
            x += 1;
        }
    }

    // Walls wherever an open cell meets rock: merged along rows and columns.
    let t = 0.3;
    for z in min.1..=max.1 {
        for side in [-1, 1] {
            let wall_at = |x: i32| l.part((x, z)).filter(|_| !l.open((x, z + side)));
            let mut x = min.0;
            while x <= max.0 {
                let Some(p) = wall_at(x) else {
                    x += 1;
                    continue;
                };
                let start = x;
                while wall_at(x + 1) == Some(p) {
                    x += 1;
                }
                let a = cell_centre((start, z));
                let b = cell_centre((x, z));
                let zz = a.y + side as f32 * (CELL / 2.0 + t / 2.0);
                block(
                    commands,
                    m,
                    &skin(p).1,
                    boxed(
                        Vec3::new((a.x + b.x) / 2.0, y + TUNNEL_H / 2.0, zz),
                        Vec3::new(b.x - a.x + CELL + 2.0 * t, TUNNEL_H, t),
                    ),
                    true,
                    true,
                );
                x += 1;
            }
        }
    }
    for x in min.0..=max.0 {
        for side in [-1, 1] {
            let wall_at = |z: i32| l.part((x, z)).filter(|_| !l.open((x + side, z)));
            let mut z = min.1;
            while z <= max.1 {
                let Some(p) = wall_at(z) else {
                    z += 1;
                    continue;
                };
                let start = z;
                while wall_at(z + 1) == Some(p) {
                    z += 1;
                }
                let a = cell_centre((x, start));
                let b = cell_centre((x, z));
                let xx = a.x + side as f32 * (CELL / 2.0 + t / 2.0);
                block(
                    commands,
                    m,
                    &skin(p).1,
                    boxed(
                        Vec3::new(xx, y + TUNNEL_H / 2.0, (a.y + b.y) / 2.0),
                        Vec3::new(t, TUNNEL_H, b.y - a.y + CELL + 2.0 * t),
                    ),
                    true,
                    true,
                );
                z += 1;
            }
        }
    }

    // Dressing and lights, cell by cell.
    let water = glow(materials, Color::srgb(0.12, 0.18, 0.1), 0.25);
    let bulb = glow(materials, Color::srgb(1.0, 0.9, 0.6), 4.0);
    let flame = glow(materials, Color::srgb(1.0, 0.55, 0.2), 6.0);
    let daylight = glow(materials, Color::srgb(0.8, 0.9, 1.0), 3.0);
    let shelf = mat(materials, Color::srgb(0.35, 0.25, 0.15), 0.9);
    let jars = [
        mat(materials, Color::srgb(0.6, 0.35, 0.2), 0.3),
        mat(materials, Color::srgb(0.45, 0.55, 0.25), 0.3),
        mat(materials, Color::srgb(0.7, 0.6, 0.3), 0.3),
    ];
    for (&c, &p) in &l.cells {
        let o = cell_centre(c);
        let floor = Vec3::new(o.x, y, o.y);
        match p {
            Part::Sewer | Part::Hub => {
                // The water channel runs towards every wet neighbour.
                for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let n = (c.0 + d.0, c.1 + d.1);
                    if !matches!(l.part(n), Some(Part::Sewer | Part::Hub)) {
                        continue;
                    }
                    let dir = Vec3::new(d.0 as f32, 0.0, d.1 as f32);
                    let along = CELL / 2.0 + 0.05;
                    let size = if d.0 != 0 {
                        Vec3::new(along, 0.02, 1.0)
                    } else {
                        Vec3::new(1.0, 0.02, along)
                    };
                    block(
                        commands,
                        m,
                        &water,
                        boxed(floor + dir * (CELL / 4.0) + Vec3::Y * 0.012, size),
                        false,
                        true,
                    );
                }
                if p == Part::Sewer && (c.0 + c.1).rem_euclid(4) == 0 {
                    let at = floor + Vec3::Y * (TUNNEL_H - 0.25);
                    block(
                        commands,
                        m,
                        &bulb,
                        boxed(at, Vec3::splat(0.18)),
                        false,
                        true,
                    );
                    light(
                        commands,
                        at - Vec3::Y * 0.2,
                        Color::srgb(0.95, 1.0, 0.7),
                        18_000.0,
                        9.0,
                    );
                }
            }
            Part::Passage if (c.0 + c.1).rem_euclid(3) == 0 => {
                let at = floor + Vec3::new(CELL / 2.0 - 0.15, 1.9, 0.0);
                block(
                    commands,
                    m,
                    &flame,
                    boxed(at, Vec3::new(0.12, 0.22, 0.12)),
                    false,
                    true,
                );
                light(
                    commands,
                    at - Vec3::X * 0.4,
                    Color::srgb(1.0, 0.6, 0.3),
                    14_000.0,
                    8.0,
                );
            }
            _ => {}
        }
    }
    // The junction: daylight falls through a grate in the ceiling.
    let hub = Vec3::new(0.0, top, 0.0);
    block(
        commands,
        m,
        &daylight,
        boxed(hub - Vec3::Y * 0.02, Vec3::new(1.4, 0.04, 1.4)),
        false,
        true,
    );
    light(
        commands,
        hub - Vec3::Y * 0.6,
        Color::srgb(0.8, 0.9, 1.0),
        90_000.0,
        12.0,
    );
    // The cellar: shelves of jars along its back wall and a bare bulb.
    let cellar = cell_centre(cell_of(ENTRANCES[2].at));
    let back = Vec3::new(cellar.x - CELL * 1.5 + 0.3, y, cellar.y);
    for k in 0..3 {
        let sh = back + Vec3::Y * (0.5 + k as f32 * 0.7);
        block(
            commands,
            m,
            &shelf,
            boxed(sh, Vec3::new(0.5, 0.06, 6.0)),
            false,
            true,
        );
        for j in 0..9 {
            let jar = &jars[(j + k) % jars.len()];
            commands.spawn((
                Deep,
                NotShadowCaster,
                Visibility::Hidden,
                Mesh3d(m.disc.clone()),
                MeshMaterial3d(jar.clone()),
                Transform::from_translation(
                    sh + Vec3::new(0.0, 0.17, -2.6 + j as f32 * 0.65 + 0.1 * k as f32),
                )
                .with_scale(Vec3::new(0.12, 0.28, 0.12)),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    let bare = Vec3::new(cellar.x, top - 0.4, cellar.y);
    block(
        commands,
        m,
        &bulb,
        boxed(bare, Vec3::splat(0.14)),
        false,
        true,
    );
    light(commands, bare, Color::srgb(1.0, 0.8, 0.5), 30_000.0, 10.0);

    // A ladder under every way in, and the hatch seen from below.
    let wood = mat(materials, Color::srgb(0.32, 0.24, 0.16), 0.9);
    let rust = mat(materials, Color::srgb(0.35, 0.22, 0.14), 0.6);
    for (i, e) in ENTRANCES.iter().enumerate() {
        let base = Vec3::new(e.at.x, y, e.at.y);
        let rail = if e.kind == Kind::Manhole {
            &rust
        } else {
            &wood
        };
        // The ladder faces you as you arrive (you arrive on its `out` side).
        let across = Vec3::new(-e.out.y, 0.0, e.out.x) * 0.25;
        for s in [-1.0, 1.0] {
            block(
                commands,
                m,
                rail,
                boxed(
                    base + across * s + Vec3::Y * TUNNEL_H / 2.0,
                    Vec3::new(0.06, TUNNEL_H, 0.06),
                ),
                false,
                true,
            );
        }
        let rung = Quat::from_rotation_y((-e.out.x).atan2(-e.out.y));
        for k in 0..9 {
            block(
                commands,
                m,
                rail,
                Transform::from_translation(base + Vec3::Y * (0.3 + k as f32 * 0.33))
                    .with_rotation(rung)
                    .with_scale(Vec3::new(0.56, 0.04, 0.04)),
                false,
                true,
            );
        }
        if e.kind == Kind::Manhole {
            light(
                commands,
                base + Vec3::Y * (TUNNEL_H - 0.4),
                Color::srgb(0.8, 0.9, 1.0),
                12_000.0,
                6.0,
            );
        }
        commands.spawn((
            Hatch(i as u8),
            Deep,
            Transform::from_translation(base),
            Visibility::Hidden,
            Clickable::at_height(1.4).radius(90.0),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Local clicks: a way in or a ladder.
fn click(ev: On<Clicked>, mut commands: Commands, hatches: Query<&Hatch>) {
    if let Ok(h) = hatches.get(ev.entity) {
        commands.client_trigger(UseHatch { id: h.0 });
    }
}

/// Local: going down puts the sky out and shows the tunnels; coming up
/// brings it all back.
#[allow(clippy::too_many_arguments)]
fn darkness(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    mut below: ResMut<Below>,
    cameras: Query<(Entity, Option<&DistanceFog>), With<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight)>,
    ambient: Option<ResMut<GlobalAmbientLight>>,
    mut deep: Query<&mut Visibility, With<Deep>>,
    toast: Option<ResMut<Toast>>,
) {
    let mine = me
        .and_then(|me| walkers.iter().find(|(p, _)| p.id == me.0))
        .map(|(_, w)| w.pos)
        .filter(|p| super::below(*p));
    if mine.is_some() == below.inside {
        return;
    }
    let Ok((camera, fog)) = cameras.single() else {
        return;
    };
    let Some(mut ambient) = ambient else { return };
    below.inside = mine.is_some();
    let show = if below.inside {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut deep {
        *v = show;
    }
    if let Some(at) = mine {
        below.saved = Some(Saved {
            fog: fog.cloned(),
            ambient: ambient.brightness,
            suns: suns.iter().map(|(e, s)| (e, s.illuminance)).collect(),
        });
        ambient.brightness = 45.0;
        for (_, mut sun) in &mut suns {
            sun.illuminance = 0.0;
        }
        commands.entity(camera).insert(DistanceFog {
            color: Color::srgb(0.015, 0.02, 0.015),
            falloff: FogFalloff::Linear {
                start: 4.0,
                end: 34.0,
            },
            ..default()
        });
        let here = layout().part(cell_of(at.xz()));
        let text = match here {
            Some(Part::Cellar) => "a damp cellar: the tunnels go on from here",
            Some(Part::Passage) => "a secret passage: it joins the sewers",
            _ => "the sewers: every tunnel down here joins up",
        };
        if let Some(mut toast) = toast {
            toast.show(text);
        }
    } else if let Some(saved) = below.saved.take() {
        ambient.brightness = saved.ambient;
        for (e, lux) in saved.suns {
            if let Ok((_, mut sun)) = suns.get_mut(e) {
                sun.illuminance = lux;
            }
        }
        match saved.fog {
            Some(f) => {
                commands.entity(camera).insert(f);
            }
            None => {
                commands.entity(camera).remove::<DistanceFog>();
            }
        }
    }
}
