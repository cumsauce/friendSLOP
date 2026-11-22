//! Quests: the shared vocabulary (the quest system replaces the chapter
//! checklist). NPCs scattered round the world give quests through a dialogue;
//! accepting one puts it on the party's one shared [`QuestLog`]; features
//! report what happened with a server-side [`QuestSignal`], which moves the
//! log on. Finishing every quest gets everyone into heaven.
//!
//! It lives here, under every game crate, so any feature can report progress
//! (`commands.trigger(QuestSignal::new("factory_bump"))`) or switch itself on
//! and off with a quest (`.run_if(quests::active("landlord"))`,
//! `.run_if(quests::enabled("<flag>"))`) without importing the story crate,
//! which owns the log (`story::features::quests`).
//!
//! What the quests are, who gives them and what they say is data:
//! `data/story/quests.ron`, `data/story/quest_givers.ron` and
//! `data/text/quest_dialogue.ron`.

use std::collections::BTreeMap;
use std::sync::RwLock;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- the data

/// Every quest (`data/story/quests.ron`).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct QuestBook {
    /// KoriCoins™ every player gets when a quest is done (unless the quest
    /// or its variant says otherwise).
    pub reward_kori: u32,
    pub quests: Vec<QuestDef>,
}

/// One quest.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct QuestDef {
    /// What code and the other files call it.
    pub id: String,
    /// What players call it.
    pub title: String,
    /// The giver (`quest_givers.ron`) who hands it out.
    pub giver: String,
    /// One line for the journal: what it's about.
    pub summary: String,
    /// Quests that have to be done before this one is offered.
    #[serde(default)]
    pub requires: Vec<String>,
    /// How many other quests have to be done before this one is offered.
    #[serde(default)]
    pub requires_count: u32,
    /// What to do, in order.
    pub steps: Vec<Step>,
    /// Signals that fail it while it's active (talk to the giver again to retry).
    #[serde(default)]
    pub fails_on: Vec<String>,
    /// Flags that are on while it's active ([`enabled`]).
    #[serde(default)]
    pub enables: Vec<String>,
    /// KoriCoins™ when it's done, instead of the book's `reward_kori`.
    #[serde(default)]
    pub reward: Option<u32>,
    /// Can't be done alone: waived (counts as done) while you're playing solo.
    #[serde(default)]
    pub needs_friends: bool,
    /// Other ways to do it, picked in the dialogue.
    #[serde(default)]
    pub variants: Vec<Variant>,
}

/// One step of a quest.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Step {
    /// What to do ("{action}" names a key, filled per device).
    pub text: String,
    /// The signals that count for it (any of them).
    pub signals: Vec<String>,
    /// How many it takes.
    #[serde(default = "one")]
    pub need: u32,
}

fn one() -> u32 {
    1
}

/// Another way to do a quest, picked in its giver's dialogue.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Variant {
    pub id: String,
    /// Its own steps, instead of the quest's.
    #[serde(default)]
    pub steps: Option<Vec<Step>>,
    /// Its own reward, instead of the quest's.
    #[serde(default)]
    pub reward: Option<u32>,
}

impl QuestBook {
    pub fn get(&self, id: &str) -> Option<&QuestDef> {
        self.quests.iter().find(|q| q.id == id)
    }

    /// The KoriCoins™ quest `id` (done the `variant` way) pays.
    pub fn reward(&self, id: &str, variant: Option<&str>) -> u32 {
        let Some(q) = self.get(id) else { return 0 };
        q.variant(variant)
            .and_then(|v| v.reward)
            .or(q.reward)
            .unwrap_or(self.reward_kori)
    }
}

impl QuestDef {
    pub fn variant(&self, id: Option<&str>) -> Option<&Variant> {
        let id = id?;
        self.variants.iter().find(|v| v.id == id)
    }

    /// The steps, done the `variant` way.
    pub fn steps_for(&self, variant: Option<&str>) -> &[Step] {
        self.variant(variant)
            .and_then(|v| v.steps.as_deref())
            .unwrap_or(&self.steps)
    }
}

pub static QUESTS: friendslop_data::Data<QuestBook> =
    friendslop_data::Data::ron("data/story/quests.ron");

