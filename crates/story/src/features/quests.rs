//! Quests (replacing the chapter checklist): NPCs round the world give them,
//! the party keeps one shared [`QuestLog`], and finishing every quest frees
//! the Commune and gets everyone into heaven (the shared goal, #75).
//!
//! - The vocabulary (the log, signals, gates, the data) is
//!   `friendslop_base::rules::quests`, so features in every crate can report
//!   progress and switch themselves on with a quest.
//! - Here: the server's log, taking quests on ([`TalkChoice`], checked for
//!   reach and the giver's hours), signals moving it on, the KoriCoin™
//!   reward, news for everyone, and the watcher that turns the commune's own
//!   mechanics (pets, the vault, HALT, ...) into signals.
//! - `quest_givers.rs`: the givers' bodies and markers, and the talk.
//! - The game's UI: the dialogue card, the quest card and the journal's tab.
//!
//! Heaven Pass tier = quests done. All of them done is the finale: the round
//! is over (`objective.rs`) and Enter starts a new campaign.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::abuela::Abuela;
use crate::features::fps::Walker;
use crate::features::gods_tears::GodsTear;
use crate::features::halt::Halt;
use crate::features::hide_and_seek::{HideSeek, Phase};
use crate::features::interact::{SERVER_REACH, body_of, deny, within};
use crate::features::landlord::{Act3, Tenancy};
use crate::features::mushroom::Mushroom;
use crate::features::pets::{Pet, biggest_pack};
use crate::features::smoking::Stash;
use crate::features::totems::{Door, Magic, Totem};
use crate::features::warehouse::Warehouse;
use crate::features::wendys::BurgerBag;
use crate::player::{Player, PlayerOwner};
pub use crate::rules::campaign::*;
pub use crate::rules::quests::*;

/// The one thing everyone is working towards.
pub const GOAL: &str = "Free the Commune and get into heaven: help everyone who asks";

/// Client -> server: I said yes to `giver` about `quest` (the `variant` way).
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TalkChoice {
    pub giver: String,
    pub quest: String,
    pub variant: Option<String>,
}

/// Server -> everyone: what happened to a quest, and who it paid.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuestNews {
    pub quest: String,
    pub change: Change,
    /// KoriCoins™ each player got (when it's done).
    pub reward: u32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<QuestLog>()
        .add_client_event::<TalkChoice>(Channel::Ordered)
        .add_server_event::<QuestNews>(Channel::Ordered)
        .add_observer(take_on)
        .add_observer(hear_signal)
        .add_observer(new_campaign_log)
        .add_observer(after_the_finale)
        .add_observer(count_mushroom)
        .add_observer(count_duel)
        .add_systems(OnEnter(ServerState::Running), spawn_log)
        .add_systems(
            Update,
            (fit_log, watch_the_world).run_if(in_state(ServerState::Running)),
        );
}

fn spawn_log(mut commands: Commands) {
    commands.spawn((QuestLog::fresh(&QUESTS.get()), Replicated));
}

/// Keep the log fitting the book (it's edited while the game runs) and
/// whether anyone else is here.
fn fit_log(players: Query<&Player>, mut log: Query<&mut QuestLog>) {
    let Ok(mut log) = log.single_mut() else {
        return;
    };
    let book = QUESTS.get();
    let mut next = log.clone();
    next.fit(&book);
    next.unlock(&book);
    next.solo = players.iter().count() < 2;
    if *log != next {
        *log = next;
    }
}

/// The server's hour, from the replicated day start.
fn hour(day: &Query<&friendslop_base::clock::DayStart>) -> f32 {
    friendslop_base::clock::hour_now(day.iter().next())
}

