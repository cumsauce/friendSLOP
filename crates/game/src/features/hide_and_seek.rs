//! Hide and seek with your friends among the scenery and the NPCs.
//! From issue #61 by doomy.
//!
//! Anyone presses K to start a round (needs at least two players). One player
//! is picked at random to seek. For the first 30 seconds the seeker is
//! blindfolded and rooted to the spot while everyone else runs off to hide
//! behind trees, sheds, hay bales and the like. Then the seeker has 3 minutes
//! to find everyone: walk up to a hider and left click while facing them to
//! tag them. Tag everyone and the seeker wins; hold out until time runs out
//! and the hiders win.
//!
//! The server runs the round; clients only send "start" and "tag".

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{Rooted, Walker};
use crate::player::{LocalPlayer, Player, PlayerOwner};

pub const HIDE_SECS: u32 = 30;
pub const SEEK_SECS: u32 = 180;
/// How close (and roughly in front) a hider must be to get tagged.
pub const TAG_REACH: f32 = 2.5;
const NEWS_SECS: f32 = 6.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Phase {
    /// No round running.
    #[default]
    Idle,
    /// The seeker counts with their eyes shut; everyone else hides.
    Hiding,
    /// The seeker hunts.
    Seeking,
}

/// The one hide-and-seek game, replicated to everyone.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct HideAndSeek {
    pub phase: Phase,
    pub seeker: u64,
    /// Everyone hiding this round, and who's been found so far.
    pub hiders: Vec<u64>,
    pub found: Vec<u64>,
    /// Whole seconds left in the current phase.
    pub secs_left: u32,
}

impl HideAndSeek {
    pub fn still_hidden(&self) -> impl Iterator<Item = u64> + '_ {
        self.hiders
            .iter()
            .copied()
            .filter(|h| !self.found.contains(h))
    }
}

/// Server only: the exact countdown behind `secs_left`.
#[derive(Component, Default)]
struct PhaseClock(f32);

/// Client -> server: "let's play hide and seek".
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct StartHideAndSeek;

/// Client -> server: the seeker tries to tag whoever is in front of them.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TagHider;

/// Server -> everyone: something happened in the game.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HideSeekNews {
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.replicate::<HideAndSeek>()
        .add_client_event::<StartHideAndSeek>(Channel::Ordered)
        .add_client_event::<TagHider>(Channel::Ordered)
        .add_server_event::<HideSeekNews>(Channel::Ordered)
        .add_observer(on_start)
        .add_observer(on_tag)
        .add_systems(OnEnter(ServerState::Running), spawn_game)
        .add_systems(
            Update,
            (tick_round, keep_seeker_rooted)
                .chain()
                .run_if(in_state(ServerState::Running)),
        );
}

fn spawn_game(mut commands: Commands) {
    commands.spawn((HideAndSeek::default(), PhaseClock::default(), Replicated));
}

fn news(commands: &mut Commands, text: String) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: HideSeekNews { text },
    });
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

/// Pick the seeker from `ids` with a cheap clock-based roll.
fn pick(ids: &[u64], roll: u32) -> Option<u64> {
    ids.get(roll as usize % ids.len().max(1)).copied()
}

fn on_start(
    ev: On<FromClient<StartHideAndSeek>>,
    time: Res<Time<Real>>,
    still: Query<&super::stillness::Stillness>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner)>,
    mut game: Query<(&mut HideAndSeek, &mut PhaseClock)>,
    mut fate: super::fate::Fate,
) {
    if super::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Ok((mut game, mut clock)) = game.single_mut() else {
        return;
    };
    if game.phase != Phase::Idle || player_of(ev.client_id, &players).is_none() {
        return;
    }
    let mut ids: Vec<u64> = players.iter().map(|(p, _)| p.id).collect();
    ids.sort_unstable();
    if ids.len() < 2 {
        news(
            &mut commands,
            "hide and seek needs at least two players".into(),
        );
        return;
    }
    if fate.fails(ev.client_id) {
        return;
    }
    let roll = time.elapsed().subsec_micros();
    let Some(seeker) = pick(&ids, roll) else {
        return;
    };
    *game = HideAndSeek {
        phase: Phase::Hiding,
        seeker,
        hiders: ids.into_iter().filter(|&id| id != seeker).collect(),
        found: Vec::new(),
        secs_left: HIDE_SECS,
    };
    clock.0 = HIDE_SECS as f32;
    news(
        &mut commands,
        format!("HIDE AND SEEK! player {seeker} is seeking. everyone else: hide! ({HIDE_SECS} s)"),
    );
}

/// Is a hider standing at `hider` taggable by a seeker at `seeker` facing `yaw`?
pub fn can_tag(seeker: Vec3, yaw: f32, hider: Vec3) -> bool {
    let to = (hider - seeker).with_y(0.0);
    let facing = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    let d = to.length();
    d < TAG_REACH && (d < 0.6 || to.normalize_or_zero().dot(facing) > 0.5)
}