/// Someone who gives quests (`data/story/quest_givers.ron`).
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct GiverDef {
    /// What the quests and the dialogue call them.
    pub id: String,
    /// The site (`data/base/sites.ron`) they're at: the journal says where.
    pub site: String,
    /// Where they stand.
    pub place: GiverPlace,
    /// Which way they face (degrees, 0 = looking down -z).
    #[serde(default)]
    pub yaw: f32,
    /// The area (`data/base/areas.ron`) they're drawn in.
    pub area: String,
    /// The hours (0..24) they're out, e.g. (21.0, 4.0) for the night. All day without.
    #[serde(default)]
    pub hours: Option<(f32, f32)>,
    /// The look of their body (a seed for the commune-person look). Ignored
    /// when `body` is false.
    #[serde(default)]
    pub look: u64,
    /// A body of their own. False for a character who's already there
    /// (Abuela, Sewer Paul, Jegg): only the marker is added, over `place`,
    /// and clicking them opens the dialogue too.
    #[serde(default = "yes")]
    pub body: bool,
    /// How high the marker floats over `place` (m).
    #[serde(default = "marker_height")]
    pub marker_height: f32,
}

fn yes() -> bool {
    true
}

fn marker_height() -> f32 {
    2.35
}

/// Where a giver stands.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub enum GiverPlace {
    /// A `Spot` on the commune map (on the ground), and how far from it (x, z).
    Spot(String, (f32, f32)),
    /// A point a feature names with [`provide_anchor`] (New York's places
    /// move with their own tuning), and how far from it (x, y, z).
    Anchor(String, (f32, f32, f32)),
}

impl Default for GiverPlace {
    fn default() -> Self {
        GiverPlace::Spot(String::new(), (0.0, 0.0))
    }
}

/// A function that says where a named place is (world, feet).
pub type AnchorFn = fn() -> Vec3;

static ANCHORS: RwLock<Vec<(&'static str, AnchorFn)>> = RwLock::new(Vec::new());

/// Name a point givers can stand at (call it from a protocol function).
pub fn provide_anchor(name: &'static str, f: AnchorFn) {
    if let Ok(mut a) = ANCHORS.write()
        && !a.iter().any(|(n, _)| *n == name)
    {
        a.push((name, f));
    }
}

/// Where the anchor `name` is, if a feature named it.
pub fn anchor(name: &str) -> Option<Vec3> {
    let a = ANCHORS.read().ok()?;
    a.iter().find(|(n, _)| *n == name).map(|(_, f)| f())
}

impl GiverPlace {
    /// Their feet (world). An unknown spot or anchor is the map's middle.
    pub fn at(&self) -> Vec3 {
        match self {
            GiverPlace::Spot(name, (dx, dz)) => {
                let p =
                    friendslop_world::places::spot(name).unwrap_or_default() + Vec2::new(*dx, *dz);
                Vec3::new(p.x, friendslop_world::terrain::height_at(p.x, p.y), p.y)
            }
            GiverPlace::Anchor(name, (dx, dy, dz)) => {
                anchor(name).unwrap_or_default() + Vec3::new(*dx, *dy, *dz)
            }
        }
    }
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Givers {
    pub givers: Vec<GiverDef>,
}

impl Givers {
    pub fn get(&self, id: &str) -> Option<&GiverDef> {
        self.givers.iter().find(|g| g.id == id)
    }
}

pub static GIVERS: friendslop_data::Data<Givers> =
    friendslop_data::Data::ron("data/story/quest_givers.ron");

impl GiverDef {
    /// Are they out at `hour`?
    pub fn out_at(&self, hour: f32) -> bool {
        self.hours
            .is_none_or(|h| crate::clock::within_hours(hour, h))
    }
}

// ---------------------------------------------------------------- dialogue

/// What every giver says (`data/text/quest_dialogue.ron`), by giver id.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Dialogues {
    pub givers: BTreeMap<String, GiverTalk>,
}

/// One giver's words.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct GiverTalk {
    /// Their name, over what they say.
    pub name: String,
    /// When they've nothing (left) to give.
    pub idle: String,
    /// Per quest they give.
    pub quests: BTreeMap<String, QuestTalk>,
}

/// What a giver says about one quest.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct QuestTalk {
    /// The scene the offer starts at.
    pub start: String,
    pub scenes: BTreeMap<String, Scene>,
    /// While it's on your list.
    pub busy: String,
    /// Once it's done.
    pub done: String,
    /// After it failed (talking again offers it from `start`).
    #[serde(default)]
    pub failed: String,
}

/// One thing a giver says, and what you can say back. No choices: it's the
/// last thing they say, and the talk is over.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Scene {
    pub line: String,
    #[serde(default)]
    pub choices: Vec<Choice>,
}

