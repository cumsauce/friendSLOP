//! The score file (`moods.ron`): every mood's songs, parsed into [`Style`]s,
//! each a list of [`Part`]s drawn from the library (`library.rs`).
//!
//! A song either lists its parts (`parts: [...]`) or fills in the shorthand
//! fields (`groove`, `comp`, `lead`, `arp`, `pad`, `horns`, `kit`, `answer`,
//! `nylon`, `keys`, `hand`, `perc`, `bass_inst`, `pad_inst`), which its band
//! (`"fusion"` unless it says) turns into parts: see [`expand`].

use super::library::{self, Band, Comp, Feel, Form, Groove, HornStyle, Kimes, Kit, Lane};
use super::library::{LeadStyle, Lib, Library, Theme, When, Writer};
use super::*;

/// A progression: (semitones above the key, quality, beats).
pub(super) type Prog = &'static [(u8, Quality, u8)];

/// A name written bare (`City`, as moods.ron writes grooves and comps).
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Name(pub String);

impl<'de> serde::Deserialize<'de> for Name {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = Name;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a name")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Name, E> {
                Ok(Name(v.to_owned()))
            }
        }
        d.deserialize_identifier(V)
    }
}

/// Extra percussion layered over a groove (shorthand).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Deserialize)]
pub(super) struct KitFlags {
    pub(super) shaker: bool,
    pub(super) conga: bool,
    pub(super) clap: bool,
    pub(super) ride: bool,
}

/// What the nylon guitar does in a song (shorthand).
#[derive(serde::Deserialize, Default, Clone)]
pub(super) enum NylonScore {
    #[default]
    None,
    /// Bossa-nova comping instead of the keys.
    Bossa,
    /// Fingerpicked arpeggios instead of the keys and arp in these sections.
    Pick(Vec<Section>),
}

// --- Parts, as written ----------------------------------------------------

/// What a part plays: one of the composer's generators and its settings.
#[derive(serde::Deserialize, Clone, Debug)]
pub enum Gen {
    /// The kit: a groove from the library (and its fills).
    Drums {
        groove: String,
        #[serde(default)]
        kit: Option<String>,
        /// The ride takes over from the hats when the kit's `ride_when` holds.
        #[serde(default)]
        ride: bool,
        /// The clap doubles the backbeat when the kit's `clap_when` holds.
        #[serde(default)]
        clap: bool,
    },
    /// A bass line: names in moods.ron's `bass`.
    Bass {
        lines: Vec<String>,
        /// Slides into nearby notes this often.
        #[serde(default)]
        glide: Option<f32>,
    },
    /// Chords, in one of the library's comps.
    Chords {
        comp: String,
        /// While this bus plays (a melody), the top note leaves its register.
        #[serde(default)]
        under: Option<String>,
    },
    /// An arpeggio through the chord.
    Arp,
    /// An interlocking two-bar figure from the library's `ostinatos`.
    Ostinato { figure: String },
    /// The lead: a style from the library's `leads`, maybe playing a theme in
    /// the hooks.
    Melody {
        style: String,
        #[serde(default)]
        theme: Option<String>,
        /// Every solo is runs.
        #[serde(default)]
        solo_runs: bool,
    },
    /// Call and response: echoes the melody's last notes in its gaps.
    Answer {
        #[serde(default)]
        chance: Option<(f32, f32)>,
    },
    /// The melody doubled in parallel: each note it plays this bar, `steps`
    /// scale steps away in the chord's scale (-2: a third below, -5: a sixth
    /// below, 2: a third above). Rests when the melody does.
    Harmony { steps: i8 },
    /// A horn section: stabs and shouts, swells, or both.
    Horns {
        style: String,
        #[serde(default)]
        stabs: bool,
        #[serde(default)]
        swells: bool,
    },
    /// Long chords under everything.
    Pad {
        /// While this bus plays (a melody), thin to two low voices.
        #[serde(default)]
        under: Option<String>,
        /// Sits out a bar the horns swelled in.
        #[serde(default)]
        after_swell: bool,
    },
    /// A hand-percussion lane from the library's `lanes`.
    Lane { lane: String },
    /// Unison hits at section ends (with the kit, the bass, the keys and the
    /// melody), replacing the other parts that bar.
    Kime {
        rhythms: String,
        #[serde(default)]
        kit: Option<String>,
        bass: Inst,
        keys: Inst,
        #[serde(default)]
        horn: Option<Inst>,
    },
    /// Bells on the chord in the second victory bar.
    Bells,
}

