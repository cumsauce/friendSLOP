//! Ways in and out. From the friends: "can you also make it more obvious
//! where entrance/exits to things in general are?"
//!
//! One marker for every way into or out of a place: a feature tags an entity
//! with a [`Doorway`] (or calls [`spawn`]) and every peer draws, locally:
//!
//! - a soft glowing threshold strip on the ground, with a chevron pointing
//!   the way through (the entity's forward, -z), and a thin glowing arch frame
//!   round it ([`Shape::Arch`]), or a glowing square round a hatch in the
//!   ground ([`Shape::Hatch`]);
//! - a gentle pulse in the glow (shared unlit see-through materials, no lights,
//!   so it's cheap on the Low preset and in the browser);
//! - a floating label that faces the camera within a few metres: what kind of
//!   way it is ("way in", "way out", "stairs down", "way up") and where it goes,
//!   on an ink chip like the HUD's.
//!
//! The colour says the kind: mint in, butter out, lilac up or down. Tuning
//! (radius, colours, sizes, pulse, words) is `assets/data/play/doorway.ron`.
//! Doorways never change what anything does: clickable ones keep their
//! `Clickable` prompt, walk-in ones keep their trigger.

use bevy::prelude::*;
use serde::Deserialize;

use crate::AppState;
use crate::world::MainCamera;

/// Tuning and words: `assets/data/play/doorway.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Labels show within this many metres of the camera.
    pub label_radius: f32,
    /// ...and only while the camera is at most this far below the doorway's
    /// floor or this far above it (so the floors above and below stay quiet).
    pub label_below: f32,
    pub label_above: f32,
    /// Label text size.
    pub label_size: f32,
    /// Glow colours (r, g, b, 0..1): in, out, and up or down.
    pub colour_in: (f32, f32, f32),
    pub colour_out: (f32, f32, f32),
    pub colour_stairs: (f32, f32, f32),
    /// How strong the glow is at its brightest and dimmest (0..1 alpha).
    pub glow_max: f32,
    pub glow_min: f32,
    /// Pulses per second.
    pub pulse_hz: f32,
    /// Depth of the threshold strip (m) and thickness of the arch frame (m).
    pub strip_depth: f32,
    pub frame: f32,
    /// The words before the place's name.
    pub word_in: String,
    pub word_out: String,
    pub word_down: String,
    pub word_up: String,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/play/doorway.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Which way a doorway takes you.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Way {
    /// Into a place.
    In,
    /// Out of one.
    Out,
    /// Stairs, a ladder or a hatch down.
    Down,
    /// Stairs, a ladder or a climb up.
    Up,
}

/// How the marker is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A strip on the ground with an arch frame over it.
    Arch,
    /// A square on the ground round a hatch (manholes, cellar doors).
    Hatch,
}

/// A way into or out of somewhere. Put it on an entity with a `Transform`
/// (its origin on the ground in the middle of the opening, its forward -z
/// pointing through the way), or use [`spawn`].
#[derive(Component, Clone, Debug)]
pub struct Doorway {
    /// Where it goes ("Fulton St", "the sewers"): shown under the kind word.
    pub label: String,
    pub way: Way,
    pub shape: Shape,
    /// Opening width and height (m).
    pub width: f32,
    pub height: f32,
}

impl Doorway {
    pub fn new(way: Way, label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            way,
            shape: Shape::Arch,
            width: 1.6,
            height: 2.4,
        }
    }
    /// A way in.
    pub fn way_in(label: impl Into<String>) -> Self {
        Self::new(Way::In, label)
    }
    /// A way out.
    pub fn way_out(label: impl Into<String>) -> Self {
        Self::new(Way::Out, label)
    }
    /// Opening size (m).
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
    /// Draw as a square on the ground round a hatch `width` across.
    pub fn hatch(mut self, width: f32) -> Self {
        self.shape = Shape::Hatch;
        self.width = width;
        self.height = 0.6;
        self
    }
}

