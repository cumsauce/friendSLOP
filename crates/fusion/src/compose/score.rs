//! The score file (`moods.ron`): every mood's songs, parsed into [`Style`]s.

use super::*;

/// A progression: (semitones above the key, quality, beats).
pub(super) type Prog = &'static [(u8, Quality, u8)];

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(super) enum Groove {
    City,
    Laid,
    Brush,
    Sparse,
    Dream,
    Bolero,
    Tense,
    Drive,
    Shred,
    March,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(super) enum Comp {
    City,
    Laid,
    Long,
    Stabs,
    Bolero,
    Tense,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(super) enum LeadKind {
    Synth,
    Brass,
    Flute,
    Bell,
    /// Minimoog-style subtractive lead.
    Mini,
    Silent,
}

/// What the nylon guitar does in a style.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
pub(super) enum Nylon {
    None,
    /// Bossa-nova comping (thumb on one and three, fingers syncopated)
    /// instead of the keys.
    Bossa,
    /// Fingerpicked arpeggios instead of the keys and arp in these
    /// sections; the keys come back elsewhere.
    Pick(&'static [Section]),
}

/// What the horn section does in a style.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(super) enum Horns {
    None,
    /// Tight section stabs and shout figures answering the lead.
    Stabs,
    /// Long chords blooming in on the changes.
    Swells,
    Both,
}

/// Extra percussion layered over a groove.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct Kit {
    pub(super) shaker: bool,
    pub(super) conga: bool,
    pub(super) clap: bool,
    pub(super) ride: bool,
}

pub(super) const KIT_NONE: Kit = Kit {
    shaker: false,
    conga: false,
    clap: false,
    ride: false,
};

pub(super) struct Style {
    pub(super) bpm: f32,
    pub(super) swing: f32,
    pub(super) key: u8,
    pub(super) a: Prog,
    pub(super) b: Prog,
    pub(super) hook: Prog,
    pub(super) groove: Groove,
    pub(super) bass: &'static [&'static str],
    pub(super) comp: Comp,
    pub(super) lead: LeadKind,
    pub(super) arp: Option<Inst>,
    pub(super) pad: bool,
    pub(super) intensity: f32,
    /// Chance the last bar of a section is a 7/8 kime bar.
    pub(super) odd: f32,
    /// Chance a new section modulates up a half or whole step.
    pub(super) modulate: f32,
    /// Chance a section ends on a unison kime.
    pub(super) kime: f32,
    /// Plays the written signature hook in Hook sections.
    pub(super) theme: bool,
    pub(super) bass_inst: Inst,
    pub(super) pad_inst: Inst,
    pub(super) horns: Horns,
    pub(super) kit: Kit,
    /// Who answers the lead in its gaps (call and response).
    pub(super) answer: Option<Inst>,
    pub(super) nylon: Nylon,
    /// Who plays the keys' comping (the electric piano unless a song says).
    pub(super) keys: Inst,
    /// Who plays the conga part (tumbao) when the kit has congas.
    pub(super) hand: Inst,
    /// Extra hand percussion with its own pattern (tambourine, claves,
    /// guiro, woodblock).
    pub(super) perc: &'static [Inst],
}

// --- The score file -------------------------------------------------------
// Every mood's songs live in `assets/data/music/moods.ron` (the synths stay in
// code). The composer reads `&'static Style`s, so each version of the file is
// parsed once and leaked: that only happens when someone edits it.

/// `data/music/moods.ron`, as written.
#[derive(serde::Deserialize, Default)]
pub struct Score {
    pub(super) progressions: BTreeMap<String, Vec<(u8, Quality, u8)>>,
    pub(super) bass: BTreeMap<String, String>,
    pub(super) moods: BTreeMap<Mood, MoodScore>,
}

#[derive(serde::Deserialize)]
pub(super) struct MoodScore {
    pub(super) tone: Tone,
    pub(super) level_db: f32,
    pub(super) songs: Vec<SongScore>,
}

#[derive(serde::Deserialize)]
pub(super) enum NylonScore {
    None,
    Bossa,
    Pick(Vec<Section>),
}