/// One part of a song, as written.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct PartFile {
    pub play: Gen,
    /// Who plays it (the generator's own default if not given).
    #[serde(default)]
    pub inst: Option<Inst>,
    /// Only in these sections (all if empty), never in `not`.
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub not: Vec<Section>,
    /// Plays while any of these holds (always if empty).
    #[serde(default)]
    pub when: Vec<When>,
    /// Only between these intensities.
    #[serde(default)]
    pub intensity: Option<(f32, f32)>,
    /// The bus whose level it sets (its instrument's if not given).
    #[serde(default)]
    pub bus: Option<String>,
    /// The bus level rules (the first that holds); default: always on.
    #[serde(default)]
    pub layer: Option<Vec<(When, f32)>>,
    /// Where it plays (the feel's register for it if not given).
    #[serde(default)]
    pub register: Option<(u8, u8)>,
}

// --- Parts, resolved --------------------------------------------------------

/// A generator with its library entries resolved.
pub(super) enum G {
    Drums {
        groove: &'static Groove,
        kit: &'static Kit,
        ride: bool,
        clap: bool,
    },
    Bass {
        lines: &'static [&'static str],
        glide: Option<f32>,
    },
    Chords {
        comp: &'static Comp,
        under: Option<usize>,
    },
    Arp,
    Ostinato {
        figs: &'static [Vec<Vec<(u8, u8)>>],
    },
    Melody {
        style: &'static LeadStyle,
        theme: Option<&'static Theme>,
        solo_runs: bool,
    },
    Answer {
        chance: (f32, f32),
    },
    Harmony {
        steps: i8,
    },
    Horns {
        style: &'static HornStyle,
        stabs: bool,
        swells: bool,
    },
    Pad {
        under: Option<usize>,
        after_swell: bool,
    },
    Lane {
        lane: &'static Lane,
    },
    Kime {
        rhythms: &'static Kimes,
        kit: &'static Kit,
        bass: Inst,
        keys: Inst,
        horn: Option<Inst>,
    },
    Bells,
}

pub(super) struct Part {
    pub(super) play: G,
    pub(super) inst: Inst,
    pub(super) sections: Vec<Section>,
    pub(super) not: Vec<Section>,
    pub(super) when: Vec<When>,
    pub(super) intensity: (f32, f32),
    pub(super) register: (u8, u8),
}

impl Part {
    /// Does it play in this bar?
    pub(super) fn on(&self, c: &library::Ctx) -> bool {
        (self.sections.is_empty() || self.sections.contains(&c.section))
            && !self.not.contains(&c.section)
            && (self.when.is_empty() || library::any(&self.when, c))
            && (self.intensity.0..=self.intensity.1).contains(&c.intensity)
    }

    /// Does it play on a kime bar (the kime replaces the rest)?
    pub(super) fn in_kime(&self) -> bool {
        matches!(
            self.play,
            G::Pad { .. } | G::Arp | G::Ostinato { .. } | G::Bells | G::Kime { .. }
        )
    }
}

/// One bus's level rules: the bus (or the part's instrument's), the rules.
pub(super) struct Layer {
    pub(super) bus: Option<usize>,
    pub(super) inst: Inst,
    pub(super) rules: Vec<(When, f32)>,
}

pub(super) struct Style {
    pub(super) bpm: f32,
    pub(super) swing: f32,
    pub(super) key: u8,
    pub(super) a: Prog,
    pub(super) b: Prog,
    pub(super) hook: Prog,
    pub(super) intensity: f32,
    /// Chance the last bar of a section is a 7/8 kime bar.
    pub(super) odd: f32,
    /// Chance a new section modulates up a half or whole step.
    pub(super) modulate: f32,
    /// Chance a section ends on a unison kime.
    pub(super) kime: f32,
    /// Steps per bar and beats per bar.
    pub(super) steps: u32,
    pub(super) beats: u32,
    pub(super) parts: Vec<Part>,
    pub(super) layers: Vec<Layer>,
    pub(super) form: &'static Form,
    pub(super) feel: &'static Feel,
    /// The crash and the reversed cymbal.
    pub(super) kit: &'static Kit,
    /// The melody part (the voice the kime and the answer follow).
    pub(super) lead: Option<usize>,
    /// The lead's range (its style's, else the feel's).
    pub(super) lead_range: (u8, u8),
    /// The melody's style, else the band's: motifs are still written (and
    /// the dice still rolled) for a song whose lead is silent.
    pub(super) lead_style: &'static LeadStyle,
    /// Who holds the stillness's pad.
    pub(super) hold_inst: Inst,
}

impl Style {
    pub(super) fn has_lead(&self) -> bool {
        self.lead.is_some()
    }

    pub(super) fn lead_part(&self) -> Option<&Part> {
        self.lead.and_then(|i| self.parts.get(i))
    }

    pub(super) fn has_kime(&self) -> bool {
        self.parts.iter().any(|p| matches!(p.play, G::Kime { .. }))
    }
}

// --- The score file -------------------------------------------------------
// The moods live in `assets/data/music/moods.ron` and each song in its own
// file in `songs/` (the synths stay in code). The composer reads
// `&'static Style`s, so each version of the files is parsed once and leaked:
// that only happens when someone edits one.

