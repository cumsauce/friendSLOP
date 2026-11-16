//! Per-song mix: a song file's `mix:` changes some buses' settings for that
//! song only (any field, partially: just `bass.trim`, just `pad.chorus`),
//! and its `mix_auto:` lanes move bus settings over time. Every other song
//! plays mix.ron as written.

use std::collections::BTreeMap;

use super::params::BusLaneFile;
use crate::synth::{BUSES, Bus, BusMix, BusOverride, Mix};

/// A song's own mix (both parts optional in the song file).
#[derive(Debug, Clone, Default)]
pub struct SongMix {
    /// Overrides by bus name ([`Bus::NAMES`]).
    pub buses: BTreeMap<String, BusOverride>,
    /// Bus automation lanes.
    pub auto: Vec<BusLaneFile>,
}

/// The mix for a song with no changes of its own.
pub static NO_MIX: SongMix = SongMix {
    buses: BTreeMap::new(),
    auto: Vec::new(),
};

/// Every bus's settings for a song: mix.ron's (a bus it doesn't list
/// passes straight through), with the song's overrides laid on top.
pub fn merged(file: Option<&Mix>, song: &SongMix) -> [BusMix; BUSES] {
    std::array::from_fn(|b| {
        let name = Bus::NAMES[b];
        let mut m = file
            .and_then(|f| f.buses.get(name))
            .cloned()
            .unwrap_or_default();
        if let Some(o) = song.buses.get(name) {
            m.merge(o);
        }
        m
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::Eq;

    fn mix(src: &str) -> Mix {
        ron::from_str(src).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn an_override_changes_only_what_it_writes() {
        let file = mix("(buses: {
            \"bass\": (trim: 1.5, pump: 0.3, eq: [Hp(30.0, 0.7)], autopan: None, chorus: None),
            \"pad\": (trim: 0.8, pump: 0.4, eq: [], autopan: Some((0.5, 0.2)), chorus: Some((0.4, 8.0, 3.0, 0.5))),
        })");
        let buses: BTreeMap<String, BusOverride> = ron::from_str(
            "{ \"bass\": (pump: 0.6), \"pad\": (chorus: None), \"keys\": (trim: 0.5) }",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let song = SongMix {
            buses,
            auto: Vec::new(),
        };
        let m = merged(Some(&file), &song);
        let (bass, pad, keys) = (
            &m[Bus::Bass as usize],
            &m[Bus::Pad as usize],
            &m[Bus::Keys as usize],
        );
        assert_eq!((bass.trim, bass.pump), (1.5, 0.6));
        assert!(matches!(bass.eq[..], [Eq::Hp(30.0, _)]));
        // Switched off, the rest kept.
        assert!(pad.chorus.is_none());
        assert_eq!(pad.autopan, Some((0.5, 0.2)));
        assert_eq!(pad.trim, 0.8);
        // A bus mix.ron doesn't list: passes through, then the override.
        assert_eq!((keys.trim, keys.pump), (0.5, 0.0));
        // Every other song keeps mix.ron as written.
        let plain = merged(Some(&file), &NO_MIX);
        assert_eq!(plain[Bus::Bass as usize].pump, 0.3);
        assert!(plain[Bus::Pad as usize].chorus.is_some());
    }

    #[test]
    fn mix_ron_parses_and_names_real_buses() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/music/mix.ron"
        );
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{e}"));
        let m: Mix = ron::from_str(&text).unwrap_or_else(|e| panic!("{e}"));
        for name in m.buses.keys() {
            assert!(
                Bus::NAMES.contains(&name.as_str()),
                "mix.ron: no bus {name}"
            );
        }
    }
}
