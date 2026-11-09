//! The fart mechanic. From issue #18 by Kori.exe.
//!
//! Every player has replicated `Gas` that slowly builds up. Press F to let it
//! out where your bottom is: the spot on the floor under the mouse cursor
//! (players have no bodies outside duels, so, like the mushroom, you point),
//! or your duel body if you're duelling. The server checks whether any NPC
//! (the bear, a crow, Dave, or one of the commune's villagers, who are NPCs
//! too since #61 and are the ones who punish you for it, #123/#135) is within
//! `SHAME_RADIUS` of that spot. If one
//! is, you are shamed: for `SHAME_SECS` your screen blurs and a scary,
//! distorted, procedurally generated chiptune plays. Everyone is told who
//! farted and whether they got caught.

use bevy::audio::AudioSource;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::post_process::dof::DepthOfField;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::{Bear, bear_position, wav_bytes};
use crate::features::crows::{Crow, crow_position};
use crate::features::duel::Duelist;
use crate::features::synth_audio::play_synth;
use crate::features::villagers::Villager;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::world::MainCamera;

/// How much gas a player is holding in, 0..=100 (percent).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gas(pub u8);

/// Client -> server: "I'm farting, and my bottom is at `at` (x, z)."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Fart {
    pub at: Vec2,
}

/// Server -> everyone: player `player` farted at `at`; `shamed` if an NPC saw.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Farted {
    pub player: u64,
    pub at: Vec2,
    pub shamed: bool,
    /// How full they were, 0..=100.
    pub pressure: u8,
}

/// Seconds for gas to go from empty to full.
pub const GAS_FILL_SECS: f32 = 40.0;
/// You need at least this much gas to fart.
pub const MIN_GAS: u8 = 20;
/// NPCs closer than this (metres) to your bottom will see you fart.
pub const SHAME_RADIUS: f32 = 4.0;
/// How long the shame lasts.
pub const SHAME_SECS: f32 = 20.0;

pub fn protocol(app: &mut App) {
    app.replicate::<Gas>()
        .add_client_event::<Fart>(Channel::Ordered)
        .add_server_event::<Farted>(Channel::Ordered)
        .add_observer(give_gas)
        .add_observer(let_rip)
        .add_systems(
            Update,
            build_pressure.run_if(in_state(ServerState::Running)),
        );
}

/// Server only: fractional gas not yet added to `Gas`.
#[derive(Component, Default)]
struct GasBuildup(f32);

fn give_gas(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() != ServerState::Running {
        return;
    }
    commands
        .entity(add.entity)
        .insert((Gas::default(), GasBuildup::default()));
}

/// Server: gas builds up steadily, in whole percent so replication stays quiet.
/// Nothing builds while the world stands still.
fn build_pressure(
    time: Res<Time>,
    still: Query<&crate::features::stillness::Stillness>,
    mut players: Query<(&mut Gas, &mut GasBuildup)>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return;
    }
    let per_sec = 100.0 / GAS_FILL_SECS;
    for (mut gas, mut buildup) in &mut players {
        if gas.0 >= 100 {
            buildup.0 = 0.0;
            continue;
        }
        buildup.0 += time.delta_secs() * per_sec;
        let whole = buildup.0.floor();
        if whole >= 1.0 {
            buildup.0 -= whole;
            gas.0 = (gas.0 as f32 + whole).min(100.0) as u8;
        }
    }
}

/// Where every NPC is at time `t` (x, z on the floor): Dave, the bear, the
/// crows and the villagers.
pub fn npc_spots<'a>(
    t: f32,
    bears: impl IntoIterator<Item = &'a Bear>,
    crows: impl IntoIterator<Item = &'a Crow>,
    villagers: impl IntoIterator<Item = &'a Villager>,
) -> Vec<Vec2> {
    let mut spots = vec![super::dave_commune::DAVE_AT];
    spots.extend(bears.into_iter().map(|b| bear_position(b, t).xz()));
    spots.extend(crows.into_iter().map(|c| crow_position(c, t).xz()));
    spots.extend(villagers.into_iter().map(|v| v.pos));
    spots
}

/// Would farting at `at` be seen by an NPC standing at one of `npcs`?
pub fn is_shameful(at: Vec2, npcs: &[Vec2]) -> bool {
    npcs.iter().any(|n| n.distance(at) < SHAME_RADIUS)
}