/// `data/music/moods.ron`, as written.
#[derive(serde::Deserialize, Default)]
pub struct Score {
    pub(super) progressions: BTreeMap<String, Vec<(u8, Quality, u8)>>,
    pub(super) bass: BTreeMap<String, String>,
    pub(super) moods: Vec<MoodScore>,
    /// What plays when (see [`crate::direct`]).
    #[serde(default)]
    pub rules: Vec<crate::direct::Rule>,
    /// When the band fades out, and when it cuts dead.
    #[serde(default)]
    pub hush: Option<crate::direct::Cond>,
    #[serde(default)]
    pub silence: Option<crate::direct::Cond>,
    /// The cues (`MusicSignals::cue`) that play a victory cadence.
    #[serde(default)]
    pub victory_cues: Vec<String>,
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

#[derive(serde::Deserialize)]
pub(super) struct MoodScore {
    pub(super) name: Mood,
    pub(super) tone: Tone,
    pub(super) level_db: f32,
    /// The room: reverb and echo send scales on top of the sections'.
    #[serde(default = "one")]
    pub(super) wet: f32,
    #[serde(default = "one")]
    pub(super) echo: f32,
    #[serde(default)]
    urgent: bool,
    #[serde(default)]
    instant: bool,
    #[serde(default)]
    wandering: bool,
    #[serde(default)]
    hold: bool,
    #[serde(default)]
    fallback: bool,
    #[serde(default = "yes")]
    victory: bool,
    #[serde(default = "yes")]
    lift: bool,
}

impl MoodScore {
    fn flags(&self) -> MoodFlags {
        MoodFlags {
            urgent: self.urgent,
            instant: self.instant,
            wandering: self.wandering,
            hold: self.hold,
            fallback: self.fallback,
            victory: self.victory,
            lift: self.lift,
        }
    }
}

fn fusion() -> String {
    "fusion".into()
}

fn four() -> u32 {
    4
}

fn sixteen() -> u32 {
    16
}

/// One song file (`data/music/songs/<name>.ron`).
#[derive(serde::Deserialize, Default, Clone)]
pub struct SongScore {
    /// Whose song it is (a mood's name in moods.ron).
    #[serde(default)]
    pub(super) mood: String,
    pub(super) bpm: f32,
    pub(super) swing: f32,
    pub(super) key: u8,
    pub(super) a: String,
    pub(super) b: String,
    pub(super) hook: String,
    pub(super) intensity: f32,
    #[serde(default)]
    pub(super) odd: f32,
    #[serde(default)]
    pub(super) modulate: f32,
    #[serde(default)]
    pub(super) kime: f32,
    /// The band its shorthand fields fill (and whose form and feel it uses).
    #[serde(default = "fusion")]
    pub(super) band: String,
    #[serde(default)]
    pub(super) form: Option<String>,
    #[serde(default)]
    pub(super) feel: Option<String>,
    #[serde(default = "sixteen")]
    pub(super) steps: u32,
    #[serde(default = "four")]
    pub(super) beats: u32,
    /// The parts, written out (instead of the shorthand below).
    #[serde(default)]
    pub(super) parts: Option<Vec<PartFile>>,
    // The shorthand.
    #[serde(default)]
    pub(super) groove: Name,
    #[serde(default)]
    pub(super) bass: Vec<String>,
    #[serde(default)]
    pub(super) comp: Name,
    #[serde(default)]
    pub(super) lead: Name,
    #[serde(default)]
    pub(super) arp: Option<Inst>,
    #[serde(default)]
    pub(super) pad: bool,
    #[serde(default)]
    pub(super) theme: bool,
    #[serde(default)]
    pub(super) bass_inst: Option<Inst>,
    #[serde(default)]
    pub(super) pad_inst: Option<Inst>,
    #[serde(default)]
    pub(super) horns: Name,
    #[serde(default)]
    pub(super) kit: KitFlags,
    #[serde(default)]
    pub(super) answer: Option<Inst>,
    #[serde(default)]
    pub(super) nylon: NylonScore,
    /// Who comps on the keys (default the band's keys: EPiano).
    #[serde(default)]
    pub(super) keys: Option<Inst>,
    /// Who plays the conga part (default the band's hand drum).
    #[serde(default)]
    pub(super) hand: Option<Inst>,
    /// Extra hand percussion: lanes named after their instruments.
    #[serde(default)]
    pub(super) perc: Vec<Inst>,
}

/// The score, read at run time (see [`friendslop_data`]).
pub static SCORE: friendslop_data::Data<Score> = friendslop_data::Data::ron("data/music/moods.ron");

/// The songs, one file each, in the index's order (a mood's first is its own).
pub static SONGS: friendslop_data::Many<SongScore> =
    friendslop_data::Many::ron("data/music/songs.ron");

/// One mood's songs, ready to play.
pub(super) struct MoodTable {
    pub(super) mood: Mood,
    pub(super) flags: MoodFlags,
    pub(super) tone: Tone,
    pub(super) level_db: f32,
    pub(super) wet: f32,
    pub(super) echo: f32,
    pub(super) songs: Vec<&'static Style>,
}

pub(super) struct Table {
    /// In the score's order.
    pub(super) moods: Vec<MoodTable>,
}

impl Table {
    pub(super) fn pos(&self, m: Mood) -> Option<usize> {
        self.moods.iter().position(|t| t.mood == m)
    }

