//! Challenge another player to a first-person "one in the chamber" duel.
//! From issue #14 by doomy.
//!
//! Press a number key to challenge someone; they press Y to accept or N to
//! decline. The two duelists get bodies in a small walled arena on the floor
//! and see through their own eyes; everyone else watches from the normal camera.
//!
//! One in the chamber rules: you start with one bullet and every shot kills.
//! Miss and you're out, so you have to punch (click with no bullets, up close).
//! Every kill gives you a bullet back. When you die you respawn with exactly
//! one bullet. Three lives each; lose them all and you lose the duel.
//!
//! The server is authoritative: clients only send stick/look input and "shoot".

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::world::MainCamera;

/// Arena centre on the floor and half its width.
pub const ARENA_CENTER: Vec3 = Vec3::new(0.0, 0.0, -12.0);
pub const ARENA_HALF: f32 = 7.0;
pub const LIVES: u8 = 3;
const SPEED: f32 = 5.0;
const EYE: f32 = 1.6;
const BODY_RADIUS: f32 = 0.45;
const PUNCH_REACH: f32 = 1.8;
const NEWS_SECS: f32 = 5.0;

/// A player's body while they're in the duel arena. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Duelist {
    pub player: u64,
    pub lives: u8,
    pub bullets: u8,
    /// Feet position.
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// Server only: the latest stick input of a duelist (local x = right, y = forward).
#[derive(Component, Default)]
struct DuelStick(Vec2);

/// Client -> server: "I challenge player `target`".
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ChallengeDuel {
    pub target: u64,
}

/// Client -> server: my answer to `from`'s challenge.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct AnswerDuel {
    pub from: u64,
    pub accept: bool,
}

/// Client -> server, every frame while duelling.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct DuelControl {
    pub stick: Vec2,
    pub yaw: f32,
    pub pitch: f32,
}

/// Client -> server: pull the trigger (or punch, with no bullets).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct DuelShoot;

/// Server -> the challenged player.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DuelChallenged {
    pub from: u64,
}

/// Server -> everyone: something happened in the duel.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DuelNews {
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Duelist>()
        .add_client_event::<ChallengeDuel>(Channel::Ordered)
        .add_client_event::<AnswerDuel>(Channel::Ordered)
        .add_client_event::<DuelControl>(Channel::Unreliable)
        .add_client_event::<DuelShoot>(Channel::Ordered)
        .add_server_event::<DuelChallenged>(Channel::Ordered)
        .add_server_event::<DuelNews>(Channel::Ordered)
        .init_resource::<DuelInvites>()
        .add_observer(on_challenge)
        .add_observer(on_answer)
        .add_observer(on_control)
        .add_observer(on_shoot)
        .add_systems(
            Update,
            (move_duelists, end_abandoned_duel).run_if(in_state(ServerState::Running)),
        );
}

/// Server only: open challenges as (from, to).
#[derive(Resource, Default)]
struct DuelInvites(Vec<(u64, u64)>);

/// The two corners duelists start from.
pub fn spawn_points() -> [Vec3; 2] {
    let d = ARENA_HALF - 1.0;
    [
        ARENA_CENTER + Vec3::new(-d, 0.0, -d),
        ARENA_CENTER + Vec3::new(d, 0.0, d),
    ]
}

/// Yaw that looks from `from` towards `to` (0 = facing -Z).
fn yaw_towards(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.z)
}

pub fn look_dir(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * Vec3::NEG_Z
}

/// Does a ray from `origin` along unit `dir` hit a duelist standing at `feet`?
/// The body is two stacked spheres (legs and chest), good enough for a duel.
pub fn ray_hits_body(origin: Vec3, dir: Vec3, feet: Vec3) -> bool {
    [0.5, 1.3].into_iter().any(|h| {
        let to = feet + Vec3::Y * h - origin;
        let along = to.dot(dir);
        along > 0.0 && (to - dir * along).length_squared() < BODY_RADIUS * BODY_RADIUS
    })
}

fn player_of(client: ClientId, players: &Query<(&Player, &PlayerOwner)>) -> Option<u64> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    players
        .iter()
        .find(|(_, o)| o.0 == owner)
        .map(|(p, _)| p.id)
}

fn client_of(id: u64, players: &Query<(&Player, &PlayerOwner)>) -> Option<ClientId> {
    players
        .iter()
        .find(|(p, _)| p.id == id)
        .map(|(_, o)| o.0.map_or(ClientId::Server, ClientId::Client))
}

