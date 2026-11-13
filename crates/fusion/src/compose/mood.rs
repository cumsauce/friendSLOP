//! Moods by name: `moods.ron` lists them (with their tone and flags), the
//! songs in `songs/` say whose they are, and its rules say where each plays.

use std::sync::RwLock;

use super::score::{style, table};
use super::{Tone, score};

/// What the game wants the music to say: a mood, by name (interned, like
/// [`crate::inst::Inst`]: the same name is always the same `Mood`, whether
/// or not the score lists it). Cheap to copy and compare; `{:?}` prints its
/// name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mood(u16);

/// Every mood name ever asked for, in order: a [`Mood`] is an index here.
static NAMES: RwLock<Vec<&'static str>> = RwLock::new(Vec::new());

/// A mood's flags, from its `moods.ron` entry (see the file's header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoodFlags {
    /// Cuts in at the next bar line instead of after a transition bar.
    pub urgent: bool,
    /// The director switches to it at once (no 2 s wait to be sure).
    pub instant: bool,
    /// The player's movement and company nudge its intensity.
    pub wandering: bool,
    /// Holds the mood before it (the form's hold section, at its tempo).
    pub hold: bool,
    /// What plays at start and when nothing else is asked for.
    pub fallback: bool,
    /// Victory cadences play in it.
    pub victory: bool,
    /// Climbing lifts its key.
    pub lift: bool,
}

impl MoodFlags {
    /// A mood the score doesn't list.
    pub const NONE: MoodFlags = MoodFlags {
        urgent: false,
        instant: false,
        wandering: false,
        hold: false,
        fallback: false,
        victory: true,
        lift: true,
    };
}

impl Mood {
    /// No mood (what plays when the score is missing: silence, in time).
    pub const NONE: Mood = Mood(u16::MAX);

    /// The mood called `name`.
    pub fn named(name: &str) -> Mood {
        if let Ok(names) = NAMES.read()
            && let Some(i) = names.iter().position(|n| *n == name)
        {
            return Mood(i as u16);
        }
        let Ok(mut names) = NAMES.write() else {
            return Mood::NONE;
        };
        if let Some(i) = names.iter().position(|n| *n == name) {
            return Mood(i as u16);
        }
        if names.len() >= u16::MAX as usize {
            return Mood::NONE;
        }
        names.push(Box::leak(name.to_owned().into_boxed_str()));
        Mood(names.len() as u16 - 1)
    }

    /// Its name ("?" for none).
    pub fn name(self) -> &'static str {
        NAMES
            .read()
            .ok()
            .and_then(|n| n.get(self.0 as usize).copied())
            .unwrap_or("?")
    }

    /// Its id, for passing through an atomic ([`Mood::from_index`] undoes
    /// it). Stable for the life of the process.
    pub fn index(self) -> u16 {
        self.0
    }

    /// The mood with id `i` (the fallback for one never named).
    pub fn from_index(i: u16) -> Mood {
        let known = NAMES.read().is_ok_and(|n| (i as usize) < n.len());
        if known { Mood(i) } else { Mood::default() }
    }

    /// Every mood in the score, in its order.
    pub fn all() -> Vec<Mood> {
        table().moods.iter().map(|m| m.mood).collect()
    }

    /// Look one up by name, ignoring case (tools).
    pub fn find(name: &str) -> Option<Mood> {
        Mood::all()
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))
    }

    /// Its flags (see [`MoodFlags`]).
    pub fn flags(self) -> MoodFlags {
        table().get(self).map_or(MoodFlags::NONE, |m| m.flags)
    }

    pub fn urgent(self) -> bool {
        self.flags().urgent
    }

    pub fn instant(self) -> bool {
        self.flags().instant
    }

    pub fn wandering(self) -> bool {
        self.flags().wandering
    }

    pub fn hold(self) -> bool {
        self.flags().hold
    }

    /// The mood's colour and feel (see [`Tone`]).
    pub fn tone(self) -> Tone {
        table().get(self).map_or(Tone::NEUTRAL, |ms| ms.tone)
    }

    /// The intensity this mood sits at with nothing else going on.
    pub fn base_intensity(self) -> f32 {
        style(self).intensity
    }

    /// Make-up gain (dB) so calm, sparse moods aren't a cliff quieter
    /// than busy ones when you walk between areas.
    pub fn level_db(self) -> f32 {
        table().get(self).map_or(0.0, |ms| ms.level_db)
    }
}

impl Default for Mood {
    /// The score's fallback mood (its first if none says so).
    fn default() -> Self {
        let t = score::table();
        t.moods
            .iter()
            .find(|m| m.flags.fallback)
            .or(t.moods.first())
            .map_or(Mood::NONE, |m| m.mood)
    }
}

impl std::fmt::Debug for Mood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl serde::Serialize for Mood {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

/// Reads a name (`"Village"`).
impl<'de> serde::Deserialize<'de> for Mood {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Name;
        impl serde::de::Visitor<'_> for Name {
            type Value = Mood;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a mood name")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Mood, E> {
                Ok(Mood::named(v))
            }
        }
        d.deserialize_str(Name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_intern_and_print() {
        let a = Mood::named("Village");
        assert_eq!(a, Mood::named("Village"));
        assert_eq!(format!("{a:?}"), "Village");
        assert_eq!(Mood::from_index(a.index()), a);
        assert_eq!(Mood::from_index(u16::MAX - 1), Mood::default());
        assert!(Mood::default().flags().fallback);
        assert!(Mood::named("Still").hold());
        assert!(!Mood::named("Still").flags().victory);
        assert!(Mood::named("Village").wandering());
    }
}
