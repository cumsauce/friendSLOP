//! Saves (#424, bben: "lock down the progress states and create saves so we
//! can update and continue where we left off").
//!
//! The progress state is the campaign: which chapter you're on, what's ticked
//! in it, and which round it is. The host keeps one save file
//! (`campaign_save.json` in the config folder, `localStorage` in the browser)
//! and writes it every time that state changes. Hosting again (after closing
//! the game or updating it) picks the campaign up at the same chapter with the
//! same ticks. Joiners play in the host's save. Finishing the finale wraps the
//! campaign back to chapter one, and that is saved too.
//!
//! The file is versioned so a later change to the campaign can read old saves;
//! a save that doesn't fit the chapters any more (they were edited) is clamped.
//! It's only on when the game inserts [`SaveGames`] (the real game does,
//! headless tests don't, so they never touch your save).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::features::campaign::{CHAPTERS, Campaign};
use crate::features::objective::Objective;

/// The save's file name (next to the other local settings).
pub const FILE: &str = "campaign_save.json";
/// Bump when [`Save`] changes shape, and teach [`Save::read`] the old one.
pub const VERSION: u32 = 1;

/// Saving is on (the game inserts this; tests leave it out).
#[derive(Resource, Default)]
pub struct SaveGames;

/// What a save holds: the progress state, nothing else.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Save {
    pub version: u32,
    pub chapter: usize,
    pub progress: Vec<u32>,
    pub round: u32,
}

impl Save {
    pub fn of(c: &Campaign, o: &Objective) -> Self {
        Save {
            version: VERSION,
            chapter: c.chapter,
            progress: c.progress.clone(),
            round: o.round,
        }
    }

    pub fn write(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// A save from text, fitted to `chapters` (each one's task count). `None`
    /// for an unreadable or newer save.
    pub fn read(text: &str, chapters: &[usize]) -> Option<Self> {
        let mut s: Save = serde_json::from_str(text).ok()?;
        if s.version > VERSION || chapters.is_empty() {
            return None;
        }
        if s.chapter >= chapters.len() {
            s.chapter = chapters.len() - 1;
            s.progress.clear();
        }
        s.progress.truncate(chapters[s.chapter]);
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

/// Each time you host, the moment the campaign exists: put the save back.
fn load(mut campaign: Query<&mut Campaign, Added<Campaign>>, mut objective: Query<&mut Objective>) {
    let Ok(mut c) = campaign.single_mut() else {
        return;
    };
    let counts: Vec<usize> = CHAPTERS.get().iter().map(|ch| ch.tasks.len()).collect();
    let Some(s) = crate::settings::load_local(FILE).and_then(|t| Save::read(&t, &counts)) else {
        return;
    };
    info!("continuing the saved campaign: chapter {}", s.chapter + 1);
    c.chapter = s.chapter;
    c.progress = s.progress;
    for mut o in &mut objective {
        o.round = s.round;
    }
}

/// Write the save whenever the progress state changes.
fn store(campaign: Query<&Campaign>, objective: Query<&Objective>, mut last: Local<Option<Save>>) {
    let (Ok(c), Ok(o)) = (campaign.single(), objective.single()) else {
        return;
    };
    let now = Save::of(c, o);
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
    fn round_trips_and_fits_the_chapters() {
        let s = Save {
            version: VERSION,
            chapter: 2,
            progress: vec![1, 0, 3, 9],
            round: 4,
        };
        assert_eq!(Save::read(&s.write(), &[3, 3, 4]), Some(s.clone()));
        let fitted = Save::read(&s.write(), &[3, 3, 2]).unwrap_or_default();
        assert_eq!(fitted.progress, vec![1, 0]);
        let past = Save::read(&s.write(), &[3]).unwrap_or_default();
        assert_eq!((past.chapter, past.progress.len()), (0, 0));
        assert!(Save::read("nonsense", &[3]).is_none());
        let newer = Save {
            version: VERSION + 1,
            ..s
        };
        assert!(Save::read(&newer.write(), &[3, 3, 4]).is_none());
    }
}