fn news(commands: &mut Commands, text: String) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: DuelNews { text },
    });
}

fn on_challenge(
    ev: On<FromClient<ChallengeDuel>>,
    mut commands: Commands,
    mut invites: ResMut<DuelInvites>,
    players: Query<(&Player, &PlayerOwner)>,
    duelists: Query<(), With<Duelist>>,
) {
    let Some(from) = player_of(ev.client_id, &players) else {
        return;
    };
    let to = ev.target;
    if from == to || !duelists.is_empty() {
        return;
    }
    let Some(target) = client_of(to, &players) else {
        return;
    };
    invites.0.retain(|&(f, _)| f != from);
    invites.0.push((from, to));
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(target),
        message: DuelChallenged { from },
    });
    news(
        &mut commands,
        format!("player {from} challenges player {to} to a duel!"),
    );
}

fn on_answer(
    ev: On<FromClient<AnswerDuel>>,
    mut commands: Commands,
    mut invites: ResMut<DuelInvites>,
    players: Query<(&Player, &PlayerOwner)>,
    duelists: Query<(), With<Duelist>>,
) {
    let Some(me) = player_of(ev.client_id, &players) else {
        return;
    };
    let Some(i) = invites.0.iter().position(|&(f, t)| f == ev.from && t == me) else {
        return;
    };
    invites.0.remove(i);
    if !ev.accept {
        news(
            &mut commands,
            format!("player {me} chickened out of player {}'s duel", ev.from),
        );
        return;
    }
    if !duelists.is_empty() || client_of(ev.from, &players).is_none() {
        return;
    }
    invites
        .0
        .retain(|&(f, t)| ![f, t].contains(&me) && ![f, t].contains(&ev.from));
    let [a, b] = spawn_points();
    for (player, pos, other) in [(ev.from, a, b), (me, b, a)] {
        commands.spawn((
            Duelist {
                player,
                lives: LIVES,
                bullets: 1,
                pos,
                yaw: yaw_towards(pos, other),
                pitch: 0.0,
            },
            DuelStick::default(),
            Replicated,
        ));
    }
    news(
        &mut commands,
        format!("DUEL: player {} vs player {me}! one bullet each.", ev.from),
    );
}

fn on_control(
    ev: On<FromClient<DuelControl>>,
    players: Query<(&Player, &PlayerOwner)>,
    mut duelists: Query<(&mut Duelist, &mut DuelStick)>,
) {
    let Some(me) = player_of(ev.client_id, &players) else {
        return;
    };
    let Some((mut d, mut stick)) = duelists.iter_mut().find(|(d, _)| d.player == me) else {
        return;
    };
    stick.0 = ev.stick.clamp_length_max(1.0);
    if ev.yaw.is_finite() && ev.pitch.is_finite() {
        d.yaw = ev.yaw;
        d.pitch = ev.pitch.clamp(-1.5, 1.5);
    }
}

fn move_duelists(time: Res<Time>, mut duelists: Query<(&mut Duelist, &DuelStick)>) {
    let dt = time.delta_secs();
    for (mut d, stick) in &mut duelists {
        if stick.0 == Vec2::ZERO {
            continue;
        }
        let rot = Quat::from_rotation_y(d.yaw);
        let step = rot * Vec3::new(stick.0.x, 0.0, -stick.0.y) * SPEED * dt;
        let lim = ARENA_HALF - BODY_RADIUS;
        let rel = (d.pos + step - ARENA_CENTER)
            .clamp(Vec3::new(-lim, 0.0, -lim), Vec3::new(lim, 0.0, lim));
        d.pos = ARENA_CENTER + rel;
    }
}

/// What happens when `shooter` pulls the trigger at `target`. Returns whether
/// the target was hit. Pure so it can be tested.
pub fn resolve_shot(shooter: &mut Duelist, target: &Duelist) -> bool {
    let dir = look_dir(shooter.yaw, shooter.pitch);
    if shooter.bullets > 0 {
        shooter.bullets -= 1;
        ray_hits_body(shooter.pos + Vec3::Y * EYE, dir, target.pos)
    } else {
        let to = target.pos - shooter.pos;
        let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        to.length() < PUNCH_REACH && to.normalize_or_zero().dot(flat) > 0.5
    }
}

