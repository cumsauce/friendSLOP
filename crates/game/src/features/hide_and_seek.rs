//! Hide and seek with the players. From issue #61 by doomy.
//!
//! Anyone presses K to start (needs at least two players). The server picks a
//! random Seeker; everyone else is a Hider. Hiders get [`HIDE_SECS`] to hide
//! while the seeker is frozen and blinded, then the seeker has [`SEEK_SECS`] to
//! find them. A seeker tags a hider by clicking them within reach; a tagged
//! hider joins the seekers. The round ends when nobody is left hiding or time
//! runs out, results show for [`RESULTS_SECS`], and the next round starts with
//! a new seeker. It stops once fewer than two players are left. The landlord
//! objective keeps running alongside; this is an extra mode, not a replacement.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{HiddenLabels, WalkSet, Walker};
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

pub const HIDE_SECS: f32 = 30.0;
pub const SEEK_SECS: f32 = 180.0;
pub const RESULTS_SECS: f32 = 10.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    /// Not playing; press K to start.
    #[default]
    Idle,
    Hiding,
    Seeking,
    Results,
}

/// Server -> everyone: the hide-and-seek round. One entity.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct HideSeek {
    pub phase: Phase,
    /// Whole seconds left in the phase (only changes once a second).
    pub secs_left: u16,
    pub round: u32,
    /// Results: did the hiders survive, and how many were found.
    pub hiders_won: bool,
    pub found: u16,
}

/// Server -> everyone, on a `Player` during a round.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The one picked to seek this round.
    Seeker,
    Hider,
    /// A hider who was found; now seeks too.
    Found,
}

impl Role {
    pub fn seeks(self) -> bool {
        matches!(self, Role::Seeker | Role::Found)
    }
}

/// Server only: precise time left in the phase.
#[derive(Resource, Default, Debug)]
pub struct HideSeekClock(pub f32);

/// Server only: where a seeker is held while the hiders hide.
#[derive(Component, Debug)]
struct HoldAt(Vec3);

/// Client -> server: start hide and seek.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct StartHideSeek;

/// Client -> server: I (a seeker) tagged player `id`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TagHider {
    pub id: u64,
}

/// Server -> everyone: something happened (shown as a toast).
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct HideSeekNews {
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.replicate::<HideSeek>()
        .replicate::<Role>()
        .add_client_event::<StartHideSeek>(Channel::Ordered)
        .add_client_event::<TagHider>(Channel::Ordered)
        .add_server_event::<HideSeekNews>(Channel::Ordered)
        .init_resource::<HideSeekClock>()
        .add_observer(on_start)
        .add_observer(on_tag)
        .add_observer(reset_hide_seek)
        .add_systems(OnEnter(ServerState::Running), spawn_state)
        .add_systems(
            Update,
            (tick, hold_seekers.after(WalkSet)).run_if(in_state(ServerState::Running)),
        );
}

/// A new round of the campaign: stop any game, drop roles and holds.
fn reset_hide_seek(
    _reset: On<crate::features::objective::RoundReset>,
    mut commands: Commands,
    mut state: Query<&mut HideSeek>,
    mut clock: ResMut<HideSeekClock>,
    roled: Query<Entity, Or<(With<Role>, With<HoldAt>)>>,
) {
    for mut s in &mut state {
        *s = HideSeek {
            round: s.round,
            ..default()
        };
    }
    clock.0 = 0.0;
    for e in &roled {
        commands.entity(e).remove::<(Role, HoldAt)>();
    }
}

fn spawn_state(mut commands: Commands) {
    commands.spawn((HideSeek::default(), Replicated));
}

fn news(commands: &mut Commands, text: impl Into<String>) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: HideSeekNews { text: text.into() },
    });
}

/// Which of `n` players seeks in `round` (spread out, changes every round).
pub fn seeker_index(round: u32, salt: u64, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut x = (round as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x % n as u64) as usize
}

/// Server: begin a new round. Returns false if there aren't enough players.
fn begin_round(
    commands: &mut Commands,
    state: &mut HideSeek,
    clock: &mut HideSeekClock,
    players: &[(Entity, u64, Vec3)],
) -> bool {
    if players.len() < 2 {
        return false;
    }
    let mut sorted = players.to_vec();
    sorted.sort_by_key(|p| p.1);
    let salt = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let pick = seeker_index(state.round + 1, salt, sorted.len());
    for (i, (e, _, pos)) in sorted.iter().enumerate() {
        if i == pick {
            commands.entity(*e).insert((Role::Seeker, HoldAt(*pos)));
        } else {
            commands.entity(*e).insert(Role::Hider).remove::<HoldAt>();
        }
    }
    *state = HideSeek {
        phase: Phase::Hiding,
        secs_left: HIDE_SECS as u16,
        round: state.round + 1,
        ..default()
    };
    clock.0 = HIDE_SECS;
    news(
        commands,
        format!(
            "hide and seek: player {} seeks! hide in {}s",
            sorted[pick].1, HIDE_SECS as u16
        ),
    );
    true
}

