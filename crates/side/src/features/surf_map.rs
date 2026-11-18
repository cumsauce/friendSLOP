//! The surf map as the modeler made it (`maps/surf.bsn`, exported from the
//! modeler document `surf-map`). All of the course lives there: its placed
//! models are the map's own entities (spawned by `friendslop_world::map`), its
//! own geometry (the city, the neon dressing, the office and mall ramps) is
//! `maps/surf.gltf`, and its markers say the rest: `Block` and `Wedge` boxes
//! are the colliders (so nothing waits on a model file), `Lamp`s are the
//! lights, `Body`s the colliders of tilted models (the train's beam), `Bob`
//! and `Spin` move the floaters, and `Pad` / `Ridge` / `course_block` markers
//! are the course itself (`rules::surf::course` reads them into the tuning).
//! The ramps' colliders are worked out from the ridge points here.
//!
//! The map's coordinates are the hall's (the start pad's top is 0, 0, 0), so
//! it stands at the hall's origin on its own flat floor.
//!
//! To change the course, edit the modeler document and re-export
//! `maps/surf.bsn`. `cargo run -p friendslop-side --example surf_map_ops`
//! regenerates its ops from scratch out of the layout code (the sections'
//! modules, `data/side/surf_*.ron`) that built it; nothing generates at run time.

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use friendslop_world::map::{Bob, FlatGround, Map, Spin};
use friendslop_world::places::{MapEntity, MapFile};

use crate::AppState;
use crate::rules::surf::{
    Block as CityBlock, Piece, course_parts, hall_at, joints, pieces, tuning,
};

/// A collider box from the map: centre, full size, turn about +y (radians),
/// and whether it's an upright cylinder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub centre: Vec3,
    pub size: Vec3,
    pub yaw: f32,
    pub round: bool,
}

/// A point light from the map: where, lumens, reach (m) and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    pub at: Vec3,
    pub lumens: f32,
    pub range: f32,
    pub colour: Color,
}

/// A box collider on a placed model: where its model stands, how that is
/// turned, the box's size and where its middle is from the model's origin
/// along the model's own axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub at: Vec3,
    pub rot: Quat,
    pub size: Vec3,
    pub offset: Vec3,
}

/// What the map's markers lay out.
#[derive(Debug, Default, PartialEq)]
pub struct Layout {
    pub blocks: Vec<Block>,
    /// Kickers: wedge colliders (the box: low at -z, high at +z).
    pub wedges: Vec<Block>,
    pub bodies: Vec<Body>,
    pub lamps: Vec<Lamp>,
}

fn block(e: &MapEntity) -> Block {
    Block {
        centre: Vec3::from_array(e.translation),
        size: e.size.map_or(Vec3::ONE, Vec3::from_array),
        yaw: e.yaw.to_radians(),
        round: e.flag("Block", "round"),
    }
}

/// Read the markers out of the map file.
pub fn layout(map: &MapFile) -> Layout {
    let mut l = Layout::default();
    for e in &map.entities {
        if e.has("Block") {
            l.blocks.push(block(e));
        }
        if e.has("Wedge") {
            l.wedges.push(block(e));
        }
        if e.has("Body") {
            let n = |f: &str| e.num("Body", f);
            l.bodies.push(Body {
                at: Vec3::from_array(e.translation),
                rot: e
                    .rotation
                    .map_or(Quat::from_rotation_y(e.yaw.to_radians()), |r| {
                        Quat::from_xyzw(r[0], r[1], r[2], r[3])
                    }),
                size: Vec3::new(n("sx"), n("sy"), n("sz")),
                offset: Vec3::new(n("ox"), n("oy"), n("oz")),
            });
        }
        if e.has("Lamp") {
            let n = |f: &str| e.num("Lamp", f);
            let rgb = Vec3::new(n("r"), n("g"), n("b"));
            l.lamps.push(Lamp {
                at: Vec3::from_array(e.translation),
                lumens: n("lumens"),
                range: n("range"),
                colour: if rgb == Vec3::ZERO {
                    LAMP_COLOUR
                } else {
                    Color::srgb(rgb.x, rgb.y, rgb.z)
                },
            });
        }
    }
    l
}

/// A lamp with no colour of its own gives this.
const LAMP_COLOUR: Color = Color::srgb(1.0, 0.9, 0.78);