fn on_shoot(
    ev: On<FromClient<DuelShoot>>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner)>,
    mut duelists: Query<(Entity, &mut Duelist)>,
) {
    let Some(me) = player_of(ev.client_id, &players) else {
        return;
    };
    let mut both: Vec<_> = duelists.iter_mut().collect();
    let Some(i) = both.iter().position(|(_, d)| d.player == me) else {
        return;
    };
    let Some(j) = both.iter().position(|(_, d)| d.player != me) else {
        return;
    };
    let target = *both[j].1;
    let punched = both[i].1.bullets == 0;
    if !resolve_shot(&mut both[i].1, &target) {
        return;
    }
    both[i].1.bullets += 1;
    let shooter_pos = both[i].1.pos;
    let victim = &mut both[j].1;
    victim.lives = victim.lives.saturating_sub(1);
    let how = if punched { "punched out" } else { "shot" };
    if victim.lives == 0 {
        news(
            &mut commands,
            format!(
                "player {me} {how} player {}. player {me} WINS THE DUEL!",
                target.player
            ),
        );
        for (e, _) in &both {
            commands.entity(*e).despawn();
        }
        return;
    }
    // Respawn at whichever corner is further from the shooter, with one bullet.
    let [a, b] = spawn_points();
    let pos = if a.distance(shooter_pos) > b.distance(shooter_pos) {
        a
    } else {
        b
    };
    victim.pos = pos;
    victim.bullets = 1;
    victim.yaw = yaw_towards(pos, ARENA_CENTER);
    let lives = victim.lives;
    news(
        &mut commands,
        format!(
            "player {me} {how} player {} ({lives} lives left)",
            target.player
        ),
    );
}

/// If a duelist leaves the game, the duel is off.
fn end_abandoned_duel(
    mut commands: Commands,
    players: Query<&Player>,
    duelists: Query<(Entity, &Duelist)>,
) {
    if duelists.is_empty() {
        return;
    }
    let gone = duelists
        .iter()
        .any(|(_, d)| !players.iter().any(|p| p.id == d.player));
    if gone || duelists.iter().count() < 2 {
        for (e, _) in &duelists {
            commands.entity(e).despawn();
        }
        news(&mut commands, "the duel is off: someone left".into());
    }
}

// ---------------------------------------------------------------- client side

pub fn plugin(app: &mut App) {
    app.init_resource::<DuelView>()
        .add_observer(on_challenged)
        .add_observer(on_news)
        .add_systems(OnEnter(AppState::InGame), (spawn_arena, spawn_duel_hud))
        .add_systems(OnExit(AppState::InGame), reset_view)
        .add_systems(
            Update,
            (
                dress_duelists,
                sync_duelist_transforms,
                challenge_keys,
                fight_controls,
                first_person_camera,
                update_duel_hud,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
}

/// This peer's view of the duel.
#[derive(Resource, Default)]
struct DuelView {
    /// Someone challenged me.
    challenged_by: Option<u64>,
    news: Option<(String, f32)>,
    /// Where the camera was before I entered the arena.
    saved_camera: Option<Transform>,
    yaw: f32,
    pitch: f32,
}

fn on_challenged(ev: On<DuelChallenged>, mut view: ResMut<DuelView>) {
    view.challenged_by = Some(ev.from);
}

fn on_news(ev: On<DuelNews>, mut view: ResMut<DuelView>) {
    view.news = Some((ev.text.clone(), NEWS_SECS));
}

fn reset_view(mut view: ResMut<DuelView>) {
    *view = DuelView::default();
}

fn spawn_arena(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wall = materials.add(Color::srgb(0.6, 0.45, 0.3));
    let ground = materials.add(Color::srgb(0.75, 0.7, 0.6));
    let size = ARENA_HALF * 2.0;
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(size, size))),
        MeshMaterial3d(ground),
        Transform::from_translation(ARENA_CENTER + Vec3::Y * 0.01),
        DespawnOnExit(AppState::InGame),
    ));
    let long = meshes.add(Cuboid::new(size, 0.6, 0.2));
    for (offset, turn) in [
        (Vec3::new(0.0, 0.3, -ARENA_HALF), false),
        (Vec3::new(0.0, 0.3, ARENA_HALF), false),
        (Vec3::new(-ARENA_HALF, 0.3, 0.0), true),
        (Vec3::new(ARENA_HALF, 0.3, 0.0), true),
    ] {
        let rot = if turn {
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
        } else {
            Quat::IDENTITY
        };
        commands.spawn((
            Mesh3d(long.clone()),
            MeshMaterial3d(wall.clone()),
            Transform::from_translation(ARENA_CENTER + offset).with_rotation(rot),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Give each duelist a body: a capsule with a little gun.
fn dress_duelists(
    mut commands: Commands,
    new: Query<(Entity, &Duelist), Added<Duelist>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, d) in &new {
        let hue = (d.player as f32 * 137.5) % 360.0;
        let body = materials.add(Color::hsl(hue, 0.7, 0.55));
        let gun = materials.add(Color::srgb(0.15, 0.15, 0.18));
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(d.pos),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Capsule3d::new(BODY_RADIUS, 0.9))),
                    MeshMaterial3d(body),
                    Transform::from_xyz(0.0, 0.9, 0.0),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.1, 0.12, 0.45))),
                    MeshMaterial3d(gun),
                    Transform::from_xyz(0.3, 1.25, -0.4),
                ));
            });
    }
}