    pub(super) fn get(&self, m: Mood) -> Option<&MoodTable> {
        self.moods.iter().find(|t| t.mood == m)
    }
}

/// What plays when the score is missing a mood (or the file is): one
/// fallback chord.
pub(super) const FALLBACK_PROG: Prog = &[(0, Quality::NONE, 8)];

fn empty<T: Default>() -> &'static T {
    Box::leak(Box::new(T::default()))
}

/// The song that plays then: silence, in time.
pub(super) fn fallback() -> &'static Style {
    static FALLBACK: std::sync::OnceLock<&'static Style> = std::sync::OnceLock::new();
    FALLBACK.get_or_init(|| {
        Box::leak(Box::new(Style {
            bpm: 96.0,
            swing: 0.1,
            key: 0,
            a: FALLBACK_PROG,
            b: FALLBACK_PROG,
            hook: FALLBACK_PROG,
            intensity: 0.4,
            odd: 0.0,
            modulate: 0.0,
            kime: 0.0,
            steps: 16,
            beats: 4,
            parts: Vec::new(),
            layers: Vec::new(),
            form: empty(),
            feel: empty(),
            kit: empty(),
            lead: None,
            lead_range: (62, 84),
            lead_style: empty(),
            hold_inst: Inst::NONE,
        }))
    })
}

pub(super) fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

fn bus_of(name: &Option<String>) -> Option<usize> {
    name.as_deref().and_then(library::bus)
}

/// Resolve a written part against the library (`None`: it names something
/// the library doesn't have, and is left out).
fn resolve(
    p: &PartFile,
    lib: &'static Library,
    band: &'static Band,
    feel: &'static Feel,
    lines: &dyn Fn(&[String]) -> &'static [&'static str],
) -> Option<Part> {
    let r = &feel.registers;
    let kit = |k: &Option<String>| lib.kits.get(k.as_deref().unwrap_or(&band.kit));
    let (play, inst, register) = match &p.play {
        Gen::Drums {
            groove,
            kit: k,
            ride,
            clap,
        } => (
            G::Drums {
                groove: lib.grooves.get(groove)?,
                kit: kit(k)?,
                ride: *ride,
                clap: *clap,
            },
            Inst::NONE,
            (0, 127),
        ),
        Gen::Bass { lines: l, glide } => (
            G::Bass {
                lines: lines(l),
                glide: *glide,
            },
            band.bass,
            (r.bass, r.bass + 12),
        ),
        Gen::Chords { comp, under } => {
            let comp = lib.comps.get(comp)?;
            let (inst, reg) = match comp.writer {
                Writer::Block => (band.keys, r.keys),
                Writer::Strum => (band.guitar, r.keys),
                Writer::Bossa | Writer::Pick => (band.guitar, r.guitar),
            };
            (
                G::Chords {
                    comp,
                    under: bus_of(under),
                },
                inst,
                reg,
            )
        }
        Gen::Arp => (G::Arp, Inst::NONE, r.arp),
        Gen::Ostinato { figure } => (
            G::Ostinato {
                figs: lib.ostinatos.get(figure)?,
            },
            Inst::NONE,
            r.ostinato,
        ),
        Gen::Melody {
            style,
            theme,
            solo_runs,
        } => (
            G::Melody {
                style: lib.leads.get(style)?,
                theme: theme.as_ref().and_then(|t| lib.themes.get(t)),
                solo_runs: *solo_runs,
            },
            Inst::NONE,
            r.lead,
        ),
        Gen::Answer { chance } => (
            G::Answer {
                chance: chance.unwrap_or((0.45, 0.35)),
            },
            Inst::NONE,
            r.answer_default,
        ),
        Gen::Harmony { steps } => (
            G::Harmony { steps: *steps },
            Inst::NONE,
            (r.lead.0.saturating_sub(12), r.lead.1),
        ),
        Gen::Horns {
            style,
            stabs,
            swells,
        } => {
            let style = lib.horns.get(style)?;
            (
                G::Horns {
                    style,
                    stabs: *stabs,
                    swells: *swells,
                },
                style.inst,
                r.horn,
            )
        }
        Gen::Pad { under, after_swell } => (
            G::Pad {
                under: bus_of(under),
                after_swell: *after_swell,
            },
            band.pad,
            r.pad,
        ),
        Gen::Lane { lane } => {
            let lane = lib.lanes.get(lane)?;
            let inst = if lane.hand {
                band.hand
            } else {
                lane.inst.unwrap_or_default()
            };
            (G::Lane { lane }, inst, (0, 127))
        }
        Gen::Kime {
            rhythms,
            kit: k,
            bass,
            keys,
            horn,
        } => (
            G::Kime {
                rhythms: lib.kimes.get(rhythms)?,
                kit: kit(k)?,
                bass: *bass,
                keys: *keys,
                horn: *horn,
            },
            Inst::NONE,
            r.keys,
        ),
        Gen::Bells => (G::Bells, band.bells, r.bells),
    };
    let inst = p.inst.unwrap_or(inst);
    let register = p.register.unwrap_or(match play {
        G::Answer { .. } => r.answer.get(&inst).copied().unwrap_or(register),
        _ => register,
    });
    Some(Part {
        play,
        inst,
        sections: p.sections.clone(),
        not: p.not.clone(),
        when: p.when.clone(),
        intensity: p.intensity.unwrap_or((0.0, 1.0)),
        register,
    })
}