/// Server: release the pressure, and judge.
fn let_rip(
    fart: On<FromClient<Fart>>,
    still: Query<&crate::features::stillness::Stillness>,
    mut commands: Commands,
    time: Res<Time>,
    mut players: Query<(&Player, &PlayerOwner, &mut Gas)>,
    duelists: Query<&Duelist>,
    bears: Query<&Bear>,
    crows: Query<&Crow>,
    villagers: Query<&Villager>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let owner = match fart.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((player, _, mut gas)) = players.iter_mut().find(|(_, o, _)| o.0 == owner) else {
        return;
    };
    if gas.0 < MIN_GAS {
        return;
    }
    let at = duelists
        .iter()
        .find(|d| d.player == player.id)
        .map(|d| d.pos.xz())
        .unwrap_or(fart.at);
    // The server's game time is the session time every peer animates by.
    let npcs = npc_spots(time.elapsed_secs(), bears, crows, villagers);
    let pressure = gas.0;
    gas.0 = 0;
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Farted {
            player: player.id,
            at,
            shamed: is_shameful(at, &npcs),
            pressure,
        },
    });
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Shame>()
        .add_observer(hear_fart)
        .add_systems(OnEnter(AppState::InGame), spawn_fart_ui)
        .add_systems(OnExit(AppState::InGame), forget_shame)
        .add_systems(
            Update,
            (
                press_to_fart,
                show_gas.run_if(crate::world::hud_tick()),
                feel_shame,
                fade_news,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// This peer's own shame, and the latest fart news.
#[derive(Resource, Default)]
struct Shame {
    remaining: f32,
    news: String,
    news_left: f32,
    song: Option<Entity>,
}

#[derive(Component)]
struct GasText;

#[derive(Component)]
struct FartNews;

#[derive(Component)]
struct ShameOverlay;

fn spawn_fart_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        GasText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        FartNews,
        crate::hud::slot(crate::hud::HudSlot::Toasts),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(
            "farting: F lets one go where you stand once your gas is built up. \
             Keep away from NPCs or you'll be shamed.",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(64.0),
        TextColor(crate::theme::ROSE_RED),
        TextShadow {
            offset: Vec2::new(3.0, 4.0),
            color: Color::srgba(0.3, 0.05, 0.15, 0.45),
        },
        crate::assets::DisplayText,
        ShameOverlay,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::NONE),
        DespawnOnExit(AppState::InGame),
    ));
}

/// F: fart with your bottom where your body is (#53), or, from the overview
/// camera, at the floor spot under the mouse cursor.
#[allow(clippy::too_many_arguments)]
fn press_to_fart(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    view: Res<super::fps::FpsView>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &super::fps::Walker)>,
    duelists: Query<&Duelist>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    if view.first_person
        && let Some(w) = super::fps::my_walker(me.as_deref(), &walkers, &duelists)
    {
        commands.client_trigger(Fart { at: w.pos.xz() });
        return;
    }
    if view.first_person {
        return; // no body yet: nothing to fart from
    }
    let Some(at) = (|| {
        let cursor = window.single().ok()?.cursor_position()?;
        let (camera, cam_tf) = camera.single().ok()?;
        let ray = camera.viewport_to_world(cam_tf, cursor).ok()?;
        let d = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
        Some(ray.get_point(d).xz())
    })() else {
        // Cursor off the window (or no floor under it): no fart rather than
        // one at the origin, where the crows circle.
        return;
    };
    commands.client_trigger(Fart { at });
}

fn show_gas(
    local: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &Gas)>,
    mut text: Query<&mut Text, With<GasText>>,
) {
    let Some(local) = local else { return };
    let Some((_, gas)) = players.iter().find(|(p, _)| p.id == local.0) else {
        return;
    };
    let filled = (gas.0 / 10) as usize;
    let bar = format!("{}{}", "•".repeat(filled), "·".repeat(10 - filled));
    let hint = if gas.0 >= MIN_GAS { "  F to fart" } else { "" };
    let want = format!("gas {bar} {}%{hint}", gas.0);
    for mut text in &mut text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

fn hear_fart(
    farted: On<Farted>,
    mut commands: Commands,
    local: Option<Res<LocalPlayer>>,
    mut shame: ResMut<Shame>,
    audio: Option<Res<Assets<AudioSource>>>,
) {
    let me = local.is_some_and(|l| l.0 == farted.player);
    let who = if me {
        "You".to_string()
    } else {
        format!("Player {}", farted.player)
    };
    shame.news = if farted.shamed {
        format!("{who} farted in front of an NPC. SHAME.")
    } else {
        format!("{who} farted discreetly.")
    };
    shame.news_left = 4.0;
    if !(me && farted.shamed) {
        return;
    }
    shame.remaining = SHAME_SECS;
    if let Some(old) = shame.song.take() {
        commands.entity(old).try_despawn();
    }
    if audio.is_none() {
        return;
    }
    let song = play_synth(&mut commands, || {
        wav_bytes(&shame_song(SONG_RATE), SONG_RATE)
    });
    shame.song = Some(song);
}

/// While shamed: blur the camera (depth of field focused right in front of
/// your face) and flash a red "SHAME" over everything.
fn feel_shame(
    mut commands: Commands,
    time: Res<Time>,
    mut shame: ResMut<Shame>,
    camera: Query<(Entity, Has<DepthOfField>), With<MainCamera>>,
    mut overlay: Query<(&mut Text, &mut BackgroundColor), With<ShameOverlay>>,
) {
    shame.remaining = (shame.remaining - time.delta_secs()).max(0.0);
    let shamed = shame.remaining > 0.0;
    for (cam, blurred) in &camera {
        if shamed && !blurred {
            commands.entity(cam).insert((
                DepthPrepass,
                DepthOfField {
                    focal_distance: 0.2,
                    aperture_f_stops: 0.05,
                    max_circle_of_confusion_diameter: 48.0,
                    ..default()
                },
            ));
        } else if !shamed && blurred {
            commands
                .entity(cam)
                .remove::<(DepthOfField, DepthPrepass)>();
        }
    }
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 6.0).sin();
    for (mut text, mut bg) in &mut overlay {
        let want = if shamed {
            format!("SHAME ({:.0})", shame.remaining.ceil())
        } else {
            String::new()
        };
        if text.0 != want {
            text.0 = want;
        }
        bg.0 = if shamed {
            Color::srgba(0.4, 0.0, 0.1, 0.15 + 0.15 * pulse)
        } else {
            Color::NONE
        };
    }
}

