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

use std::collections::HashMap;

use bevy::asset::LoadState;
use bevy::gltf::{Gltf, GltfAssetLabel};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy::world_serialization::WorldAsset;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::assets::{Sfx, play_sfx};
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::rigged::{RigClip, Rigged};
use crate::world::MainCamera;

pub use crate::rules::Duelist;
/// Arena centre on the floor and half its width.
pub use friendslop_world::places::{arena_center, arena_half};
/// Tuning numbers: `assets/data/play/duel.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub lives: u8,
    pub speed: f32,
    pub punch_reach: f32,
    pub news_secs: f32,
    /// The first-person revolver is modeler-made at real size (34 cm, barrel
    /// along +X; source in `.modeler/`), shown a touch big so it reads in first
    /// person. Nudge after eyeballing a duel.
    pub gun_scale: f32,
    /// Lengths of the one-shot clips as authored, in seconds.
    pub shoot_secs: f32,
    pub hit_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/play/duel.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
const EYE: f32 = 1.6;
const BODY_RADIUS: f32 = 0.45;

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

/// Server -> the challenged player. `cancel` retracts a previous challenge
/// from the same player (they challenged someone else instead): the target
/// should drop any prompt for it.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DuelChallenged {
    pub from: u64,
    pub cancel: bool,
}

/// Server -> everyone: something happened in the duel.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DuelNews {
    pub text: String,
}

/// Server only: a duel was fought to the end: `winner` slew `loser`.
/// Abandoned duels don't fire it.
#[derive(Event, Clone, Copy, Debug)]
pub struct DuelFought {
    pub winner: u64,
    pub loser: u64,
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
        )
        .add_systems(OnExit(ServerState::Running), clear_invites);
}

/// Server only: open challenges as (from, to), plus whether a duel is live.
/// `active` is set the moment an answer spawns duelists: the `Duelist`
/// entities themselves only exist after deferred commands apply, so reading
/// them alone lets two answers in one frame start overlapping duels.
/// `spawn_pending` covers the accept→spawn window: `end_abandoned_duel` must
/// not free the guard there.
#[derive(Resource, Default)]
struct DuelInvites {
    open: Vec<(u64, u64)>,
    active: bool,
    spawn_pending: bool,
}

/// The two corners duelists start from.
pub fn spawn_points() -> [Vec3; 2] {
    let d = arena_half() - 1.0;
    [
        arena_center() + Vec3::new(-d, 0.0, -d),
        arena_center() + Vec3::new(d, 0.0, d),
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
    still: Query<&crate::features::stillness::Stillness>,
    mut commands: Commands,
    mut invites: ResMut<DuelInvites>,
    players: Query<(&Player, &PlayerOwner)>,
    duelists: Query<(), With<Duelist>>,
    mut fate: super::fate::Fate,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(from) = player_of(ev.client_id, &players) else {
        return;
    };
    let to = ev.target;
    if from == to || invites.active || !duelists.is_empty() {
        return;
    }
    if fate.fails(ev.client_id) {
        return;
    }
    let Some(target) = client_of(to, &players) else {
        return;
    };
    // If this replaces an earlier challenge of ours, void the old target's
    // prompt so they don't answer a challenge that no longer exists.
    if let Some(&(_, old_to)) = invites.open.iter().find(|&&(f, _)| f == from)
        && let Some(old_target) = client_of(old_to, &players)
    {
        commands.server_trigger(ToClients {
            targets: SendTargets::Single(old_target),
            message: DuelChallenged { from, cancel: true },
        });
    }
    invites.open.retain(|&(f, _)| f != from);
    invites.open.push((from, to));
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(target),
        message: DuelChallenged {
            from,
            cancel: false,
        },
    });
    news(
        &mut commands,
        format!("player {from} challenges player {to} to a duel!"),
    );
}