/// A part with the generator's defaults (shorthand).
fn part(play: Gen) -> PartFile {
    PartFile {
        play,
        inst: None,
        sections: Vec::new(),
        not: Vec::new(),
        when: Vec::new(),
        intensity: None,
        bus: None,
        layer: Some(Vec::new()),
        register: None,
    }
}

/// The band's bus rules called `name`.
fn rules(band: &Band, name: &str) -> Vec<(When, f32)> {
    band.layers.get(name).cloned().unwrap_or_default()
}

/// A layer on the bus `name` from the band's rules of the same name.
fn layer(band: &Band, name: &str) -> PartFile {
    PartFile {
        bus: Some(name.into()),
        layer: Some(rules(band, name)),
        ..part(Gen::Bells)
    }
}

/// Turn a song's shorthand fields into its band's parts, in the order they
/// play (which is also the order their notes are written). The second list
/// is bus layers with no part of their own.
pub(super) fn expand(s: &SongScore, lib: &Library, band: &Band) -> (Vec<PartFile>, Vec<PartFile>) {
    let mut parts = Vec::new();
    let mut buses = vec![layer(band, "keys"), layer(band, "perc")];
    let groove = lib.grooves.get(&s.groove.0);
    let lead = band.leads.get(&s.lead.0).copied();
    let horns = band.horn_modes.get(&s.horns.0).copied();
    let keys = s.keys.unwrap_or(band.keys);
    let comp = lib.comps.get(&s.comp.0);
    let strum = comp.is_some_and(|c| c.writer == Writer::Strum);
    let pick_sections = match &s.nylon {
        NylonScore::Pick(at) => at.clone(),
        _ => Vec::new(),
    };
    let lead_bus = Some("lead".to_owned());
    // The kime stands in for the band on a section's last bar.
    parts.push(part(Gen::Kime {
        rhythms: band.kimes.clone(),
        kit: None,
        bass: band.kime_bass,
        keys,
        horn: horns.and(lib.horns.get(&band.horns).map(|h| h.inst)),
    }));
    parts.push(PartFile {
        layer: Some(
            groove
                .and_then(|g| g.layer.clone())
                .unwrap_or_else(|| rules(band, "drums")),
        ),
        bus: Some("drums".into()),
        ..part(Gen::Drums {
            groove: s.groove.0.clone(),
            kit: None,
            ride: s.kit.ride,
            clap: s.kit.clap,
        })
    });
    let bass_inst = s.bass_inst.unwrap_or(band.bass);
    parts.push(PartFile {
        inst: Some(bass_inst),
        bus: Some("bass".into()),
        layer: Some(rules(band, "bass")),
        ..part(Gen::Bass {
            lines: s.bass.clone(),
            glide: band.glides.contains(&bass_inst).then_some(band.glide),
        })
    });
    if strum {
        parts.push(part(Gen::Chords {
            comp: s.comp.0.clone(),
            under: lead_bus.clone(),
        }));
    } else if matches!(s.nylon, NylonScore::Bossa) {
        parts.push(part(Gen::Chords {
            comp: band.bossa.clone(),
            under: None,
        }));
    } else {
        parts.push(PartFile {
            inst: Some(keys),
            not: pick_sections.clone(),
            ..part(Gen::Chords {
                comp: s.comp.0.clone(),
                under: lead_bus.clone(),
            })
        });
        if !pick_sections.is_empty() {
            parts.push(PartFile {
                sections: pick_sections.clone(),
                ..part(Gen::Chords {
                    comp: band.pick.clone(),
                    under: None,
                })
            });
        }
    }
    if let Some(voice) = lead {
        parts.push(PartFile {
            inst: Some(voice),
            bus: lead_bus.clone(),
            layer: Some(rules(band, "lead")),
            ..part(Gen::Melody {
                style: band.lead.clone(),
                theme: s.theme.then(|| band.theme.clone()),
                solo_runs: groove.is_some_and(|g| g.solo_runs),
            })
        });
    }
    if let Some(who) = s.answer {
        parts.push(PartFile {
            inst: Some(who),
            sections: vec![Section::A, Section::Hook, Section::B],
            ..part(Gen::Answer { chance: None })
        });
    }
    if let Some((stabs, swells)) = horns {
        parts.push(PartFile {
            bus: Some("brass".into()),
            layer: Some(rules(band, "brass")),
            ..part(Gen::Horns {
                style: band.horns.clone(),
                stabs,
                swells,
            })
        });
    }
    // A song without a pad still has one in breakdowns and when calm.
    let sparse = rules(band, "pad_sparse");
    parts.push(PartFile {
        inst: Some(s.pad_inst.unwrap_or(band.pad)),
        bus: Some("pad".into()),
        when: if s.pad {
            Vec::new()
        } else {
            sparse.iter().map(|(w, _)| w.clone()).collect()
        },
        layer: Some(if s.pad { rules(band, "pad") } else { sparse }),
        ..part(Gen::Pad {
            under: lead_bus,
            after_swell: true,
        })
    });
    match s.arp {
        Some(i) if band.ostinato_insts.contains(&i) => parts.push(PartFile {
            inst: Some(i),
            not: pick_sections.clone(),
            ..part(Gen::Ostinato {
                figure: band.ostinato.clone(),
            })
        }),
        Some(i) => parts.push(PartFile {
            inst: Some(i),
            not: pick_sections.clone(),
            ..part(Gen::Arp)
        }),
        None => parts.push(PartFile {
            sections: vec![Section::Victory],
            ..part(Gen::Bells)
        }),
    }
    // Hand percussion plays inside the kit's bar, in this order.
    if s.kit.shaker {
        parts.push(part(Gen::Lane {
            lane: band.shaker.clone(),
        }));
    }
    if s.kit.conga {
        parts.push(PartFile {
            inst: Some(s.hand.unwrap_or(band.hand)),
            ..part(Gen::Lane {
                lane: band.tumbao.clone(),
            })
        });
    }
    for p in &s.perc {
        parts.push(part(Gen::Lane {
            lane: p.name().to_owned(),
        }));
    }
    // The guitar bus is up whenever a guitar plays any part.
    let guitar = !matches!(s.nylon, NylonScore::None) || strum || s.answer == Some(band.guitar);
    if guitar {
        buses.push(layer(band, "guitar"));
    }
    (parts, buses)
}

