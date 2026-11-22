//! Saves (#424, bben: "lock down the progress states and create saves so we
//! can update and continue where we left off").
//!
//! The progress state is the quest log: where every quest stands, and which
//! round it is. The host keeps one save file (`campaign_save.json` in the
//! config folder, `localStorage` in the browser) and writes it every time that
//! state changes. Hosting again (after closing the game or updating it) picks
//! the quests up where they were. Joiners play in the host's save. Finishing
//! every quest starts a new campaign, and that is saved too.
//!
//! The file is versioned; a save from the chapter days (version 1) starts
//! over, and quests added or removed since are fitted to the book.
//! It's only on when the game inserts [`SaveGames`] (the real game does,
//! headless tests don't, so they never touch your save).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::features::objective::Objective;
use crate::features::quests::{QUESTS, QuestLog, QuestState};

/// The save's file name (next to the other local settings).
pub const FILE: &str = "campaign_save.json";
/// Bump when [`Save`] changes shape, and teach [`Save::read`] the old one.
/// 2: the quest log (version 1 held chapters; those saves start over).
pub const VERSION: u32 = 2;

/// Saving is on (the game inserts this; tests leave it out).
#[derive(Resource, Default)]
pub struct SaveGames;

/// What a save holds: the progress state, nothing else.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Save {
    pub version: u32,
    #[serde(default)]
    pub quests: BTreeMap<String, QuestState>,
    pub round: u32,
}

impl Save {
    pub fn of(log: &QuestLog, o: &Objective) -> Self {
        Save {
            version: VERSION,
            quests: log.quests.clone(),
            round: o.round,
        }
    }

    pub fn write(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// A save from text. `None` for an unreadable, older (chapters) or newer save.
    pub fn read(text: &str) -> Option<Self> {
        let mut s: Save = serde_json::from_str(text).ok()?;
        if s.version != VERSION {
            return None;
        }
        s.round = s.round.max(1);
        Some(s)
    }
}

pub fn protocol(app: &mut App) {
    app.add_systems(
        Update,
        (load, store)
            .chain()
            .run_if(in_state(ServerState::Running))
            .run_if(resource_exists::<SaveGames>),
    );
}

/// Each time you host, the moment the quest log exists: put the save back.
fn load(mut log: Query<&mut QuestLog, Added<QuestLog>>, mut objective: Query<&mut Objective>) {
    let Ok(mut l) = log.single_mut() else {
        return;
    };
    let Some(s) = crate::settings::load_local(FILE).and_then(|t| Save::read(&t)) else {
        return;
    };
    info!("continuing the saved quests: round {}", s.round);
    l.quests = s.quests;
    // Quests added or removed since are fitted next frame (`quests::fit_log`).
    l.fit(&QUESTS.get());
    for mut o in &mut objective {
        o.round = s.round;
    }
}

/// Write the save whenever the quest log changes.
fn store(log: Query<&QuestLog>, objective: Query<&Objective>, mut last: Local<Option<Save>>) {
    let (Ok(l), Ok(o)) = (log.single(), objective.single()) else {
        return;
    };
    let now = Save::of(l, o);
    if last.as_ref() == Some(&now) {
        return;
    }
    // The first frame only learns what's there (it was just loaded).
    if last.is_some() {
        crate::settings::save_local(FILE, &now.write());
    }
    *last = Some(now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_drops_old_saves() {
        let mut quests = BTreeMap::new();
        quests.insert("burgers".to_string(), QuestState::Done);
        quests.insert(
            "landlord".to_string(),
            QuestState::Active {
                step: 1,
                count: 2,
                variant: None,
            },
        );
        let s = Save {
            version: VERSION,
            quests,
            round: 4,
        };
        assert_eq!(Save::read(&s.write()), Some(s.clone()));
        assert!(Save::read("nonsense").is_none());
        let chapters = r#"{"version":1,"chapter":2,"progress":[1,0],"round":3}"#;
        assert!(Save::read(chapters).is_none(), "chapter saves start over");
        let newer = Save {
            version: VERSION + 1,
            ..s
        };
        assert!(Save::read(&newer.write()).is_none());
    }
}
