//! The mute spell: the first voice-chat spell. From issue #146 by just meat
//! and cheese.
//!
//! Look at another player, say "mute" out loud and press Q. The game can't
//! understand words, so "saying it" means your mic has to be sending voice
//! when you press Q (if you have no working mic, or you muted yourself with
//! M, Q alone does it). The target is muted: their mic is forced off and they
//! get an RPG with infinite rockets. Q fires it where they look. A rocket only
//! hurts the player who muted them (it flies straight through everyone else)
//! and one hit kills: the caster is sent back to their spawn point. That
//! ends the mission: the RPG disappears and the target can talk again.
//!
//! The server owns all of it: [`Silenced`] sits on the muted player's
//! `Player` entity and [`Rocket`]s are replicated entities it moves. One
//! spell per caster at a time; you can't cast while you're muted yourself.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fps::{EYE, Walker, spawn_point};
use crate::features::interact::deny;
use crate::features::synth_audio::RATE;
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::voice_chat::{DeviceState, TalkMode, VoiceActivity, VoiceChatState, VoiceInput};
use crate::world::MainCamera;

/// How far away you can mute someone.
pub const CAST_RANGE: f32 = 30.0;
/// How far off your line of sight (metres) someone can be and still count as aimed at.
pub const AIM_SLACK: f32 = 1.2;
/// You must have been sending voice this recently when you press Q.
pub const SPOKE_WITHIN: f32 = 2.0;
/// Rocket speed, metres per second.
pub const ROCKET_SPEED: f32 = 30.0;
/// A rocket gives up (fizzles) after this long.
pub const ROCKET_LIFE: f32 = 3.0;
/// A rocket this close to the caster's chest kills them.
pub const HIT_RADIUS: f32 = 1.0;
/// Seconds between rockets.
pub const RELOAD: f32 = 1.0;
/// Chest height above the feet.
const CHEST: f32 = 1.0;

/// On a muted player's `Player` entity: player `by` muted them, so they hold
/// an RPG that only hurts `by`. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Silenced {
    pub by: u64,
}

/// A flying rocket fired by `shooter` at `victim`. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Rocket {
    pub pos: Vec3,
    pub vel: Vec3,
    pub shooter: u64,
    pub victim: u64,
}

/// Server only: seconds a rocket has left.
#[derive(Component)]
struct Fuse(f32);

/// Server only: seconds until this muted player can fire again.
#[derive(Component, Default)]
struct Reload(f32);

/// Client -> server: I said "mute" at player `target`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct CastMute {
    pub target: u64,
}

/// Client -> server: fire my RPG in direction `dir`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct FireRpg {
    pub dir: Vec3,
}

/// Server -> everyone: `caster` muted `target`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SpellCast {
    pub caster: u64,
    pub target: u64,
}

/// Server -> everyone: a rocket blew up at `at`; `killed` if it got its victim.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct RocketBoom {
    pub at: Vec3,
    pub shooter: u64,
    pub victim: u64,
    pub killed: bool,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Silenced>()
        .replicate::<Rocket>()
        .add_client_event::<CastMute>(Channel::Ordered)
        .add_client_event::<FireRpg>(Channel::Ordered)
        .add_server_event::<SpellCast>(Channel::Ordered)
        .add_server_event::<RocketBoom>(Channel::Ordered)
        .add_observer(on_cast)
        .add_observer(on_fire)
        .add_systems(
            Update,
            (fly_rockets, lift_orphaned_spells).run_if(in_state(ServerState::Running)),
        );
}

/// Server: the player who sent a client event.
fn sender<'a>(
    client: ClientId,
    players: impl Iterator<Item = (Entity, &'a Player, &'a PlayerOwner)>,
) -> Option<(Entity, u64)> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    players
        .into_iter()
        .find(|(_, _, o)| o.0 == owner)
        .map(|(e, p, _)| (e, p.id))
}