fn on_answer(
    ev: On<FromClient<AnswerDuel>>,
    still: Query<&crate::features::stillness::Stillness>,
    mut commands: Commands,
    mut invites: ResMut<DuelInvites>,
    players: Query<(&Player, &PlayerOwner)>,
    duelists: Query<(), With<Duelist>>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(me) = player_of(ev.client_id, &players) else {
        return;
    };
    let Some(i) = invites
        .open
        .iter()
        .position(|&(f, t)| f == ev.from && t == me)
    else {
        return;
    };
    invites.open.remove(i);
    if !ev.accept {
        news(
            &mut commands,
            format!("player {me} chickened out of player {}'s duel", ev.from),
        );
        return;
    }
    // `active` is the synchronous guard: the `Duelist` spawns below are
    // deferred, so a second accept in this same batch would otherwise still
    // see an empty `duelists` query and start an overlapping duel.
    if invites.active || !duelists.is_empty() || client_of(ev.from, &players).is_none() {
        return;
    }
    invites.active = true;
    invites.spawn_pending = true;
    invites
        .open
        .retain(|&(f, t)| ![f, t].contains(&me) && ![f, t].contains(&ev.from));
    let [a, b] = spawn_points();
    for (player, pos, other) in [(ev.from, a, b), (me, b, a)] {
        commands.spawn((
            Duelist {
                player,
                // Lives 0 would mean "already dead" to `on_shoot`'s guard.
                lives: tuning().lives.max(1),
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
    still: Query<&crate::features::stillness::Stillness>,
    players: Query<(&Player, &PlayerOwner)>,
    mut duelists: Query<(&mut Duelist, &mut DuelStick)>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(me) = player_of(ev.client_id, &players) else {
        return;
    };
    let Some((mut d, mut stick)) = duelists.iter_mut().find(|(d, _)| d.player == me) else {
        return;
    };
    // Non-finite sticks would NaN the duelist's replicated position forever.
    stick.0 = if ev.stick.is_finite() {
        ev.stick.clamp_length_max(1.0)
    } else {
        Vec2::ZERO
    };
    // Only touch the replicated duelist when the aim moved (#151).
    let pitch = ev.pitch.clamp(-1.5, 1.5);
    if ev.yaw.is_finite() && ev.pitch.is_finite() && (d.yaw != ev.yaw || d.pitch != pitch) {
        d.yaw = ev.yaw;
        d.pitch = pitch;
    }
}

fn move_duelists(
    time: Res<Time>,
    mut duelists: Query<(&mut Duelist, &DuelStick)>,
    stats: Query<(&Player, &super::stats::Stats)>,
) {
    let dt = time.delta_secs();
    for (mut d, stick) in &mut duelists {
        if stick.0 == Vec2::ZERO {
            continue;
        }
        let hrumph = stats
            .iter()
            .find(|(p, _)| p.id == d.player)
            .map_or(0, |(_, s)| s.hrumph);
        let speed = tuning().speed * super::stats::hrumph_speed(hrumph);
        let rot = Quat::from_rotation_y(d.yaw);
        let step = rot * Vec3::new(stick.0.x, 0.0, -stick.0.y) * speed * dt;
        let lim = arena_half() - BODY_RADIUS;
        let rel = (d.pos + step - arena_center())
            .clamp(Vec3::new(-lim, 0.0, -lim), Vec3::new(lim, 0.0, lim));
        let pos = arena_center() + rel;
        if d.pos != pos {
            d.pos = pos; // pressed against the wall: nothing to replicate
        }
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
        to.length() < tuning().punch_reach && to.normalize_or_zero().dot(flat) > 0.5
    }
}

fn on_shoot(
    ev: On<FromClient<DuelShoot>>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner)>,
    mut duelists: Query<(Entity, &mut Duelist)>,
    still: Query<&super::stillness::Stillness>,
    mut fate: super::fate::Fate,
    mut invites: ResMut<DuelInvites>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Some(me) = player_of(ev.client_id, &players) else {
        return;
    };
    if duelists.iter().any(|(_, d)| d.player == me) && fate.fails(ev.client_id) {
        return;
    }
    let mut both: Vec<_> = duelists.iter_mut().collect();
    let Some(i) = both.iter().position(|(_, d)| d.player == me) else {
        return;
    };
    let Some(j) = both.iter().position(|(_, d)| d.player != me) else {
        return;
    };
    // A shot earlier in this batch already ended the duel (the winning
    // despawn is still queued): `lives` is the synchronous claim, so stop
    // before the refund-and-kill path can fire `DuelFought` twice.
    if both[i].1.lives == 0 || both[j].1.lives == 0 {
        return;
    }
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
        invites.active = false;
        // A real fight, as opposed to a duel that was called off.
        commands.trigger(DuelFought {
            winner: me,
            loser: target.player,
        });
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
    victim.yaw = yaw_towards(pos, arena_center());
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
    mut invites: ResMut<DuelInvites>,
) {
    if duelists.is_empty() {
        // The duel is over — but never free the guard during the
        // accept→spawn window: an answer's duelists are still queued then,
        // and a cleared guard lets a second answer start an overlapping duel.
        if !invites.spawn_pending {
            invites.active = false;
        }
        return;
    }
    invites.spawn_pending = false;
    let gone = duelists
        .iter()
        .any(|(_, d)| !players.iter().any(|p| p.id == d.player));
    if gone || duelists.iter().count() < 2 {
        for (e, _) in &duelists {
            commands.entity(e).despawn();
        }
        invites.active = false;
        news(&mut commands, "the duel is off: someone left".into());
    }
}

/// Session over: no invite may outlive it (a stuck `spawn_pending` would
/// block every future duel).
fn clear_invites(mut invites: ResMut<DuelInvites>) {
    *invites = DuelInvites::default();
}

// ---------------------------------------------------------------- client side

pub fn plugin(app: &mut App) {
    app.add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
    app.init_resource::<DuelView>()
        .init_resource::<DuelRigs>()
        .add_observer(on_challenged)
        .add_observer(on_news)
        .add_systems(OnEnter(AppState::InGame), (spawn_arena, spawn_duel_hud))
        .add_systems(OnExit(AppState::InGame), reset_view)
        .add_systems(
            Update,
            (
                load_gun,
                dress_duelists,
                duelist_fallback,
                sync_duelist_transforms,
                challenge_keys,
                fight_controls,
                first_person_camera,
                animate_duelists,
                viewmodel_gun,
                update_duel_hud.run_if(crate::world::hud_tick()),
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
    /// The first-person revolver child of the camera, while I'm duelling.
    viewmodel: Option<Entity>,
}

fn on_challenged(ev: On<DuelChallenged>, mut view: ResMut<DuelView>) {
    if ev.cancel {
        // The challenger moved on: only drop the prompt if it's for theirs.
        if view.challenged_by == Some(ev.from) {
            view.challenged_by = None;
        }
    } else {
        view.challenged_by = Some(ev.from);
    }
}

/// Everyone hears a duel they're watching: a shot or a punch lands.
fn on_news(
    ev: On<DuelNews>,
    mut view: ResMut<DuelView>,
    mut commands: Commands,
    sfx: Option<Res<Sfx>>,
) {
    view.news = Some((ev.text.clone(), tuning().news_secs));
    let Some(sfx) = sfx else { return };
    if ev.text.contains("punched") {
        play_sfx(&mut commands, &sfx.punch, 0.75);
    } else if ev.text.contains("shot") {
        play_sfx(&mut commands, &sfx.gunshot, 0.6);
    }
}

fn reset_view(
    mut commands: Commands,
    mut view: ResMut<DuelView>,
    guns: Query<(), With<ViewModelGun>>,
) {
    if let Some(gun) = view.viewmodel.take()
        && guns.contains(gun)
    {
        commands.entity(gun).despawn();
    }
    *view = DuelView::default();
}

/// A dusty frontier pit: sandy floor, a wooden plank fence with posts, bunting
/// strung between tall corner poles, and barrels and hay bales outside. Visual
/// only: the footprint is still `arena_half()` and movement is clamped in
/// `move_duelists`, so gameplay is unchanged.
fn spawn_arena(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use std::f32::consts::FRAC_PI_2;
    let mut spawn = |mesh: Handle<Mesh>, mat: Handle<StandardMaterial>, t: Transform| {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            t,
            DespawnOnExit(AppState::InGame),
        ));
    };
    let size = arena_half() * 2.0;
    // Sand: a vertex-coloured grid with ripples and darker scuffs.
    spawn(
        meshes.add(sand_mesh(size)),
        materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            reflectance: 0.05,
            ..default()
        }),
        Transform::from_translation(arena_center() + Vec3::Y * 0.015),
    );
    // Bunting: little flags hanging on a sagging string between corner poles.
    spawn(
        meshes.add(bunting_mesh()),
        materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.8,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
        Transform::from_translation(arena_center()),
    );
    // Modeler-made fence, barrels and bales (sources in `.modeler/`, prefabs
    // `sp-fence-*`, `sp-barrel`, `sp-hay-bale`), placed exactly as the old
    // primitives were (each model is centred like the shape it replaced).
    let model = |file: &str| -> WorldAssetRoot {
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
        )
    };
    let post = model("fence_post.gltf");
    let tall = model("fence_corner.gltf");
    let plank = model("fence_plank.gltf");
    let plank_light = model("fence_plank_light.gltf");
    let mut place = |m: &WorldAssetRoot, t: Transform| {
        commands.spawn((m.clone(), t, DespawnOnExit(AppState::InGame)));
    };
    // Fence: posts every ~1.4 m, two rails of slightly crooked planks.
    let posts = 10;
    for side in 0..4 {
        let rot = Quat::from_rotation_y(FRAC_PI_2 * side as f32);
        let along = rot * Vec3::X;
        let out = rot * Vec3::Z * arena_half();
        for k in 0..=posts {
            let t = -arena_half() + size * k as f32 / posts as f32;
            let at = arena_center() + out + along * t;
            let corner = k == 0 || k == posts;
            if corner {
                place(&tall, Transform::from_translation(at + Vec3::Y * 1.2));
            } else {
                let lean = (k as f32 * 1.7 + side as f32).sin() * 0.04;
                place(
                    &post,
                    Transform::from_translation(at + Vec3::Y * 0.45)
                        .with_rotation(Quat::from_axis_angle(along, lean)),
                );
            }
            if k < posts {
                let seg = size / posts as f32;
                for (row, y) in [0.28f32, 0.62].into_iter().enumerate() {
                    let wobble = ((k * 7 + row * 3 + side * 5) % 5) as f32 * 0.012 - 0.024;
                    let m = if (k + row + side) % 3 == 0 {
                        &plank_light
                    } else {
                        &plank
                    };
                    place(
                        m,
                        Transform::from_translation(
                            at + along * (seg * 0.5)
                                + Vec3::Y * (y + wobble)
                                + rot * Vec3::Z * 0.06,
                        )
                        .with_rotation(rot * Quat::from_rotation_z(wobble * 2.0))
                        .with_scale(Vec3::new(seg + 0.12, 1.0, 1.0)),
                    );
                }
            }
        }
    }
    // Barrels and hay bales just outside the fence.
    let barrel = model("barrel.gltf");
    let bale = model("hay_bale.gltf");
    let o = arena_half() + 0.9;
    for (x, z) in [(-o, -o + 1.2), (o - 0.6, o), (o, -o + 0.4)] {
        let at = arena_center() + Vec3::new(x, 0.0, z);
        place(&barrel, Transform::from_translation(at + Vec3::Y * 0.475));
    }
    for (x, z, yaw, stack) in [
        (-o + 0.2, o - 0.5, 0.3, true),
        (o, 1.5, FRAC_PI_2, false),
        (-2.5, -o, 0.1, true),
    ] {
        let at = arena_center() + Vec3::new(x, 0.0, z);
        let r = Quat::from_rotation_y(yaw);
        place(
            &bale,
            Transform::from_translation(at + Vec3::Y * 0.275).with_rotation(r),
        );
        if stack {
            place(
                &bale,
                Transform::from_translation(at + Vec3::Y * 0.825)
                    .with_rotation(r * Quat::from_rotation_y(0.4)),
            );
        }
    }
}