/// One song, resolved against the library.
fn song(
    s: &SongScore,
    lib: &'static Lib,
    prog: &dyn Fn(&str) -> Prog,
    lines: &dyn Fn(&[String]) -> &'static [&'static str],
) -> Style {
    let file = lib.file;
    let band = file.bands.get(&s.band).unwrap_or_else(|| empty());
    let form = s.form.as_ref().unwrap_or(&band.form);
    let feel = s.feel.as_ref().unwrap_or(&band.feel);
    let form: &'static Form = file.forms.get(form).unwrap_or_else(|| empty());
    let feel: &'static Feel = file.feels.get(feel).unwrap_or_else(|| empty());
    let (written, buses) = match &s.parts {
        Some(p) => (p.clone(), Vec::new()),
        None => expand(s, file, band),
    };
    let mut parts = Vec::new();
    let mut layers = Vec::new();
    for p in buses.iter().filter(|p| p.layer.is_some()) {
        layers.push(Layer {
            bus: bus_of(&p.bus),
            inst: Inst::NONE,
            rules: p.layer.clone().unwrap_or_default(),
        });
    }
    for p in &written {
        let Some(r) = resolve(p, file, band, feel, lines) else {
            continue;
        };
        let rules = p
            .layer
            .clone()
            .unwrap_or_else(|| vec![(When::default(), 1.0)]);
        if !rules.is_empty() {
            layers.push(Layer {
                bus: bus_of(&p.bus),
                inst: r.inst,
                rules,
            });
        }
        parts.push(r);
    }
    let lead = parts
        .iter()
        .position(|p| matches!(p.play, G::Melody { .. }));
    let (lead_range, lead_style) = match lead.map(|i| &parts[i].play) {
        Some(G::Melody { style, .. }) => (style.range, *style),
        _ => (
            feel.registers.lead,
            file.leads.get(&band.lead).unwrap_or_else(|| empty()),
        ),
    };
    let hold_inst = parts
        .iter()
        .find(|p| matches!(p.play, G::Pad { .. }))
        .map_or(band.pad, |p| p.inst);
    Style {
        bpm: s.bpm,
        swing: s.swing,
        key: s.key,
        a: prog(&s.a),
        b: prog(&s.b),
        hook: prog(&s.hook),
        intensity: s.intensity,
        odd: s.odd,
        modulate: s.modulate,
        kime: s.kime,
        steps: s.steps.clamp(4, 32),
        beats: s.beats.clamp(1, s.steps.clamp(4, 32)),
        parts,
        layers,
        form,
        feel,
        kit: file.kits.get(&band.kit).unwrap_or_else(|| empty()),
        lead,
        lead_range,
        lead_style,
        hold_inst,
    }
}