/// Server: someone said yes to a giver. Check they're with them, that the
/// giver is out and gives that quest, then put it on the list.
#[allow(clippy::too_many_arguments)]
fn take_on(
    ev: On<FromClient<TalkChoice>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    day: Query<&friendslop_base::clock::DayStart>,
    mut log: Query<&mut QuestLog>,
) {
    let book = QUESTS.get();
    let givers = GIVERS.get();
    let TalkChoice {
        giver,
        quest,
        variant,
    } = &ev.message;
    let (Some(q), Some(g)) = (book.get(quest), givers.get(giver)) else {
        return;
    };
    if q.giver != *giver {
        return;
    }
    // On the ground like every reach check in the game (`interact::within`).
    let near = body_of(ev.client_id, &walkers)
        .is_some_and(|me| within(me, g.place.at(), SERVER_REACH + 1.5));
    if !near {
        deny(&mut commands, ev.client_id, "too far away to talk");
        return;
    }
    if !g.out_at(hour(&day)) {
        deny(&mut commands, ev.client_id, "nobody's here right now");
        return;
    }
    let Ok(mut log) = log.single_mut() else {
        return;
    };
    if log.accept(&book, quest, variant.as_deref()) {
        commands.trigger(QuestStarted(quest.clone()));
        commands.server_trigger(ToClients {
            targets: SendTargets::All,
            message: QuestNews {
                quest: quest.clone(),
                change: Change::Accepted,
                reward: 0,
            },
        });
    }
}

/// Server: something happened. Move the log on, pay out what's done, and
/// tell everyone.
fn hear_signal(
    ev: On<QuestSignal>,
    mut commands: Commands,
    mut log: Query<&mut QuestLog>,
    mut purses: Query<&mut crate::rules::kori::KoriCoins, With<Player>>,
) {
    let Ok(mut log) = log.single_mut() else {
        return;
    };
    let book = QUESTS.get();
    // Read the variants before the signal moves them on.
    let variants: HashMap<String, Option<String>> = log
        .quests
        .iter()
        .filter_map(|(id, s)| match s {
            QuestState::Active { variant, .. } => Some((id.clone(), variant.clone())),
            _ => None,
        })
        .collect();
    let mut next = log.clone();
    let changes = next.signal(&book, &ev.key, ev.amount);
    if changes.is_empty() {
        return;
    }
    *log = next;
    for (quest, change) in changes {
        let reward = if change == Change::Done {
            let v = variants.get(&quest).cloned().flatten();
            let n = book.reward(&quest, v.as_deref());
            for purse in &mut purses {
                crate::rules::kori::pay(Some(purse), n);
            }
            n
        } else {
            0
        };
        commands.server_trigger(ToClients {
            targets: SendTargets::All,
            message: QuestNews {
                quest,
                change,
                reward,
            },
        });
    }
}

/// The round after the finale is a new campaign.
fn after_the_finale(_reset: On<RoundReset>, mut commands: Commands, log: Query<&QuestLog>) {
    if log.iter().any(|l| l.finale(&QUESTS.get())) {
        commands.trigger(NewCampaign);
    }
}

/// After the finale, a new campaign: a fresh log.
fn new_campaign_log(_ev: On<NewCampaign>, mut log: Query<&mut QuestLog>) {
    for mut l in &mut log {
        let solo = l.solo;
        *l = QuestLog::fresh(&QUESTS.get());
        l.solo = solo;
    }
}

/// A mushroom only ever disappears by being eaten.
fn count_mushroom(_ev: On<Remove<Mushroom>>, mut commands: Commands) {
    commands.trigger(QuestSignal::new("mushroom"));
}

/// Only duels fought to a winner count, not ones called off.
fn count_duel(_ev: On<super::duel::DuelFought>, mut commands: Commands) {
    commands.trigger(QuestSignal::new("duel"));
}

/// What went up since last frame. Only rises count, so a burger bag or stash
/// that empties (smoking, a new round) never counts back.
fn rises(seen: &mut HashMap<Entity, u32>, now: impl Iterator<Item = (Entity, u32)>) -> u32 {
    let mut up = 0;
    for (e, n) in now {
        let before = seen.insert(e, n).unwrap_or(n);
        up += n.saturating_sub(before);
    }
    up
}