fn on_tag(
    ev: On<FromClient<TagHider>>,
    still: Query<&super::stillness::Stillness>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner)>,
    walkers: Query<(&Player, &Walker)>,
    mut game: Query<(&mut HideAndSeek, &mut PhaseClock)>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Ok((mut game, mut clock)) = game.single_mut() else {
        return;
    };
    if game.phase != Phase::Seeking || player_of(ev.client_id, &players) != Some(game.seeker) {
        return;
    }
    let Some(me) = walkers.iter().find(|(p, _)| p.id == game.seeker) else {
        return;
    };
    let me = *me.1;
    let hidden: Vec<u64> = game.still_hidden().collect();
    let Some((found, _)) = walkers
        .iter()
        .filter(|(p, w)| hidden.contains(&p.id) && can_tag(me.pos, me.yaw, w.pos))
        .map(|(p, w)| (p.id, me.pos.distance(w.pos)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
    else {
        return;
    };
    game.found.push(found);
    let left = game.still_hidden().count();
    if left == 0 {
        news(
            &mut commands,
            format!(
                "found player {found}! that's everyone: player {} WINS hide and seek!",
                game.seeker
            ),
        );
        end_round(&mut game, &mut clock);
    } else {
        news(
            &mut commands,
            format!(
                "player {} found player {found}! {left} still hiding",
                game.seeker
            ),
        );
    }
}

fn end_round(game: &mut HideAndSeek, clock: &mut PhaseClock) {
    *game = HideAndSeek::default();
    clock.0 = 0.0;
}

fn tick_round(
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    mut commands: Commands,
    players: Query<&Player>,
    mut game: Query<(&mut HideAndSeek, &mut PhaseClock)>,
) {
    let Ok((mut game, mut clock)) = game.single_mut() else {
        return;
    };
    if game.phase == Phase::Idle {
        return;
    }
    // Someone left: drop them; if it was the seeker, the round is off.
    let here: Vec<u64> = players.iter().map(|p| p.id).collect();
    if !here.contains(&game.seeker) {
        news(
            &mut commands,
            "hide and seek is off: the seeker left".into(),
        );
        end_round(&mut game, &mut clock);
        return;
    }
    if game.hiders.iter().any(|h| !here.contains(h)) {
        game.hiders.retain(|h| here.contains(h));
        game.found.retain(|h| here.contains(h));
        if game.still_hidden().count() == 0 {
            news(
                &mut commands,
                "hide and seek is over: nobody left to find".into(),
            );
            end_round(&mut game, &mut clock);
            return;
        }
    }
    if super::stillness::is_frozen(&still) {
        return; // timers stop too
    }
    clock.0 -= time.delta_secs();
    if clock.0 > 0.0 {
        let secs = clock.0.ceil() as u32;
        if game.secs_left != secs {
            game.secs_left = secs;
        }
        return;
    }
    match game.phase {
        Phase::Hiding => {
            game.phase = Phase::Seeking;
            game.secs_left = SEEK_SECS;
            clock.0 = SEEK_SECS as f32;
            news(
                &mut commands,
                format!("ready or not, here comes player {}!", game.seeker),
            );
        }
        Phase::Seeking => {
            let left: Vec<String> = game.still_hidden().map(|h| h.to_string()).collect();
            news(
                &mut commands,
                format!(
                    "time's up! the hiders win (never found: player {})",
                    left.join(", player ")
                ),
            );
            end_round(&mut game, &mut clock);
        }
        Phase::Idle => {}
    }
}

/// The seeker can't move while counting.
fn keep_seeker_rooted(
    mut commands: Commands,
    game: Query<&HideAndSeek>,
    players: Query<(Entity, &Player, Has<Rooted>)>,
) {
    let Ok(game) = game.single() else {
        return;
    };
    for (entity, player, rooted) in &players {
        let want = game.phase == Phase::Hiding && player.id == game.seeker;
        if want && !rooted {
            commands.entity(entity).insert(Rooted);
        } else if !want && rooted {
            commands.entity(entity).remove::<Rooted>();
        }
    }
}

// ---------------------------------------------------------------- client side

pub fn plugin(app: &mut App) {
    app.init_resource::<HideSeekView>()
        .add_observer(on_news)
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            (keys, update_ui.run_if(crate::world::hud_tick()), blindfold)
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Resource, Default)]
struct HideSeekView {
    news: Option<(String, f32)>,
}

fn on_news(ev: On<HideSeekNews>, mut view: ResMut<HideSeekView>) {
    view.news = Some((ev.text.clone(), NEWS_SECS));
}