pub(super) fn build(score: &Score, songs: &[Arc<SongScore>], lib: &'static Lib) -> Table {
    let prog = |name: &str| -> Prog {
        score
            .progressions
            .get(name)
            .map_or(FALLBACK_PROG, |p| leak(p.clone()))
    };
    let lines = |names: &[String]| -> &'static [&'static str] {
        let lines: Vec<&'static str> = names
            .iter()
            .filter_map(|n| score.bass.get(n))
            .map(|l| &*Box::leak(l.clone().into_boxed_str()))
            .collect();
        leak(lines)
    };
    let moods = score
        .moods
        .iter()
        .map(|ms| {
            let songs = songs
                .iter()
                .filter(|s| s.mood == ms.name.name())
                .map(|s| &*Box::leak(Box::new(song(s, lib, &prog, &lines))))
                .collect();
            MoodTable {
                mood: ms.name,
                flags: ms.flags(),
                tone: ms.tone,
                level_db: ms.level_db,
                wet: ms.wet,
                echo: ms.echo,
                songs,
            }
        })
        .collect();
    Table { moods }
}

/// The score as the composer plays it, rebuilt when it or the library
/// changes.
pub(super) fn table() -> &'static Table {
    static TABLE: RwLock<Option<((usize, usize, usize), &'static Table)>> = RwLock::new(None);
    let score = SCORE.get();
    let songs = SONGS.get();
    let lib = library::lib();
    let key = (
        if SCORE.loaded() {
            Arc::as_ptr(&score) as usize
        } else {
            0
        },
        if songs.is_empty() {
            0
        } else {
            Arc::as_ptr(&songs) as usize
        },
        library::version(),
    );
    if let Ok(t) = TABLE.read()
        && let Some((k, t)) = *t
        && k == key
    {
        return t;
    }
    let t: &'static Table = Box::leak(Box::new(build(&score, &songs, lib)));
    if let Ok(mut slot) = TABLE.write() {
        *slot = Some((key, t));
    }
    t
}

/// How many songs a mood has to choose from.
pub(super) fn songs(m: Mood) -> usize {
    table().get(m).map_or(1, |ms| ms.songs.len().max(1))
}

pub(super) fn style(m: Mood) -> &'static Style {
    style_song(m, 0)
}

/// Mood `m`'s song number `song` (0 = its original; wraps round).
pub(super) fn style_song(m: Mood, song: u8) -> &'static Style {
    let t = table();
    t.get(m)
        .and_then(|ms| ms.songs.get(song as usize % ms.songs.len().max(1)))
        .copied()
        .unwrap_or_else(fallback)
}

#[cfg(test)]
pub(super) fn song_for_test(s: &SongScore, score: &Score) -> &'static Style {
    let prog = |name: &str| -> Prog {
        score
            .progressions
            .get(name)
            .map_or(FALLBACK_PROG, |p| leak(p.clone()))
    };
    let lines = |names: &[String]| -> &'static [&'static str] {
        leak(
            names
                .iter()
                .filter_map(|n| score.bass.get(n))
                .map(|l| &*Box::leak(l.clone().into_boxed_str()))
                .collect(),
        )
    };
    Box::leak(Box::new(song(s, library::lib(), &prog, &lines)))
}

#[cfg(test)]
mod score_file {
    use super::*;

