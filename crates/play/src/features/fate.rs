//! Action failing. From issue #8.
//!
//! Every action has a small chance (5%) of failing. A failed action doesn't
//! happen; instead the server rolls a random result from the list in the issue
//! (the six from the issue plus more in the same spirit from #49)
//! and puts it on the replicated `FateBoard`, which everyone sees on screen.
//! Results last a while (a "month" or an "hour" is squashed into 60 s) and
//! then wear off.
//!
//! Action handlers opt in with one line: add a `Fate` system param and
//! `if fate.fails(client) { return; }` at the point the action would happen.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::{Player, PlayerOwner};

/// Tuning numbers: `assets/data/play/fate.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Chance that any action fails.
    pub fail_chance: f32,
    /// How long a failure's result lasts.
    pub result_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/play/fate.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// What a failed action caused.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    WorldDestroyed,
    GovernmentCares,
    OwnLanguages,
    GenderTransed,
    Hampter,
    PottedPetunias,
    // More failures (#49).
    MoonApologises,
    EveryoneIsBees,
    TaxesFiledThemselves,
    GravityForgotYou,
    BecameAGoose,
    TimeWentSideways,
    LandlordsFeelShame,
    SoupEverywhere,
}

impl Outcome {
    pub const ALL: [Outcome; 14] = [
        Outcome::WorldDestroyed,
        Outcome::GovernmentCares,
        Outcome::OwnLanguages,
        Outcome::GenderTransed,
        Outcome::Hampter,
        Outcome::PottedPetunias,
        Outcome::MoonApologises,
        Outcome::EveryoneIsBees,
        Outcome::TaxesFiledThemselves,
        Outcome::GravityForgotYou,
        Outcome::BecameAGoose,
        Outcome::TimeWentSideways,
        Outcome::LandlordsFeelShame,
        Outcome::SoupEverywhere,
    ];

    /// Pick an outcome; `roll` is in `0.0..1.0`.
    pub fn pick(roll: f32) -> Self {
        let i = (roll.clamp(0.0, 0.999_999) * Self::ALL.len() as f32) as usize;
        Self::ALL[i.min(Self::ALL.len() - 1)]
    }

    /// What it looks like on screen, given who failed.
    pub fn describe(self, player: u64) -> String {
        match self {
            Outcome::WorldDestroyed => {
                "the world and everything in it was destroyed (it got better)".into()
            }
            Outcome::GovernmentCares => {
                "the nearest government cares about its citizens this month".into()
            }
            Outcome::OwnLanguages => "everyone speaks their own unique language this hour".into(),
            Outcome::GenderTransed => format!("player {player}'s gender has been transed"),
            Outcome::Hampter => format!("player {player}: hampter"),
            Outcome::PottedPetunias => format!("player {player} is now a pot of petunias"),
            Outcome::MoonApologises => "the moon has publicly apologised for everything".into(),
            Outcome::EveryoneIsBees => "everyone is bees for the next fortnight".into(),
            Outcome::TaxesFiledThemselves => {
                "everyone's taxes filed themselves correctly this year".into()
            }
            Outcome::GravityForgotYou => format!("gravity has forgotten player {player}'s name"),
            Outcome::BecameAGoose => format!("player {player} is now, legally, a goose"),
            Outcome::TimeWentSideways => "time went sideways for a week".into(),
            Outcome::LandlordsFeelShame => "every landlord briefly feels shame".into(),
            Outcome::SoupEverywhere => format!("player {player} has turned the sea into soup"),
        }
    }
}

/// One active failure result.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Mishap {
    pub player: u64,
    pub outcome: Outcome,
    pub secs_left: f32,
}

/// Replicated singleton: every failure result currently in effect.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct FateBoard {
    pub mishaps: Vec<Mishap>,
    /// How many failures have ever been pushed here. Monotone: the sob-mail
    /// counts adds from this, so a wear-off never nets a new failure away.
    pub total_fails: u64,
}

/// Server only: chance of failure (tests can change it) and dice.
#[derive(Resource)]
pub struct FateDice {
    pub chance: f32,
    seed: u32,
}

impl Default for FateDice {
    fn default() -> Self {
        Self {
            chance: tuning().fail_chance,
            seed: 0x0fa7_e80d,
        }
    }
}

