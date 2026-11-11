//! Every peer: the ways in up top, the tunnels below (built from
//! [`Tunnels`] with the structures hallway kit, and rebuilt whenever the
//! world seed changes them), and the dark while you're down there.

use std::collections::HashMap;

use avian3d::prelude::{Collider, RigidBody};
use bevy::light::NotShadowCaster;
use friendslop_structures::build::{batches, bodies};
use friendslop_structures::hall::{Fixture, Hall, Style, hall};
use friendslop_structures::{Kit, Palette, Solid};

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
        .add_systems(
            Update,
            (
                build_tunnels.run_if(resource_exists_and_changed::<Tunnels>),
                show_new,
                darkness,
                guide,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
}

/// The "way up" line shown while you're down there.
#[derive(Component)]
struct Guide;

/// While you're below: how far the nearest ladder is along the tunnels and
/// which way to turn for it (checked a few times a second).
#[allow(clippy::too_many_arguments)]
fn guide(
    mut commands: Commands,
    time: Res<Time>,
    below: Res<Below>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut line: Query<&mut Text, With<Guide>>,
    tunnels: Option<Res<Tunnels>>,
    mut wait: Local<f32>,
) {
    if line.is_empty() {
        commands.spawn((
            Guide,
            Text::new(""),
            crate::hud::slot(crate::hud::HudSlot::TopCenter),
            DespawnOnExit(AppState::InGame),
        ));
        return;
    }
    *wait -= time.delta_secs();
    if *wait > 0.0 && below.inside {
        return;
    }
    *wait = 0.25;
    let want = if let (true, Some(l)) = (below.inside, tunnels.as_deref()) {
        let at = me
            .and_then(|me| walkers.iter().find(|(p, _)| p.id == me.0))
            .map(|(_, w)| w.pos.xz());
        let fwd = camera
            .single()
            .map(|c| c.forward().as_vec3().xz())
            .unwrap_or(Vec2::NEG_Y);
        match at.and_then(|a| way_up(l, a)) {
            Some((_, m, dir)) if dir == Vec2::ZERO => {
                format!("Way up: the ladder is right here ({m:.0} m). Click it to climb out")
            }
            Some((_, m, dir)) => {
                format!(
                    "Way up: nearest ladder {m:.0} m, {}. Click it to climb out",
                    turn_word(fwd, dir)
                )
            }
            None => "Way up: find a ladder and click it to climb out".to_string(),
        }
    } else {
        String::new()
    };
    for mut t in &mut line {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
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

/// Built from the current [`Tunnels`]: replaced when they change.
#[derive(Component)]
struct Tunnel;

struct Mats {
    cube: Handle<Mesh>,
}

/// A model that's part of the underground (hidden while you're up top).
fn deep_model(
    commands: &mut Commands,
    assets: &AssetServer,
    file: &str,
    tf: Transform,
    show: Visibility,
) {
    commands.spawn((
        Deep,
        Tunnel,
        NotShadowCaster,
        show,
        model(assets, file),
        tf,
        DespawnOnExit(AppState::InGame),
    ));
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
        e.insert((Deep, Tunnel, NotShadowCaster, Visibility::Hidden));
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
        Tunnel,
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

fn build(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    let Some(assets) = assets else {
        return;
    };
    build_tops(&mut commands, &assets);
}

/// A modeler-made model in `assets/models/features` (sources in `.modeler/`).
fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

/// The ways in, up top: a manhole, a cellar door against the cottage wall
/// (to the -x side) and a trapdoor in the floorboards, each a model.
fn build_tops(commands: &mut Commands, assets: &AssetServer) {
    for (i, e) in ENTRANCES.iter().enumerate() {
        let h = crate::terrain::height_at(e.at.x, e.at.y);
        let at = Vec3::new(e.at.x, h, e.at.y);
        let file = match e.kind {
            Kind::Manhole => "manhole.gltf",
            Kind::CellarDoor => "cellar_door.gltf",
            Kind::Trapdoor => "trapdoor.gltf",
        };
        commands.spawn((
            model(assets, file),
            Transform::from_translation(at),
            DespawnOnExit(AppState::InGame),
        ));
        commands.spawn((
            Hatch(i as u8),
            Transform::from_translation(at),
            Visibility::default(),
            Clickable::at_height(0.3)
                .radius(80.0)
                .labelled("climb down"),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// The tunnel kit's materials.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum M {
    Hidden,
    Floor(Part),
    Wall(Part),
    Trim(Part),
    Metal,
    Water,
    Bulb,
    Flame,
}

impl Palette for M {
    const HIDDEN: Self = M::Hidden;
    const TIMBER: Self = M::Trim(Part::Passage);
    const WOOD: Self = M::Trim(Part::Cellar);
    const PLANK: Self = M::Trim(Part::Cellar);
    const CLOTH: Self = M::Wall(Part::Cellar);
    const LINEN: Self = M::Wall(Part::Cellar);
    const FRUIT: Self = M::Wall(Part::Cellar);
    const RED: Self = M::Wall(Part::Passage);
    const IRON: Self = M::Metal;
    const LAMP: Self = M::Bulb;
}

fn color(m: M) -> (Color, f32, f32) {
    let looks = |p: Part| match p {
        Part::Hub | Part::Sewer => (Color::srgb(0.2, 0.21, 0.19), Color::srgb(0.27, 0.3, 0.25)),
        Part::Passage => (Color::srgb(0.24, 0.2, 0.17), Color::srgb(0.38, 0.22, 0.16)),
        Part::Cellar => (Color::srgb(0.3, 0.22, 0.14), Color::srgb(0.5, 0.47, 0.4)),
    };
    match m {
        M::Hidden => (Color::NONE, 1.0, 0.0),
        M::Floor(p) => (looks(p).0, 0.9, 0.0),
        M::Wall(p) => (looks(p).1, 0.95, 0.0),
        M::Trim(p) => (looks(p).1.darker(0.08), 0.85, 0.0),
        M::Metal => (Color::srgb(0.2, 0.17, 0.14), 0.5, 0.0),
        M::Water => (Color::srgb(0.12, 0.18, 0.1), 0.15, 0.25),
        M::Bulb => (Color::srgb(1.0, 0.9, 0.6), 0.5, 4.0),
        M::Flame => (Color::srgb(1.0, 0.55, 0.2), 0.5, 6.0),
    }
}

fn style(p: Part) -> Style<M> {
    let wet = matches!(p, Part::Hub | Part::Sewer);
    Style {
        floor: M::Floor(p),
        wall: M::Wall(p),
        ceiling: M::Floor(p),
        trim: M::Trim(p),
        metal: M::Metal,
        water: M::Water,
        glow: if p == Part::Passage {
            M::Flame
        } else {
            M::Bulb
        },
        wet,
        pipes: wet,
        beams: p != Part::Hub,
        fixture: match p {
            Part::Sewer => Fixture::Ceiling,
            Part::Passage => Fixture::Torch,
            Part::Hub | Part::Cellar => Fixture::None,
        },
        every: if p == Part::Passage { 3 } else { 4 },
    }
}

/// The network below, from [`Tunnels`]: the old one goes, the new one is
/// laid with the hallway kit, merged into a few meshes and colliders.
#[allow(clippy::too_many_arguments)]
fn build_tunnels(
    mut commands: Commands,
    tunnels: Res<Tunnels>,
    old: Query<Entity, With<Tunnel>>,
    below: Res<Below>,
    assets: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(assets), Some(mut meshes), Some(mut materials)) = (assets, meshes, materials) else {
        return;
    };
    for e in &old {
        commands.entity(e).despawn();
    }
    let m = Mats {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
    };
    let l = &tunnels.0;
    let (y, top) = (TUNNEL_Y, TUNNEL_Y + TUNNEL_H);
    let mut kit = Kit::<M>::default();
    kit.place(Vec3::ZERO, 0);
    let spec = Hall {
        cell: CELL,
        floor: y,
        height: TUNNEL_H,
        thick: 0.3,
    };
    let cells: Vec<Cell> = l.cells.keys().copied().collect();
    hall(&mut kit, &spec, &cells, |c| l.part(c).map(style));
    let mut handles: HashMap<M, Handle<StandardMaterial>> = HashMap::new();
    let show = if below.inside {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for batch in batches(&kit.pieces) {
        if batch.mat == M::Hidden {
            continue;
        }
        let handle = handles
            .entry(batch.mat)
            .or_insert_with(|| {
                let (c, rough, power) = color(batch.mat);
                if power > 0.0 {
                    glow(&mut materials, c, power)
                } else {
                    mat(&mut materials, c, rough)
                }
            })
            .clone();
        commands.spawn((
            Deep,
            Tunnel,
            NotShadowCaster,
            show,
            Mesh3d(meshes.add(batch.mesh)),
            MeshMaterial3d(handle),
            Transform::from_translation(batch.origin),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // Colliders as before: plain static bodies, kept out of the navmesh.
    for body in bodies(&kit.pieces) {
        if body.solid == Solid::None {
            continue;
        }
        commands.spawn((
            Tunnel,
            Transform::from_translation(body.centre).with_rotation(body.rot),
            RigidBody::Static,
            body.collider,
            avian_rerecast::ExcludeColliderFromNavmesh,
            DespawnOnExit(AppState::InGame),
        ));
    }
    for at in &kit.lights {
        if l.part(cell_of(at.xz())) == Some(Part::Passage) {
            light(
                &mut commands,
                *at,
                Color::srgb(1.0, 0.6, 0.3),
                14_000.0,
                8.0,
            );
        } else {
            light(
                &mut commands,
                *at,
                Color::srgb(0.95, 1.0, 0.7),
                18_000.0,
                9.0,
            );
        }
    }

    // Dressing.
    let daylight = glow(&mut materials, Color::srgb(0.8, 0.9, 1.0), 3.0);
    let bulb = glow(&mut materials, Color::srgb(1.0, 0.9, 0.6), 4.0);
    // The junction: daylight falls through a grate in the ceiling.
    let hub = Vec3::new(0.0, top, 0.0);
    block(
        &mut commands,
        &m,
        &daylight,
        boxed(hub - Vec3::Y * 0.02, Vec3::new(1.4, 0.04, 1.4)),
        false,
        true,
    );
    deep_model(
        &mut commands,
        &assets,
        "grate.gltf",
        Transform::from_translation(hub),
        show,
    );
    light(
        &mut commands,
        hub - Vec3::Y * 0.6,
        Color::srgb(0.8, 0.9, 1.0),
        90_000.0,
        12.0,
    );
    // The cellar: shelves of jars along its back wall and a bare bulb.
    let cellar = cell_centre(cell_of(ENTRANCES[2].at));
    let back = Vec3::new(cellar.x - CELL * 1.5 + 0.3, y, cellar.y);
    // Three shelves of 27 jars (brown, green, honey), one model.
    deep_model(
        &mut commands,
        &assets,
        "jar_shelf.gltf",
        Transform::from_translation(back),
        show,
    );
    let bare = Vec3::new(cellar.x, top - 0.4, cellar.y);
    block(
        &mut commands,
        &m,
        &bulb,
        boxed(bare, Vec3::splat(0.14)),
        false,
        true,
    );
    light(
        &mut commands,
        bare,
        Color::srgb(1.0, 0.8, 0.5),
        30_000.0,
        10.0,
    );

    // A ladder under every way in, and the hatch seen from below.
    let hatch_glow = glow(&mut materials, Color::srgb(1.0, 0.88, 0.45), 4.0);
    for (i, e) in ENTRANCES.iter().enumerate() {
        let base = Vec3::new(e.at.x, y, e.at.y);
        // The ladder faces you as you arrive (you arrive on its `out` side):
        // a modeler-made ladder the tunnel's height, rusty iron under the
        // manhole, wood elsewhere.
        let rung = Quat::from_rotation_y((-e.out.x).atan2(-e.out.y));
        let file = if e.kind == Kind::Manhole {
            "ladder_iron.gltf"
        } else {
            "ladder_wood.gltf"
        };
        deep_model(
            &mut commands,
            &assets,
            file,
            Transform::from_translation(base).with_rotation(rung),
            show,
        );
        // Every ladder is lit from above, with a glowing hatch in the
        // ceiling, so the ways out stand out down the tunnels.
        let (sky, power) = if e.kind == Kind::Manhole {
            (Color::srgb(0.8, 0.9, 1.0), 12_000.0)
        } else {
            (Color::srgb(1.0, 0.86, 0.5), 9_000.0)
        };
        light(
            &mut commands,
            base + Vec3::Y * (TUNNEL_H - 0.4),
            sky,
            power,
            7.0,
        );
        block(
            &mut commands,
            &m,
            &hatch_glow,
            boxed(
                base + Vec3::Y * (TUNNEL_H - 0.02),
                Vec3::new(0.9, 0.03, 0.9),
            ),
            false,
            true,
        );
        commands.spawn((
            Hatch(i as u8),
            Deep,
            Tunnel,
            Transform::from_translation(base),
            show,
            Clickable::at_height(1.4).radius(90.0).labelled("climb up"),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Local: tunnels rebuilt while you're down there show straight away.
fn show_new(below: Res<Below>, mut deep: Query<&mut Visibility, Added<Deep>>) {
    if !below.inside {
        return;
    }
    for mut v in &mut deep {
        *v = Visibility::Inherited;
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
    cameras: Query<(Entity, Option<&DistanceFog>, &GlobalTransform), With<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight)>,
    ambient: Option<ResMut<GlobalAmbientLight>>,
    mut deep: Query<&mut Visibility, With<Deep>>,
    toast: Option<ResMut<Toast>>,
    tunnels: Option<Res<Tunnels>>,
) {
    let mine = me
        .and_then(|me| walkers.iter().find(|(p, _)| p.id == me.0))
        .map(|(_, w)| w.pos)
        .filter(|p| super::below(*p))
        // A camera put down there on its own (screenshots) sees it too.
        .or_else(|| {
            let eye = cameras.single().ok()?.2.translation();
            super::below(eye).then_some(eye)
        });
    if mine.is_some() == below.inside {
        return;
    }
    let Ok((camera, fog, _)) = cameras.single() else {
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
        let here = tunnels.and_then(|t| t.part(cell_of(at.xz())));
        let text = match here {
            Some(Part::Cellar) => "a damp cellar: the tunnels go on from here",
            Some(Part::Passage) => "a secret passage: it joins the sewers",
            _ => "the sewers: every tunnel down here joins up",
        };
        let text = format!("{text}. Click a lit ladder to climb out");
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
