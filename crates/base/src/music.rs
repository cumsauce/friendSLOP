//! What the soundtrack listens to: named numbers ("signals") that any
//! feature sets from its own systems, and cues (counters) it bumps. The
//! music (`features::fusion`, in side) reads only these, the camera and the
//! zones; the score's rules (`assets/data/music/moods.ron`) say which mood
//! each combination plays, so the music never names a feature.
//!
//! ```ignore
//! fn music(mut signals: ResMut<MusicSignals>, duel: Query<&Duelist>) {
//!     signals.flag("duel", !duel.is_empty());
//! }
//! app.add_systems(Update, music.in_set(MusicSignalSet));
//! ```
//!
//! Local only (every peer hears its own take): nothing here is replicated.

use std::collections::BTreeMap;

use bevy::prelude::*;

/// The signals and cues (see the module docs). Unset signals read 0.
#[derive(Resource, Default, Debug, Clone)]
pub struct MusicSignals {
    values: BTreeMap<String, f32>,
    cues: BTreeMap<String, u32>,
}

impl MusicSignals {
    pub fn set(&mut self, name: &str, value: f32) {
        match self.values.get_mut(name) {
            Some(v) => *v = value,
            None => {
                self.values.insert(name.to_owned(), value);
            }
        }
    }

    /// A yes/no signal (1 or 0).
    pub fn flag(&mut self, name: &str, on: bool) {
        self.set(name, f32::from(u8::from(on)));
    }

    pub fn get(&self, name: &str) -> f32 {
        self.values.get(name).copied().unwrap_or(0.0)
    }

    /// Something happened (a victory...): bumps the cue's counter.
    pub fn cue(&mut self, name: &str) {
        *self.cues.entry(name.to_owned()).or_default() += 1;
    }

    /// How many times the cue has fired.
    pub fn cues(&self, name: &str) -> u32 {
        self.cues.get(name).copied().unwrap_or(0)
    }

    /// Every signal set so far.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }
}

/// Features publish their signals in this set; the music reads them after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MusicSignalSet;

/// Where the camera is (what the place signals measure from).
pub fn camera_at(camera: &Query<&GlobalTransform, With<Camera3d>>) -> Option<Vec3> {
    camera.iter().next().map(|g| g.translation())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals_and_cues() {
        let mut s = MusicSignals::default();
        assert_eq!(s.get("duel"), 0.0);
        s.flag("duel", true);
        s.set("duel_lives", 2.0);
        assert_eq!(s.get("duel"), 1.0);
        s.flag("duel", false);
        assert_eq!(s.get("duel"), 0.0);
        s.cue("victory");
        s.cue("victory");
        assert_eq!(s.cues("victory"), 2);
        assert_eq!(s.names().count(), 2);
    }
}