/// One thing you can say.
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Choice {
    pub text: String,
    /// The scene it leads to (none: the talk ends).
    #[serde(default)]
    pub next: Option<String>,
    /// What saying it does.
    #[serde(default)]
    pub effect: Effect,
}

/// What a choice does to the quest.
#[derive(Deserialize, Serialize, Default, Debug, Clone, PartialEq)]
pub enum Effect {
    /// Nothing; the talk goes on.
    #[default]
    None,
    /// Take the quest on (the plain way, or a variant).
    Accept(Option<String>),
}

pub static DIALOGUE: friendslop_data::Data<Dialogues> =
    friendslop_data::Data::ron("data/text/quest_dialogue.ron");

// ---------------------------------------------------------------- the log

/// Where one quest stands.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub enum QuestState {
    /// Not offered yet (its `requires` aren't done).
    #[default]
    Locked,
    /// Its giver will offer it.
    Available,
    /// On the list: at `step`, with `count` of its `need`.
    Active {
        step: u8,
        count: u32,
        variant: Option<String>,
    },
    Done,
    /// It went wrong; its giver offers it again.
    Failed,
}

/// Server -> everyone: the party's quest log. One for everyone (the shared goal).
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct QuestLog {
    pub quests: BTreeMap<String, QuestState>,
    /// Fewer than two players: quests that need friends are waived.
    pub solo: bool,
}

/// What a signal or an accept did to a quest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Change {
    Accepted,
    /// Moved on to step n (or counted towards it).
    Step(u8),
    Done,
    Failed,
    Unlocked,
}

impl QuestLog {
    /// A new log: every quest locked, then the ones with nothing required
    /// unlocked.
    pub fn fresh(book: &QuestBook) -> Self {
        let mut log = QuestLog::default();
        log.fit(book);
        log.unlock(book);
        log
    }

    /// Add quests the book has and the log doesn't (the book was edited),
    /// and drop ones it doesn't have any more.
    pub fn fit(&mut self, book: &QuestBook) {
        self.quests
            .retain(|id, _| book.quests.iter().any(|q| q.id == *id));
        for q in &book.quests {
            self.quests.entry(q.id.clone()).or_default();
        }
    }

    pub fn state(&self, id: &str) -> QuestState {
        self.quests.get(id).cloned().unwrap_or_default()
    }

    pub fn is_active(&self, id: &str) -> bool {
        matches!(self.quests.get(id), Some(QuestState::Active { .. }))
    }

    pub fn is_done(&self, id: &str) -> bool {
        matches!(self.quests.get(id), Some(QuestState::Done))
    }

    /// Done, or waived while playing alone.
    pub fn counts_done(&self, book: &QuestBook, id: &str) -> bool {
        self.is_done(id) || (self.solo && book.get(id).is_some_and(|q| q.needs_friends))
    }

    pub fn waived(&self, q: &QuestDef) -> bool {
        self.solo && q.needs_friends && !self.is_done(&q.id)
    }

    /// Quests done (the Heaven Pass tier).
    pub fn done_count(&self, book: &QuestBook) -> usize {
        book.quests
            .iter()
            .filter(|q| self.counts_done(book, &q.id))
            .count()
    }

    /// Every quest is done: the Commune is free and heaven opens.
    pub fn finale(&self, book: &QuestBook) -> bool {
        !book.quests.is_empty() && self.done_count(book) == book.quests.len()
    }

    /// Can `id` be offered now? (Its requirements are done.)
    fn ready(&self, book: &QuestBook, q: &QuestDef) -> bool {
        let others = book
            .quests
            .iter()
            .filter(|o| o.id != q.id && self.is_done(&o.id))
            .count() as u32;
        q.requires.iter().all(|r| self.is_done(r)) && others >= q.requires_count
    }

    /// Offer every locked quest whose requirements are done. Returns them.
    pub fn unlock(&mut self, book: &QuestBook) -> Vec<String> {
        let mut out = Vec::new();
        for q in &book.quests {
            if self.state(&q.id) == QuestState::Locked && self.ready(book, q) {
                self.quests.insert(q.id.clone(), QuestState::Available);
                out.push(q.id.clone());
            }
        }
        out
    }