/// The commune's own mechanics, as they were watched for the old checklist.
#[derive(Default)]
struct Seen {
    abuela: Option<u32>,
    bags: HashMap<Entity, u32>,
    stashes: HashMap<Entity, u32>,
    jegg: HashMap<Entity, u32>,
    escapes: HashMap<Entity, u32>,
    raises: HashMap<Entity, u32>,
    heists: HashMap<Entity, u32>,
    act: Option<Act3>,
    tears: Option<u32>,
    results: bool,
    pack: bool,
    door: bool,
    fire: bool,
}

/// Server: turn what the commune's mechanics did this frame into signals.
#[allow(clippy::too_many_arguments)]
fn watch_the_world(
    mut commands: Commands,
    mut seen: Local<Seen>,
    players: Query<&Player>,
    pets: Query<&Pet>,
    doors: Query<&Door>,
    totems: Query<&Totem>,
    tenancy: Query<(Entity, &Tenancy)>,
    halt: Query<(Entity, &Halt)>,
    tears: Query<&GodsTear>,
    hide_seek: Query<&HideSeek>,
    warehouse: Query<(Entity, &Warehouse)>,
    abuela: Query<&Abuela>,
    bags: Query<(Entity, &BurgerBag)>,
    stashes: Query<(Entity, &Stash)>,
) {
    let mut send = |key: &str, n: u32| {
        if n > 0 {
            commands.trigger(QuestSignal::times(key, n));
        }
    };
    let s = &mut *seen;
    // Edges: true now and not last frame.
    let edge = |was: &mut bool, now: bool| {
        let up = now && !*was;
        *was = now;
        up
    };

    let results = hide_seek.iter().any(|g| g.phase == Phase::Results);
    let hs = edge(&mut s.results, results);
    send("hide_seek", u32::from(hs));

    let want_pets = players.iter().count().clamp(1, 4);
    let adopted: Vec<Vec2> = pets
        .iter()
        .filter(|p| p.owner.is_some())
        .map(|p| p.at)
        .collect();
    let pack = adopted.len() >= want_pets && biggest_pack(&adopted) == adopted.len();
    let p = edge(&mut s.pack, pack);
    send("pack", u32::from(p));

    let door = doors.iter().any(|d| d.is_open());
    let d = edge(&mut s.door, door);
    send("vault_open", u32::from(d));

    let fire = totems
        .iter()
        .any(|t| t.magic == Magic::Fire && t.holder.is_some());
    let f = edge(&mut s.fire, fire);
    send("fire_totem", u32::from(f));

    let up = rises(&mut s.escapes, halt.iter().map(|(e, h)| (e, h.escapes)));
    send("halt_escape", up);

    // A tear that isn't lying on the ground any more has been used.
    let lying = tears.iter().count() as u32;
    if let Some(before) = s.tears {
        send("tear", before.saturating_sub(lying));
    }
    s.tears = Some(lying);

    let up = rises(
        &mut s.jegg,
        warehouse.iter().map(|(e, w)| (e, u32::from(w.delivered))),
    );
    send("jegg_delivery", up);

    if let Ok(a) = abuela.single() {
        if let Some(before) = s.abuela {
            send("abuela_visit", a.visits.saturating_sub(before));
        }
        s.abuela = Some(a.visits);
    }
    let up = rises(&mut s.bags, bags.iter().map(|(e, b)| (e, b.0)));
    send("burger", up);
    let up = rises(
        &mut s.stashes,
        stashes.iter().map(|(e, st)| (e, st.cigs + st.joints)),
    );
    send("paul_stash", up);

    let up = rises(&mut s.raises, tenancy.iter().map(|(e, t)| (e, t.raises)));
    send("landlord:raise", up);
    let up = rises(&mut s.heists, tenancy.iter().map(|(e, t)| (e, t.heists)));
    send("landlord:heist", up);
    if let Some((_, t)) = tenancy.iter().next() {
        if s.act.is_some_and(|a| a != t.act) {
            let key = match t.act {
                Act3::Legal => "",
                Act3::Crime => "landlord:crime",
                Act3::Boss => "landlord:boss",
                Act3::Won => "landlord:won",
            };
            if !key.is_empty() {
                send(key, 1);
            }
        }
        s.act = Some(t.act);
    }
}

// ---------------------------------------------------------------- reading it

