//! The shared goal: free the Commune from your landlord and get into heaven,
//! over five chapters. From issue #75 (a question; answered by just meat and
//! cheese and cumsauce, who added "it can be like getting into heaven" and "it
//! can be like fortnite").
//!
//! The friends asked to keep every mechanic, make it one cohesive experience
//! for a long session (4-6 hours), and make the goal *really obvious* in the
//! UI. So the landlord story (#11) is the spine: every chapter ends with
//! beating him again, and on the way each chapter asks the group to do things
//! with the other mechanics (pets, the mushroom, the vault and totems, HALT,
//! gods tears, hide and seek, duels). Later chapters ask for more. Finishing
//! chapter five frees the Commune for good.
//!
//! The server owns one replicated [`Campaign`]. It watches the other features'
//! state each frame and ticks the checklist; ticks never untick. The objective
//! module ends the round when the chapter's checklist is done, and the next
//! [`RoundReset`] moves on to the next chapter (after the last one, back to 1).
//! Tasks that need two players (hide and seek, duels) are waived while you're
//! playing alone.
//!
//! Heaven is where the campaign ends: freeing the Commune opens the gates.
//! The "like fortnite" part is a season-pass style ladder: every ticked task
//! is one tier on the Heaven Pass, shown in the goal line, with a toast each
//! time a tier unlocks. Tier [`heaven_tiers`] is heaven.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::gods_tears::Transformation;
use crate::features::halt::Halt;
use crate::features::hide_and_seek::{HideSeek, Phase};
use crate::features::landlord::{Act3, Tenancy};
use crate::features::mushroom::Mushroom;
use crate::features::objective::RoundReset;
use crate::features::pets::{Pet, biggest_pack};
use crate::features::totems::{Door, Magic, Totem};
use crate::player::Player;

/// The one thing everyone is working towards. Shown at the top of the screen.
pub const GOAL: &str = "Free the Commune from your landlord and get into heaven";

/// One thing the group has to do in a chapter.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    /// Adopt pets (one each, up to four) and leash them all into one pack.
    Pack,
    Mushroom(u32),
    VaultDoor,
    FireTotem,
    Halt(u32),
    Tears(u32),
    HideSeek(u32),
    Duel(u32),
    Landlord,
}

impl Task {
    pub fn need(self) -> u32 {
        match self {
            Task::Mushroom(n)
            | Task::Halt(n)
            | Task::Tears(n)
            | Task::HideSeek(n)
            | Task::Duel(n) => n,
            _ => 1,
        }
    }

    /// Can't be done alone: waived when there's only one player.
    pub fn needs_friends(self) -> bool {
        matches!(self, Task::HideSeek(_) | Task::Duel(_))
    }

    /// What to do, and how.
    pub fn label(self) -> &'static str {
        match self {
            Task::Pack => "Adopt pets and leash them all into one pack",
            Task::Mushroom(_) => "Eat the voice mushroom",
            Task::VaultDoor => "Open a vault door (bring it the matching totem)",
            Task::FireTotem => "Take the Fire totem out of the vault",
            Task::Halt(_) => "Break out of HALT (G to pry)",
            Task::Tears(_) => "Use gods tears",
            Task::HideSeek(_) => "Play a round of hide and seek (K)",
            Task::Duel(_) => "Fight a duel (1-9 to challenge)",
            Task::Landlord => "Beat the landlord",
        }
    }
}

pub struct Chapter {
    pub name: &'static str,
    pub intro: &'static str,
    pub tasks: &'static [Task],
}

pub const CHAPTERS: [Chapter; 5] = [
    Chapter {
        name: "Moving In",
        intro: "Your landlord has just raised the rent. Find your feet, make friends, stand up to him.",
        tasks: &[Task::Pack, Task::Mushroom(1), Task::Landlord],
    },
    Chapter {
        name: "Rent Day",
        intro: "He's back and the rent's gone up again. Crack the vault and test HALT's walls.",
        tasks: &[
            Task::VaultDoor,
            Task::Halt(1),
            Task::HideSeek(1),
            Task::Landlord,
        ],
    },
    Chapter {
        name: "The Heist",
        intro: "The tears hold power and the Fire totem burns in the vault. Take what's yours.",
        tasks: &[
            Task::FireTotem,
            Task::Tears(3),
            Task::Duel(1),
            Task::Halt(1),
            Task::Landlord,
        ],
    },
    Chapter {
        name: "General Strike",
        intro: "The whole commune downs tools. Everyone, everything, all at once.",
        tasks: &[
            Task::Pack,
            Task::Mushroom(3),
            Task::HideSeek(2),
            Task::Duel(2),
            Task::Halt(2),
            Task::Landlord,
        ],
    },
    Chapter {
        name: "The Eviction",
        intro: "One last time. Beat him now and the Commune is free for good.",
        tasks: &[
            Task::FireTotem,
            Task::Tears(6),
            Task::HideSeek(3),
            Task::Duel(3),
            Task::Halt(3),
            Task::Landlord,
        ],
    },
];

