//! The surf map. From issue #306 by doomy: "surf map in one of the buildings
//! in nyc" (no details: surprise me).
//!
//! The brownstone on the avenue's east side has a neon SURF sign over its
//! door. Walk up the stoop into the door and the server puts you on the
//! start pad of a surf hall (bigger on the inside: it hangs high over the
//! city). Five stages of ridge ramps with 59 degree faces (S-bends,
//! zig-zag transfers, a big sweep, wiggles into a speed ramp and a jump, a
//! long carve) wind between glass towers to a neon finish pad, 2.4 km of
//! ramp in all. Each stage starts on a rooftop pad: a checkpoint. Inside the hall the walker's air control is cranked
//! up the way surf servers do it ([`crate::rules::surf::air_at`]): the faces
//! are too steep to stand on, so you slide, and strafing into a face while
//! turning keeps you gliding along it and gains speed. Fall off a stage
//! and you're back on its pad; nothing hangs over anything else, so
//! dropping down skips nothing. Reaching the finish pad shows your time and
//! your best this game (each screen keeps its own). A post on each pad takes
//! you back out to the street.
//!
//! The course lives in `data/side/surf.ron` (the stages' pads and ramp
//! points, and the solid boxes), the same joints the hall's model
//! (`surf-hall`, made in the modeler) was built from; the colliders come
//! from the data, so they never wait on the model file. The course is far
//! bigger than the world, so its edge doesn't hold you in the hall.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker, teleport};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::surf::*;

/// The start pad (centre, size), hall coordinates: the first stage's pad.
pub fn start_pad(t: &Tuning) -> (Vec3, Vec3) {
    t.stages
        .first()
        .map_or((Vec3::ZERO, Vec3::ONE), |s| pad(s.pad))
}

/// The finish pad.
pub fn finish_pad(t: &Tuning) -> (Vec3, Vec3) {
    pad(t.finish)
}

/// Where you land on the start pad (world).
pub fn start_at(t: &Tuning) -> Vec3 {
    stage_spawn(t, 0).unwrap_or(hall_at(t)) + Vec3::new(0.0, 0.0, 3.0)
}

/// Does a body with feet at `feet` walk into the brownstone's door?
pub fn entering(t: &Tuning, feet: Vec3) -> bool {
    (0.5..4.0).contains(&feet.y) && feet.xz().distance(Vec2::new(t.door.0, t.door.1)) < t.door_reach
}

/// Has a body in the hall fallen into the void?
pub fn fallen(t: &Tuning, feet: Vec3) -> bool {
    in_hall(t, feet).is_some_and(|l| l.y < -t.fall_below)
}

/// Where you come back out on the street.
pub fn street_at(t: &Tuning) -> Vec3 {
    Vec3::new(t.street.0, crate::rules::nyc::CITY_Y + 0.05, t.street.1)
}

/// Posts out of the hall (world): on the start pad and on the finish pad.
pub fn posts(t: &Tuning) -> [Vec3; 2] {
    let o = hall_at(t);
    let (s, f) = (start_pad(t), finish_pad(t));
    [
        o + s.0 + Vec3::new(3.5, s.1.y / 2.0, 3.5),
        o + f.0 + Vec3::new(5.0, f.1.y / 2.0, -6.0),
    ]
}

// ------------------------------------------------------------------ server

/// Client → server: leave the hall (a post).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct LeaveSurf;

pub fn protocol(app: &mut App) {
    app.add_client_event::<LeaveSurf>(Channel::Ordered)
        .add_observer(leave)
        .add_systems(Update, move_bodies.run_if(in_state(ServerState::Running)));
}

type Body<'a> = (
    &'a PlayerOwner,
    &'a mut Walker,
    Option<&'a mut WalkInput>,
    Option<&'a mut crate::kcc::Kcc>,
);

fn put(
    to: Vec3,
    walker: &mut Walker,
    mut input: Option<Mut<WalkInput>>,
    mut kcc: Option<Mut<crate::kcc::Kcc>>,
) {
    let yaw = walker.yaw;
    teleport(walker, input.as_deref_mut(), kcc.as_deref_mut(), to, yaw);
}

/// Server: which stage a body has reached (its last checkpoint pad).
#[derive(Component, Default)]
struct OnStage(usize);

/// Server: in through the door, back to your stage's pad when you fall off
/// it, and back to the top when you fall right out.
fn move_bodies(mut commands: Commands, mut bodies: Query<(Entity, Body, Option<&mut OnStage>)>) {
    let t = tuning();
    for (e, (_, mut walker, input, kcc), stage) in &mut bodies {
        if entering(&t, walker.pos) || fallen(&t, walker.pos) {
            put(start_at(&t), &mut walker, input, kcc);
            commands.entity(e).insert(OnStage(0));
            continue;
        }
        let Some(local) = in_hall(&t, walker.pos) else {
            continue;
        };
        let Some(mut stage) = stage else {
            commands.entity(e).insert(OnStage(0));
            continue;
        };
        let (now, fell) = stage_step(&t, stage.0, local);
        if now != stage.0 {
            stage.0 = now;
        }
        if fell && let Some(at) = stage_spawn(&t, now) {
            put(at, &mut walker, input, kcc);
        }
    }
}