/// The quest's title (its id if the book doesn't have it).
pub fn title(id: &str) -> String {
    QUESTS
        .get()
        .get(id)
        .map_or_else(|| id.to_string(), |q| q.title.clone())
}

/// A giver's name (their id without words).
pub fn giver_name(id: &str) -> String {
    DIALOGUE
        .get()
        .givers
        .get(id)
        .map_or_else(|| id.to_string(), |g| g.name.clone())
}

/// Where a giver is, for the journal ("by the bonfire").
pub fn giver_where(id: &str) -> String {
    GIVERS
        .get()
        .get(id)
        .and_then(|g| {
            let sites = crate::sites::sites();
            sites
                .sites
                .iter()
                .find(|s| s.id == g.site)
                .map(|s| format!("{}, {}", s.name, s.district))
        })
        .unwrap_or_default()
}

/// "Step text  2/3" for a quest on the list.
pub fn step_line(log: &QuestLog, id: &str) -> Option<String> {
    let book = QUESTS.get();
    let (s, n) = log.at_step(&book, id)?;
    let text = crate::input::fill(&s.text);
    Some(if s.need > 1 {
        format!("{text}  {n}/{}", s.need)
    } else {
        text
    })
}

/// The quest the quest card follows: the one you're `tracking` if it's on
/// the list, else the first on the list.
pub fn followed(log: &QuestLog, tracking: Option<&str>) -> Option<String> {
    let book = QUESTS.get();
    tracking
        .filter(|t| log.is_active(t))
        .map(str::to_string)
        .or_else(|| log.active(&book).first().map(|q| q.id.clone()))
}

/// The quest card's "now: ..." line.
pub fn next_line(log: &QuestLog, tracking: Option<&str>) -> String {
    let book = QUESTS.get();
    if log.finale(&book) {
        return "now: walk into heaven".to_string();
    }
    match followed(log, tracking).and_then(|id| step_line(log, &id)) {
        Some(step) => format!("now: {step}"),
        None => "now: find someone with a ! over their head".to_string(),
    }
}

/// Heaven Pass: quests done of all of them.
pub fn tier(log: &QuestLog) -> (usize, usize) {
    let book = QUESTS.get();
    (log.done_count(&book), book.quests.len())
}

/// The journal's list: on the list, on offer, done.
pub fn journal(log: &QuestLog) -> String {
    let book = QUESTS.get();
    let (done, total) = tier(log);
    let mut s = format!("QUESTS  {done}/{total} done  ·  Heaven Pass\n");
    let mut section = |head: &str, lines: Vec<String>| {
        if !lines.is_empty() {
            s.push_str(&format!("\n{head}\n"));
            for l in lines {
                s.push_str(&l);
                s.push('\n');
            }
        }
    };
    section(
        "ON YOUR LIST",
        log.active(&book)
            .iter()
            .map(|q| {
                format!(
                    "[  ] {}: {}",
                    q.title,
                    step_line(log, &q.id).unwrap_or_default()
                )
            })
            .collect(),
    );
    section(
        "SOMEONE WANTS HELP",
        book.quests
            .iter()
            .filter(|q| {
                matches!(log.state(&q.id), QuestState::Available | QuestState::Failed)
                    && !log.waived(q)
            })
            .map(|q| {
                let again = if log.state(&q.id) == QuestState::Failed {
                    " (failed: try again)"
                } else {
                    ""
                };
                let at = giver_where(&q.giver);
                let at = if at.is_empty() {
                    String::new()
                } else {
                    format!(" ({at})")
                };
                format!(" !  {}{again}: ask {}{at}", q.title, giver_name(&q.giver))
            })
            .collect(),
    );
    section(
        "DONE",
        book.quests
            .iter()
            .filter(|q| log.counts_done(&book, &q.id))
            .map(|q| {
                if log.waived(q) {
                    format!("[-] {}  (needs 2+ players)", q.title)
                } else {
                    format!("[x] {}", q.title)
                }
            })
            .collect(),
    );
    let locked = book
        .quests
        .iter()
        .filter(|q| log.state(&q.id) == QuestState::Locked)
        .count();
    if locked > 0 {
        s.push_str(&format!("\n+ {locked} more, once you've helped others"));
    }
    s
}