    /// songs.ron lists exactly the files in songs/, each once.
    #[test]
    fn the_song_index_matches_the_folder() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/music/songs"
        );
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.ends_with(".ron"))
                    .collect()
            })
            .unwrap_or_default();
        on_disk.sort();
        SONGS.get();
        let mut listed = SONGS.names();
        let n = listed.len();
        listed.sort();
        listed.dedup();
        assert_eq!(listed.len(), n, "songs.ron lists a file twice");
        assert!(!listed.is_empty());
        assert_eq!(listed, on_disk, "songs.ron vs the files in songs/");
        assert_eq!(SONGS.get().len(), n, "a song file didn't parse");
    }

    /// The generators as moods.ron's header writes them.
    #[test]
    fn written_parts_parse() {
        let text = r#"#![enable(implicit_some)] [
            (play: Drums(groove: "City", ride: true, clap: true)),
            (play: Bass(lines: ["SLAP1"]), inst: Upright),
            (play: Chords(comp: "City", under: "lead")),
            (play: Arp, inst: Vibes, sections: [B], not: [Intro]),
            (play: Ostinato(figure: "kalimba")),
            (play: Melody(style: "fusion", theme: "signature"), inst: Flute),
            (play: Answer(), inst: Flute, intensity: (0.2, 0.9)),
            (play: Horns(style: "fusion", stabs: true, swells: true)),
            (play: Pad(under: "lead", after_swell: true), bus: "pad", layer: [((), 1.0)]),
            (play: Lane(lane: "shaker"), when: [(above: 0.3)]),
            (play: Kime(rhythms: "fusion", bass: Bass, keys: EPiano, horn: PolyBrass)),
            (play: Bells, register: (72, 96)),
        ]"#;
        let parts: Vec<PartFile> = match ron::from_str(text) {
            Ok(p) => p,
            Err(e) => panic!("{e}"),
        };
        assert_eq!(parts.len(), 12);
    }

    fn lib_file() -> Library {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/music/library.ron"
        ))
        .unwrap_or_default();
        ron::from_str(&text).unwrap_or_default()
    }

    #[test]
    fn every_mood_has_songs_and_every_name_resolves() {
        let score = SCORE.get();
        let lib = lib_file();
        let table = crate::inst::table();
        let inst = |m: Mood, i: Inst| assert!(table.get(i).is_some(), "{m:?}: no instrument {i:?}");
        for (name, p) in &score.progressions {
            for (_, q, _) in p {
                assert!(
                    lib.qualities.contains_key(q.name()),
                    "{name}: no quality {q:?}"
                );
            }
        }
        let songs = SONGS.get();
        for s in songs.iter() {
            assert!(
                score.moods.iter().any(|m| m.name.name() == s.mood),
                "a song for {:?}, which moods.ron doesn't list",
                s.mood
            );
        }
        for m in score.moods.iter().map(|m| m.name) {
            let mine: Vec<_> = songs.iter().filter(|s| s.mood == m.name()).collect();
            assert!(!mine.is_empty(), "{m:?} has no songs");
            for s in mine {
                let Some(band) = lib.bands.get(&s.band) else {
                    panic!("{m:?}: no band {}", s.band);
                };
                for i in [s.bass_inst, s.pad_inst, s.arp, s.answer, s.keys, s.hand]
                    .into_iter()
                    .flatten()
                    .chain(s.perc.iter().copied())
                {
                    inst(m, i);
                }
                for p in [&s.a, &s.b, &s.hook] {
                    assert!(
                        score.progressions.contains_key(p),
                        "{m:?}: no progression {p}"
                    );
                }
                for b in &s.bass {
                    assert!(score.bass.contains_key(b), "{m:?}: no bass line {b}");
                    let steps = score.bass[b].len() as u32;
                    assert_eq!(
                        steps, s.steps,
                        "{m:?}: bass line {b} isn't {} steps",
                        s.steps
                    );
                }
                if s.parts.is_none() {
                    let g = &s.groove.0;
                    assert!(lib.grooves.contains_key(g), "{m:?}: no groove {g}");
                    assert!(
                        lib.comps.contains_key(&s.comp.0),
                        "{m:?}: no comp {:?}",
                        s.comp
                    );
                    assert!(
                        s.lead.0 == "Silent" || band.leads.contains_key(&s.lead.0),
                        "{m:?}: no lead {:?}",
                        s.lead
                    );
                    assert!(
                        s.horns.0 == "None" || band.horn_modes.contains_key(&s.horns.0),
                        "{m:?}: no horns {:?}",
                        s.horns
                    );
                    for p in &s.perc {
                        assert!(lib.lanes.contains_key(p.name()), "{m:?}: no lane {p:?}");
                    }
                    if let Some(h) = s.hand {
                        assert!(lib.hands.contains_key(&h), "{m:?}: no hand drum {h:?}");
                    }
                }
                // Every part the song ends up with resolves.
                let (parts, _) = match &s.parts {
                    Some(p) => (p.clone(), Vec::new()),
                    None => expand(s, &lib, band),
                };
                let built = song_for_test(s, &score);
                assert_eq!(
                    built.parts.len(),
                    parts.len(),
                    "{m:?}: a part didn't resolve"
                );
                for p in &built.parts {
                    if p.inst != Inst::NONE {
                        inst(m, p.inst);
                    }
                }
            }
        }
        assert_eq!(
            score.moods.iter().filter(|m| m.fallback).count(),
            1,
            "one fallback mood"
        );
        for r in &score.rules {
            assert!(
                score.moods.iter().any(|m| m.name == r.mood),
                "a rule plays {:?}, which moods.ron doesn't list",
                r.mood
            );
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