/// Server -> everyone: where the campaign stands.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Campaign {
    /// Index into [`CHAPTERS`].
    pub chapter: usize,
    /// Per task of the current chapter, capped at its `need`.
    pub progress: Vec<u32>,
    /// Fewer than two players: tasks that need friends are waived.
    pub solo: bool,
}

impl Campaign {
    pub fn current(&self) -> &'static Chapter {
        &CHAPTERS[self.chapter.min(CHAPTERS.len() - 1)]
    }

    pub fn waived(&self, task: Task) -> bool {
        self.solo && task.needs_friends()
    }

    pub fn got(&self, i: usize) -> u32 {
        self.progress.get(i).copied().unwrap_or(0)
    }

    pub fn task_done(&self, i: usize) -> bool {
        let Some(&task) = self.current().tasks.get(i) else {
            return true;
        };
        self.waived(task) || self.got(i) >= task.need()
    }

    pub fn done_count(&self) -> usize {
        (0..self.current().tasks.len())
            .filter(|&i| self.task_done(i))
            .count()
    }

    /// Every task in this chapter is done.
    pub fn complete(&self) -> bool {
        self.done_count() == self.current().tasks.len()
    }

    /// Heaven Pass tier: every task ticked so far, across all chapters.
    pub fn tier(&self) -> usize {
        let before: usize = CHAPTERS[..self.chapter.min(CHAPTERS.len())]
            .iter()
            .map(|ch| ch.tasks.len())
            .sum();
        before + self.done_count()
    }

    /// The last chapter is done: the Commune is free.
    pub fn finale(&self) -> bool {
        self.complete() && self.chapter + 1 >= CHAPTERS.len()
    }

    /// Raise task progress to what's been seen; never lowers it.
    pub fn observe(&mut self, seen: impl Fn(Task) -> u32) {
        let tasks = self.current().tasks;
        self.progress.resize(tasks.len(), 0);
        for (i, &t) in tasks.iter().enumerate() {
            let v = seen(t).min(t.need());
            if v > self.progress[i] {
                self.progress[i] = v;
            }
        }
    }
}

/// Tiers on the Heaven Pass (one per task in the whole campaign). The top
/// tier is heaven.
pub fn heaven_tiers() -> usize {
    CHAPTERS.iter().map(|ch| ch.tasks.len()).sum()
}

/// Server only: counts of things that happened this chapter (the rest is read
/// straight from the other features' state).
#[derive(Resource, Default, Debug)]
pub struct Tally {
    pub mushrooms: u32,
    pub hide_seek: u32,
    pub duels: u32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Campaign>()
        .add_observer(next_chapter)
        .add_observer(count_mushroom)
        .add_observer(count_duel)
        .add_systems(OnEnter(ServerState::Running), spawn_campaign)
        .add_systems(
            Update,
            watch_the_world.run_if(in_state(ServerState::Running)),
        );
}

fn spawn_campaign(mut commands: Commands) {
    // Fresh counters every session (teardown despawns bump the old ones).
    commands.insert_resource(Tally::default());
    commands.spawn((Campaign::default(), Replicated));
}

/// A mushroom only ever disappears by being eaten.
fn count_mushroom(_ev: On<Remove<Mushroom>>, tally: Option<ResMut<Tally>>) {
    if let Some(mut t) = tally {
        t.mushrooms += 1;
    }
}

/// Only duels fought to a winner count, not ones called off.
fn count_duel(_ev: On<super::duel::DuelFought>, tally: Option<ResMut<Tally>>) {
    if let Some(mut t) = tally {
        t.duels += 1;
    }
}