fn leave(ev: On<FromClient<LeaveSurf>>, mut commands: Commands, mut bodies: Query<Body>) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((_, mut walker, input, kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner) else {
        return;
    };
    let t = tuning();
    if !posts(&t)
        .iter()
        .any(|p| within(walker.pos, *p, SERVER_REACH + 3.0))
    {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    put(street_at(&t), &mut walker, input, kcc);
}

// ------------------------------------------------------------------ every peer

/// `FRIENDSLOP_START=surf`: the start pad.
fn starts() -> Vec<(String, Vec3)> {
    vec![("surf".to_owned(), start_at(&tuning()))]
}

pub fn plugin(app: &mut App) {
    crate::features::start::provide(starts);
    app.add_systems(Update, facades.run_if(any_with_component::<FacadeSwatch>));
    app.add_observer(click)
        .spawn_into("surf", build)
        .spawn_into("nyc", build_sign)
        .add_systems(Update, timer.run_if(in_state(AppState::InGame)))
        .add_systems(
            Update,
            music_signals
                .in_set(crate::music::MusicSignalSet)
                .run_if(in_state(AppState::InGame)),
        );
}

/// For the music: the camera's in the surf hall (`surf`), so the liquid DnB plays.
fn music_signals(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let t = tuning();
    let at = crate::music::camera_at(&camera);
    signals.flag("surf", at.is_some_and(|p| in_hall(&t, p).is_some()));
}

#[derive(Component)]
struct Post;

fn click(ev: On<Clicked>, mut commands: Commands, posts: Query<&Post>) {
    if posts.get(ev.entity).is_ok() {
        commands.client_trigger(LeaveSurf);
    }
}

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

/// The neon sign over the brownstone's door (faces the avenue, -x): New York's.
fn build_sign(mut commands: Commands, assets: Res<AssetServer>) {
    let t = tuning();
    commands.spawn((
        model(&assets, "surf-door-sign.gltf"),
        Transform::from_xyz(t.sign_at.0, t.sign_at.1, t.sign_at.2)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
        DespawnOnExit(AppState::InGame),
    ));
}

fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    let t = tuning();
    let o = hall_at(&t);
    commands.spawn((
        model(&assets, "surf-hall.gltf"),
        Transform::from_translation(o),
        DespawnOnExit(AppState::InGame),
    ));
    // Colliders never wait on the model: the ramps' pieces and the boxes
    // come from the data file the model was made from.
    let ramps = t.stages.iter().flat_map(|s| s.ramps.iter());
    for ramp in ramps {
        for piece in pieces(ramp, t.ramp_samples, t.ramp_half_width, t.ramp_height) {
            let Some(hull) = avian3d::prelude::Collider::convex_hull(&piece) else {
                continue;
            };
            commands.spawn((
                Transform::from_translation(o),
                avian3d::prelude::RigidBody::Static,
                hull,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    for (c, s) in t.blocks.iter().map(|&b| pad(b)) {
        commands.spawn((
            Transform::from_translation(o + c),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    build_city(&mut commands, &mut meshes, &mut materials, &t);
    // Out under the sky; a light over the start and the finish.
    for at in [start_pad(&t).0, finish_pad(&t).0] {
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.6, 0.95),
                intensity: 400_000.0,
                range: 30.0,
                ..default()
            },
            Transform::from_translation(o + at + Vec3::Y * 6.0),
            DespawnOnExit(AppState::InGame),
        ));
    }
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    dress(&mut commands, &cube, &mut materials, &t, o);
    let pole = materials.add(Color::srgb(0.2, 0.2, 0.3));
    let sign = materials.add(Color::srgb(1.0, 0.3, 0.85));
    for at in posts(&t) {
        commands
            .spawn((
                Post,
                Transform::from_translation(at),
                Visibility::default(),
                // Read once a game from the data file.
                Clickable::at_height(1.6)
                    .radius(60.0)
                    .labelled(t.exit_label.clone().leak()),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(pole.clone()),
                    Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(0.12, 2.2, 0.12)),
                ));
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(sign.clone()),
                    Transform::from_xyz(0.0, 1.9, 0.0).with_scale(Vec3::new(1.2, 0.35, 0.06)),
                ));
            });
    }
}