/// One surf area: its id in `areas.ron`, its map's plain data and its scene.
/// A map that isn't there yet (a pack still being made) spawns nothing.
macro_rules! areas {
    ($(($build:ident, $id:literal, $data:expr, $bsn:literal, $ramps:literal)),* $(,)?) => {
        $(
            fn $build(
                commands: Commands,
                assets: Res<AssetServer>,
                meshes: ResMut<Assets<Mesh>>,
                materials: ResMut<Assets<StandardMaterial>>,
            ) {
                build(commands, assets, meshes, materials, &$data, $bsn, $ramps);
            }
        )*
        pub fn plugin(app: &mut App) {
            app$(.spawn_into($id, $build))*.add_systems(
                Update,
                (rest, animate)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
        }
    };
}

areas!(
    (
        build_base,
        "surf",
        crate::rules::surf::COURSE,
        "maps/surf.bsn",
        false
    ),
    (
        build_office,
        "surf_office",
        crate::rules::surf::COURSE_OFFICE,
        "maps/surf_office.bsn",
        true
    ),
    (
        build_mall,
        "surf_mall",
        crate::rules::surf::COURSE_MALL,
        "maps/surf_mall.bsn",
        true
    ),
    (
        build_neon,
        "surf_neon",
        crate::rules::surf::COURSE_NEON,
        "maps/surf_neon.bsn",
        true
    ),
    (
        build_overgrown,
        "surf_overgrown",
        crate::rules::surf::COURSE_OVERGROWN,
        "maps/surf_overgrown.bsn",
        true
    ),
    (
        build_foundry,
        "surf_foundry",
        crate::rules::surf::COURSE_FOUNDRY,
        "maps/surf_foundry.bsn",
        true
    ),
);

/// How a stage's ramps look: its `RampLook` marker, or dark blue steel.
fn ramp_look(map: &MapFile, stage: u32) -> StandardMaterial {
    let (mut c, mut glow, mut rough, mut metal) = (Vec3::new(0.16, 0.19, 0.32), 0.0, 0.35, 0.5);
    if let Some(e) = map
        .entities
        .iter()
        .find(|e| e.has("RampLook") && e.num("RampLook", "stage") as u32 == stage)
    {
        let n = |f: &str| e.num("RampLook", f);
        (c, glow, rough, metal) = (
            Vec3::new(n("r"), n("g"), n("b")),
            n("glow"),
            n("rough"),
            n("metal"),
        );
    }
    StandardMaterial {
        base_color: Color::srgb(c.x, c.y, c.z),
        emissive: LinearRgba::rgb(c.x * glow, c.y * glow, c.z * glow),
        metallic: metal,
        perceptual_roughness: rough,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

/// A ramp's two faces and underside as one flat-shaded mesh (hall coordinates).
fn ramp_mesh(points: &[(f32, f32, f32)], samples: u32, half: f32, height: f32) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let js = joints(points, samples, half, height);
    let (mut pos, mut nor, mut idx) = (Vec::new(), Vec::new(), Vec::new());
    for w in js.windows(2) {
        let (a, b) = (w[0], w[1]);
        for (quad, up) in [
            ([a[0], a[1], b[1], b[0]], true),
            ([a[0], b[0], b[2], a[2]], true),
            ([a[1], a[2], b[2], b[1]], false),
        ] {
            let mut q = quad;
            let mut n = (q[1] - q[0]).cross(q[2] - q[0]).normalize_or_zero();
            if (n.y < 0.0) == up {
                q.reverse();
                n = -n;
            }
            let base = pos.len() as u32;
            for v in q {
                pos.push(v.to_array());
                nor.push(n.to_array());
            }
            idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
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

/// Where a bobbing thing rests.
#[derive(Component)]
struct Rest(Vec3);

/// Note where each bobbing thing starts.
fn rest(mut commands: Commands, bobs: Query<(Entity, &Transform), (With<Bob>, Without<Rest>)>) {
    for (e, t) in &bobs {
        commands.entity(e).insert(Rest(t.translation));
    }
}

/// Floaters bob and the disco balls turn (local looks only).
fn animate(
    time: Res<Time>,
    mut bobs: Query<(&Bob, &Rest, &mut Transform), Without<Spin>>,
    mut spins: Query<(&Spin, &mut Transform), Without<Bob>>,
) {
    let s = time.elapsed_secs();
    for (b, r, mut t) in &mut bobs {
        t.translation =
            r.0 + Vec3::Y * b.height * (s * b.hz * std::f32::consts::TAU + b.phase).sin();
    }
    for (spin, mut t) in &mut spins {
        t.rotate_local_y(spin.rpm / 60.0 * std::f32::consts::TAU * time.delta_secs());
    }
}

fn build(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    data: &friendslop_data::Data<MapFile>,
    bsn: &'static str,
    ramp_meshes: bool,
) {
    let map = data.get();
    if map.entities.is_empty() {
        return;
    }
    let t = tuning();
    let o = hall_at(&t);
    // SALT: not needing `Cargo.lock` for this `.bsn` either; x didn't set us
    // up for success, we need more SME's, we haven't considered the problem
    // space enough, BSN syntax needs more changes, and `Children` sugaring?
    // Ask Sam "Salt" Saltman.
    commands.spawn((
        Map(assets.load(bsn)),
        FlatGround,
        Transform::from_translation(o),
        Visibility::default(),
        DespawnOnExit(AppState::InGame),
    ));
    let l = layout(&map);
    // The ramps' colliders come from the course's ridge points (so they never
    // wait on a model file).
    let parts = course_parts(&map);
    if ramp_meshes {
        for (stage, st) in &parts.stages {
            let look = ramp_look(&map, *stage);
            let mat = materials.add(look);
            for ramp in &st.ramps {
                commands.spawn((
                    Mesh3d(meshes.add(ramp_mesh(
                        ramp,
                        t.ramp_samples,
                        t.ramp_half_width,
                        t.ramp_height,
                    ))),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(o),
                    DespawnOnExit(AppState::InGame),
                ));
            }
        }
    }
    for ramp in parts.stages.values().flat_map(|s| s.ramps.iter()) {
        for piece in pieces(ramp, t.ramp_samples, t.ramp_half_width, t.ramp_height) {
            let Some(hull) = Collider::convex_hull(&piece) else {
                continue;
            };
            commands.spawn((
                Transform::from_translation(o),
                RigidBody::Static,
                hull,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    for w in &l.wedges {
        let city = CityBlock {
            piece: Piece::Kicker,
            centre: w.centre,
            size: w.size,
            yaw: w.yaw,
        };
        commands.spawn((
            Transform::from_translation(o + w.centre).with_rotation(Quat::from_rotation_y(w.yaw)),
            RigidBody::Static,
            Collider::convex_hull(&city.wedge())
                .unwrap_or(Collider::cuboid(w.size.x, w.size.y, w.size.z)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for b in &l.blocks {
        let collider = if b.round {
            Collider::cylinder(b.size.x / 2.0, b.size.y)
        } else {
            Collider::cuboid(b.size.x, b.size.y, b.size.z)
        };
        commands.spawn((
            Transform::from_translation(o + b.centre).with_rotation(Quat::from_rotation_y(b.yaw)),
            RigidBody::Static,
            collider,
            DespawnOnExit(AppState::InGame),
        ));
    }
    for b in &l.bodies {
        commands.spawn((
            Transform::from_translation(o + b.at + b.rot * b.offset).with_rotation(b.rot),
            RigidBody::Static,
            Collider::cuboid(b.size.x, b.size.y, b.size.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for lamp in &l.lamps {
        commands.spawn((
            PointLight {
                color: lamp.colour,
                intensity: lamp.lumens,
                range: lamp.range,
                ..default()
            },
            Transform::from_translation(o + lamp.at),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Tests: read the course out of the map files (`rules::surf::tuning` merges
/// them); a map that isn't there yet is skipped.
#[cfg(test)]
pub(crate) fn load_course_for_tests() {
    for c in crate::rules::surf::courses() {
        let path = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), c.path());
        if let Ok(bytes) = std::fs::read(&path) {
            assert!(c.set_bytes(&bytes), "{path} doesn't parse");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every map of the course that's there, as one.
    fn map() -> MapFile {
        let mut all = MapFile::default();
        for c in crate::rules::surf::courses() {
            let path = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), c.path());
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            match serde_json::from_str::<MapFile>(&text) {
                Ok(m) => all.entities.extend(m.entities),
                Err(e) => panic!("{path}: {e}"),
            }
        }
        assert!(!all.entities.is_empty(), "no surf maps");
        all
    }

    #[test]
    fn the_map_has_its_colliders_lights_and_models() {
        let m = map();
        let l = layout(&m);
        assert!(l.blocks.len() > 60 && l.lamps.len() > 20);
        let models = m.entities.iter().filter(|e| e.gltf.is_some()).count();
        assert!(models > 1000, "{models} placed models");
        // Every block has a size, every lamp a brightness.
        assert!(
            m.entities
                .iter()
                .filter(|e| e.has("Block"))
                .all(|e| e.size.is_some())
        );
        assert!(l.lamps.iter().all(|l| l.lumens > 0.0 && l.range > 0.0));
    }

    #[test]
    fn every_model_the_map_places_exists() {
        let root = format!("{}/../game/assets", env!("CARGO_MANIFEST_DIR"));
        for e in &map().entities {
            let path = e.gltf.clone().or_else(|| {
                e.components
                    .get("GltfSource")
                    .and_then(|g| g.get("path"))
                    .and_then(|p| p.as_str())
                    .map(str::to_owned)
            });
            let Some(path) = path else { continue };
            assert!(
                std::path::Path::new(&format!("{root}/{path}")).exists(),
                "{} places {path}, which isn't there",
                e.name
            );
        }
    }

    #[test]
    fn the_map_sits_inside_the_hall() {
        load_course_for_tests();
        let t = {
            let path = format!(
                "{}/../game/assets/{}",
                env!("CARGO_MANIFEST_DIR"),
                crate::rules::surf::TUNING.path()
            );
            let Ok(bytes) = std::fs::read(&path) else {
                panic!("no {path}")
            };
            assert!(crate::rules::surf::TUNING.set_bytes(&bytes));
            tuning()
        };
        let o = hall_at(&t);
        for b in &layout(&map()).blocks {
            assert!(
                crate::rules::surf::in_hall(&t, o + b.centre).is_some(),
                "{} is outside the hall",
                b.centre
            );
        }
    }

    /// No solid of the map (but a pad's own box and pillar) sits in the core of a
    /// ramp's body, under its ridge (the pillars and fins that stand in the
    /// faces are meant to).
    #[test]
    fn nothing_solid_is_inside_a_ramp() {
        use crate::rules::surf::ridge;
        load_course_for_tests();
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            crate::rules::surf::TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(crate::rules::surf::TUNING.set_bytes(&bytes));
        let t = tuning();
        let m = map();
        let samples: Vec<Vec3> = t
            .stages
            .iter()
            .flat_map(|s| s.ramps.iter())
            .flat_map(|r| ridge(r, t.ramp_samples * 2))
            .collect();
        assert!(samples.len() > 500, "{} ridge samples", samples.len());
        for e in m
            .entities
            .iter()
            .filter(|e| e.has("Block") && e.kind != "course_block")
        {
            let b = block(e);
            // The shells' floors, roofs and walls are far bigger than any prop.
            if b.size.x.max(b.size.z) > 100.0 {
                continue;
            }
            let (s, c) = (-b.yaw).sin_cos();
            for p in &samples {
                let d = Vec2::new(p.x - b.centre.x, p.z - b.centre.z);
                let l = Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c);
                let q = Vec2::new(l.x.abs() - b.size.x / 2.0, l.y.abs() - b.size.z / 2.0);
                let flat = q.max(Vec2::ZERO).length();
                let (lo, hi) = (b.centre.y - b.size.y / 2.0, b.centre.y + b.size.y / 2.0);
                let in_body = hi > p.y - t.ramp_height + 2.5 && lo < p.y - 2.0;
                assert!(
                    !(flat < 1.0 && in_body),
                    "{} is inside a ramp at {p}",
                    e.name
                );
            }
        }
    }

    /// Each stage's pad lies in the area of the pack that owns it.
    #[test]
    fn each_stage_pad_is_in_its_packs_area() {
        load_course_for_tests();
        let text = std::fs::read_to_string(format!(
            "{}/../game/assets/data/base/areas.ron",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_default();
        assert!(friendslop_base::areas::AREAS.set_bytes(text.as_bytes()));
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            crate::rules::surf::TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(crate::rules::surf::TUNING.set_bytes(&bytes));
        let t = tuning();
        let o = hall_at(&t);
        let want = [
            "surf",
            "surf",
            "surf",
            "surf",
            "surf",
            "surf_office",
            "surf_mall",
        ];
        for (i, w) in want.iter().enumerate() {
            let Some(at) = crate::rules::surf::stage_spawn(&t, i) else {
                panic!("no stage {i}")
            };
            let _ = o;
            let areas = friendslop_base::areas::areas();
            let Some(area) = areas.areas.iter().find(|a| a.id == *w) else {
                panic!("no area {w}")
            };
            assert!(area.holds(at), "stage {i} at {at} is outside {w}");
        }
    }
}