fn on_start(
    ev: On<FromClient<StartHideSeek>>,
    mut commands: Commands,
    mut state: Query<&mut HideSeek>,
    mut clock: ResMut<HideSeekClock>,
    players: Query<(Entity, &Player, &Walker)>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Ok(mut state) = state.single_mut() else {
        return;
    };
    if state.phase != Phase::Idle {
        deny(&mut commands, ev.client_id, "hide and seek is already on");
        return;
    }
    let list: Vec<_> = players.iter().map(|(e, p, w)| (e, p.id, w.pos)).collect();
    if !begin_round(&mut commands, &mut state, &mut clock, &list) {
        deny(
            &mut commands,
            ev.client_id,
            "hide and seek needs at least 2 players",
        );
    }
}

fn on_tag(
    ev: On<FromClient<TagHider>>,
    mut commands: Commands,
    state: Query<&HideSeek>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut players: Query<(&Player, &PlayerOwner, &Walker, &mut Role)>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Ok(state) = state.single() else { return };
    if state.phase != Phase::Seeking {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some(me) = players
        .iter()
        .find(|(_, o, _, _)| o.0 == owner)
        .map(|(p, _, _, r)| (p.id, *r))
    else {
        return;
    };
    if !me.1.seeks() {
        return;
    }
    let Some((_, _, target_walker, _)) = players
        .iter()
        .find(|(p, _, _, r)| p.id == ev.id && **r == Role::Hider)
    else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, target_walker.pos) {
        return;
    }
    for (p, _, _, mut role) in &mut players {
        if p.id == ev.id {
            *role = Role::Found;
        }
    }
    news(
        &mut commands,
        format!("player {} found player {}!", me.0, ev.id),
    );
}

fn tick(
    time: Res<Time>,
    mut commands: Commands,
    mut clock: ResMut<HideSeekClock>,
    mut state: Query<&mut HideSeek>,
    players: Query<(Entity, &Player, &Walker, Option<&Role>)>,
) {
    let Ok(mut state) = state.single_mut() else {
        return;
    };
    if state.phase == Phase::Idle {
        return;
    }
    clock.0 = (clock.0 - time.delta_secs()).max(0.0);
    let secs = clock.0.ceil() as u16;
    if state.secs_left != secs {
        state.secs_left = secs;
    }
    let hiders = players.iter().filter(|p| p.3 == Some(&Role::Hider)).count();
    let seekers = players
        .iter()
        .filter(|p| p.3.is_some_and(|r| r.seeks()))
        .count();
    let found = players.iter().filter(|p| p.3 == Some(&Role::Found)).count() as u16;
    match state.phase {
        Phase::Hiding | Phase::Seeking if hiders == 0 || seekers == 0 => {
            finish(&mut commands, &mut state, &mut clock, hiders > 0, found);
        }
        Phase::Hiding if clock.0 <= 0.0 => {
            for (e, ..) in &players {
                commands.entity(e).remove::<HoldAt>();
            }
            state.phase = Phase::Seeking;
            clock.0 = SEEK_SECS;
            state.secs_left = SEEK_SECS as u16;
            news(&mut commands, "ready or not, here they come!");
        }
        Phase::Seeking if clock.0 <= 0.0 => {
            finish(&mut commands, &mut state, &mut clock, true, found);
        }
        Phase::Results if clock.0 <= 0.0 => {
            let list: Vec<_> = players
                .iter()
                .map(|(e, p, w, _)| (e, p.id, w.pos))
                .collect();
            if !begin_round(&mut commands, &mut state, &mut clock, &list) {
                for (e, ..) in &players {
                    commands.entity(e).remove::<(Role, HoldAt)>();
                }
                state.phase = Phase::Idle;
                news(&mut commands, "hide and seek stopped: not enough players");
            }
        }
        _ => {}
    }
}