    /// Take quest `id` on (the `variant` way). False when it isn't on offer
    /// or the variant isn't one of its.
    pub fn accept(&mut self, book: &QuestBook, id: &str, variant: Option<&str>) -> bool {
        let Some(q) = book.get(id) else { return false };
        if !matches!(self.state(id), QuestState::Available | QuestState::Failed) {
            return false;
        }
        if variant.is_some() && q.variant(variant).is_none() {
            return false;
        }
        self.quests.insert(
            id.to_string(),
            QuestState::Active {
                step: 0,
                count: 0,
                variant: variant.map(str::to_string),
            },
        );
        true
    }

    /// Something happened (`key`, `amount` times): move every active quest
    /// that was waiting for it on. Returns what changed.
    pub fn signal(&mut self, book: &QuestBook, key: &str, amount: u32) -> Vec<(String, Change)> {
        let mut out = Vec::new();
        for q in &book.quests {
            let Some(QuestState::Active {
                step,
                count,
                variant,
            }) = self.quests.get(&q.id).cloned()
            else {
                continue;
            };
            if q.fails_on.iter().any(|f| f == key) {
                self.quests.insert(q.id.clone(), QuestState::Failed);
                out.push((q.id.clone(), Change::Failed));
                continue;
            }
            let steps = q.steps_for(variant.as_deref());
            let Some(s) = steps.get(step as usize) else {
                continue;
            };
            if !s.signals.iter().any(|k| k == key) {
                continue;
            }
            let count = count.saturating_add(amount.max(1));
            if count < s.need {
                self.quests.insert(
                    q.id.clone(),
                    QuestState::Active {
                        step,
                        count,
                        variant,
                    },
                );
                out.push((q.id.clone(), Change::Step(step)));
            } else if step as usize + 1 < steps.len() {
                self.quests.insert(
                    q.id.clone(),
                    QuestState::Active {
                        step: step + 1,
                        count: 0,
                        variant,
                    },
                );
                out.push((q.id.clone(), Change::Step(step + 1)));
            } else {
                self.quests.insert(q.id.clone(), QuestState::Done);
                out.push((q.id.clone(), Change::Done));
            }
        }
        if out.iter().any(|(_, c)| *c == Change::Done) {
            out.extend(
                self.unlock(book)
                    .into_iter()
                    .map(|id| (id, Change::Unlocked)),
            );
        }
        out
    }

    /// The step quest `id` is at, its count and need, while it's active.
    pub fn at_step<'a>(&self, book: &'a QuestBook, id: &str) -> Option<(&'a Step, u32)> {
        let QuestState::Active {
            step,
            count,
            variant,
        } = self.quests.get(id)?
        else {
            return None;
        };
        let q = book.get(id)?;
        q.steps_for(variant.as_deref())
            .get(*step as usize)
            .map(|s| (s, *count))
    }

    /// The active quests, in the book's order.
    pub fn active<'a>(&self, book: &'a QuestBook) -> Vec<&'a QuestDef> {
        book.quests
            .iter()
            .filter(|q| self.is_active(&q.id))
            .collect()
    }

    /// Is any active quest switching flag `flag` on?
    pub fn flag_on(&self, book: &QuestBook, flag: &str) -> bool {
        self.active(book)
            .iter()
            .any(|q| q.enables.iter().any(|f| f == flag))
    }
}

/// The quest a giver talks about now: the first of theirs that's on offer
/// (or failed), else the first that's active, else none (they're idle, or
/// everything of theirs is done: the last one done).
pub fn topic<'a>(book: &'a QuestBook, log: &QuestLog, giver: &str) -> Option<&'a QuestDef> {
    let theirs = || book.quests.iter().filter(move |q| q.giver == giver);
    theirs()
        .find(|q| matches!(log.state(&q.id), QuestState::Available | QuestState::Failed))
        .or_else(|| theirs().find(|q| log.is_active(&q.id)))
        .or_else(|| theirs().rfind(|q| log.is_done(&q.id)))
}

/// What a giver's marker shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Something to give: "!".
    Offer,
    /// One of theirs is on your list: "?".
    Busy,
    None,
}

pub fn mark(book: &QuestBook, log: &QuestLog, giver: &str) -> Mark {
    match topic(book, log, giver).map(|q| log.state(&q.id)) {
        Some(QuestState::Available | QuestState::Failed) => Mark::Offer,
        Some(QuestState::Active { .. }) => Mark::Busy,
        _ => Mark::None,
    }
}

// ---------------------------------------------------------------- signals

/// Server only: something a quest might be waiting for happened. Features
/// trigger it where they decide things (`commands.trigger(QuestSignal::new("burger"))`).
#[derive(Event, Clone, Debug, PartialEq)]
pub struct QuestSignal {
    pub key: String,
    pub amount: u32,
}