fn sync_duelist_transforms(mut q: Query<(&Duelist, &mut Transform)>) {
    for (d, mut t) in &mut q {
        t.translation = d.pos;
        t.rotation = Quat::from_rotation_y(d.yaw);
    }
}

/// Number keys pick who to challenge (players other than me, by id).
const DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

fn others(me: Option<&LocalPlayer>, players: &Query<&Player>) -> Vec<u64> {
    let mut ids: Vec<u64> = players
        .iter()
        .map(|p| p.id)
        .filter(|&id| me.is_none_or(|m| m.0 != id))
        .collect();
    ids.sort_unstable();
    ids
}

fn challenge_keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    me: Option<Res<LocalPlayer>>,
    players: Query<&Player>,
    duelists: Query<(), With<Duelist>>,
    mut view: ResMut<DuelView>,
) {
    if let Some(from) = view.challenged_by {
        let answer = if keys.just_pressed(KeyCode::KeyY) {
            Some(true)
        } else if keys.just_pressed(KeyCode::KeyN) {
            Some(false)
        } else {
            None
        };
        if let Some(accept) = answer {
            commands.client_trigger(AnswerDuel { from, accept });
            view.challenged_by = None;
        }
    }
    if !duelists.is_empty() {
        view.challenged_by = None;
        return;
    }
    let others = others(me.as_deref(), &players);
    for (i, key) in DIGITS.iter().enumerate() {
        if keys.just_pressed(*key)
            && let Some(&target) = others.get(i)
        {
            commands.client_trigger(ChallengeDuel { target });
        }
    }
}

fn my_duelist<'a>(me: Option<&LocalPlayer>, duelists: &'a Query<&Duelist>) -> Option<&'a Duelist> {
    let me = me?;
    duelists.iter().find(|d| d.player == me.0)
}

#[allow(clippy::too_many_arguments)]
fn fight_controls(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    me: Option<Res<LocalPlayer>>,
    duelists: Query<&Duelist>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut view: ResMut<DuelView>,
) {
    let Some(d) = my_duelist(me.as_deref(), &duelists) else {
        return;
    };
    if view.saved_camera.is_none() {
        // Just entered the arena: take the server's facing.
        view.yaw = d.yaw;
        view.pitch = d.pitch;
    }
    if let Ok(mut c) = cursor.single_mut()
        && c.grab_mode != CursorGrabMode::Locked
    {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
    }
    view.yaw -= motion.delta.x * 0.003;
    view.pitch = (view.pitch - motion.delta.y * 0.003).clamp(-1.5, 1.5);
    let mut stick = Vec2::ZERO;
    for (key, dir) in [
        (KeyCode::KeyW, Vec2::Y),
        (KeyCode::KeyS, Vec2::NEG_Y),
        (KeyCode::KeyA, Vec2::NEG_X),
        (KeyCode::KeyD, Vec2::X),
    ] {
        if keys.pressed(key) {
            stick += dir;
        }
    }
    commands.client_trigger(DuelControl {
        stick: stick.normalize_or_zero(),
        yaw: view.yaw,
        pitch: view.pitch,
    });
    if mouse.just_pressed(MouseButton::Left) {
        commands.client_trigger(DuelShoot);
    }
}