fn fade_news(
    time: Res<Time>,
    mut shame: ResMut<Shame>,
    mut text: Query<&mut Text, With<FartNews>>,
) {
    shame.news_left = (shame.news_left - time.delta_secs()).max(0.0);
    let want = if shame.news_left > 0.0 {
        shame.news.as_str()
    } else {
        ""
    };
    for mut text in &mut text {
        if text.0 != want {
            text.0 = want.to_string();
        }
    }
}

fn forget_shame(mut shame: ResMut<Shame>) {
    *shame = Shame::default();
}

const SONG_RATE: u32 = 22_050;

/// The shame song: a scary, distorted chiptune, `SHAME_SECS` long. A square
/// lead creeps around a diminished arpeggio over a pulsing tritone bass,
/// slowly detuning, then everything is bit-crushed, sample-rate-crushed and
/// overdriven.
pub fn shame_song(rate: u32) -> Vec<f32> {
    // Semitones above A2 (110 Hz): a diminished, wrong-feeling crawl.
    const LEAD: [f32; 16] = [
        12.0, 15.0, 18.0, 21.0, 18.0, 15.0, 13.0, 12.0, 24.0, 21.0, 18.0, 19.0, 18.0, 15.0, 12.0,
        11.0,
    ];
    const BASS: [f32; 4] = [0.0, 6.0, 1.0, 6.0];
    let n = (SHAME_SECS * rate as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let (mut lead_phase, mut bass_phase) = (0.0f32, 0.0f32);
    let mut held = 0.0f32;
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let step = (t * 6.0) as usize;
        let creep = t / SHAME_SECS; // gets more out of tune as it goes
        let wobble = (t * 5.3).sin() * (0.15 + 0.6 * creep);
        let lead_f = 110.0 * 2f32.powf((LEAD[step % LEAD.len()] + wobble) / 12.0);
        let bass_f = 55.0 * 2f32.powf(BASS[(step / 8) % BASS.len()] / 12.0);
        lead_phase = (lead_phase + lead_f / rate as f32).fract();
        bass_phase = (bass_phase + bass_f / rate as f32).fract();
        let lead = if lead_phase < 0.25 { 1.0 } else { -1.0 };
        let bass = if bass_phase < 0.5 { 1.0 } else { -1.0 };
        // Each note plucks then decays; the bass throbs.
        let note_t = (t * 6.0).fract();
        let lead_env = (1.0 - note_t).powf(1.5);
        let throb = 0.6 + 0.4 * (t * std::f32::consts::TAU * 1.5).sin();
        let mix = lead * lead_env * 0.45 + bass * throb * 0.35;
        // Sample-rate crush (hold every 4th), 4-bit crush, overdrive.
        if i % 4 == 0 {
            held = (mix * 8.0).round() / 8.0;
        }
        let driven = (held * 2.5).tanh();
        let fade = (t * 4.0).min(1.0) * ((SHAME_SECS - t) * 2.0).min(1.0);
        out.push((driven * fade * 0.4).clamp(-1.0, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npcs_shame_you_only_up_close() {
        let npcs = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        assert!(is_shameful(Vec2::new(1.0, 1.0), &npcs));
        assert!(is_shameful(Vec2::new(9.0, 0.0), &npcs));
        assert!(!is_shameful(Vec2::new(5.0, 0.0), &npcs));
        assert!(!is_shameful(Vec2::new(-20.0, 20.0), &npcs));
    }

    #[test]
    fn dave_is_always_watching_where_he_stands() {
        let spots = npc_spots(0.0, [], [], []);
        assert!(is_shameful(super::super::dave_commune::DAVE_AT, &spots));
        assert!(!is_shameful(
            super::super::dave_commune::PARLIAMENT_AT,
            &spots
        ));
        assert!(!is_shameful(Vec2::new(15.0, 15.0), &spots));
    }

    #[test]
    fn shame_song_is_twenty_seconds_and_sane() {
        let s = shame_song(SONG_RATE);
        assert_eq!(s.len(), (SHAME_SECS * SONG_RATE as f32) as usize);
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
        assert!(s.iter().all(|x| x.is_finite()));
    }
}