/// Spawn a doorway marker at `at` (ground, middle of the opening), its way
/// through pointing along `yaw` (radians about +y; 0 is -z). Gone on leaving
/// the game.
pub fn spawn(commands: &mut Commands, at: Vec3, yaw: f32, doorway: Doorway) -> Entity {
    commands
        .spawn((
            doorway,
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .id()
}

/// The yaw (radians, for [`spawn`]) whose way through points along `dir` (x, z).
pub fn yaw_towards(dir: Vec2) -> f32 {
    (-dir.x).atan2(-dir.y)
}

/// The label's two lines: the kind word, and where it goes.
pub fn label_lines(t: &Tuning, d: &Doorway) -> (String, String) {
    let word = match d.way {
        Way::In => &t.word_in,
        Way::Out => &t.word_out,
        Way::Down => &t.word_down,
        Way::Up => &t.word_up,
    };
    (word.clone(), d.label.trim().to_string())
}

/// Should a doorway at `at` show its label to a camera at `cam`?
pub fn label_shown(t: &Tuning, cam: Vec3, at: Vec3) -> bool {
    let dy = cam.y - at.y;
    dy > -t.label_below && dy < t.label_above && cam.xz().distance(at.xz()) < t.label_radius
}

/// The glow's alpha at time `secs`: a slow sine between min and max.
pub fn glow_at(t: &Tuning, secs: f32) -> f32 {
    let s = 0.5 + 0.5 * (secs * t.pulse_hz * std::f32::consts::TAU).sin();
    t.glow_min + (t.glow_max - t.glow_min) * s
}

/// The thin boxes a marker is made of, in its own frame: (centre, size).
pub fn pieces(t: &Tuning, d: &Doorway) -> Vec<(Vec3, Vec3)> {
    let (w, h, f, s) = (d.width, d.height, t.frame, t.strip_depth);
    let lift = 0.03;
    let mut out = Vec::new();
    match d.shape {
        Shape::Arch => {
            // Threshold strip, posts and lintel.
            out.push((Vec3::new(0.0, lift, 0.0), Vec3::new(w, 0.01, s)));
            for side in [-1.0, 1.0] {
                out.push((
                    Vec3::new(side * (w / 2.0 + f / 2.0), h / 2.0, 0.0),
                    Vec3::new(f, h, f),
                ));
            }
            out.push((
                Vec3::new(0.0, h + f / 2.0, 0.0),
                Vec3::new(w + 2.0 * f, f, f),
            ));
        }
        Shape::Hatch => {
            let r = w / 2.0 + f;
            for (c, sz) in [
                (Vec3::new(0.0, lift, r), Vec3::new(2.0 * r + f, 0.01, f)),
                (Vec3::new(0.0, lift, -r), Vec3::new(2.0 * r + f, 0.01, f)),
                (Vec3::new(r, lift, 0.0), Vec3::new(f, 0.01, 2.0 * r)),
                (Vec3::new(-r, lift, 0.0), Vec3::new(f, 0.01, 2.0 * r)),
            ] {
                out.push((c, sz));
            }
        }
    }
    out
}

/// The chevron on the ground in front of the opening, pointing through it
/// (-z): two bars, (centre, size, turn about +y). None for a hatch.
pub fn chevron(t: &Tuning, d: &Doorway) -> Vec<(Vec3, Vec3, f32)> {
    if d.shape == Shape::Hatch {
        return Vec::new();
    }
    let arm = (d.width * 0.28).clamp(0.25, 0.6);
    let z = t.strip_depth / 2.0 + 0.45;
    let a = std::f32::consts::FRAC_PI_4;
    [-1.0f32, 1.0]
        .into_iter()
        .map(|side| {
            let c = Vec3::new(side * arm * 0.35, 0.03, z);
            (c, Vec3::new(arm, 0.01, t.strip_depth * 0.5), -side * a)
        })
        .collect()
}

/// The shared look: one box mesh and a material per kind.
#[derive(Resource)]
struct Kit {
    cube: Handle<Mesh>,
    mats: [(Way, Handle<StandardMaterial>); 4],
}

fn colour(t: &Tuning, way: Way) -> Color {
    let (r, g, b) = match way {
        Way::In => t.colour_in,
        Way::Out => t.colour_out,
        Way::Down | Way::Up => t.colour_stairs,
    };
    Color::srgb(r, g, b)
}

impl Kit {
    fn mat(&self, way: Way) -> Handle<StandardMaterial> {
        self.mats
            .iter()
            .find(|(w, _)| *w == way)
            .map(|(_, m)| m.clone())
            .unwrap_or_default()
    }
}

/// Marks a doorway whose marker is drawn on this peer.
#[derive(Component)]
struct Dressed;

/// A doorway's floating label (UI), following entity `.0`.
#[derive(Component)]
struct DoorLabel(Entity);

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (dress, pulse, place_labels)
            .chain()
            .run_if(in_state(AppState::InGame)),
    )
    .add_systems(OnExit(AppState::InGame), |mut commands: Commands| {
        commands.remove_resource::<Kit>()
    });
}