fn keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    me: Option<Res<LocalPlayer>>,
    game: Query<&HideAndSeek>,
) {
    let Ok(game) = game.single() else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyK) && game.phase == Phase::Idle {
        commands.client_trigger(StartHideAndSeek);
    }
    if mouse.just_pressed(MouseButton::Left)
        && game.phase == Phase::Seeking
        && me.is_some_and(|m| m.0 == game.seeker)
    {
        commands.client_trigger(TagHider);
    }
}

#[derive(Component)]
struct HideSeekText;

#[derive(Component)]
struct HideSeekNewsText;

#[derive(Component)]
struct Blindfold;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        HideSeekText,
        crate::hud::slot(crate::hud::HudSlot::TopCenter),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        HideSeekNewsText,
        crate::hud::slot(crate::hud::HudSlot::Toasts),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(
            "hide and seek: K starts a round. The seeker counts blindfolded, then has \
             to walk up to hiders and click them.",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(42.0),
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),
        Blindfold,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::BLACK),
        GlobalZIndex(50),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
}

/// Status line for everyone.
pub fn status_line(game: &HideAndSeek, me: Option<u64>) -> String {
    let mins = |s: u32| format!("{}:{:02}", s / 60, s % 60);
    let seeker = me == Some(game.seeker);
    match game.phase {
        Phase::Idle => String::new(),
        Phase::Hiding if seeker => format!("you're IT. counting... {}", game.secs_left),
        Phase::Hiding => format!(
            "HIDE! player {} starts seeking in {}",
            game.seeker, game.secs_left
        ),
        Phase::Seeking => {
            let left = game.still_hidden().count();
            let you = if seeker {
                " - walk up to them and click!".to_string()
            } else if me.is_some_and(|m| game.found.contains(&m)) {
                " - you've been found".to_string()
            } else {
                " - stay hidden!".to_string()
            };
            format!(
                "hide and seek: player {} seeking, {left} still hidden, {}{you}",
                game.seeker,
                mins(game.secs_left)
            )
        }
    }
}

fn update_ui(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    game: Query<&HideAndSeek>,
    mut view: ResMut<HideSeekView>,
    mut status: Query<&mut Text, (With<HideSeekText>, Without<HideSeekNewsText>)>,
    mut news: Query<&mut Text, (With<HideSeekNewsText>, Without<HideSeekText>)>,
    mut last_run: Local<Option<f32>>,
) {
    let now = time.elapsed_secs();
    let dt = now - last_run.replace(now).unwrap_or(now);
    if let Some((_, left)) = &mut view.news {
        *left -= dt;
        if *left <= 0.0 {
            view.news = None;
        }
    }
    let line = game
        .single()
        .map(|g| status_line(g, me.as_deref().map(|m| m.0)))
        .unwrap_or_default();
    for mut t in &mut status {
        if t.0 != line {
            t.0.clone_from(&line);
        }
    }
    let n = view
        .news
        .as_ref()
        .map(|(s, _)| s.clone())
        .unwrap_or_default();
    for mut t in &mut news {
        if t.0 != n {
            t.0.clone_from(&n);
        }
    }
}

/// The seeker sees nothing but the countdown while everyone hides.
fn blindfold(
    me: Option<Res<LocalPlayer>>,
    game: Query<&HideAndSeek>,
    mut cover: Query<(&mut Visibility, &mut Text), With<Blindfold>>,
) {
    let Ok(game) = game.single() else {
        return;
    };
    let blind = game.phase == Phase::Hiding && me.is_some_and(|m| m.0 == game.seeker);
    for (mut vis, mut text) in &mut cover {
        vis.set_if_neq(if blind {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if blind {
            let want = format!("eyes shut! counting...\n{}", game.secs_left);
            if text.0 != want {
                text.0 = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagging_needs_you_close_and_facing_them() {
        let here = Vec3::ZERO;
        // yaw 0 faces -Z.
        assert!(can_tag(here, 0.0, Vec3::new(0.0, 0.0, -2.0)));
        assert!(!can_tag(here, 0.0, Vec3::new(0.0, 0.0, 2.0)), "behind you");
        assert!(!can_tag(here, 0.0, Vec3::new(0.0, 0.0, -4.0)), "too far");
        assert!(
            can_tag(here, 0.0, Vec3::new(0.0, 0.0, 0.3)),
            "bumping into them"
        );
    }

    #[test]
    fn status_tells_the_seeker_and_hiders_apart() {
        let game = HideAndSeek {
            phase: Phase::Hiding,
            seeker: 1,
            hiders: vec![0, 2],
            found: vec![],
            secs_left: 12,
        };
        assert!(status_line(&game, Some(1)).contains("you're IT"));
        assert!(status_line(&game, Some(0)).contains("HIDE!"));
        assert_eq!(pick(&[3, 5, 7], 4), Some(5));
    }
}