/// The course's dressing: neon, lights, markers. Nothing here collides.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Dressing {
    /// Each stage's neon colour, in order (the last is reused).
    pub stage_colours: Vec<(f32, f32, f32)>,
    /// The finish's colour.
    pub finish_colour: (f32, f32, f32),
    /// Emissive multiplier for the neon.
    pub glow: f32,
    /// Thickness of the neon strip round each pad (m).
    pub strip: f32,
    /// The light over each stage pad: intensity and range (m).
    pub pad_light: f32,
    pub pad_light_range: f32,
    /// Height of each pad's stage marker post (m).
    pub marker_height: f32,
    /// Lane lights: every this many ridge samples, out past the face's foot,
    /// below the ridge, and their size (m).
    pub lane_every: usize,
    pub lane_out: f32,
    pub lane_drop: f32,
    pub lane_size: f32,
    /// The finish arch's height and width (m).
    pub arch_height: f32,
    pub arch_width: f32,
    /// The city's tower facades, one per tower style (the style wraps).
    pub facades: Vec<Skin>,
    /// The towers' roofs and the building sites' concrete.
    pub roof: Skin,
    pub deck: Skin,
}

/// A city surface: a modeler swatch tiled at real size, and the flat
/// colour it keeps if the swatch file is missing.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Skin {
    /// The swatch model (`models/surfaces/...`).
    pub file: String,
    /// How wide (m) one repeat of its texture is.
    pub tile: f32,
    /// Emissive multiplier for its lit windows / neon (0: none).
    pub glow: f32,
    /// Flat colour (r, g, b, 0..1) and roughness without the swatch.
    pub colour: (f32, f32, f32),
    pub rough: f32,
}

pub static DRESSING: friendslop_data::Data<Dressing> =
    friendslop_data::Data::ron("data/side/surf_dressing.ron");

/// Stage `i`'s colour (the last one repeats; pink with none).
pub fn stage_colour(d: &Dressing, i: usize) -> Vec3 {
    let c = d
        .stage_colours
        .get(i)
        .or(d.stage_colours.last())
        .copied()
        .unwrap_or((1.0, 0.3, 0.85));
    Vec3::from(c)
}

/// Thin boxes (centre, size) tracing a pad's top edge.
pub fn edge_strips(pad: (Vec3, Vec3), thick: f32) -> [(Vec3, Vec3); 4] {
    let (c, s) = pad;
    let y = c.y + s.y / 2.0 + thick / 2.0;
    let (hx, hz) = (s.x / 2.0, s.z / 2.0);
    [
        (Vec3::new(c.x, y, c.z - hz), Vec3::new(s.x, thick, thick)),
        (Vec3::new(c.x, y, c.z + hz), Vec3::new(s.x, thick, thick)),
        (Vec3::new(c.x - hx, y, c.z), Vec3::new(thick, thick, s.z)),
        (Vec3::new(c.x + hx, y, c.z), Vec3::new(thick, thick, s.z)),
    ]
}

/// Where a ramp's lane lights float: beside each face, out past its foot.
pub fn lane_lights(t: &Tuning, d: &Dressing, ramp: &[(f32, f32, f32)]) -> Vec<Vec3> {
    let line = ridge(ramp, t.ramp_samples);
    let every = d.lane_every.max(1);
    let mut out = Vec::new();
    for i in (0..line.len()).step_by(every) {
        let a = line[i.saturating_sub(1)];
        let b = line[(i + 1).min(line.len() - 1)];
        let along = Vec3::new(b.x - a.x, 0.0, b.z - a.z).normalize_or_zero();
        let side = Vec3::new(-along.z, 0.0, along.x);
        let p = line[i] - Vec3::Y * d.lane_drop;
        let out_by = t.ramp_half_width * 1.7 + d.lane_out;
        out.push(p + side * out_by);
        out.push(p - side * out_by);
    }
    out
}

fn neon(materials: &mut Assets<StandardMaterial>, c: Vec3, glow: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgb(c.x, c.y, c.z),
        emissive: LinearRgba::rgb(c.x * glow, c.y * glow, c.z * glow),
        unlit: true,
        ..default()
    })
}

