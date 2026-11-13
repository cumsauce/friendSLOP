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

use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::post_process::dof::DepthOfField;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::{Bear, bear_position};
use crate::features::crows::{Crow, crow_position};
use crate::features::duel::Duelist;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::rules::villagers::Villager;
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

/// Tuning numbers: `assets/data/places/fart.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds for gas to go from empty to full.
    pub gas_fill_secs: f32,
    /// You need at least this much gas to fart.
    pub min_gas: u8,
    /// NPCs closer than this (metres) to your bottom will see you fart.
    pub shame_radius: f32,
    /// How long the shame lasts.
    pub shame_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/places/fart.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub fn protocol(app: &mut App) {
    app.replicate::<Gas>()
        .add_client_event::<Fart>(Channel::Ordered)
        .add_server_event::<Farted>(Channel::Ordered)
        .add_observer(give_gas)
        .add_observer(let_rip)
        .add_systems(
            Update,
            (build_pressure, burst)
                .chain()
                .run_if(in_state(ServerState::Running)),
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
    mut players: Query<(&mut Gas, &mut GasBuildup, Option<&super::skills::Skills>)>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return;
    }
    let per_sec = 100.0 / tuning().gas_fill_secs;
    for (mut gas, mut buildup, skills) in &mut players {
        // Farting skill fills you faster (#176).
        let per_sec = per_sec * skills.map_or(1.0, |s| s.gas_rate());
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
    let mut spots = vec![super::dave_commune::dave_at()];
    spots.extend(bears.into_iter().map(|b| bear_position(b, t).xz()));
    spots.extend(crows.into_iter().map(|c| crow_position(c, t).xz()));
    spots.extend(villagers.into_iter().map(|v| v.pos));
    spots
}

/// Would farting at `at` be seen by an NPC standing at one of `npcs`?
pub fn is_shameful(at: Vec2, npcs: &[Vec2]) -> bool {
    is_shameful_within(at, npcs, tuning().shame_radius)
}

/// Like [`is_shameful`], with NPCs that see `radius` metres (Marmalade's
/// wheel moves it, #173).
pub fn is_shameful_within(at: Vec2, npcs: &[Vec2], radius: f32) -> bool {
    npcs.iter().any(|n| n.distance(at) < radius)
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
    odds: Query<&crate::rules::house_odds::HouseOdds>,
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
    if gas.0 < tuning().min_gas {
        return;
    }
    let at = duelists
        .iter()
        .find(|d| d.player == player.id)
        .map(|d| d.pos.xz())
        .unwrap_or(fart.at);
    // The server's game time is the session time every peer animates by.
    let npcs = npc_spots(time.elapsed_secs(), bears, crows, villagers);
    let radius = crate::rules::house_odds::odds(&odds).shame_radius();
    release(&mut commands, player.id, &mut gas, at, &npcs, radius);
}

/// Server: let `player`'s gas go at `at`, judged by the NPCs at `npcs`, and tell everyone.
fn release(
    commands: &mut Commands,
    player: u64,
    gas: &mut Gas,
    at: Vec2,
    npcs: &[Vec2],
    radius: f32,
) {
    let pressure = gas.0;
    gas.0 = 0;
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Farted {
            player,
            at,
            shamed: is_shameful_within(at, npcs, radius),
            pressure,
        },
    });
}

/// Server: a full meter goes off by itself, wherever you are, whether you like it or not.
#[allow(clippy::too_many_arguments)]
fn burst(
    still: Query<&crate::features::stillness::Stillness>,
    mut commands: Commands,
    time: Res<Time>,
    mut players: Query<(&Player, &mut Gas, &super::fps::Walker)>,
    duelists: Query<&Duelist>,
    bears: Query<&Bear>,
    crows: Query<&Crow>,
    villagers: Query<&Villager>,
    odds: Query<&crate::rules::house_odds::HouseOdds>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return;
    }
    let mut npcs = None;
    for (player, mut gas, walker) in &mut players {
        if gas.0 < 100 {
            continue;
        }
        let at = duelists
            .iter()
            .find(|d| d.player == player.id)
            .map_or(walker.pos.xz(), |d| d.pos.xz());
        let npcs = npcs.get_or_insert_with(|| {
            npc_spots(
                time.elapsed_secs(),
                bears.iter(),
                crows.iter(),
                villagers.iter(),
            )
        });
        let radius = crate::rules::house_odds::odds(&odds).shame_radius();
        release(&mut commands, player.id, &mut gas, at, npcs, radius);
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
    app.init_resource::<Shame>()
        .add_observer(hear_fart)
        .add_systems(OnEnter(AppState::InGame), spawn_fart_ui)
        .add_systems(OnExit(AppState::InGame), forget_shame)
        .add_systems(
            Update,
            (press_to_fart, feel_shame, fade_news).run_if(in_state(AppState::InGame)),
        );
}

/// This peer's own shame, and the latest fart news.
#[derive(Resource, Default)]
pub struct Shame {
    remaining: f32,
    news: String,
    news_left: f32,
}

#[derive(Component)]
struct FartNews;

#[derive(Component)]
struct ShameOverlay;

fn spawn_fart_ui(mut commands: Commands) {
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

fn hear_fart(farted: On<Farted>, local: Option<Res<LocalPlayer>>, mut shame: ResMut<Shame>) {
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
    // The band plays the fart alert (the `FartAlert` mood, via the `shamed` music signal) while this lasts.
    shame.remaining = tuning().shame_secs;
}

impl Shame {
    /// Is the local player being shamed right now (the band plays the fart alert)?
    pub fn active(&self) -> bool {
        self.remaining > 0.0
    }
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
        // set_if_neq: don't re-render the full-screen overlay every frame
        // while nobody is shamed (#151).
        bg.set_if_neq(BackgroundColor(if shamed {
            Color::srgba(0.4, 0.0, 0.1, 0.15 + 0.15 * pulse)
        } else {
            Color::NONE
        }));
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

/// For the music: I'm being shamed.
fn music_signals(shame: Option<Res<Shame>>, signals: Option<ResMut<crate::music::MusicSignals>>) {
    let Some(mut signals) = signals else { return };
    signals.flag("shamed", shame.is_some_and(|s| s.active()));
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
        assert!(is_shameful(super::super::dave_commune::dave_at(), &spots));
        assert!(!is_shameful(
            super::super::dave_commune::parliament_at(),
            &spots
        ));
        assert!(!is_shameful(Vec2::new(15.0, 15.0), &spots));
    }
}