/// Cat-filtered linear vertex colour from pre-filter sRGB.
fn cat_vertex(r: f32, g: f32, b: f32) -> [f32; 4] {
    let lin = crate::features::cat_palette::cat_palette(Color::srgb(r, g, b)).to_linear();
    [lin.red, lin.green, lin.blue, 1.0]
}

/// The arena's sand floor (centred on the origin), with ripples and scuffs.
fn sand_mesh(size: f32) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    const CELLS: u32 = 40;
    let n = CELLS + 1;
    let half = size * 0.5;
    let mut pos = Vec::new();
    let mut col = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let x = -half + size * i as f32 / CELLS as f32;
            let z = -half + size * j as f32 / CELLS as f32;
            let ripple = (x * 2.3 + (z * 0.7).sin() * 1.5).sin() * 0.5 + 0.5;
            let speck = crate::art::hash(i as i32, j as i32, 404);
            let scuff = crate::art::hash(i as i32 / 5, j as i32 / 5, 405);
            let edge = (half - x.abs().max(z.abs())).clamp(0.0, 1.0);
            let l = 0.82 + ripple * 0.08 + speck * 0.1
                - (scuff > 0.8) as u8 as f32 * 0.12
                - (1.0 - edge) * 0.2;
            pos.push([x, 0.0, z]);
            col.push(cat_vertex(0.95 * l, 0.8 * l, 0.58 * l));
        }
    }
    let mut idx = Vec::new();
    for j in 0..CELLS {
        for i in 0..CELLS {
            let a = j * n + i;
            idx.extend_from_slice(&[a, a + n, a + 1, a + 1, a + n, a + n + 1]);
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
    .with_inserted_indices(Indices::U32(idx))
}

/// Triangle pennants on strings along all four sides, relative to the centre.
fn bunting_mesh() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::PrimitiveTopology;
    const FLAGS: u32 = 14;
    let colors = [
        (0.95, 0.3, 0.3),
        (0.3, 0.5, 1.0),
        (1.0, 0.9, 0.3),
        (1.0, 1.0, 1.0),
    ];
    let h = arena_half();
    let corners = [
        Vec3::new(-h, 0.0, -h),
        Vec3::new(h, 0.0, -h),
        Vec3::new(h, 0.0, h),
        Vec3::new(-h, 0.0, h),
    ];
    let mut pos = Vec::new();
    let mut col = Vec::new();
    let string = cat_vertex(0.9, 0.85, 0.7);
    for side in 0..4 {
        let (a, b) = (corners[side], corners[(side + 1) % 4]);
        let at = |t: f32| a.lerp(b, t) + Vec3::Y * (2.3 - (t * (1.0 - t)) * 1.6);
        for f in 0..FLAGS {
            let (t0, t1) = (f as f32 / FLAGS as f32, (f as f32 + 0.8) / FLAGS as f32);
            let (p0, p1) = (at(t0 + 0.02), at(t1));
            let tip = (p0 + p1) * 0.5 - Vec3::Y * 0.35;
            let (r, g, bl) = colors[(f as usize + side) % colors.len()];
            let c = cat_vertex(r, g, bl);
            pos.extend_from_slice(&[p0.to_array(), p1.to_array(), tip.to_array()]);
            col.extend_from_slice(&[c, c, c]);
            // The string between this flag and the next, as a thin sliver.
            let p2 = at((f as f32 + 1.0) / FLAGS as f32 + 0.02);
            let d = Vec3::Y * 0.02;
            pos.extend_from_slice(&[p1.to_array(), p2.to_array(), (p1 + d).to_array()]);
            col.extend_from_slice(&[string, string, string]);
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
}

/// Give each duelist a body: a rigged cowboy from `assets/models` (drawn and
/// animated by `crate::rigged`), with a capsule-and-gun fallback if the
/// model file isn't there.
fn dress_duelists(
    mut commands: Commands,
    new: Query<(Entity, &Duelist), Added<Duelist>>,
    assets: Res<AssetServer>,
) {
    for (entity, d) in &new {
        let path = body_model(d.player);
        commands.entity(entity).insert((
            Transform::from_translation(d.pos),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
            Rigged { path },
            RigClip::looping("idle"),
            BodyPending(assets.load(path)),
        ));
    }
}

/// Couples duel as cowgirls, odd-numbered players as cowboys.
fn body_model(player: u64) -> &'static str {
    if player.is_multiple_of(2) {
        FEMALE
    } else {
        MALE
    }
}

/// Duelists still waiting to hear whether their model file exists.
#[derive(Component)]
struct BodyPending(Handle<Gltf>);

/// Settle each body once its file loads, or give it a capsule if it never does.
fn duelist_fallback(
    mut commands: Commands,
    pending: Query<(Entity, &BodyPending, &Duelist)>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, waiting, d) in &pending {
        match assets.load_state(waiting.0.id()) {
            LoadState::Loaded => {
                commands.entity(entity).remove::<BodyPending>();
            }
            LoadState::Failed(_) => {
                commands.entity(entity).remove::<BodyPending>();
                spawn_capsule_body(&mut commands, entity, &mut meshes, &mut materials, d);
            }
            _ => {}
        }
    }
}