fn neon_box(
    commands: &mut Commands,
    cube: &Handle<Mesh>,
    m: &Handle<StandardMaterial>,
    at: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(m.clone()),
        Transform::from_translation(at).with_scale(size),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Neon round the pads, a numbered marker and a light on each, lane lights
/// along the ramps and an arch over the finish. Local looks only.
fn dress(
    commands: &mut Commands,
    cube: &Handle<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    t: &Tuning,
    o: Vec3,
) {
    let d = DRESSING.get();
    let post = materials.add(Color::srgb(0.12, 0.12, 0.18));
    for (i, stage) in t.stages.iter().enumerate() {
        let col = stage_colour(&d, i);
        let m = neon(materials, col, d.glow);
        let p = pad(stage.pad);
        for (c, s) in edge_strips(p, d.strip) {
            neon_box(commands, cube, &m, o + c, s);
        }
        // The marker on the back corner: a post and one block per stage.
        let (c, s) = p;
        let top = c.y + s.y / 2.0;
        let corner = o + Vec3::new(c.x - s.x / 2.0 + 0.6, top, c.z + s.z / 2.0 - 0.6);
        let h = d.marker_height;
        neon_box(
            commands,
            cube,
            &post,
            corner + Vec3::Y * h / 2.0,
            Vec3::new(0.2, h, 0.2),
        );
        for n in 0..=i {
            let at = corner + Vec3::Y * (h + 0.35 + n as f32 * 0.55);
            neon_box(commands, cube, &m, at, Vec3::splat(0.4));
        }
        commands.spawn((
            PointLight {
                color: Color::srgb(col.x, col.y, col.z),
                intensity: d.pad_light,
                range: d.pad_light_range,
                ..default()
            },
            Transform::from_translation(o + c + Vec3::Y * 4.0),
            DespawnOnExit(AppState::InGame),
        ));
        for ramp in &stage.ramps {
            for at in lane_lights(t, &d, ramp) {
                neon_box(commands, cube, &m, o + at, Vec3::splat(d.lane_size));
            }
        }
    }
    // The finish: gold strips and an arch over the pad.
    let gold = neon(materials, Vec3::from(d.finish_colour), d.glow);
    let f = pad(t.finish);
    for (c, s) in edge_strips(f, d.strip * 1.5) {
        neon_box(commands, cube, &gold, o + c, s);
    }
    let (c, s) = f;
    let top = c.y + s.y / 2.0;
    let (h, w) = (d.arch_height, d.arch_width);
    for side in [-1.0, 1.0] {
        let at = o + Vec3::new(c.x + side * w / 2.0, top + h / 2.0, c.z);
        neon_box(commands, cube, &gold, at, Vec3::new(0.4, h, 0.4));
    }
    neon_box(
        commands,
        cube,
        &gold,
        o + Vec3::new(c.x, top + h, c.z),
        Vec3::new(w + 0.4, 0.5, 0.4),
    );
}

/// A wedge (a kicker) as a mesh: its six corners, flat shaded.
fn wedge_mesh(c: [Vec3; 6]) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    // Corners: 0..4 the base (-z pair, +z pair), 4, 5 the high +z edge.
    let faces: [&[usize]; 5] = [
        &[0, 2, 3, 1], // underside
        &[0, 1, 5, 4], // the slope
        &[2, 4, 5, 3], // the tall back
        &[0, 4, 2],    // left side
        &[1, 3, 5],    // right side
    ];
    let mut pos = Vec::new();
    let mut nor = Vec::new();
    let mut idx = Vec::new();
    let mid = c.iter().copied().sum::<Vec3>() / 6.0;
    for f in faces {
        let v: Vec<Vec3> = f.iter().map(|&i| c[i]).collect();
        let mut n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        let mut v = v;
        if n.dot(v[0] - mid) < 0.0 {
            v.reverse();
            n = -n;
        }
        let base = pos.len() as u32;
        for p in &v {
            pos.push(p.to_array());
            nor.push(n.to_array());
        }
        for i in 1..v.len() as u32 - 1 {
            idx.extend([base, base + i, base + i + 1]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
    .with_inserted_indices(Indices::U32(idx))
}

/// The city the course weaves through: solid boxes from [`city`], flat
/// placeholder colours (glass towers, concrete sites, yellow steel).
fn build_city(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    t: &Tuning,
) {
    use avian3d::prelude::{Collider, RigidBody};
    let o = hall_at(t);
    let d = DRESSING.get();
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let mat = |m: &mut Assets<StandardMaterial>, c: Color, rough: f32, glow: f32| {
        m.add(StandardMaterial {
            base_color: c,
            perceptual_roughness: rough,
            metallic: if rough < 0.4 { 0.6 } else { 0.0 },
            emissive: c.to_linear() * glow,
            ..default()
        })
    };
    // Textured surfaces: one material per skin, flat until its swatch loads.
    let mut skin = |s: &Skin| {
        let c = Color::srgb(s.colour.0, s.colour.1, s.colour.2);
        let h = mat(materials, c, s.rough, 0.0);
        if let Some(mut m) = materials.get_mut(&h) {
            m.uv_transform = bevy::math::Affine2::from_scale(Vec2::splat(1.0 / s.tile.max(0.01)));
        }
        commands.spawn(FacadeSwatch {
            file: s.file.clone(),
            material: h.clone(),
            glow: s.glow,
        });
        h
    };
    let glass: Vec<_> = d.facades.iter().map(&mut skin).collect();
    let roof = skin(&d.roof);
    let concrete = skin(&d.deck);
    let band = mat(materials, Color::srgb(1.0, 0.45, 0.85), 0.5, 4.0);
    let netting = mat(materials, Color::srgb(1.0, 0.45, 0.1), 0.8, 0.6);
    let steel = mat(materials, Color::srgb(0.95, 0.75, 0.1), 0.6, 0.0);
    let ply = mat(materials, Color::srgb(0.8, 0.62, 0.38), 0.85, 0.0);
    let pallet = mat(materials, Color::srgb(0.5, 0.36, 0.22), 0.9, 0.0);
    let Some(fallback) = glass.first().cloned() else {
        return;
    };
    // Textured boxes get a metric-uv mesh per size (cm buckets, shared), so
    // windows stay window-sized; flat ones stay a scaled unit cube.
    let mut boxes: std::collections::HashMap<[i32; 3], Handle<Mesh>> = default();
    let blocks = city(t);
    let wedges: Vec<Option<Handle<Mesh>>> = (blocks.iter())
        .map(|b| (b.piece == Piece::Kicker).then(|| meshes.add(wedge_mesh(b.wedge()))))
        .collect();
    let mut part = |c: &mut Commands,
                    b: &Block,
                    m: &Handle<StandardMaterial>,
                    at: Vec3,
                    size: Vec3,
                    textured: bool| {
        let tf = Transform::from_translation(o + b.centre + b.rotation() * at)
            .with_rotation(b.rotation());
        let (mesh, tf) = if textured {
            let key = (size * 100.0).round().as_ivec3().to_array();
            let mesh = boxes
                .entry(key)
                .or_insert_with(|| {
                    meshes.add(metric_box(Vec3::from(key.map(|k| k as f32)) / 100.0))
                })
                .clone();
            (mesh, tf)
        } else {
            (cube.clone(), tf.with_scale(size))
        };
        c.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(m.clone()),
            tf,
            DespawnOnExit(AppState::InGame),
        ));
    };
    for (b, wedge) in blocks.into_iter().zip(wedges) {
        let tf = Transform::from_translation(o + b.centre).with_rotation(b.rotation());
        if let Some(wedge) = wedge {
            let corners = b.wedge();
            commands.spawn((
                Mesh3d(wedge),
                MeshMaterial3d(ply.clone()),
                tf,
                RigidBody::Static,
                Collider::convex_hull(&corners)
                    .unwrap_or(Collider::cuboid(b.size.x, b.size.y, b.size.z)),
                DespawnOnExit(AppState::InGame),
            ));
            continue;
        }
        commands.spawn((
            tf,
            RigidBody::Static,
            Collider::cuboid(b.size.x, b.size.y, b.size.z),
            DespawnOnExit(AppState::InGame),
        ));
        let top = b.size.y / 2.0;
        match b.piece {
            Piece::Tower { style } => {
                let m = glass.get(style as usize % glass.len()).unwrap_or(&fallback);
                part(commands, &b, m, Vec3::ZERO, b.size, true);
                part(
                    commands,
                    &b,
                    &roof,
                    Vec3::Y * (top + 0.15),
                    b.size.with_y(0.3) - Vec3::new(0.4, 0.0, 0.4),
                    true,
                );
                // A neon band under the roof, and a parapet.
                if style % 2 == 0 {
                    part(
                        commands,
                        &b,
                        &band,
                        Vec3::Y * (top - 2.5),
                        b.size.with_y(0.5) + Vec3::new(0.2, 0.0, 0.2),
                        false,
                    );
                }
            }
            Piece::Site => {
                part(
                    commands,
                    &b,
                    &concrete,
                    Vec3::ZERO,
                    b.size - Vec3::new(0.6, 0.0, 0.6),
                    true,
                );
                // The open top floors: columns at the corners and a slab.
                part(
                    commands,
                    &b,
                    &concrete,
                    Vec3::Y * (top - 0.3),
                    b.size.with_y(0.6),
                    true,
                );
                for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                    let at = Vec3::new(
                        x * (b.size.x / 2.0 - 0.6),
                        top - 5.0,
                        z * (b.size.z / 2.0 - 0.6),
                    );
                    part(commands, &b, &concrete, at, Vec3::new(0.8, 9.4, 0.8), true);
                }
                // Safety netting round the floors under the deck.
                part(
                    commands,
                    &b,
                    &netting,
                    Vec3::Y * (top - 6.0),
                    b.size.with_y(5.0) + Vec3::new(0.1, 0.0, 0.1),
                    false,
                );
            }
            Piece::Steel => part(commands, &b, &steel, Vec3::ZERO, b.size, false),
            Piece::Clutter => part(commands, &b, &pallet, Vec3::ZERO, b.size, false),
            Piece::Kicker => {}
        }
    }
}