fn dress(
    mut commands: Commands,
    new: Query<(Entity, &Doorway), Without<Dressed>>,
    kit: Option<Res<Kit>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if new.is_empty() {
        return;
    }
    let t = tuning();
    let fresh = kit.is_none().then(|| {
        let mat = |way: Way, m: &mut Assets<StandardMaterial>| {
            m.add(StandardMaterial {
                base_color: colour(&t, way).with_alpha(t.glow_max),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            })
        };
        Kit {
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            mats: [
                (Way::In, mat(Way::In, &mut materials)),
                (Way::Out, mat(Way::Out, &mut materials)),
                (Way::Down, mat(Way::Down, &mut materials)),
                (Way::Up, mat(Way::Down, &mut materials)),
            ],
        }
    });
    let Some(kit) = kit.as_deref().or(fresh.as_ref()) else {
        return;
    };
    for (e, d) in &new {
        let mat = kit.mat(d.way);
        let mut boxes: Vec<(Vec3, Vec3, f32)> = pieces(&t, d)
            .into_iter()
            .map(|(c, s)| (c, s, 0.0))
            .collect();
        boxes.extend(chevron(&t, d));
        commands.entity(e).insert(Dressed).with_children(|p| {
            for (c, s, yaw) in boxes {
                p.spawn((
                    Mesh3d(kit.cube.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_translation(c)
                        .with_rotation(Quat::from_rotation_y(yaw))
                        .with_scale(s),
                    bevy::light::NotShadowCaster,
                ));
            }
        });
        let (word, name) = label_lines(&t, d);
        let accent = colour(&t, d.way);
        commands
            .spawn((
                DoorLabel(e),
                Node {
                    position_type: PositionType::Absolute,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(crate::theme::S2 + 2.0), px(crate::theme::S1)),
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(crate::theme::R_CHIP)),
                    ..default()
                },
                UiTransform {
                    translation: Val2::percent(-50.0, -100.0),
                    ..default()
                },
                BackgroundColor(crate::theme::indigo::INK.with_alpha(0.86)),
                BorderColor::all(accent.with_alpha(0.9)),
                Visibility::Hidden,
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(word),
                    TextFont::from_font_size(t.label_size * 0.75),
                    TextColor(accent),
                    crate::assets::DisplayText,
                ));
                if !name.is_empty() {
                    p.spawn((
                        Text::new(name),
                        TextFont::from_font_size(t.label_size),
                        TextColor(crate::theme::indigo::CREAM),
                        crate::assets::DisplayText,
                    ));
                }
            });
    }
    if let Some(k) = fresh {
        commands.insert_resource(k);
    }
}

