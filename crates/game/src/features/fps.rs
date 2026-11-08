//! First-person camera and controls for everyone, all the time.
//! From issue #53 ("fps cam/controls").
//!
//! Every player now has a body (a `Walker` on their `Player` entity) that
//! walks around the meadow: WASD to walk, mouse to look, Space to jump, Shift
//! to run. The camera sits in your eyes and the cursor is locked; left click
//! uses whatever is under the crosshair (the same things you used to click
//! with the mouse pointer). Press V to swap back to the old overview camera
//! (pan with WASD, free cursor) and V again to return to your body.
//!
//! The server is authoritative: clients send stick/look/jump input, the server
//! moves the body, and on the host the scenery colliders stop you walking
//! through trees, walls and rocks. While you duel, the duel takes over.

use avian3d::prelude::*;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::camera::{CameraRig, FLOOR_HALF};
use crate::features::duel::Duelist;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::world::MainCamera;

pub const WALK_SPEED: f32 = 5.0;
pub const RUN_SPEED: f32 = 10.5;
pub const EYE: f32 = 1.6;
const JUMP_SPEED: f32 = 5.0;
const GRAVITY: f32 = 14.0;
const BODY_RADIUS: f32 = 0.35;
/// The soft wall: how far out you can walk (the edge hills rise beyond it).
const EDGE: f32 = FLOOR_HALF - 2.0;
const LOOK_SENSITIVITY: f32 = 0.003;

/// A player's body out in the world. Lives on the `Player` entity. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Walker {
    /// Feet position.
    pub pos: Vec3,
    pub yaw: f32,
}

/// Server only: the latest input of a walker, plus its fall speed.
#[derive(Component, Default, Debug)]
pub struct WalkInput {
    stick: Vec2,
    jump: bool,
    run: bool,
    vy: f32,
}

/// Server only: where the walker stood before this frame's step, so the
/// collision pass (host only, it needs the scenery) can undo or slide it.
#[derive(Component, Default, Debug)]
pub struct WalkPrev(pub Vec3);

/// Client -> server, every frame while walking (local x = right, y = forward).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct WalkControl {
    pub stick: Vec2,
    pub yaw: f32,
    pub jump: bool,
    pub run: bool,
}

/// Ordering hook: movement, then (on the host) collision.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WalkSet;

pub fn protocol(app: &mut App) {
    app.replicate::<Walker>()
        .add_client_event::<WalkControl>(Channel::Unreliable)
        .add_observer(on_walk_control)
        .add_systems(
            Update,
            (give_bodies, move_walkers)
                .chain()
                .in_set(WalkSet)
                .run_if(in_state(ServerState::Running)),
        );
}

/// Where player `id` appears: a loose line south of the centre, facing it.
pub fn spawn_point(id: u64) -> Vec3 {
    let col = (id % 5) as f32 - 2.0;
    let row = (id / 5 % 4) as f32;
    Vec3::new(col * 1.6, 0.0, 9.0 + row * 1.6)
}

fn give_bodies(mut commands: Commands, new: Query<(Entity, &Player), Without<Walker>>) {
    for (entity, player) in &new {
        commands.entity(entity).insert((
            Walker {
                pos: spawn_point(player.id),
                yaw: 0.0,
            },
            WalkInput::default(),
            WalkPrev::default(),
        ));
    }
}

fn on_walk_control(
    ev: On<FromClient<WalkControl>>,
    mut walkers: Query<(&PlayerOwner, &mut Walker, &mut WalkInput)>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((_, mut walker, mut input)) = walkers.iter_mut().find(|(o, _, _)| o.0 == owner) else {
        return;
    };
    input.stick = if ev.stick.is_finite() {
        ev.stick.clamp_length_max(1.0)
    } else {
        Vec2::ZERO
    };
    input.jump = ev.jump;
    input.run = ev.run;
    if ev.yaw.is_finite() && walker.yaw != ev.yaw {
        walker.yaw = ev.yaw;
    }
}