/// A city material waiting for its modeler swatch's textures.
#[derive(Component)]
struct FacadeSwatch {
    file: String,
    material: Handle<StandardMaterial>,
    glow: f32,
}

/// A box `size` (m) about its centre with uvs in metres from its top-left,
/// so a tiled texture keeps its real size on any box. Tangents point along
/// u, bitangents up (sides) or towards -z (top and bottom).
fn metric_box(size: Vec3) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let h = size / 2.0;
    let (mut pos, mut nor, mut uv, mut tan, mut idx) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for n in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z, Vec3::Y, -Vec3::Y] {
        let side = n.y == 0.0;
        // u along t, v along -up.
        let (t, up) = if side {
            (Vec3::Y.cross(n), Vec3::Y)
        } else {
            (Vec3::X, -Vec3::Z)
        };
        let (ht, hu) = (t.abs().dot(h), up.abs().dot(h));
        let base = pos.len() as u32;
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = n * n.abs().dot(h) + t * a * ht + up * b * hu;
            pos.push(p.to_array());
            nor.push(n.to_array());
            uv.push([(a + 1.0) * ht, (1.0 - b) * hu]);
            let w = if n.cross(t).dot(up) > 0.0 { 1.0 } else { -1.0 };
            tan.push([t.x, t.y, t.z, w]);
        }
        let quad = [base, base + 1, base + 2, base, base + 2, base + 3];
        let (p0, p1, p2) = (
            Vec3::from(pos[base as usize]),
            Vec3::from(pos[base as usize + 1]),
            Vec3::from(pos[base as usize + 2]),
        );
        if (p1 - p0).cross(p2 - p0).dot(n) >= 0.0 {
            idx.extend(quad);
        } else {
            idx.extend(quad.iter().rev());
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tan)
    .with_inserted_indices(Indices::U32(idx))
}