fn finish(
    commands: &mut Commands,
    state: &mut HideSeek,
    clock: &mut HideSeekClock,
    hiders_won: bool,
    found: u16,
) {
    state.phase = Phase::Results;
    state.hiders_won = hiders_won;
    state.found = found;
    state.secs_left = RESULTS_SECS as u16;
    clock.0 = RESULTS_SECS;
    news(
        commands,
        if hiders_won {
            "time's up: the hiders win!"
        } else {
            "everyone was found: the seekers win!"
        },
    );
}

/// Server: seekers can't move while the hiders hide.
fn hold_seekers(state: Query<&HideSeek>, mut seekers: Query<(&mut Walker, &HoldAt)>) {
    if !state.single().is_ok_and(|s| s.phase == Phase::Hiding) {
        return;
    }
    for (mut w, hold) in &mut seekers {
        if w.pos != hold.0 {
            w.pos = hold.0;
        }
    }
}

/// The objective-line text for the current state, if any.
pub fn status_line(s: &HideSeek, mine: Option<Role>) -> String {
    let role = match mine {
        Some(Role::Seeker) => "you seek",
        Some(Role::Found) => "found: now you seek",
        Some(Role::Hider) => "you hide",
        None => "sitting this round out",
    };
    match s.phase {
        Phase::Idle => String::new(),
        Phase::Hiding => format!("Hide & seek {} | hide! {}s | {role}", s.round, s.secs_left),
        Phase::Seeking => format!("Hide & seek {} | seek! {}s | {role}", s.round, s.secs_left),
        Phase::Results => format!(
            "Hide & seek {} | {} ({} found) | next round in {}s",
            s.round,
            if s.hiders_won {
                "hiders win"
            } else {
                "seekers win"
            },
            s.found,
            s.secs_left
        ),
    }
}

// ---------------------------------------------------------------- local side

pub fn plugin(app: &mut App) {
    app.add_observer(show_news)
        .add_observer(click_hider)
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            (start_key, show_status, tag_targets, hide_labels, heartbeat)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::InGame), clear_labels);
}

/// Hiders hear a heartbeat when a seeker is this close (metres).
pub const HEARTBEAT_RANGE: f32 = 6.0;

/// Does player `theirs` keep their floating name label hidden from me during a
/// round? Mine always shows; during Hiding/Seeking seekers see no labels at
/// all, and nobody sees a hider's.
pub fn label_hidden(phase: Phase, is_me: bool, mine: Option<Role>, theirs: Option<Role>) -> bool {
    if is_me || !matches!(phase, Phase::Hiding | Phase::Seeking) {
        return false;
    }
    mine.is_some_and(Role::seeks) || theirs == Some(Role::Hider)
}

/// Seconds between heartbeats for a hider with the nearest seeker `dist` away,
/// or `None` if no seeker is close enough. Faster the closer they are.
pub fn heartbeat_interval(dist: f32) -> Option<f32> {
    (dist.is_finite() && dist <= HEARTBEAT_RANGE)
        .then(|| 0.45 + 0.75 * (dist / HEARTBEAT_RANGE).clamp(0.0, 1.0))
}

fn hide_labels(
    me: Option<Res<LocalPlayer>>,
    state: Query<&HideSeek>,
    players: Query<(&Player, Option<&Role>)>,
    hidden: Option<ResMut<HiddenLabels>>,
) {
    let Some(mut hidden) = hidden else { return };
    let phase = state.single().map(|s| s.phase).unwrap_or_default();
    let mine = my_role(&me, &players);
    let me = me.as_deref().map(|m| m.0);
    let want: std::collections::HashSet<u64> = players
        .iter()
        .filter(|(p, r)| label_hidden(phase, me == Some(p.id), mine, r.copied()))
        .map(|(p, _)| p.id)
        .collect();
    if hidden.0 != want {
        hidden.0 = want;
    }
}

fn clear_labels(hidden: Option<ResMut<HiddenLabels>>) {
    if let Some(mut hidden) = hidden {
        hidden.0.clear();
    }
}