fn on_cast(
    ev: On<FromClient<CastMute>>,
    mut commands: Commands,
    players: Query<(Entity, &Player, &PlayerOwner, &Walker, Option<&Silenced>)>,
) {
    let Some((_, caster)) = sender(ev.client_id, players.iter().map(|(e, p, o, ..)| (e, p, o)))
    else {
        return;
    };
    let target = ev.target;
    let find = |id: u64| players.iter().find(|(_, p, ..)| p.id == id);
    let (Some(me), Some(them)) = (find(caster), find(target)) else {
        return;
    };
    if target == caster {
        return;
    }
    if me.4.is_some() {
        deny(
            &mut commands,
            ev.client_id,
            "you're muted: no spells for you",
        );
        return;
    }
    if players
        .iter()
        .any(|(.., s)| s.is_some_and(|s| s.by == caster))
    {
        deny(
            &mut commands,
            ev.client_id,
            "your mute spell is still working on someone",
        );
        return;
    }
    if them.4.is_some() {
        deny(&mut commands, ev.client_id, "they're already muted");
        return;
    }
    if me.3.pos.distance(them.3.pos) > CAST_RANGE {
        deny(&mut commands, ev.client_id, "too far away for the spell");
        return;
    }
    commands
        .entity(them.0)
        .insert((Silenced { by: caster }, Reload::default()));
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: SpellCast { caster, target },
    });
}

fn on_fire(
    ev: On<FromClient<FireRpg>>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut players: Query<(
        Entity,
        &Player,
        &PlayerOwner,
        &Walker,
        Option<&Silenced>,
        Option<&mut Reload>,
    )>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Some((entity, shooter)) =
        sender(ev.client_id, players.iter().map(|(e, p, o, ..)| (e, p, o)))
    else {
        return;
    };
    let Ok((_, _, _, walker, silenced, reload)) = players.get_mut(entity) else {
        return;
    };
    let (Some(silenced), Some(mut reload)) = (silenced, reload) else {
        return; // no RPG
    };
    let Some(dir) = ev.dir.try_normalize() else {
        return;
    };
    if reload.0 > 0.0 {
        return;
    }
    reload.0 = RELOAD;
    let eye = walker.pos + Vec3::Y * EYE;
    commands.spawn((
        Rocket {
            pos: eye + dir * 0.8,
            vel: dir * ROCKET_SPEED,
            shooter,
            victim: silenced.by,
        },
        Fuse(ROCKET_LIFE),
        Replicated,
    ));
}

/// Does a rocket at `rocket` hit someone whose feet are at `feet`?
pub fn rocket_hits(rocket: Vec3, feet: Vec3) -> bool {
    rocket.distance(feet + Vec3::Y * CHEST) <= HIT_RADIUS
}