#[derive(serde::Deserialize)]
pub(super) struct SongScore {
    pub(super) bpm: f32,
    pub(super) swing: f32,
    pub(super) key: u8,
    pub(super) a: String,
    pub(super) b: String,
    pub(super) hook: String,
    pub(super) groove: Groove,
    pub(super) bass: Vec<String>,
    pub(super) comp: Comp,
    pub(super) lead: LeadKind,
    pub(super) arp: Option<Inst>,
    pub(super) pad: bool,
    pub(super) intensity: f32,
    pub(super) odd: f32,
    pub(super) modulate: f32,
    pub(super) kime: f32,
    pub(super) theme: bool,
    pub(super) bass_inst: Inst,
    pub(super) pad_inst: Inst,
    pub(super) horns: Horns,
    pub(super) kit: Kit,
    pub(super) answer: Option<Inst>,
    pub(super) nylon: NylonScore,
    /// Optional: who comps on the keys (default EPiano; Clav, Organ...).
    #[serde(default)]
    pub(super) keys: Option<Inst>,
    /// Optional: who plays the conga part (default Conga; Bongo, Timbales,
    /// Cajon, Tabla).
    #[serde(default)]
    pub(super) hand: Option<Inst>,
    /// Optional: extra hand percussion (Tambourine, Claves, Guiro, Woodblock).
    #[serde(default)]
    pub(super) perc: Vec<Inst>,
}

/// The score, read at run time (see [`friendslop_data`]).
pub static SCORE: friendslop_data::Data<Score> = friendslop_data::Data::ron("data/music/moods.ron");

/// One mood's songs, ready to play.
pub(super) struct MoodTable {
    pub(super) tone: Tone,
    pub(super) level_db: f32,
    pub(super) songs: Vec<&'static Style>,
}

pub(super) struct Table {
    pub(super) moods: BTreeMap<Mood, MoodTable>,
}

/// What plays when the score is missing a mood (or the file is).
pub(super) const FALLBACK_PROG: Prog = &[(0, Maj9, 8), (5, Maj7s11, 8)];
/// The song that plays then.
pub(super) fn fallback() -> &'static Style {
    static FALLBACK: std::sync::OnceLock<Style> = std::sync::OnceLock::new();
    FALLBACK.get_or_init(|| Style {
        bpm: 96.0,
        swing: 0.1,
        key: 0,
        a: FALLBACK_PROG,
        b: FALLBACK_PROG,
        hook: FALLBACK_PROG,
        groove: Groove::Laid,
        bass: &["R-----.5..R---A."],
        comp: Comp::Laid,
        lead: LeadKind::Silent,
        arp: None,
        pad: true,
        intensity: 0.4,
        odd: 0.0,
        modulate: 0.0,
        kime: 0.0,
        theme: false,
        bass_inst: pk().bass,
        pad_inst: pk().pad,
        horns: Horns::None,
        kit: KIT_NONE,
        answer: None,
        nylon: Nylon::None,
        keys: pk().epiano,
        hand: pk().conga,
        perc: &[],
    })
}

pub(super) fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