/// A tiny generated "lub-dub" (16-bit mono WAV), so no asset file is needed.
fn heartbeat_wav() -> Vec<u8> {
    const RATE: u32 = 22_050;
    let n = (RATE as f32 * 0.4) as usize;
    let thump = |t: f32, at: f32, amp: f32| {
        let x = t - at;
        if x < 0.0 {
            return 0.0;
        }
        amp * (-x * 28.0).exp() * (std::f32::consts::TAU * 55.0 * x).sin()
    };
    let samples: Vec<i16> = (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let v = thump(t, 0.0, 0.9) + thump(t, 0.16, 0.6);
            (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        })
        .collect();
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Local: a hider hears their heart thump when a seeker is near.
fn heartbeat(
    mut commands: Commands,
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    state: Query<&HideSeek>,
    players: Query<(&Player, &Walker, Option<&Role>)>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    mut sound: Local<Option<Handle<AudioSource>>>,
    mut wait: Local<f32>,
) {
    *wait -= time.delta_secs();
    let Ok(s) = state.single() else { return };
    if s.phase != Phase::Seeking {
        return;
    }
    let Some(me) = me.as_deref() else { return };
    let Some((_, here, _)) = players
        .iter()
        .find(|(p, _, r)| p.id == me.0 && *r == Some(&Role::Hider))
    else {
        return;
    };
    let nearest = players
        .iter()
        .filter(|(_, _, r)| r.is_some_and(|r| r.seeks()))
        .map(|(_, w, _)| w.pos.distance(here.pos))
        .fold(f32::INFINITY, f32::min);
    let Some(every) = heartbeat_interval(nearest) else {
        return;
    };
    if *wait > 0.0 {
        return;
    }
    *wait = every;
    let Some(mut audio) = audio else { return };
    let handle = sound
        .get_or_insert_with(|| {
            audio.add(AudioSource {
                bytes: heartbeat_wav().into(),
            })
        })
        .clone();
    let volume = 0.35 + 0.5 * (1.0 - nearest / HEARTBEAT_RANGE);
    crate::assets::play_sfx(&mut commands, &handle, volume);
}

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct Blindfold;

/// Local: this hider can be tagged by me right now.
#[derive(Component)]
struct TagTarget;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        StatusText,
        crate::hud::slot(crate::hud::HudSlot::TopCenter),
        DespawnOnExit(AppState::InGame),
    ));
    commands
        .spawn((
            Blindfold,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.13, 0.07, 0.15)),
            GlobalZIndex(5),
            Visibility::Hidden,
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("You're the seeker. Eyes shut, count..."),
                TextFont::from_font_size(26.0),
                TextColor(crate::hud::LABEL),
                crate::assets::DisplayText,
            ));
        });
}

fn start_key(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::KeyK) {
        commands.client_trigger(StartHideSeek);
    }
}

fn show_news(ev: On<HideSeekNews>, toast: Option<ResMut<Toast>>) {
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

fn my_role(
    me: &Option<Res<LocalPlayer>>,
    players: &Query<(&Player, Option<&Role>)>,
) -> Option<Role> {
    let me = me.as_deref()?;
    players
        .iter()
        .find(|(p, _)| p.id == me.0)
        .and_then(|(_, r)| r.copied())
}

fn show_status(
    me: Option<Res<LocalPlayer>>,
    state: Query<&HideSeek>,
    players: Query<(&Player, Option<&Role>)>,
    mut line: Query<&mut Text, With<StatusText>>,
    mut blind: Query<&mut Visibility, With<Blindfold>>,
) {
    let Ok(s) = state.single() else { return };
    let mine = my_role(&me, &players);
    let text = status_line(s, mine);
    for mut t in &mut line {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
    let want = if s.phase == Phase::Hiding && mine == Some(Role::Seeker) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut blind {
        v.set_if_neq(want);
    }
}

/// While I'm seeking, hiders become clickable (and only then, so they never
/// steal clicks from anything else).
fn tag_targets(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    state: Query<&HideSeek>,
    all: Query<(&Player, Option<&Role>)>,
    players: Query<(Entity, &Player, Option<&Role>, Has<TagTarget>)>,
) {
    let seeking = state.single().is_ok_and(|s| s.phase == Phase::Seeking)
        && my_role(&me, &all).is_some_and(|r| r.seeks());
    for (e, p, role, tagged) in &players {
        let want =
            seeking && role == Some(&Role::Hider) && me.as_deref().is_some_and(|m| m.0 != p.id);
        if want && !tagged {
            commands
                .entity(e)
                .insert((TagTarget, Clickable::at_height(1.0).radius(60.0)));
        } else if !want && tagged {
            commands.entity(e).remove::<(TagTarget, Clickable)>();
        }
    }
}

fn click_hider(ev: On<Clicked>, mut commands: Commands, targets: Query<&Player, With<TagTarget>>) {
    if let Ok(p) = targets.get(ev.entity) {
        commands.client_trigger(TagHider { id: p.id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeker_index_stays_in_range_and_varies() {
        let picks: Vec<usize> = (0..20).map(|r| seeker_index(r, 7, 4)).collect();
        assert!(picks.iter().all(|&i| i < 4));
        assert!(picks.iter().any(|&i| i != picks[0]));
    }
}