#[allow(clippy::type_complexity)]
fn fly_rockets(
    time: Res<Time>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut rockets: Query<(Entity, &mut Rocket, &mut Fuse)>,
    mut reloads: Query<&mut Reload>,
    mut walkers: Query<(Entity, &Player, &mut Walker, Option<&mut crate::kcc::Kcc>)>,
) {
    let dt = time.delta_secs().min(0.1);
    for mut r in &mut reloads {
        if r.0 > 0.0 {
            r.0 -= dt;
        }
    }
    if super::stillness::is_frozen(&still) {
        return; // rockets hang in the air too
    }
    for (entity, mut rocket, mut fuse) in &mut rockets {
        let from = rocket.pos;
        let to = from + rocket.vel * dt;
        fuse.0 -= dt;
        // Check along the whole step so a fast rocket can't skip past.
        let victim = walkers.iter_mut().find(|(_, p, ..)| p.id == rocket.victim);
        let mut killed = false;
        if let Some((_, _, mut walker, kcc)) = victim {
            let chest = walker.pos + Vec3::Y * CHEST;
            let seg = to - from;
            let t = if seg.length_squared() > 0.0 {
                ((chest - from).dot(seg) / seg.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if rocket_hits(from + seg * t, walker.pos) {
                killed = true;
                walker.pos = spawn_point(rocket.victim);
                if let Some(mut kcc) = kcc {
                    kcc.velocity = Vec3::ZERO;
                }
            }
        }
        let ground = crate::terrain::height_at(to.x, to.z);
        if killed || to.y <= ground || fuse.0 <= 0.0 {
            let at = if killed {
                from
            } else {
                to.with_y(to.y.max(ground))
            };
            commands.entity(entity).despawn();
            commands.server_trigger(ToClients {
                targets: SendTargets::All,
                message: RocketBoom {
                    at,
                    shooter: rocket.shooter,
                    victim: rocket.victim,
                    killed,
                },
            });
            if killed {
                // Mission complete: the RPG goes away and they can talk again.
                if let Some((shooter, ..)) =
                    walkers.iter().find(|(_, p, ..)| p.id == rocket.shooter)
                {
                    commands.entity(shooter).remove::<(Silenced, Reload)>();
                }
            }
        } else {
            rocket.pos = to;
        }
    }
}

/// Server: a muted player whose caster left is free again.
fn lift_orphaned_spells(
    mut commands: Commands,
    muted: Query<(Entity, &Silenced)>,
    players: Query<&Player>,
) {
    for (entity, s) in &muted {
        if !players.iter().any(|p| p.id == s.by) {
            commands.entity(entity).remove::<(Silenced, Reload)>();
        }
    }
}

// ---------------------------------------------------------------- client side

/// Local: seconds since this peer last sent voice.
#[derive(Resource)]
struct LastSpoke(f32);

impl Default for LastSpoke {
    fn default() -> Self {
        Self(f32::INFINITY)
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<LastSpoke>()
        .add_observer(announce_cast)
        .add_observer(boom)
        .add_systems(OnEnter(AppState::InGame), spawn_hud)
        .add_systems(
            Update,
            gag_my_mic
                .after(crate::voice::drive_voice)
                .before(crate::voice_chat::VoiceChatSystems::Devices),
        )
        .add_systems(
            Update,
            (
                cast_or_fire,
                dress_rockets,
                carry_rpgs,
                fade_flashes,
                update_hud.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Is player `id` muted by the spell?
fn is_silenced(id: u64, muted: &Query<(&Player, &Silenced)>) -> Option<Silenced> {
    muted.iter().find(|(p, _)| p.id == id).map(|(_, s)| *s)
}

/// Muted by the spell: the mic stays off whatever M says.
fn gag_my_mic(
    me: Option<Res<LocalPlayer>>,
    muted: Query<(&Player, &Silenced)>,
    mut input: ResMut<VoiceInput>,
) {
    let Some(me) = me else { return };
    if is_silenced(me.0, &muted).is_some() {
        input.mode = TalkMode::PushToTalk;
        input.talk_held = false;
    }
}

/// Of `players` (id, chest), the one the ray from `eye` along `dir` points at:
/// in front, within [`CAST_RANGE`] and [`AIM_SLACK`] of the line, nearest first.
pub fn aimed_at(eye: Vec3, dir: Vec3, players: &[(u64, Vec3)]) -> Option<u64> {
    let dir = dir.try_normalize()?;
    players
        .iter()
        .filter_map(|(id, chest)| {
            let along = (*chest - eye).dot(dir);
            if along <= 0.0 || along > CAST_RANGE {
                return None;
            }
            let off = (*chest - (eye + dir * along)).length();
            (off <= AIM_SLACK).then_some((*id, along))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Can this peer's voice count as "saying" the spell? With a live, unmuted
/// mic you have to have been talking; otherwise pressing Q is enough.
pub fn voice_says_it(mic_live: bool, self_muted: bool, since_spoke: f32) -> bool {
    !mic_live || self_muted || since_spoke <= SPOKE_WITHIN
}

#[allow(clippy::too_many_arguments)]
fn cast_or_fire(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    activity: Option<Res<VoiceActivity>>,
    voice: Option<Res<VoiceChatState>>,
    self_muted: Option<Res<crate::voice::VoiceMuted>>,
    mut spoke: ResMut<LastSpoke>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    walkers: Query<(&Player, &Walker)>,
    muted: Query<(&Player, &Silenced)>,
    mut toast: ResMut<Toast>,
) {
    if activity.is_some_and(|a| a.transmitting) {
        spoke.0 = 0.0;
    } else {
        spoke.0 += time.delta_secs();
    }
    if !keys.just_pressed(KeyCode::KeyQ) {
        return;
    }
    let Some(me) = me else { return };
    let Ok(cam) = camera.single() else { return };
    let dir = cam.forward().as_vec3();
    if is_silenced(me.0, &muted).is_some() {
        commands.client_trigger(FireRpg { dir });
        return;
    }
    let mic_live = voice.is_some_and(|v| matches!(v.mic, DeviceState::Live(_)));
    let self_muted = self_muted.is_some_and(|m| m.0);
    if !voice_says_it(mic_live, self_muted, spoke.0) {
        toast.show("say \"mute\" out loud while you press Q");
        return;
    }
    let others: Vec<(u64, Vec3)> = walkers
        .iter()
        .filter(|(p, _)| p.id != me.0)
        .map(|(p, w)| (p.id, w.pos + Vec3::Y * CHEST))
        .collect();
    let Some(target) = aimed_at(cam.translation(), dir, &others) else {
        toast.show("look at a player to mute them");
        return;
    };
    commands.client_trigger(CastMute { target });
}

fn who(id: u64, me: Option<u64>) -> String {
    if me == Some(id) {
        "you".to_string()
    } else {
        format!("player {id}")
    }
}

fn announce_cast(
    ev: On<SpellCast>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<Toast>>,
    state: Option<Res<State<AppState>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let Some(mut toast) = toast else { return };
    let me = me.map(|m| m.0);
    let text = if me == Some(ev.target) {
        format!(
            "player {} said MUTE: you're muted and hold an RPG. Q fires. Kill them to talk again.",
            ev.caster
        )
    } else if me == Some(ev.caster) {
        format!(
            "you muted player {}. They have an RPG that only hurts you. Run.",
            ev.target
        )
    } else {
        format!("player {} muted player {}", ev.caster, ev.target)
    };
    toast.show(text);
}

/// Local: a rocket's look, given once it arrives.
#[derive(Component)]
struct Dressed;

fn dress_rockets(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut new: Query<(Entity, &Rocket), Without<Dressed>>,
    mut flying: Query<(&Rocket, &mut Transform), With<Dressed>>,
) {
    for (rocket, mut tf) in &mut flying {
        tf.translation = rocket.pos;
        if let Some(dir) = rocket.vel.try_normalize() {
            tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        }
    }
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for (entity, rocket) in &mut new {
        let rotation = rocket
            .vel
            .try_normalize()
            .map_or(Quat::IDENTITY, |d| Quat::from_rotation_arc(Vec3::Y, d));
        commands.entity(entity).insert((
            Dressed,
            Mesh3d(meshes.add(Capsule3d::new(0.09, 0.45))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.35, 0.4, 0.3),
                emissive: LinearRgba::rgb(4.0, 1.4, 0.2),
                ..default()
            })),
            Transform::from_translation(rocket.pos).with_rotation(rotation),
            Visibility::default(),
        ));
    }
}

/// Local: the RPG tube on a muted player's shoulder.
#[derive(Component)]
struct RpgTube(u64);

fn carry_rpgs(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    muted: Query<(&Player, &Walker), With<Silenced>>,
    mut tubes: Query<(Entity, &RpgTube, &mut Transform)>,
) {
    let me = me.map(|m| m.0);
    for (entity, tube, mut tf) in &mut tubes {
        let Some((_, w)) = muted.iter().find(|(p, _)| p.id == tube.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let yaw = Quat::from_rotation_y(w.yaw);
        tf.translation = w.pos + yaw * Vec3::new(0.3, 1.45, 0.0);
        tf.rotation = yaw * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    }
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for (p, w) in &muted {
        // Your own is in your hands where you can't see it.
        if Some(p.id) == me || tubes.iter().any(|(_, t, _)| t.0 == p.id) {
            continue;
        }
        let yaw = Quat::from_rotation_y(w.yaw);
        commands.spawn((
            RpgTube(p.id),
            Mesh3d(meshes.add(Cylinder::new(0.1, 1.1))),
            MeshMaterial3d(materials.add(Color::srgb(0.25, 0.32, 0.2))),
            Transform::from_translation(w.pos + yaw * Vec3::new(0.3, 1.45, 0.0))
                .with_rotation(yaw * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Local: an explosion flash that grows and goes.
#[derive(Component)]
struct Flash(f32);

#[allow(clippy::too_many_arguments)]
fn boom(
    ev: On<RocketBoom>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<Toast>>,
    state: Option<Res<State<AppState>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    audio: Option<Res<Assets<bevy::audio::AudioSource>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let me = me.map(|m| m.0);
    if ev.killed
        && let Some(mut toast) = toast
    {
        let text = if me == Some(ev.victim) {
            format!(
                "player {}'s rocket got you. Back to the start. They can talk again.",
                ev.shooter
            )
        } else if me == Some(ev.shooter) {
            format!(
                "you blew up player {}. Mission done: the RPG is gone and your mic is back.",
                ev.victim
            )
        } else {
            format!(
                "{} blew up {} with an RPG",
                who(ev.shooter, me),
                who(ev.victim, me)
            )
        };
        toast.show(text);
    }
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        commands.spawn((
            Flash(0.0),
            Mesh3d(meshes.add(Sphere::new(1.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.6, 0.2, 0.8),
                emissive: LinearRgba::rgb(8.0, 3.0, 0.5),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(ev.at).with_scale(Vec3::splat(0.3)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    if audio.is_some() {
        let seed = (ev.at.x * 13.0 + ev.at.z * 7.0) as i32 as u32;
        crate::spatial::play_wav_at(
            &mut commands,
            ev.at,
            1.0,
            crate::spatial::Carry::LOUD,
            move || wav_bytes(&boom_samples(seed, RATE), RATE),
        );
    }
}

fn fade_flashes(
    time: Res<Time>,
    mut commands: Commands,
    mut flashes: Query<(Entity, &mut Flash, &mut Transform)>,
) {
    for (entity, mut flash, mut tf) in &mut flashes {
        flash.0 += time.delta_secs();
        if flash.0 > 0.4 {
            commands.entity(entity).despawn();
        } else {
            tf.scale = Vec3::splat(0.3 + flash.0 * 6.0);
        }
    }
}

/// An explosion: a noise burst over a dropping thump, about a second long.
pub fn boom_samples(seed: u32, rate: u32) -> Vec<f32> {
    let n = rate as usize;
    let mut x = seed | 1;
    let mut low = 0.0f32;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let noise = (x as f32 / u32::MAX as f32) * 2.0 - 1.0;
            low += (noise - low) * 0.08;
            phase += std::f32::consts::TAU * (110.0 * (-t * 6.0).exp() + 35.0) / rate as f32;
            let attack = (t / 0.004).min(1.0);
            let s = low * 2.2 * (-t * 4.0).exp() + phase.sin() * 0.8 * (-t * 5.0).exp();
            (s * attack).clamp(-1.0, 1.0)
        })
        .collect()
}

#[derive(Component)]
struct MuteHud;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::PINK),
        MuteHud,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_hud(
    me: Option<Res<LocalPlayer>>,
    muted: Query<(&Player, &Silenced)>,
    mut hud: Query<&mut Text, With<MuteHud>>,
) {
    let mut lines = Vec::new();
    if let Some(me) = me {
        if let Some(s) = is_silenced(me.0, &muted) {
            lines.push(format!(
                "MUTED. RPG (infinite): Q fires. Only player {} can be hurt.",
                s.by
            ));
        }
        for (p, s) in &muted {
            if s.by == me.0 {
                lines.push(format!("player {} has an RPG for you", p.id));
            }
        }
    }
    let want = lines.join("\n");
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn you_aim_at_the_nearest_player_in_your_sights() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let ps = [
            (1, Vec3::new(0.0, 1.0, -10.0)),
            (2, Vec3::new(0.3, 1.0, -5.0)),
            (3, Vec3::new(5.0, 1.0, -5.0)),
            (4, Vec3::new(0.0, 1.0, 10.0)),
        ];
        assert_eq!(aimed_at(eye, Vec3::NEG_Z, &ps), Some(2));
        assert_eq!(
            aimed_at(eye, Vec3::Z, &ps),
            Some(4),
            "behind you is another way"
        );
        assert_eq!(aimed_at(eye, Vec3::X, &ps), None);
        let far = [(1, Vec3::new(0.0, 1.0, -(CAST_RANGE + 5.0)))];
        assert_eq!(aimed_at(eye, Vec3::NEG_Z, &far), None);
    }

    #[test]
    fn a_live_mic_has_to_say_it() {
        assert!(!voice_says_it(true, false, 10.0));
        assert!(voice_says_it(true, false, 0.5));
        assert!(voice_says_it(false, false, 10.0), "no mic: Q is enough");
        assert!(voice_says_it(true, true, 10.0), "self-muted: Q is enough");
    }

    #[test]
    fn rockets_hit_the_chest() {
        let feet = Vec3::new(3.0, 0.0, 3.0);
        assert!(rocket_hits(feet + Vec3::Y, feet));
        assert!(!rocket_hits(feet + Vec3::new(3.0, 1.0, 0.0), feet));
    }

    #[test]
    fn the_boom_is_loud_and_finite() {
        let s = boom_samples(7, 44_100);
        assert_eq!(s.len(), 44_100);
        assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        assert!(s.iter().take(4410).any(|x| x.abs() > 0.3));
    }
}