impl FateDice {
    fn roll(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Server: ask fate whether an action by `client` fails.
#[derive(SystemParam)]
pub struct Fate<'w, 's> {
    dice: ResMut<'w, FateDice>,
    boards: Query<'w, 's, &'static mut FateBoard>,
    players: Query<'w, 's, (&'static Player, &'static PlayerOwner)>,
    /// Marmalade's wheel (#173) scales the chance.
    odds: Query<'w, 's, &'static crate::rules::house_odds::HouseOdds>,
}

impl Fate<'_, '_> {
    /// True if the action fails (and a random result has been applied).
    pub fn fails(&mut self, client: ClientId) -> bool {
        let scale = crate::rules::house_odds::odds(&self.odds).fail_chance() / tuning().fail_chance;
        if self.dice.roll() >= self.dice.chance * scale {
            return false;
        }
        let owner = match client {
            ClientId::Server => None,
            ClientId::Client(e) => Some(e),
        };
        // A failure with no player behind it (the client just left) cannot
        // be blamed on anyone: never default to player 0.
        let Some(player) = self
            .players
            .iter()
            .find(|(_, o)| o.0 == owner)
            .map(|(p, _)| p.id)
        else {
            return true;
        };
        let outcome = Outcome::pick(self.dice.roll());
        if let Some(mut board) = self.boards.iter_mut().next() {
            board.mishaps.push(Mishap {
                player,
                outcome,
                secs_left: tuning().result_secs,
            });
            board.total_fails += 1;
        }
        true
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<FateBoard>()
        .init_resource::<FateDice>()
        .init_resource::<MailSeen>()
        .add_systems(OnEnter(ServerState::Running), spawn_board)
        .add_systems(OnExit(ServerState::Running), clear_boards)
        .add_systems(Update, wear_off.run_if(in_state(ServerState::Running)));
}

fn spawn_board(mut commands: Commands) {
    commands.spawn((FateBoard::default(), Replicated));
}

/// Session over: no board (and no mail memory) may outlive it.
fn clear_boards(
    mut commands: Commands,
    boards: Query<Entity, With<FateBoard>>,
    mut seen: ResMut<MailSeen>,
) {
    for b in &boards {
        commands.entity(b).try_despawn();
    }
    seen.0 = 0;
}

fn wear_off(time: Res<Time>, mut boards: Query<&mut FateBoard>) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for mut board in &mut boards {
        if board.mishaps.is_empty() {
            continue;
        }
        // Count down without marking the board changed, so it replicates
        // once a second (when the shown whole seconds tick), not every frame
        // (#151).
        let quiet = board.bypass_change_detection();
        let before = quiet.mishaps.len();
        let mut ticked = false;
        for m in &mut quiet.mishaps {
            let shown = m.secs_left.ceil();
            m.secs_left -= dt;
            ticked |= m.secs_left.ceil() != shown;
        }
        quiet.mishaps.retain(|m| m.secs_left > 0.0);
        if ticked || quiet.mishaps.len() != before {
            board.set_changed();
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_fate_hud)
        .add_systems(
            Update,
            (
                update_fate_hud.run_if(crate::world::hud_tick()),
                email_the_developer,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct FateHud;

fn spawn_fate_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::PINK.mix(&crate::theme::INK, 0.35)),
        FateHud,
        crate::hud::slot(crate::hud::HudSlot::TopRight),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_fate_hud(boards: Query<&FateBoard>, mut hud: Query<&mut Text, With<FateHud>>) {
    let mut lines = Vec::new();
    for board in &boards {
        for m in &board.mishaps {
            lines.push(format!(
                "ACTION FAILED: {} ({:.0}s)",
                m.outcome.describe(m.player),
                m.secs_left.ceil()
            ));
        }
    }
    let want = lines.join("\n");
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

/// What a failure sends the developer (#190, bben: "email the developer a
/// sobbing emoji"). Pretend: nothing actually leaves the game.
pub const SOB_MAIL: &str = "📧 emailing the developer a 😭 ... sent";

/// How many failures the sob-mail has already sent for (their total count).
#[derive(Resource, Default)]
struct MailSeen(u64);

/// How many new failures appeared since the mail last saw `before` of them
/// (both totals are monotone, so wearing off never nets an add away).
pub fn new_failures(before: u64, now: u64) -> u64 {
    now.saturating_sub(before)
}

/// Every new failure emails the developer a sobbing emoji (in a toast).
fn email_the_developer(
    boards: Query<&FateBoard>,
    changed: Query<(), Changed<FateBoard>>,
    mut seen: ResMut<MailSeen>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let Some(board) = boards.iter().next() else {
        return;
    };
    if changed.is_empty() {
        return;
    }
    let fresh = new_failures(seen.0, board.total_fails);
    seen.0 = board.total_fails;
    if fresh > 0
        && let Some(mut toast) = toast
    {
        toast.show(SOB_MAIL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_new_failures_send_mail() {
        assert_eq!(new_failures(0, 1), 1);
        assert_eq!(new_failures(2, 1), 0, "seen ahead of total mails nothing");
        assert_eq!(new_failures(1, 1), 0);
    }

    /// play:mail-netting — the mail counts failures added, never netted
    /// against wear-offs: one failure that wears off while another lands
    /// still mails for the new one.
    #[test]
    fn mail_counts_adds_not_net() {
        let mut app = App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins);
        app.init_resource::<crate::hud::Toast>();
        app.init_resource::<MailSeen>();
        app.add_systems(bevy::prelude::Update, email_the_developer);
        let board = app.world_mut().spawn(FateBoard::default()).id();

        // One failure: one mail.
        {
            let mut b = app
                .world_mut()
                .get_mut::<FateBoard>(board)
                .expect("test setup");
            b.mishaps.push(Mishap {
                player: 1,
                outcome: Outcome::Hampter,
                secs_left: 60.0,
            });
            b.total_fails += 1;
        }
        app.update();
        assert_eq!(
            app.world().resource::<crate::hud::Toast>().secs_left,
            crate::hud::Toast::SECS,
            "the first failure mails"
        );

        // It wears off exactly as the next failure lands: the visible count
        // nets to zero, but the add must still mail.
        app.world_mut()
            .resource_mut::<crate::hud::Toast>()
            .secs_left = 0.0;
        {
            let mut b = app
                .world_mut()
                .get_mut::<FateBoard>(board)
                .expect("test setup");
            b.mishaps.clear();
            b.mishaps.push(Mishap {
                player: 1,
                outcome: Outcome::BecameAGoose,
                secs_left: 60.0,
            });
            b.total_fails += 1;
        }
        app.update();
        assert_eq!(
            app.world().resource::<crate::hud::Toast>().secs_left,
            crate::hud::Toast::SECS,
            "the add mails even beside a wear-off"
        );
    }

    /// play:mail-netting (session half) — the session cleanup drops the
    /// boards and the mail memory, so a re-host starts fresh.
    #[test]
    fn mail_memory_resets_with_the_session() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins);
        app.init_resource::<MailSeen>();
        app.world_mut().spawn(FateBoard::default());
        app.world_mut().resource_mut::<MailSeen>().0 = 7;
        app.world_mut()
            .run_system_once(clear_boards)
            .expect("test setup");
        assert_eq!(app.world().resource::<MailSeen>().0, 0);
        let world = app.world_mut();
        assert_eq!(
            world.query::<&FateBoard>().iter(world).count(),
            0,
            "boards are session state"
        );
    }

    #[test]
    fn every_outcome_can_happen() {
        let picked: Vec<Outcome> = (0..140).map(|i| Outcome::pick(i as f32 / 140.0)).collect();
        for o in Outcome::ALL {
            assert!(picked.contains(&o), "{o:?}");
        }
    }

    #[test]
    fn every_outcome_has_its_own_text() {
        let texts: std::collections::HashSet<String> =
            Outcome::ALL.iter().map(|o| o.describe(1)).collect();
        assert_eq!(texts.len(), Outcome::ALL.len());
    }

    #[test]
    fn failure_is_rare_but_happens() {
        let mut dice = FateDice::default();
        let fails = (0..2000)
            .filter(|_| dice.roll() < tuning().fail_chance)
            .count();
        assert!((40..200).contains(&fails), "{fails}");
    }

    #[test]
    fn the_first_few_actions_succeed() {
        // Keeps other features' multiplayer tests deterministic.
        let mut dice = FateDice::default();
        assert!((0..32).all(|_| dice.roll() >= tuning().fail_chance));
    }
}