/// The ending, after every quest.
pub fn ending() -> String {
    "THE COMMUNE IS FREE! YOU GOT INTO HEAVEN!\n\
     Everyone who asked got helped, the landlord is evicted for good and the\n\
     pearly gates swing open for everyone. HALT sulks. The goat is grudgingly\n\
     impressed. Heaven Pass: top tier.\n\n\
     Special thanks to just meat and cheese, who gooned through the entire\n\
     campaign on a second monitor. Bad. Oh so bad. Baddie bad.\n\n\
     Press [Enter] to start again"
        .to_string()
}

// ---------------------------------------------------------------- client

/// Which quest the quest card follows (local: each player picks their own).
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Tracking(pub Option<String>);

pub fn plugin(app: &mut App) {
    app.init_resource::<Tracking>()
        .add_observer(hear_news)
        .add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet))
        .add_hud(spawn_ui)
        .add_systems(
            Update,
            show_journal
                .run_if(crate::world::hud_tick())
                .run_if(in_state(AppState::InGame)),
        );
}

/// The news, as toasts. Taking one on tracks it.
fn hear_news(
    ev: On<QuestNews>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut tracking: ResMut<Tracking>,
    log: Query<&QuestLog>,
) {
    let book = QUESTS.get();
    let QuestNews {
        quest,
        change,
        reward,
    } = &*ev;
    let name = title(quest);
    let giver = book
        .get(quest)
        .map(|q| giver_name(&q.giver))
        .unwrap_or_default();
    let text = match change {
        Change::Accepted => {
            tracking.0 = Some(quest.clone());
            format!("New quest: {name}")
        }
        Change::Step(_) => match log.iter().next().and_then(|l| step_line(l, quest)) {
            Some(step) => format!("{name}: {step}"),
            None => return,
        },
        Change::Done => {
            if tracking.0.as_deref() == Some(quest.as_str()) {
                tracking.0 = None;
            }
            let coins = match reward {
                0 => String::new(),
                1 => "  +1 KoriCoin™ each".to_string(),
                n => format!("  +{n} KoriCoins™ each"),
            };
            format!("QUEST DONE: {name}!{coins}")
        }
        Change::Failed => format!("Quest failed: {name}. Talk to {giver} to try again"),
        Change::Unlocked => format!("{giver} has something for you"),
    };
    if let Some(mut toast) = toast {
        toast.show(text);
    }
}

#[derive(Component)]
struct JournalText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        JournalText,
        TextFont::from_font_size(14.0),
        TextColor(crate::hud::TEXT),
        // The whole list lives on Tab (the journal); the quest card
        // (`ui::quest`) has the one you're following.
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        crate::hud::First,
        DespawnOnExit(AppState::InGame),
    ));
}

fn show_journal(log: Query<&QuestLog>, mut text: Query<&mut Text, With<JournalText>>) {
    let Ok(l) = log.single() else { return };
    let want = journal(l);
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

/// For the music: a victory when a quest is done.
fn music_signals(
    log: Query<&QuestLog>,
    mut before: Local<Option<usize>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let book = QUESTS.get();
    let now = log.iter().next().map(|l| l.done_count(&book));
    if let (Some(now), Some(before)) = (now, *before)
        && now > before
    {
        signals.cue("victory");
    }
    *before = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_read_from_one_log() {
        let book = QUESTS.get();
        let mut log = QuestLog::fresh(&book);
        assert!(next_line(&log, None).contains("!"));
        let first = book
            .quests
            .iter()
            .find(|q| log.state(&q.id) == QuestState::Available)
            .map(|q| q.id.clone())
            .unwrap_or_default();
        assert!(log.accept(&book, &first, None));
        assert!(journal(&log).contains("ON YOUR LIST"));
        assert_eq!(followed(&log, Some("nope")), Some(first.clone()));
        assert!(next_line(&log, None).starts_with("now: "));
        assert!(ending().contains("HEAVEN"));
    }
}