/// A new round is a new chapter (if this one was finished).
fn next_chapter(
    _reset: On<RoundReset>,
    tally: Option<ResMut<Tally>>,
    mut campaign: Query<&mut Campaign>,
) {
    if let Some(mut t) = tally {
        *t = Tally::default();
    }
    for mut c in &mut campaign {
        let chapter = if c.complete() {
            (c.chapter + 1) % CHAPTERS.len()
        } else {
            c.chapter
        };
        *c = Campaign {
            chapter,
            progress: Vec::new(),
            solo: c.solo,
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn watch_the_world(
    mut tally: ResMut<Tally>,
    mut was_results: Local<bool>,
    players: Query<&Player>,
    pets: Query<&Pet>,
    doors: Query<&Door>,
    totems: Query<&Totem>,
    tenancy: Query<&Tenancy>,
    halt: Query<&Halt>,
    tears: Query<&Transformation>,
    hide_seek: Query<&HideSeek>,
    mut campaign: Query<&mut Campaign>,
) {
    // Games that just ended.
    // A hide-and-seek game is played once it reaches its results.
    let results = hide_seek.iter().any(|g| g.phase == Phase::Results);
    if results && !*was_results {
        tally.hide_seek += 1;
    }
    *was_results = results;

    let player_count = players.iter().count();
    let adopted: Vec<Vec2> = pets
        .iter()
        .filter(|p| p.owner.is_some())
        .map(|p| p.at)
        .collect();
    let want_pets = player_count.clamp(1, 4);
    let pack = adopted.len() >= want_pets && biggest_pack(&adopted) == adopted.len();
    let door_open = doors.iter().any(|d| d.is_open());
    let fire = totems
        .iter()
        .any(|t| t.magic == Magic::Fire && t.holder.is_some());
    let landlord = tenancy.iter().any(|t| t.act == Act3::Won);
    let escapes = halt.iter().map(|h| h.escapes).max().unwrap_or(0);
    let used: u32 = tears.iter().map(|t| t.0).sum();
    let flag = |b: bool| u32::from(b);

    let Ok(mut c) = campaign.single_mut() else {
        return;
    };
    let mut next = c.clone();
    next.solo = player_count < 2;
    next.observe(|t| match t {
        Task::Pack => flag(pack),
        Task::Mushroom(_) => tally.mushrooms,
        Task::VaultDoor => flag(door_open),
        Task::FireTotem => flag(fire),
        Task::Halt(_) => escapes,
        Task::Tears(_) => used,
        Task::HideSeek(_) => tally.hide_seek,
        Task::Duel(_) => tally.duels,
        Task::Landlord => flag(landlord),
    });
    if *c != next {
        *c = next;
    }
}

/// The checklist, one line per task.
pub fn checklist(c: &Campaign, landlord: Option<&str>) -> String {
    let ch = c.current();
    let mut s = format!(
        "CHAPTER {}/{}: {}",
        c.chapter + 1,
        CHAPTERS.len(),
        ch.name.to_uppercase()
    );
    for (i, &t) in ch.tasks.iter().enumerate() {
        let mark = if c.waived(t) {
            "[-]"
        } else if c.task_done(i) {
            "[x]"
        } else {
            "[  ]"
        };
        s.push_str(&format!("\n{mark} {}", t.label()));
        if c.waived(t) {
            s.push_str("  (needs 2+ players)");
        } else if t.need() > 1 {
            s.push_str(&format!("  {}/{}", c.got(i), t.need()));
        }
        if t == Task::Landlord
            && !c.task_done(i)
            && let Some(act) = landlord
        {
            s.push_str(&format!("\n      now: {act}"));
        }
    }
    s
}

/// The big line at the top of the screen.
pub fn goal_line(c: &Campaign) -> String {
    format!(
        "THE GOAL: {GOAL}\nHeaven Pass tier {}/{}  |  chapter {}/{}  |  {}/{} done",
        c.tier(),
        heaven_tiers(),
        c.chapter + 1,
        CHAPTERS.len(),
        c.done_count(),
        c.current().tasks.len()
    )
}

/// The ending, after chapter five.
pub fn ending() -> String {
    "THE COMMUNE IS FREE! YOU GOT INTO HEAVEN!\n\
     The landlord is evicted for good and the pearly gates swing open for everyone.\n\
     HALT sulks. The goat is grudgingly impressed. Heaven Pass: top tier.\n\n\
     Thanks for playing all five chapters. Special thanks to just meat and cheese,\n\
     who gooned through the entire campaign on a second monitor. Bad. Oh so bad. Baddie bad.\n\n\
     Press [Enter] to start again from Chapter 1"
        .to_string()
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            (show_checklist, chapter_banner, tier_toast).run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct ChecklistText;

#[derive(Component)]
struct ChapterBanner;

/// How long the "CHAPTER N" card stays up when a chapter starts.
const BANNER_SECS: f32 = 9.0;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        ChecklistText,
        TextFont::from_font_size(14.0),
        TextColor(crate::hud::TEXT),
        crate::hud::slot(crate::hud::HudSlot::TopLeft),
        crate::hud::First,
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        ChapterBanner,
        TextFont::from_font_size(30.0),
        TextColor(crate::hud::ACCENT),
        TextLayout::justify(Justify::Center),
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
}

fn show_checklist(
    campaign: Query<&Campaign>,
    tenancy: Query<&Tenancy>,
    mut text: Query<&mut Text, With<ChecklistText>>,
) {
    let Ok(c) = campaign.single() else { return };
    let act = tenancy.single().ok().map(|t| match t.act {
        Act3::Legal => "Act I, keep up with the rent",
        Act3::Crime => "Act II, take it back",
        Act3::Boss => "Act III, the boss fight",
        Act3::Won => "defeated",
    });
    let want = checklist(c, act);
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

fn chapter_banner(
    time: Res<Time>,
    campaign: Query<&Campaign>,
    mut shown: Local<Option<usize>>,
    mut left: Local<f32>,
    mut banner: Query<(&mut Text, &mut Visibility), With<ChapterBanner>>,
) {
    let Ok(c) = campaign.single() else { return };
    let Ok((mut text, mut vis)) = banner.single_mut() else {
        return;
    };
    if *shown != Some(c.chapter) {
        *shown = Some(c.chapter);
        *left = BANNER_SECS;
        let ch = c.current();
        text.0 = format!(
            "CHAPTER {}: {}\n{}\n\nTHE GOAL: {GOAL}",
            c.chapter + 1,
            ch.name.to_uppercase(),
            ch.intro
        );
    }
    *left -= time.delta_secs();
    vis.set_if_neq(if *left > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
}

/// "Tier N unlocked" whenever the group ticks a task.
fn tier_toast(
    campaign: Query<&Campaign>,
    mut last: Local<Option<usize>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let Ok(c) = campaign.single() else { return };
    let tier = c.tier();
    let up = last.is_some_and(|l| tier > l);
    *last = Some(tier);
    if !up {
        return;
    }
    if let Some(mut toast) = toast {
        let total = heaven_tiers();
        if tier >= total {
            toast.show("HEAVEN PASS: TOP TIER! The gates of heaven open");
        } else {
            toast.show(format!(
                "Heaven Pass tier {tier}/{total} unlocked! {} to heaven",
                total - tier
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapters_end_with_the_landlord_and_ask_for_more() {
        for ch in &CHAPTERS {
            assert_eq!(ch.tasks.last(), Some(&Task::Landlord), "{}", ch.name);
        }
        let total = |ch: &Chapter| ch.tasks.iter().map(|t| t.need()).sum::<u32>();
        assert!(total(&CHAPTERS[4]) > total(&CHAPTERS[0]));
    }

    #[test]
    fn ticks_never_untick_and_solo_waives_friend_tasks() {
        let mut c = Campaign {
            chapter: 1,
            ..default()
        };
        c.observe(|t| u32::from(t == Task::VaultDoor));
        assert!(c.task_done(0));
        c.observe(|_| 0);
        assert!(c.task_done(0), "the door closing doesn't untick it");
        assert!(!c.complete());
        c.solo = true;
        c.observe(|t| u32::from(matches!(t, Task::Halt(_) | Task::Landlord)));
        assert!(c.complete(), "hide and seek waived when alone");
        assert!(!c.finale());
        assert!(checklist(&c, None).contains("[-] Play a round of hide and seek"));
    }

    #[test]
    fn the_last_chapter_is_the_finale() {
        let mut c = Campaign {
            chapter: CHAPTERS.len() - 1,
            ..default()
        };
        assert_eq!(c.tier(), heaven_tiers() - CHAPTERS[4].tasks.len());
        c.observe(|t| t.need());
        assert!(c.finale());
        assert_eq!(c.tier(), heaven_tiers(), "the top tier is heaven");
        assert!(ending().contains("just meat and cheese"));
        assert!(ending().contains("HEAVEN"));
    }
}