fn first_person_camera(
    me: Option<Res<LocalPlayer>>,
    duelists: Query<&Duelist>,
    mut camera: Query<&mut Transform, (With<MainCamera>, Without<Duelist>)>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut view: ResMut<DuelView>,
) {
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    match my_duelist(me.as_deref(), &duelists) {
        Some(d) => {
            if view.saved_camera.is_none() {
                view.saved_camera = Some(*cam);
            }
            cam.translation = d.pos + Vec3::Y * EYE;
            cam.rotation = Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, 0.0);
        }
        None => {
            let Some(saved) = view.saved_camera.take() else {
                return;
            };
            *cam = saved;
            if let Ok(mut c) = cursor.single_mut() {
                c.grab_mode = CursorGrabMode::None;
                c.visible = true;
            }
        }
    }
}

#[derive(Component)]
struct DuelHud;

#[derive(Component)]
struct Crosshair;

fn spawn_duel_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        DuelHud,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(12),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        Crosshair,
        Node {
            position_type: PositionType::Absolute,
            top: percent(50),
            left: percent(50),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_duel_hud(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    players: Query<&Player>,
    duelists: Query<&Duelist>,
    mut view: ResMut<DuelView>,
    mut hud: Query<&mut Text, (With<DuelHud>, Without<Crosshair>)>,
    mut crosshair: Query<&mut Text, (With<Crosshair>, Without<DuelHud>)>,
) {
    if let Some((_, left)) = &mut view.news {
        *left -= time.delta_secs();
        if *left <= 0.0 {
            view.news = None;
        }
    }
    let mut lines = Vec::new();
    if let Some((text, _)) = &view.news {
        lines.push(text.clone());
    }
    let mine = my_duelist(me.as_deref(), &duelists);
    if let Some(d) = mine {
        let bullets = if d.bullets == 0 {
            "no bullets: get close and punch!".to_string()
        } else {
            format!("bullets: {}", d.bullets)
        };
        lines.push(format!("DUEL  lives: {}  {bullets}", d.lives));
        lines.push("WASD move, mouse look, click to shoot".into());
    } else if let Some(from) = view.challenged_by {
        lines.push(format!(
            "player {from} challenges you to a duel! [Y] accept [N] decline"
        ));
    } else if duelists.is_empty() {
        let others = others(me.as_deref(), &players);
        if !others.is_empty() {
            let list: Vec<String> = others
                .iter()
                .take(DIGITS.len())
                .enumerate()
                .map(|(i, id)| format!("[{}] player {id}", i + 1))
                .collect();
            lines.push(format!("challenge to a duel: {}", list.join("  ")));
        }
    } else {
        let score: Vec<String> = duelists
            .iter()
            .map(|d| format!("player {} ({} lives)", d.player, d.lives))
            .collect();
        lines.push(format!("duel in progress: {}", score.join(" vs ")));
    }
    let text = lines.join("\n");
    for mut t in &mut hud {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
    let cross = if mine.is_some() { "+" } else { "" };
    for mut t in &mut crosshair {
        if t.0 != cross {
            t.0 = cross.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duelist(player: u64, pos: Vec3, look_at: Vec3) -> Duelist {
        Duelist {
            player,
            lives: LIVES,
            bullets: 1,
            pos,
            yaw: yaw_towards(pos, look_at),
            pitch: 0.0,
        }
    }

    #[test]
    fn aiming_at_someone_hits_and_aiming_away_misses() {
        let [a, b] = spawn_points();
        let mut shooter = duelist(0, a, b);
        let target = duelist(1, b, a);
        assert!(
            resolve_shot(&mut shooter, &target),
            "dead centre should hit"
        );
        assert_eq!(shooter.bullets, 0, "the one bullet is spent");

        let mut wild = duelist(0, a, a - (b - a));
        assert!(
            !resolve_shot(&mut wild, &target),
            "shooting backwards misses"
        );
    }

    #[test]
    fn no_bullets_means_punching_only_up_close() {
        let here = ARENA_CENTER;
        let near = here + Vec3::new(0.0, 0.0, -1.0);
        let far = here + Vec3::new(0.0, 0.0, -5.0);
        let mut puncher = duelist(0, here, near);
        puncher.bullets = 0;
        assert!(resolve_shot(&mut puncher, &duelist(1, near, here)));
        assert!(!resolve_shot(&mut puncher, &duelist(1, far, here)));
    }
}