/// The original look: a capsule with a little gun, tinted per player.
fn spawn_capsule_body(
    commands: &mut Commands,
    entity: Entity,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    d: &Duelist,
) {
    let hue = (d.player as f32 * 137.5) % 360.0;
    let body = materials.add(Color::hsl(hue, 0.7, 0.55));
    let gun = materials.add(Color::srgb(0.15, 0.15, 0.18));
    commands.entity(entity).with_children(|p| {
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

// ------------------------------------------------------------ duel models

/// The modeler's duel cowboys (sources in `.modeler/`): real size, boots on
/// the floor at the origin, facing -Z, revolver in the right hand (bone
/// `hand_r`). Clips: `idle`, `walk`, `shoot`, `hit`.
const MALE: &str = "models/characters/cowboy_male.gltf";
const FEMALE: &str = "models/characters/cowboy_female.gltf";
const GUN: &str = "models/props/revolver.gltf";
/// The walk clip strides ~1.4 m/s; duelists move faster, so play it quicker.
const WALK_SPEED: f32 = 1.8;

#[derive(Resource, Default)]
struct DuelRigs {
    gun: GunState,
    /// A strong handle to the gun while it loads. Without it the per-frame
    /// `load` handle drops at the end of the frame, the half-done load is
    /// thrown away and the next frame starts it again, forever.
    loading: Option<Handle<WorldAsset>>,
}

/// The first-person revolver is a plain static model: a scene, no rig.
#[derive(Default)]
enum GunState {
    #[default]
    Pending,
    Failed,
    Ready(Handle<WorldAsset>),
}

/// Load the first-person revolver.
fn load_gun(assets: Res<AssetServer>, mut rigs: ResMut<DuelRigs>) {
    if !matches!(rigs.gun, GunState::Pending) {
        return;
    }
    let scene = rigs
        .loading
        .get_or_insert_with(|| assets.load(GltfAssetLabel::Scene(0).from_asset(GUN)))
        .clone();
    if assets.is_loaded_with_dependencies(&scene) {
        rigs.gun = GunState::Ready(scene);
    } else if matches!(assets.load_state(scene.id()), LoadState::Failed(_)) {
        rigs.gun = GunState::Failed;
    }
}

fn sync_duelist_transforms(mut q: Query<(&Duelist, &mut Transform)>) {
    for (d, mut t) in &mut q {
        t.translation = d.pos;
        t.rotation = Quat::from_rotation_y(d.yaw);
    }
}

/// A first-person revolver held by this peer while duelling. The glb variant
/// is a plain scene; the procedural fallback is just a box.
#[derive(Component)]
struct ViewModelGun;

/// My revolver, in my hands (first person) while I'm duelling.
fn viewmodel_gun(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    duelists: Query<&Duelist>,
    camera: Query<Entity, With<MainCamera>>,
    mut view: ResMut<DuelView>,
    rigs: Res<DuelRigs>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mine = my_duelist(me.as_deref(), &duelists).is_some();
    match (mine, view.viewmodel) {
        (true, None) => {
            let Ok(camera) = camera.single() else {
                return;
            };
            let gun = match &rigs.gun {
                GunState::Ready(scene) => commands
                    .spawn((
                        ViewModelGun,
                        WorldAssetRoot(scene.clone()),
                        // Model space has the barrel along +X; aim it forward.
                        Transform::from_xyz(0.28, -0.24, -0.55)
                            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
                            .with_scale(Vec3::splat(tuning().gun_scale)),
                        DespawnOnExit(AppState::InGame),
                    ))
                    .id(),
                GunState::Failed => commands
                    .spawn((
                        ViewModelGun,
                        Mesh3d(meshes.add(Cuboid::new(0.1, 0.12, 0.45))),
                        MeshMaterial3d(materials.add(Color::srgb(0.15, 0.15, 0.18))),
                        Transform::from_xyz(0.28, -0.24, -0.55),
                        DespawnOnExit(AppState::InGame),
                    ))
                    .id(),
                GunState::Pending => return,
            };
            commands.entity(camera).add_child(gun);
            view.viewmodel = Some(gun);
        }
        (false, Some(gun)) => {
            // Duel's over: put the gun away.
            commands.entity(gun).despawn();
            view.viewmodel = None;
        }
        _ => {}
    }
}

/// Drive the cowboy animations from replicated state: movement speed picks
/// walk or idle, a falling bullet count plays the shot, falling lives play
/// the hit reaction. Misses (a bullet spent, no news) also fire the gun sound.
fn animate_duelists(
    mut commands: Commands,
    mut duelists: Query<(Entity, &Duelist, &mut RigClip)>,
    mut prev: Local<HashMap<Entity, Previous>>,
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    sfx: Option<Res<Sfx>>,
) {
    let dt = time.delta_secs().max(1e-3);
    // A hit already announces itself with a "shot" news line (and its sound),
    // so the shooter only adds their own bang on a miss.
    let any_hit = duelists
        .iter()
        .any(|(e, d, _)| prev.get(&e).is_some_and(|p| d.lives < p.lives));
    for (entity, d, mut clip) in &mut duelists {
        let now = Previous {
            pos: d.pos,
            bullets: d.bullets,
            lives: d.lives,
            busy: 0.0,
        };
        let last = prev.get(&entity).copied().unwrap_or(now);
        let moving = last.pos.distance_squared(d.pos) > (tuning().speed * dt * 0.25).powi(2);
        let fired = d.bullets < last.bullets;
        let hit = d.lives < last.lives;
        if fired && !any_hit && d.player == me.as_deref().map(|m| m.0).unwrap_or(u64::MAX) {
            // My own bang: everyone hears the shot.
            if let Some(sfx) = &sfx {
                play_sfx(&mut commands, &sfx.gunshot, 0.6);
            }
        }
        let (next, busy) = pick_clip(fired, hit, moving, (last.busy - dt).max(0.0));
        prev.insert(entity, Previous { busy, ..now });
        match next {
            // One-shots restart even when the same clip is already playing.
            Some(next) if next.once => *clip = next,
            Some(next) => {
                clip.set_if_neq(next);
            }
            None => {}
        }
    }
    prev.retain(|e, _| duelists.contains(*e));
}

/// What a duelist looked like last frame, and how long its one-shot has left.
#[derive(Clone, Copy)]
struct Previous {
    pos: Vec3,
    bullets: u8,
    lives: u8,
    busy: f32,
}

/// The clip to switch to (None: keep the one-shot playing) and how long a
/// one-shot still has to run. Shots take precedence, then hits, then moving.
fn pick_clip(fired: bool, hit: bool, moving: bool, busy: f32) -> (Option<RigClip>, f32) {
    if fired {
        (Some(RigClip::once("shoot").fade(0.05)), tuning().shoot_secs)
    } else if hit {
        (Some(RigClip::once("hit").fade(0.05)), tuning().hit_secs)
    } else if busy > 0.0 {
        (None, busy)
    } else if moving {
        (Some(RigClip::looping("walk").speed(WALK_SPEED)), 0.0)
    } else {
        (Some(RigClip::looping("idle")), 0.0)
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
    mut ctx: ResMut<crate::input::InputContexts>,
    keys: Res<ButtonInput<KeyCode>>,
    me: Option<Res<LocalPlayer>>,
    players: Query<&Player>,
    duelists: Query<(), With<Duelist>>,
    mut view: ResMut<DuelView>,
) {
    crate::input::switch(&mut ctx, "duel_invite", view.challenged_by.is_some());
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
        crate::hud::slot(crate::hud::HudSlot::BottomLeft),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        Crosshair,
        TextColor(crate::hud::LABEL),
        TextShadow::default(),
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
    mut last_run: Local<Option<f32>>,
) {
    // Throttled: count down by the time since this system last ran.
    let now = time.elapsed_secs();
    let dt = now - last_run.replace(now).unwrap_or(now);
    if let Some((_, left)) = &mut view.news {
        *left -= dt;
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
        lines.push(crate::input::fill(
            "{move} move, {look} look, {use} to shoot",
        ));
    } else if let Some(from) = view.challenged_by {
        lines.push(format!(
            "player {from} challenges you to a duel! {{duel_accept}} accept {{duel_decline}} decline"
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
    let text = crate::input::fill(&lines.join("\n"));
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

/// For the music: duelling (and on how many lives), and a victory when my
/// duel ends with me still standing.
fn music_signals(
    me: Option<Res<crate::player::LocalPlayer>>,
    duelists: Query<&Duelist>,
    mut lives_before: Local<Option<u8>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let mine = me.and_then(|me| duelists.iter().find(|d| d.player == me.0).copied());
    if mine.is_none() && lives_before.is_some_and(|l| l > 0) {
        signals.cue("victory");
    }
    *lives_before = mine.map(|d| d.lives);
    signals.flag("duel", mine.is_some());
    signals.set("duel_lives", mine.map_or(0.0, |d| f32::from(d.lives)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duelist(player: u64, pos: Vec3, look_at: Vec3) -> Duelist {
        Duelist {
            player,
            lives: tuning().lives,
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
        let here = arena_center();
        let near = here + Vec3::new(0.0, 0.0, -1.0);
        let far = here + Vec3::new(0.0, 0.0, -5.0);
        let mut puncher = duelist(0, here, near);
        puncher.bullets = 0;
        assert!(resolve_shot(&mut puncher, &duelist(1, near, here)));
        assert!(!resolve_shot(&mut puncher, &duelist(1, far, here)));
    }

    #[test]
    fn one_shots_play_out_before_walking_resumes() {
        let (clip, busy) = pick_clip(true, false, true, 0.0);
        assert_eq!(clip.map(|c| c.name), Some("shoot"));
        assert_eq!(busy, tuning().shoot_secs);
        assert!(pick_clip(false, false, true, 0.3).0.is_none());
        assert_eq!(
            pick_clip(false, true, false, 0.3).0.map(|c| c.name),
            Some("hit")
        );
        assert_eq!(
            pick_clip(false, false, true, 0.0).0.map(|c| c.name),
            Some("walk")
        );
        assert_eq!(
            pick_clip(false, false, false, 0.0).0.map(|c| c.name),
            Some("idle")
        );
    }

    /// A server with this feature's protocol, four players (0..3), and two
    /// open invites: (0, 1) and (2, 3). Returns the server and each remote
    /// player's client entity (index `id - 1`).
    fn server_with_two_invites() -> (App, Vec<Entity>) {
        use bevy::state::app::StatesPlugin;
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            StatesPlugin,
            RepliconPlugins.set(ServerPlugin::new(PostUpdate)),
        ));
        protocol(&mut app);
        app.finish();
        let owners: Vec<Entity> = (0..3).map(|_| app.world_mut().spawn_empty().id()).collect();
        for id in 0..4u64 {
            let owner = if id == 0 {
                None
            } else {
                Some(owners[(id - 1) as usize])
            };
            app.world_mut()
                .spawn((crate::player::Player { id }, crate::player::PlayerOwner(owner)));
        }
        app.world_mut().resource_mut::<DuelInvites>().open = vec![(0, 1), (2, 3)];
        app.world_mut()
            .resource_mut::<NextState<ServerState>>()
            .set_if_different(ServerState::Running);
        app.update();
        (app, owners)
    }

    fn duelist_count(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query::<&Duelist>().iter(world).count()
    }

    /// play:invites-window — `end_abandoned_duel` must not free the `active`
    /// guard between an answer and its deferred duelists' spawn: a second
    /// answer in that window would start an overlapping duel.
    #[test]
    fn accept_in_spawn_window_starts_one_duel() {
        let (mut app, owners) = server_with_two_invites();
        // Player 1 accepts (0, 1)'s challenge: the guard is claimed and the
        // duelists spawn (queued commands apply on flush).
        app.world_mut().trigger(FromClient {
            client_id: ClientId::Client(owners[0]),
            message: AnswerDuel {
                from: 0,
                accept: true,
            },
        });
        app.world_mut().flush();
        assert_eq!(duelist_count(&mut app), 2, "the first answer starts its duel");
        // Now stand in the accept→spawn window: the duelists are not visible
        // yet (their spawn is deferred), but the guard is already claimed.
        {
            let world = app.world_mut();
            let gone: Vec<Entity> = world
                .query::<(Entity, &Duelist)>()
                .iter(world)
                .map(|(e, _)| e)
                .collect();
            for e in gone {
                world.despawn(e);
            }
        }
        app.update();
        assert!(
            app.world().resource::<DuelInvites>().active,
            "the guard survives the window"
        );
        // Player 3 answers (2, 3) inside the window: no second duel.
        app.world_mut().trigger(FromClient {
            client_id: ClientId::Client(owners[2]),
            message: AnswerDuel {
                from: 2,
                accept: true,
            },
        });
        app.world_mut().flush();
        assert_eq!(
            duelist_count(&mut app),
            0,
            "the second answer starts no overlapping duel"
        );
    }
}