impl QuestSignal {
    pub fn new(key: impl Into<String>) -> Self {
        QuestSignal {
            key: key.into(),
            amount: 1,
        }
    }

    pub fn times(key: impl Into<String>, amount: u32) -> Self {
        QuestSignal {
            key: key.into(),
            amount,
        }
    }
}

/// Server only: quest `0` was just taken on. A feature whose quest it is
/// can reset itself for a fresh go (the landlord does).
#[derive(Event, Clone, Debug, PartialEq)]
pub struct QuestStarted(pub String);

// ---------------------------------------------------------------- gates

/// Run while quest `id` is on the list (or there's no log at all, so
/// headless tests that leave quests out keep running everything).
pub fn active(id: &'static str) -> impl FnMut(Query<&QuestLog>) -> bool + Clone {
    move |log: Query<&QuestLog>| log.iter().next().is_none_or(|l| l.is_active(id))
}

/// Run once quest `id` is done.
pub fn done(id: &'static str) -> impl FnMut(Query<&QuestLog>) -> bool + Clone {
    move |log: Query<&QuestLog>| log.iter().next().is_some_and(|l| l.is_done(id))
}

/// Run while an active quest switches `flag` on (or there's no log).
pub fn enabled(flag: &'static str) -> impl FnMut(Query<&QuestLog>) -> bool + Clone {
    move |log: Query<&QuestLog>| {
        log.iter()
            .next()
            .is_none_or(|l| l.flag_on(&QUESTS.get(), flag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(signal: &str, need: u32) -> Step {
        Step {
            text: signal.into(),
            signals: vec![signal.into()],
            need,
        }
    }

    fn book() -> QuestBook {
        QuestBook {
            reward_kori: 1,
            quests: vec![
                QuestDef {
                    id: "burgers".into(),
                    giver: "cashier".into(),
                    steps: vec![step("burger", 3)],
                    ..default()
                },
                QuestDef {
                    id: "date".into(),
                    giver: "roommate".into(),
                    steps: vec![Step {
                        text: "date".into(),
                        signals: vec!["second_date".into(), "wall_husband".into()],
                        need: 1,
                    }],
                    fails_on: vec!["caught".into()],
                    variants: vec![Variant {
                        id: "husband".into(),
                        steps: Some(vec![step("wall_husband", 1)]),
                        reward: Some(2),
                    }],
                    ..default()
                },
                QuestDef {
                    id: "landlord".into(),
                    giver: "organiser".into(),
                    requires_count: 1,
                    steps: vec![step("raise", 1), step("won", 1)],
                    enables: vec!["rent".into()],
                    ..default()
                },
                QuestDef {
                    id: "duels".into(),
                    giver: "kid".into(),
                    needs_friends: true,
                    steps: vec![step("duel", 1)],
                    ..default()
                },
            ],
        }
    }

    #[test]
    fn quests_unlock_accept_count_and_finish() {
        let b = book();
        let mut log = QuestLog::fresh(&b);
        assert_eq!(log.state("burgers"), QuestState::Available);
        assert_eq!(log.state("landlord"), QuestState::Locked);
        assert!(
            log.signal(&b, "burger", 1).is_empty(),
            "not on the list yet"
        );
        assert!(log.accept(&b, "burgers", None));
        assert!(!log.accept(&b, "burgers", None), "only once");
        log.signal(&b, "burger", 2);
        assert_eq!(log.at_step(&b, "burgers").map(|(_, n)| n), Some(2));
        let changes = log.signal(&b, "burger", 1);
        assert!(changes.contains(&("burgers".into(), Change::Done)));
        assert!(changes.contains(&("landlord".into(), Change::Unlocked)));
        assert!(log.accept(&b, "landlord", None));
        assert!(log.flag_on(&b, "rent"));
        log.signal(&b, "raise", 1);
        assert_eq!(
            log.at_step(&b, "landlord").map(|(s, _)| s.text.clone()),
            Some("won".into())
        );
    }

    #[test]
    fn variants_change_steps_and_rewards_and_failing_offers_it_again() {
        let b = book();
        let mut log = QuestLog::fresh(&b);
        assert!(!log.accept(&b, "date", Some("nope")));
        assert!(log.accept(&b, "date", Some("husband")));
        log.signal(&b, "second_date", 1);
        assert!(
            log.is_active("date"),
            "the husband way wants the top ending"
        );
        log.signal(&b, "caught", 1);
        assert_eq!(log.state("date"), QuestState::Failed);
        assert_eq!(mark(&b, &log, "roommate"), Mark::Offer);
        assert!(log.accept(&b, "date", None));
        assert_eq!(mark(&b, &log, "roommate"), Mark::Busy);
        log.signal(&b, "second_date", 1);
        assert!(log.is_done("date"));
        assert_eq!(mark(&b, &log, "roommate"), Mark::None);
        assert_eq!(b.reward("date", Some("husband")), 2);
        assert_eq!(b.reward("date", None), 1);
    }

    #[test]
    fn solo_waives_friend_quests_and_everything_done_is_the_finale() {
        let b = book();
        let mut log = QuestLog::fresh(&b);
        log.solo = true;
        for (id, sig) in [("burgers", "burger"), ("date", "second_date")] {
            log.accept(&b, id, None);
            log.signal(&b, sig, 3);
        }
        assert!(!log.finale(&b));
        log.accept(&b, "landlord", None);
        log.signal(&b, "raise", 1);
        log.signal(&b, "won", 1);
        assert_eq!(log.done_count(&b), 4, "duels are waived alone");
        assert!(log.finale(&b));
        log.solo = false;
        assert!(!log.finale(&b));
    }

    #[test]
    fn the_data_files_parse_and_hang_together() {
        let b = QUESTS.get();
        let givers = GIVERS.get();
        let talk = DIALOGUE.get();
        assert!(b.reward_kori >= 1);
        assert!(b.quests.len() >= 15, "{}", b.quests.len());
        for q in &b.quests {
            assert!(!q.steps.is_empty(), "{} has no steps", q.id);
            assert!(
                givers.get(&q.giver).is_some(),
                "{}: no giver {}",
                q.id,
                q.giver
            );
            for r in &q.requires {
                assert!(b.get(r).is_some(), "{} requires unknown {r}", q.id);
            }
            let Some(t) = talk.givers.get(&q.giver).and_then(|g| g.quests.get(&q.id)) else {
                panic!("{}: {} says nothing about it", q.id, q.giver);
            };
            check_talk(q, t);
        }
        for g in &givers.givers {
            assert!(talk.givers.contains_key(&g.id), "{} has no words", g.id);
        }
        let mut log = QuestLog::fresh(&b);
        assert!(
            b.quests
                .iter()
                .any(|q| log.state(&q.id) == QuestState::Available),
            "something is on offer from the start"
        );
        // Doing every quest (plain way, in book order, as they unlock) ends it.
        for _ in 0..b.quests.len() {
            for q in &b.quests {
                if log.accept(&b, &q.id, None) {
                    for s in &q.steps {
                        log.signal(&b, &s.signals[0], s.need);
                    }
                }
            }
        }
        assert!(log.finale(&b), "every quest can be done");
    }

    /// Every scene leads somewhere real, every path ends, and the quest
    /// (and each variant) can be accepted.
    fn check_talk(q: &QuestDef, t: &QuestTalk) {
        assert!(t.scenes.contains_key(&t.start), "{}: no start scene", q.id);
        let mut accepts = Vec::new();
        for (name, s) in &t.scenes {
            for c in &s.choices {
                if let Some(n) = &c.next {
                    assert!(t.scenes.contains_key(n), "{}/{name}: no scene {n}", q.id);
                }
                if let Effect::Accept(v) = &c.effect {
                    if let Some(v) = v {
                        assert!(q.variant(Some(v)).is_some(), "{}: no variant {v}", q.id);
                    }
                    accepts.push(v.clone());
                }
            }
        }
        assert!(accepts.contains(&None) || !accepts.is_empty(), "{}", q.id);
        for v in &q.variants {
            assert!(
                accepts.contains(&Some(v.id.clone())),
                "{}: variant {} can't be picked",
                q.id,
                v.id
            );
        }
        // Every path ends (no loops that never end): walk them all.
        fn walk(t: &QuestTalk, at: &str, depth: usize) {
            assert!(depth < 24, "the talk loops at {at}");
            let Some(s) = t.scenes.get(at) else { return };
            for c in &s.choices {
                if let Some(n) = &c.next {
                    walk(t, n, depth + 1);
                }
            }
        }
        walk(t, &t.start, 0);
        assert!(!t.busy.is_empty() && !t.done.is_empty(), "{}", q.id);
    }
}