/// One step of walking for a body: returns the new feet position and fall
/// speed. Pure so it can be tested.
pub fn step(
    pos: Vec3,
    yaw: f32,
    stick: Vec2,
    run: bool,
    jump: bool,
    vy: f32,
    dt: f32,
) -> (Vec3, f32) {
    let speed = if run { RUN_SPEED } else { WALK_SPEED };
    let flat = Quat::from_rotation_y(yaw) * Vec3::new(stick.x, 0.0, -stick.y) * speed * dt;
    let mut next = pos + flat;
    next.x = next.x.clamp(-EDGE, EDGE);
    next.z = next.z.clamp(-EDGE, EDGE);
    let ground = crate::terrain::height_at(next.x, next.z);
    // Standing on the ground where we were: follow it up and down slopes
    // (no little falls walking downhill) unless we jump.
    let here = crate::terrain::height_at(pos.x, pos.z);
    let on_ground = vy <= 0.0 && pos.y <= here + 1e-3;
    if on_ground && !jump {
        return (next.with_y(ground), 0.0);
    }
    let vy = if on_ground {
        JUMP_SPEED
    } else {
        vy - GRAVITY * dt
    };
    next.y = pos.y + vy * dt;
    if next.y <= ground {
        return (next.with_y(ground), 0.0);
    }
    (next, vy)
}

fn move_walkers(
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    duelists: Query<&Duelist>,
    mut walkers: Query<(&Player, &mut Walker, &mut WalkInput, &mut WalkPrev)>,
) {
    if super::stillness::is_frozen(&still) {
        return; // the world stands still, and so do you
    }
    let dt = time.delta_secs().min(0.1);
    for (player, mut walker, mut input, mut prev) in &mut walkers {
        prev.0 = walker.pos;
        if duelists.iter().any(|d| d.player == player.id) {
            continue; // your duel body is the one moving
        }
        let idle = input.stick == Vec2::ZERO && !input.jump && input.vy == 0.0;
        if idle && walker.pos.y <= crate::terrain::height_at(walker.pos.x, walker.pos.z) {
            continue; // don't touch it, so nothing is re-sent
        }
        let (pos, vy) = step(
            walker.pos,
            walker.yaw,
            input.stick,
            input.run,
            input.jump,
            input.vy,
            dt,
        );
        input.vy = vy;
        if walker.pos != pos {
            walker.pos = pos;
        }
    }
}

// ---------------------------------------------------------------- host side

/// Host only: stop bodies walking through trees, rocks, walls and sheds by
/// sweeping a capsule along each step, sliding along whatever it hits.
fn collide_walkers(
    spatial: SpatialQuery,
    ground: Query<Entity, With<crate::terrain::GroundCollider>>,
    mut walkers: Query<(&mut Walker, &WalkPrev)>,
) {
    let body = Collider::capsule(BODY_RADIUS, 0.9);
    // Capsule centre above the feet: bottom ~0.1 m off the ground.
    let lift = Vec3::Y * (0.45 + BODY_RADIUS + 0.1);
    // The ground itself is followed in `step`, so only bump into things on it.
    let filter = SpatialQueryFilter::default().with_excluded_entities(ground);
    let sweep = |from: Vec3, delta: Vec3| -> Vec3 {
        let flat = delta.with_y(0.0);
        let Ok(dir) = Dir3::new(flat) else {
            return from + delta;
        };
        let config = ShapeCastConfig {
            ignore_origin_penetration: true,
            ..ShapeCastConfig::from_max_distance(flat.length())
        };
        match spatial.cast_shape(&body, from + lift, Quat::IDENTITY, dir, &config, &filter) {
            None => from + delta,
            Some(hit) => {
                let n = hit.normal2.with_y(0.0).normalize_or_zero();
                let slide = flat - n * flat.dot(n);
                let to_wall = dir * (hit.distance - 0.02).max(0.0);
                let mut out = from + to_wall + Vec3::Y * delta.y;
                if let Ok(sdir) = Dir3::new(slide) {
                    let config = ShapeCastConfig {
                        ignore_origin_penetration: true,
                        ..ShapeCastConfig::from_max_distance(slide.length())
                    };
                    if spatial
                        .cast_shape(&body, out + lift, Quat::IDENTITY, sdir, &config, &filter)
                        .is_none()
                    {
                        out += slide;
                    }
                }
                out
            }
        }
    };
    for (mut walker, prev) in &mut walkers {
        let delta = walker.pos - prev.0;
        if delta.with_y(0.0).length_squared() < 1e-8 {
            continue;
        }
        let mut pos = sweep(prev.0, delta);
        // Sliding moved us sideways on a slope: stay on (never under) the ground.
        let ground = crate::terrain::height_at(pos.x, pos.z);
        let walking = walker.pos.y <= crate::terrain::height_at(walker.pos.x, walker.pos.z);
        if pos.y < ground || (walking && pos.y - ground < 0.5) {
            pos.y = ground;
        }
        if pos != walker.pos {
            walker.pos = pos;
        }
    }
}