/// Put each city swatch's textures on its material once the file loads;
/// with no file (no LFS data) the flat colour stays.
fn facades(
    mut commands: Commands,
    mut files: Local<std::collections::HashMap<String, Handle<bevy::gltf::Gltf>>>,
    waiting: Query<(Entity, &FacadeSwatch)>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    looks: Res<Assets<bevy::gltf::GltfMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, w) in &waiting {
        let h = files
            .entry(w.file.clone())
            .or_insert_with(|| assets.load(w.file.clone()))
            .clone();
        if assets.load_state(&h).is_failed() {
            commands.entity(e).despawn();
            continue;
        }
        if !assets.is_loaded_with_dependencies(&h) {
            continue;
        }
        commands.entity(e).despawn();
        let look = (gltfs.get(&h))
            .and_then(|g| g.materials.first())
            .and_then(|m| looks.get(m));
        let (Some(look), Some(mut m)) = (look, materials.get_mut(&w.material)) else {
            continue;
        };
        let Some(base) = look.base_color_texture.clone() else {
            continue;
        };
        m.base_color = Color::WHITE;
        m.base_color_texture = Some(base);
        m.metallic_roughness_texture = look.metallic_roughness_texture.clone();
        m.normal_map_texture = look.normal_map_texture.clone();
        m.perceptual_roughness = 1.0;
        m.metallic = look.metallic;
        if let Some(em) = look.emissive_texture.clone() {
            m.emissive_texture = Some(em);
            m.emissive = LinearRgba::WHITE * w.glow;
        }
    }
}

/// This screen's run: when it left the start pad, and its best time.
#[derive(Default)]
struct Run {
    inside: bool,
    started: Option<f32>,
    best: Option<f32>,
}

/// What a run's step did, for the toasts.
#[derive(Debug, PartialEq)]
pub enum RunEvent {
    Entered,
    WipedOut(f32),
    Finished { secs: f32, best: f32 },
}

/// One step of a screen's run timer for feet at `local` (hall coordinates,
/// `None` outside the hall) at time `now`. Pure, so it's tested.
fn run_step(
    run: &mut Run,
    pads: [(Vec3, Vec3); 2],
    local: Option<Vec3>,
    now: f32,
) -> Option<RunEvent> {
    let [start, finish] = pads;
    let Some(l) = local else {
        *run = Run {
            best: run.best,
            ..default()
        };
        return None;
    };
    if !run.inside {
        run.inside = true;
        return Some(RunEvent::Entered);
    }
    let on_start = on_pad(start, l);
    match run.started {
        None if !on_start && l.z < start.0.z - start.1.z / 2.0 => {
            run.started = Some(now);
            None
        }
        Some(t0) if on_start => {
            run.started = None;
            Some(RunEvent::WipedOut(now - t0))
        }
        Some(t0) if through_checkpoint(finish, l, tuning().checkpoint_reach) => {
            run.started = None;
            let secs = now - t0;
            let best = run.best.map_or(secs, |b| b.min(secs));
            run.best = Some(best);
            Some(RunEvent::Finished { secs, best })
        }
        _ => None,
    }
}