pub(super) fn build(score: &Score) -> Table {
    let prog = |name: &str| -> Prog {
        score
            .progressions
            .get(name)
            .map_or(FALLBACK_PROG, |p| leak(p.clone()))
    };
    let bass = |names: &[String]| -> &'static [&'static str] {
        let lines: Vec<&'static str> = names
            .iter()
            .filter_map(|n| score.bass.get(n))
            .map(|l| &*Box::leak(l.clone().into_boxed_str()))
            .collect();
        if lines.is_empty() {
            fallback().bass
        } else {
            leak(lines)
        }
    };
    let moods = score
        .moods
        .iter()
        .map(|(&m, ms)| {
            let songs = ms
                .songs
                .iter()
                .map(|s| {
                    &*Box::leak(Box::new(Style {
                        bpm: s.bpm,
                        swing: s.swing,
                        key: s.key,
                        a: prog(&s.a),
                        b: prog(&s.b),
                        hook: prog(&s.hook),
                        groove: s.groove,
                        bass: bass(&s.bass),
                        comp: s.comp,
                        lead: s.lead,
                        arp: s.arp,
                        pad: s.pad,
                        intensity: s.intensity,
                        odd: s.odd,
                        modulate: s.modulate,
                        kime: s.kime,
                        theme: s.theme,
                        bass_inst: s.bass_inst,
                        pad_inst: s.pad_inst,
                        horns: s.horns,
                        kit: s.kit,
                        answer: s.answer,
                        nylon: match &s.nylon {
                            NylonScore::None => Nylon::None,
                            NylonScore::Bossa => Nylon::Bossa,
                            NylonScore::Pick(at) => Nylon::Pick(leak(at.clone())),
                        },
                        keys: s.keys.unwrap_or(pk().epiano),
                        hand: s.hand.unwrap_or(pk().conga),
                        perc: leak(s.perc.clone()),
                    }))
                })
                .collect();
            (
                m,
                MoodTable {
                    tone: ms.tone,
                    level_db: ms.level_db,
                    songs,
                },
            )
        })
        .collect();
    Table { moods }
}

/// The score as the composer plays it, rebuilt when the file changes.
pub(super) fn table() -> &'static Table {
    static TABLE: RwLock<Option<(usize, &'static Table)>> = RwLock::new(None);
    let score = SCORE.get();
    let key = Arc::as_ptr(&score) as usize;
    if let Ok(t) = TABLE.read()
        && let Some((k, t)) = *t
        && k == key
    {
        return t;
    }
    let t: &'static Table = Box::leak(Box::new(build(&score)));
    if let Ok(mut slot) = TABLE.write() {
        *slot = Some((key, t));
    }
    t
}

// The victory cadence: bVI - bVII - I.
pub(super) const VICTORY: Prog = &[(8, Maj7s11, 2), (10, Dom13, 2), (0, Maj9, 4)];

/// How many songs a mood has to choose from.
pub(super) fn songs(m: Mood) -> usize {
    table().moods.get(&m).map_or(1, |ms| ms.songs.len().max(1))
}

pub(super) fn style(m: Mood) -> &'static Style {
    style_song(m, 0)
}

/// Mood `m`'s song number `song` (0 = its original; wraps round).
pub(super) fn style_song(m: Mood, song: u8) -> &'static Style {
    let t = table();
    t.moods
        .get(&m)
        .and_then(|ms| ms.songs.get(song as usize % ms.songs.len().max(1)))
        .copied()
        .unwrap_or_else(fallback)
}

#[cfg(test)]
mod score_file {
    use super::*;

    #[test]
    fn every_mood_has_songs_and_every_name_resolves() {
        let score = SCORE.get();
        for m in Mood::ALL {
            let Some(ms) = score.moods.get(&m) else {
                panic!("{m:?} has no songs in moods.ron");
            };
            assert!(!ms.songs.is_empty(), "{m:?}");
            let table = crate::inst::table();
            for s in &ms.songs {
                let insts = [
                    Some(s.bass_inst),
                    Some(s.pad_inst),
                    s.arp,
                    s.answer,
                    s.keys,
                    s.hand,
                ]
                .into_iter()
                .flatten()
                .chain(s.perc.iter().copied());
                for i in insts {
                    assert!(table.get(i).is_some(), "{m:?}: no instrument {i:?}");
                }
                for p in [&s.a, &s.b, &s.hook] {
                    assert!(
                        score.progressions.contains_key(p),
                        "{m:?}: no progression {p}"
                    );
                }
                for b in &s.bass {
                    assert!(score.bass.contains_key(b), "{m:?}: no bass line {b}");
                }
            }
        }
        let mix = crate::synth::MIX.get();
        for b in crate::synth::Bus::NAMES {
            assert!(mix.buses.contains_key(b), "mix.ron has no bus {b}");
        }
        assert_eq!(
            mix.buses.len(),
            BUSES,
            "mix.ron names a bus the band doesn't have"
        );
    }
}