// ---------------------------------------------------------------- client side

/// This peer's camera mode and look angles.
#[derive(Resource, Debug)]
pub struct FpsView {
    /// In your body (true) or the overview camera (false). V toggles.
    pub first_person: bool,
    pub yaw: f32,
    pub pitch: f32,
    /// First Esc frees the cursor (a click grabs it again); a second Esc leaves.
    pub released: bool,
    /// Have we taken our facing from the server yet?
    synced: bool,
}

impl Default for FpsView {
    fn default() -> Self {
        Self {
            first_person: true,
            yaw: 0.0,
            pitch: 0.0,
            released: false,
            synced: false,
        }
    }
}

/// Where on screen "what I'm pointing at" is: the crosshair (screen centre)
/// in first person, otherwise the mouse cursor. Click features use this.
pub fn aim_point(window: &Window, view: &FpsView) -> Option<Vec2> {
    if view.first_person {
        Some(window.size() / 2.0)
    } else {
        window.cursor_position()
    }
}

/// My body, if I have one and I'm not duelling.
pub fn my_walker(
    me: Option<&LocalPlayer>,
    walkers: &Query<(&Player, &Walker)>,
    duelists: &Query<&Duelist>,
) -> Option<Walker> {
    let me = me?;
    if duelists.iter().any(|d| d.player == me.0) {
        return None;
    }
    walkers.iter().find(|(p, _)| p.id == me.0).map(|(_, w)| *w)
}

/// Run condition: I'm in first person and walking (not duelling).
pub fn walking_first_person(
    view: Res<FpsView>,
    me: Option<Res<LocalPlayer>>,
    duelists: Query<&Duelist>,
) -> bool {
    view.first_person && !me.is_some_and(|m| duelists.iter().any(|d| d.player == m.0))
}

/// Local: players whose floating name label must stay hidden on this peer
/// (filled by hide and seek so labels don't give hiders away).
#[derive(Resource, Default, Debug)]
pub struct HiddenLabels(pub std::collections::HashSet<u64>);