fn pulse(time: Res<Time>, kit: Option<Res<Kit>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(kit) = kit else {
        return;
    };
    let t = tuning();
    let a = glow_at(&t, time.elapsed_secs());
    for (way, m) in &kit.mats {
        if let Some(mut m) = materials.get_mut(m) {
            m.base_color = colour(&t, *way).with_alpha(a);
        }
    }
}

fn place_labels(
    mut commands: Commands,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    doors: Query<(&Doorway, &GlobalTransform, &InheritedVisibility)>,
    mut labels: Query<(Entity, &DoorLabel, &mut Node, &mut Visibility)>,
) {
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    let t = tuning();
    let cam = cam_tf.translation();
    for (label, of, mut node, mut vis) in &mut labels {
        let Ok((d, tf, shown)) = doors.get(of.0) else {
            commands.entity(label).despawn();
            continue;
        };
        let at = tf.translation();
        let top = at + Vec3::Y * (d.height + 0.5);
        let screen = (shown.get() && label_shown(&t, cam, at))
            .then(|| camera.world_to_viewport(cam_tf, top).ok())
            .flatten();
        vis.set_if_neq(if screen.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(p) = screen
            && (node.left != Val::Px(p.x) || node.top != Val::Px(p.y))
        {
            node.left = Val::Px(p.x);
            node.top = Val::Px(p.y);
        }
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
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.label_radius > 0.0 && t.label_size > 0.0);
        assert!(t.label_below >= 0.0 && t.label_above > 0.0);
        assert!(t.glow_max > t.glow_min && t.glow_min >= 0.0 && t.glow_max <= 1.0);
        assert!(t.pulse_hz > 0.0 && t.strip_depth > 0.0 && t.frame > 0.0);
        for w in [&t.word_in, &t.word_out, &t.word_down, &t.word_up] {
            assert!(!w.is_empty());
        }
    }

    #[test]
    fn labels_show_only_near_and_on_the_same_floor() {
        let t = t();
        let at = Vec3::new(0.0, 0.0, 0.0);
        assert!(label_shown(&t, Vec3::new(3.0, 1.6, 0.0), at));
        assert!(!label_shown(
            &t,
            Vec3::new(t.label_radius + 1.0, 1.6, 0.0),
            at
        ));
        // Station levels are 4 m apart: neither the one above nor below shows.
        assert!(!label_shown(&t, Vec3::new(1.0, 1.6 - 4.0, 0.0), at));
        assert!(!label_shown(&t, Vec3::new(1.0, 1.6 + 4.0, 0.0), at));
    }

    #[test]
    fn the_glow_pulses_between_its_bounds() {
        let t = t();
        for i in 0..50 {
            let a = glow_at(&t, i as f32 * 0.1);
            assert!(a >= t.glow_min - 1e-4 && a <= t.glow_max + 1e-4);
        }
    }

    #[test]
    fn markers_frame_their_opening() {
        let t = t();
        let d = Doorway::way_in("Fulton St").size(2.0, 2.5);
        let p = pieces(&t, &d);
        assert_eq!(p.len(), 4);
        assert!(
            p.iter()
                .all(|(c, s)| c.x.abs() + s.x / 2.0 <= 1.0 + 2.0 * t.frame + 1e-4)
        );
        assert_eq!(chevron(&t, &d).len(), 2);
        let h = Doorway::new(Way::Down, "the sewers").hatch(0.9);
        assert_eq!(pieces(&t, &h).len(), 4);
        assert!(chevron(&t, &h).is_empty());
        assert_eq!(label_lines(&t, &h).1, "the sewers");
    }

    #[test]
    fn yaw_points_the_way_through() {
        for dir in [Vec2::X, Vec2::Y, -Vec2::X, Vec2::new(0.6, -0.8)] {
            let f = Quat::from_rotation_y(yaw_towards(dir)) * Vec3::NEG_Z;
            assert!(f.xz().distance(dir.normalize()) < 1e-4, "{dir}: {f}");
        }
    }
}