/// Local: time each run and say how it went.
fn timer(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut run: Local<Run>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let t = tuning();
    let secs = |s: f32| format!("{s:.2}");
    let pads = [start_pad(&t), finish_pad(&t)];
    match run_step(&mut run, pads, in_hall(&t, w.pos), time.elapsed_secs()) {
        Some(RunEvent::Entered) => toast.show(t.enter_line.clone()),
        Some(RunEvent::WipedOut(s)) => toast.show(t.wipeout_line.replace("{secs}", &secs(s))),
        Some(RunEvent::Finished { secs: s, best }) => toast.show(
            t.finish_line
                .replace("{secs}", &secs(s))
                .replace("{best}", &secs(best)),
        ),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_dressing_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            DRESSING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(DRESSING.set_bytes(&bytes), "{path} doesn't parse");
        let d = DRESSING.get();
        assert!(d.stage_colours.len() >= 5 && d.glow > 0.0 && d.strip > 0.0);
        assert!(d.pad_light > 0.0 && d.pad_light_range > 0.0 && d.marker_height > 0.0);
        assert!(d.lane_every > 0 && d.lane_size > 0.0 && d.lane_drop > 0.0);
        assert!(d.arch_height > 2.0 && d.arch_width > 0.0);
        assert!(d.facades.len() >= 4);
        for s in d.facades.iter().chain([&d.roof, &d.deck]) {
            assert!(
                s.tile > 0.0 && s.file.starts_with("models/surfaces/"),
                "{s:?}"
            );
        }
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.door_reach > 0.0 && t.fall_below > 0.0 && t.sign_at.1 > 3.0);
        assert!(
            t.air_accel_hz > 10.0,
            "surf needs more air control than walking"
        );
        assert!(t.max_speed > 30.0 && t.air_max_wish > 0.0);
        assert!(!t.enter_line.is_empty() && !t.exit_label.is_empty());
        assert!(t.wipeout_line.contains("{secs}"));
        assert!(t.finish_line.contains("{secs}") && t.finish_line.contains("{best}"));
    }

    #[test]
    fn the_ramps_are_too_steep_to_stand_on_and_never_stack() {
        let t = t();
        assert!((t.ramp_height / t.ramp_half_width).atan().to_degrees() > 46.0);
        assert!(t.stages.len() >= 3 && t.stages.iter().all(|s| !s.ramps.is_empty()));
        let mut length = 0.0;
        let mut ridges = Vec::new();
        for (i, s) in t.stages.iter().enumerate() {
            for r in &s.ramps {
                let p = ridge(r, t.ramp_samples);
                length += p.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
                assert!(
                    p.last().is_some_and(|e| e.y < p[0].y),
                    "ramps drop as they go"
                );
                ridges.extend(p.into_iter().map(|q| (i, q)));
                assert!(!pieces(r, t.ramp_samples, t.ramp_half_width, t.ramp_height).is_empty());
            }
        }
        assert!(length > 1500.0, "a long course: {length} m");
        // No ramp of one stage hangs over another's: dropping down skips nothing.
        for (i, (a, p)) in ridges.iter().enumerate() {
            for (b, q) in &ridges[i + 1..] {
                assert!(a == b || p.xz().distance(q.xz()) > 14.0, "{p} over {q}");
            }
        }
    }

    #[test]
    fn the_hall_is_in_the_sky_and_holds_everything() {
        let t = t();
        let o = hall_at(&t);
        assert!(o.y > 560.0, "over One World Trade Center");
        let (min, max) = (Vec3::from(t.hall.0), Vec3::from(t.hall.1));
        // Nowhere near the city, whose edges would hold you in it.
        for c in [
            min,
            max,
            Vec3::new(min.x, 0.0, max.z),
            Vec3::new(max.x, 0.0, min.z),
        ] {
            assert!(!crate::rules::nyc::in_city(o.x + c.x, o.z + c.z));
        }
        assert!(unbounded(start_at(&t)) && !unbounded(street_at(&t)));
        assert!(on_pad(start_pad(&t), start_at(&t) - o));
        for p in posts(&t) {
            assert!(in_hall(&t, p).is_some());
        }
        for (i, s) in t.stages.iter().enumerate() {
            let at = stage_spawn(&t, i).expect("stage");
            assert!(on_pad(pad(s.pad), at - o));
            assert!(!fallen(&t, at));
            // Standing on your pad isn't falling off your stage.
            assert_eq!(stage_step(&t, i, at - o), (i, false));
        }
        assert!(fallen(&t, o + Vec3::new(0.0, -t.fall_below - 1.0, -80.0)));
        assert!(finish_pad(&t).0.y > -t.fall_below && in_hall(&t, o + finish_pad(&t).0).is_some());
    }

    #[test]
    fn the_city_crowds_the_ramps_but_never_blocks_them() {
        let t = t();
        let c = city(&t);
        let count = |f: fn(&Piece) -> bool| c.iter().filter(|b| f(&b.piece)).count();
        let towers = count(|p| matches!(p, Piece::Tower { .. }));
        let sites = count(|p| *p == Piece::Site);
        let kickers = count(|p| *p == Piece::Kicker);
        assert!(towers > 40, "{towers} towers");
        assert!(
            sites > 5 && kickers == sites,
            "{sites} sites, {kickers} kickers"
        );
        assert!(count(|p| *p == Piece::Steel) > 5);
        let ridges: Vec<Vec3> = t
            .stages
            .iter()
            .flat_map(|s| s.ramps.iter().flat_map(|r| ridge(r, t.ramp_samples)))
            .collect();
        // Plenty rise above the ramps beside them: you weave between them.
        let above = c
            .iter()
            .filter(|b| matches!(b.piece, Piece::Tower { .. }))
            .filter(|b| {
                ridges
                    .iter()
                    .any(|q| b.ground_distance(q.xz()) < 30.0 && b.top() > q.y + 5.0)
            })
            .count();
        assert!(
            above > towers / 2,
            "{above} of {towers} rise past the ramps"
        );
        // No tower or site reaches into a ramp's span.
        for b in c
            .iter()
            .filter(|b| matches!(b.piece, Piece::Tower { .. } | Piece::Site))
        {
            for q in &ridges {
                assert!(
                    q.y - t.ramp_height > b.top() || b.ground_distance(q.xz()) > t.ramp_half_width,
                    "{b:?} blocks the ramp at {q}"
                );
            }
        }
        // The same city every time (every peer builds its own).
        assert_eq!(city(&t).len(), c.len());
    }

    #[test]
    fn stage_pads_are_checkpoints() {
        let t = t();
        let o = hall_at(&t);
        let two = stage_spawn(&t, 1).expect("two stages") - o;
        // Landing on stage 2's pad moves you on to it...
        assert_eq!(stage_step(&t, 0, two), (1, false));
        // ...but not back: stage 1's pad from stage 2 changes nothing.
        let one = stage_spawn(&t, 0).expect("start") - o;
        assert_eq!(stage_step(&t, 1, one).0, 1);
        // Falling well below stage 2's ramps puts you back on its pad.
        let low = stage_low(&t, 1).expect("ramps") - t.stage_fall - 1.0;
        assert_eq!(stage_step(&t, 1, Vec3::new(two.x, low, two.z)), (1, true));
    }

    #[test]
    fn the_door_lets_you_in_and_the_street_doesnt() {
        let t = t();
        let door = Vec3::new(t.door.0, 1.55, t.door.1);
        assert!(entering(&t, door));
        assert!(!entering(&t, street_at(&t)));
        assert!(air_at(street_at(&t)) == crate::kcc::Air::NORMAL);
        assert!(air_at(start_at(&t)).accel_hz > crate::kcc::Air::NORMAL.accel_hz);
    }

    #[test]
    fn a_run_is_timed_from_the_pad_to_the_finish() {
        let mut run = Run::default();
        let start = (Vec3::new(0.0, -0.5, -5.0), Vec3::new(10.0, 1.0, 10.0));
        let fin = (Vec3::new(0.0, -34.5, -160.0), Vec3::new(16.0, 1.0, 20.0));
        let pads = [start, fin];
        let pad = Vec3::new(0.0, 0.0, -2.0);
        let finish = Vec3::new(0.0, -34.0, -160.0);
        let away = Vec3::new(0.0, -2.0, -12.0);
        assert_eq!(
            run_step(&mut run, pads, Some(pad), 0.0),
            Some(RunEvent::Entered)
        );
        assert_eq!(run_step(&mut run, pads, Some(away), 1.0), None);
        assert_eq!(
            run_step(&mut run, pads, Some(pad), 3.0),
            Some(RunEvent::WipedOut(2.0))
        );
        run_step(&mut run, pads, Some(away), 4.0);
        assert_eq!(
            run_step(&mut run, pads, Some(finish), 14.0),
            Some(RunEvent::Finished {
                secs: 10.0,
                best: 10.0
            })
        );
        run_step(&mut run, pads, Some(away), 20.0);
        assert_eq!(
            run_step(&mut run, pads, Some(finish), 32.0),
            Some(RunEvent::Finished {
                secs: 12.0,
                best: 10.0
            })
        );
        assert_eq!(run_step(&mut run, pads, None, 40.0), None);
        assert_eq!(run.best, Some(10.0));
    }
}