pub fn plugin(app: &mut App) {
    app.init_resource::<FpsView>()
        .init_resource::<HiddenLabels>()
        .add_systems(OnEnter(AppState::InGame), (reset_view, spawn_crosshair))
        .add_systems(OnExit(AppState::InGame), free_cursor)
        .add_systems(
            Update,
            collide_walkers
                .after(WalkSet)
                .run_if(in_state(ServerState::Running))
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            (
                toggle_view,
                walk_controls,
                dress_walkers,
                sync_walker_bodies,
                fps_camera.run_if(walking_first_person),
                update_crosshair,
                place_labels,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
}

fn reset_view(mut view: ResMut<FpsView>) {
    *view = FpsView::default();
}

fn free_cursor(mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut c) = cursor.single_mut() {
        c.grab_mode = CursorGrabMode::None;
        c.visible = true;
    }
}

fn toggle_view(
    keys: Res<ButtonInput<KeyCode>>,
    mut view: ResMut<FpsView>,
    mut rig: ResMut<CameraRig>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
) {
    if !keys.just_pressed(KeyCode::KeyV) {
        return;
    }
    view.first_person = !view.first_person;
    if !view.first_person
        && let Some(w) = my_walker(me.as_deref(), &walkers, &duelists)
    {
        // Overview starts looking at where you were standing.
        rig.focus = Vec3::ZERO;
        rig.pan(w.pos.xz());
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_controls(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut view: ResMut<FpsView>,
    mut was_walking: Local<bool>,
) {
    let Some(w) = my_walker(me.as_deref(), &walkers, &duelists) else {
        // Duelling (the duel drives the cursor) or not spawned yet.
        view.synced = false;
        *was_walking = false;
        return;
    };
    if !view.synced {
        view.yaw = w.yaw;
        view.synced = true;
    }
    let grab = view.first_person && !view.released;
    let want = if grab {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if let Ok(mut c) = cursor.single_mut()
        && c.grab_mode != want
    {
        c.grab_mode = want;
        c.visible = !grab;
    }
    if !view.first_person {
        if *was_walking {
            // Stop walking when you leave your body.
            commands.client_trigger(WalkControl {
                stick: Vec2::ZERO,
                yaw: view.yaw,
                jump: false,
                run: false,
            });
            *was_walking = false;
        }
        return;
    }
    if grab {
        view.yaw -= motion.delta.x * LOOK_SENSITIVITY;
        view.pitch = (view.pitch - motion.delta.y * LOOK_SENSITIVITY).clamp(-1.5, 1.5);
    }
    let mut stick = Vec2::ZERO;
    for (a, b, dir) in [
        (KeyCode::KeyW, KeyCode::ArrowUp, Vec2::Y),
        (KeyCode::KeyS, KeyCode::ArrowDown, Vec2::NEG_Y),
        (KeyCode::KeyA, KeyCode::ArrowLeft, Vec2::NEG_X),
        (KeyCode::KeyD, KeyCode::ArrowRight, Vec2::X),
    ] {
        if keys.pressed(a) || keys.pressed(b) {
            stick += dir;
        }
    }
    commands.client_trigger(WalkControl {
        stick: stick.normalize_or_zero(),
        yaw: view.yaw,
        jump: keys.pressed(KeyCode::Space),
        run: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
    });
    *was_walking = true;
}

/// The "that's you" marker over a body; only shown on its owner's screen.
#[derive(Component)]
struct OwnMarker(u64);

/// A floating name label (UI) that follows a body on screen.
#[derive(Component)]
struct WalkerLabel(Entity);

/// Keep labels over heads, and the own-marker only on my own body.
fn place_labels(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    hidden: Res<HiddenLabels>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    walkers: Query<(&Player, &GlobalTransform, &InheritedVisibility), With<WalkerDressed>>,
    mut markers: Query<(&OwnMarker, &mut Visibility), Without<WalkerLabel>>,
    mut labels: Query<(Entity, &WalkerLabel, &mut Node, &mut Text, &mut Visibility)>,
) {
    for (m, mut vis) in &mut markers {
        let mine = me.as_deref().is_some_and(|me| me.0 == m.0);
        vis.set_if_neq(if mine {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    for (label, of, mut node, mut text, mut vis) in &mut labels {
        let Ok((player, tf, shown)) = walkers.get(of.0) else {
            commands.entity(label).despawn();
            continue;
        };
        let screen = camera
            .world_to_viewport(cam_tf, tf.translation() + Vec3::Y * 2.0)
            .ok()
            .filter(|_| shown.get() && !hidden.0.contains(&player.id));
        vis.set_if_neq(if screen.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(p) = screen {
            node.left = Val::Px(p.x - 24.0);
            node.top = Val::Px(p.y - 20.0);
        }
        let want = if me.as_deref().is_some_and(|me| me.0 == player.id) {
            format!("you ({})", player.id)
        } else {
            format!("player {}", player.id)
        };
        if text.0 != want {
            text.0 = want;
        }
    }
}

/// Marks a walker whose body meshes have been added on this peer.
#[derive(Component)]
struct WalkerDressed;

/// Everyone gets the shared commune-person body (people.rs) in their colour.
fn dress_walkers(
    mut commands: Commands,
    new: Query<(Entity, &Player, &Walker), Without<WalkerDressed>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut kit: ResMut<crate::people::PeopleKit>,
) {
    for (entity, player, walker) in &new {
        let marker = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.9, 0.3),
            emissive: LinearRgba::rgb(1.0, 0.8, 0.2),
            ..default()
        });
        commands.spawn((
            WalkerLabel(entity),
            Text::new(format!("player {}", player.id)),
            TextFont::from_font_size(13.0),
            TextColor(crate::hud::TEXT),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Visibility::Hidden,
            DespawnOnExit(AppState::InGame),
        ));
        commands
            .entity(entity)
            .insert((
                WalkerDressed,
                crate::people::Gait::default(),
                Transform::from_translation(walker.pos)
                    .with_rotation(Quat::from_rotation_y(walker.yaw)),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((
                    OwnMarker(player.id),
                    Mesh3d(meshes.add(Cone::new(0.16, 0.28))),
                    MeshMaterial3d(marker.clone()),
                    Transform::from_xyz(0.0, 2.15, 0.0)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
                ));
                crate::people::spawn_person(
                    p,
                    entity,
                    &mut kit,
                    &mut meshes,
                    &mut materials,
                    crate::people::Look::for_index(player.id, player.id.wrapping_mul(7) + 1),
                );
            });
    }
}

/// Glide bodies to their replicated spot. Hide my own (I'm inside it) and
/// anyone's while they're in the duel arena (their duel body stands in).
fn sync_walker_bodies(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    view: Res<FpsView>,
    duelists: Query<&Duelist>,
    mut walkers: Query<(&Player, &Walker, &mut Transform, &mut Visibility), With<WalkerDressed>>,
) {
    let t = 1.0 - (-20.0 * time.delta_secs()).exp();
    for (player, walker, mut tf, mut vis) in &mut walkers {
        let mine = me.as_deref().is_some_and(|m| m.0 == player.id);
        let duelling = duelists.iter().any(|d| d.player == player.id);
        let want = if duelling || (mine && view.first_person) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        vis.set_if_neq(want);
        let target = if mine && view.first_person {
            // Snap: the camera smooths, the (hidden) body doesn't need to.
            walker.pos
        } else {
            tf.translation.lerp(walker.pos, t)
        };
        tf.translation = target;
        tf.rotation = Quat::from_rotation_y(walker.yaw);
    }
}

fn fps_camera(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
    view: Res<FpsView>,
    mut camera: Query<&mut Transform, (With<MainCamera>, Without<Player>)>,
) {
    let Some(w) = my_walker(me.as_deref(), &walkers, &duelists) else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    let eye = w.pos + Vec3::Y * EYE;
    // Replicated positions arrive in steps; ease towards them, but never lag far.
    let t = 1.0 - (-25.0 * time.delta_secs()).exp();
    cam.translation = if cam.translation.distance(eye) > 3.0 {
        eye
    } else {
        cam.translation.lerp(eye, t)
    };
    cam.rotation = Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, 0.0);
}

#[derive(Component)]
struct WalkCrosshair;

fn spawn_crosshair(mut commands: Commands) {
    commands.spawn((
        WalkCrosshair,
        Node {
            position_type: PositionType::Absolute,
            top: percent(50),
            left: percent(50),
            width: px(6),
            height: px(6),
            margin: UiRect::all(px(-3)),
            border_radius: BorderRadius::all(px(3)),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_crosshair(
    view: Res<FpsView>,
    me: Option<Res<LocalPlayer>>,
    duelists: Query<&Duelist>,
    mut dot: Query<&mut Visibility, With<WalkCrosshair>>,
) {
    let show = view.first_person && !me.is_some_and(|m| duelists.iter().any(|d| d.player == m.0));
    let want = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut dot {
        v.set_if_neq(want);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_forward_moves_along_the_view_and_stays_on_the_floor() {
        let (p, vy) = step(Vec3::ZERO, 0.0, Vec2::Y, false, false, 0.0, 1.0);
        assert!((p - Vec3::new(0.0, 0.0, -WALK_SPEED)).length() < 1e-4);
        assert_eq!(vy, 0.0);
        let (p, _) = step(Vec3::ZERO, 0.0, Vec2::Y, true, false, 0.0, 1.0);
        assert!(p.z < -WALK_SPEED, "running is faster");
        let (p, _) = step(Vec3::ZERO, 0.0, Vec2::Y, true, false, 0.0, 100.0);
        assert_eq!(p.z, -EDGE, "can't walk off the floor");
    }

    #[test]
    fn jumping_goes_up_and_comes_back_down() {
        let (mut p, mut vy) = step(Vec3::ZERO, 0.0, Vec2::ZERO, false, true, 0.0, 0.02);
        assert!(p.y > 0.0 && vy > 0.0);
        let mut peak: f32 = 0.0;
        for _ in 0..200 {
            (p, vy) = step(p, 0.0, Vec2::ZERO, false, false, vy, 0.02);
            peak = peak.max(p.y);
        }
        assert!(peak > 0.5, "a real hop");
        assert_eq!(p.y, 0.0, "landed");
        assert_eq!(vy, 0.0);
    }
}
