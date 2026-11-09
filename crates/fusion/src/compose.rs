//! The composer and music director: decides, one bar at a time, what the
//! band plays next.
//!
//! The game only sets a target [`Mood`], an intensity and a victory
//! counter. At every bar line the director looks at them and picks the next
//! measures: keep the section going, plan the next one (Intro, A groove,
//! Hook, B bridge, Solo, Breakdown) with a turnaround and maybe a key change,
//! or play a one-bar transition (drum fill over an approach chord) into a
//! new mood. Nothing ever changes mid-bar.
//!
//! The writing: rootless, voice-led EP voicings over ii-V-Is, tritone subs,
//! modal interchange and lydian colours; slap-bass templates with octave
//! pops, ghosts and chromatic approaches; 16th-note kits with ghost notes
//! and fills; a lead that states a motif, answers it, varies it
//! (sequence, displacement, inversion, fragmentation) and cadences, landing
//! on chord tones on strong beats with chromatic approach tones between;
//! bebop and pentatonic runs in solos; unison kimes at section ends and the
//! odd 7/8 bar.

use super::synth::{BUSES, Bus, Ev, Inst, RATE};
use super::theory::{Chord, Quality, Quality::*, Voicing, voice_lead};

/// What the game wants the music to say.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mood {
    /// The title screen: the signature theme.
    #[default]
    Menu,
    /// Wandering the meadows: laid back.
    Explore,
    /// Commune and village: warm city-pop.
    Village,
    /// Forest and valley: airy, flute and harp.
    Forest,
    /// The maze and the ruins: mysterious minor.
    Maze,
    /// Up on the sky islands: dreamy bells.
    Sky,
    /// With Abuela in heaven: a gentle bolero.
    Abuela,
    /// Hide and seek, hiding: tense, muted, ticking.
    Hiding,
    /// Hide and seek, seeking: driving.
    Seeking,
    /// A duel: double-time fusion shred.
    Duel,
    /// The landlord and his rent: dissonant, building.
    Landlord,
    /// The stillness: everything holds on a pad.
    Still,
    /// Under the commune, in the sewers and passages (#127): dub-wet minor,
    /// a bottle-flute lead and kalimba drips echoing down the tunnels.
    Underground,
}

/// How a mood sounds on top of what it plays: the band's overall colour
/// and feel, so the same instruments fit each place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    /// Above ~1 kHz, the mix is cut (positive, warm and woolly) or lifted
    /// (negative, crisp): the top end's gain is `1 - 0.5 * warmth`.
    pub warmth: f32,
    /// Stereo width: 1 as mixed, below narrower, above wider.
    pub width: f32,
    /// Tape-style pitch wobble, 0..1 (1 is about 4 cents): old gear drifting.
    pub wow: f32,
    /// The backbeat (snare, clap, rim) sits this many ms behind the grid.
    pub lay_back_ms: f32,
    /// Hats and shakers wander by up to this many ms either way.
    pub loose_ms: f32,
}

impl Tone {
    pub const NEUTRAL: Tone = Tone {
        warmth: 0.0,
        width: 1.0,
        wow: 0.0,
        lay_back_ms: 0.0,
        loose_ms: 0.0,
    };
}

impl Mood {
    /// The mood's colour and feel (see [`Tone`]).
    pub fn tone(self) -> Tone {
        let t = |warmth, width, wow, lay_back_ms, loose_ms| Tone {
            warmth,
            width,
            wow,
            lay_back_ms,
            loose_ms,
        };
        match self {
            // The theme: polished but friendly.
            Mood::Menu => t(0.1, 1.15, 0.25, 5.0, 3.0),
            // Meadows: sunny and open, a lazy backbeat.
            Mood::Explore => t(-0.05, 1.2, 0.2, 7.0, 4.0),
            // City-pop: crisp, wide, tight in the pocket.
            Mood::Village => t(-0.15, 1.15, 0.1, 3.0, 2.5),
            // Woody and warm, a little loose, very wide like the trees.
            Mood::Forest => t(0.3, 1.3, 0.35, 8.0, 5.0),
            // Mysterious: dry and centred, played carefully.
            Mood::Maze => t(0.15, 0.95, 0.2, 2.0, 2.0),
            // Airy, shimmering, as wide as the sky.
            Mood::Sky => t(-0.2, 1.4, 0.3, 5.0, 3.5),
            // An old bolero record: warm, narrow, wobbly, behind the beat.
            Mood::Abuela => t(0.5, 0.85, 0.7, 10.0, 6.0),
            // Hiding: muffled and close, holding its breath.
            Mood::Hiding => t(0.4, 0.8, 0.15, 1.0, 1.5),
            // Seeking: bright and urgent, on top of the beat.
            Mood::Seeking => t(-0.25, 1.05, 0.0, 0.0, 1.0),
            // Duel: dusty western shred.
            Mood::Duel => t(0.1, 1.1, 0.3, 2.0, 2.5),
            // The landlord: harsh and pressing.
            Mood::Landlord => t(-0.2, 0.9, 0.1, 0.0, 1.0),
            // The stillness: soft, vast and drifting.
            Mood::Still => t(0.35, 1.15, 0.6, 0.0, 0.0),
            // Tunnels: dark, wide echoes, a dub drag.
            Mood::Underground => t(0.45, 1.3, 0.45, 12.0, 5.0),
        }
    }

    pub const ALL: [Mood; 13] = [
        Mood::Menu,
        Mood::Explore,
        Mood::Village,
        Mood::Forest,
        Mood::Maze,
        Mood::Sky,
        Mood::Abuela,
        Mood::Hiding,
        Mood::Seeking,
        Mood::Duel,
        Mood::Landlord,
        Mood::Still,
        Mood::Underground,
    ];

    pub fn from_index(i: u8) -> Mood {
        Self::ALL.get(i as usize).copied().unwrap_or_default()
    }

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0) as u8
    }

    /// The intensity this mood sits at with nothing else going on.
    pub fn base_intensity(self) -> f32 {
        style(self).intensity
    }

    /// Make-up gain (dB) so calm, sparse moods aren't a cliff quieter
    /// than busy ones when you walk between areas.
    pub fn level_db(self) -> f32 {
        match self {
            Mood::Forest | Mood::Abuela => 0.5,
            Mood::Sky => 2.0,
            Mood::Maze => 2.5,
            Mood::Underground => 2.0,
            Mood::Hiding | Mood::Still => 3.5,
            _ => 0.0,
        }
    }

    /// Moods that cut in at the very next bar line instead of after a
    /// transition bar.
    fn urgent(self) -> bool {
        matches!(self, Mood::Duel | Mood::Seeking | Mood::Still | Mood::Menu)
    }
}

/// The parts of a tune.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Intro,
    A,
    Hook,
    B,
    Solo,
    Breakdown,
    Victory,
    Still,
}

/// A progression: (semitones above the key, quality, beats).
type Prog = &'static [(u8, Quality, u8)];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Groove {
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Comp {
    City,
    Laid,
    Long,
    Stabs,
    Bolero,
    Tense,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LeadKind {
    Synth,
    Brass,
    Flute,
    Bell,
    /// Minimoog-style subtractive lead.
    Mini,
    Silent,
}

/// What the nylon guitar does in a style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Nylon {
    None,
    /// Bossa-nova comping (thumb on one and three, fingers syncopated)
    /// instead of the keys.
    Bossa,
    /// Fingerpicked arpeggios instead of the keys and arp in these
    /// sections; the keys come back elsewhere.
    Pick(&'static [Section]),
}

/// What the horn section does in a style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Horns {
    None,
    /// Tight section stabs and shout figures answering the lead.
    Stabs,
    /// Long chords blooming in on the changes.
    Swells,
    Both,
}

/// Extra percussion layered over a groove.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Kit {
    shaker: bool,
    conga: bool,
    clap: bool,
    ride: bool,
}

const KIT_NONE: Kit = Kit {
    shaker: false,
    conga: false,
    clap: false,
    ride: false,
};

struct Style {
    bpm: f32,
    swing: f32,
    key: u8,
    a: Prog,
    b: Prog,
    hook: Prog,
    groove: Groove,
    bass: &'static [&'static str],
    comp: Comp,
    lead: LeadKind,
    arp: Option<Inst>,
    pad: bool,
    intensity: f32,
    /// Chance the last bar of a section is a 7/8 kime bar.
    odd: f32,
    /// Chance a new section modulates up a half or whole step.
    modulate: f32,
    /// Chance a section ends on a unison kime.
    kime: f32,
    /// Plays the written signature hook in Hook sections.
    theme: bool,
    bass_inst: Inst,
    pad_inst: Inst,
    horns: Horns,
    kit: Kit,
    /// Who answers the lead in its gaps (call and response).
    answer: Option<Inst>,
    nylon: Nylon,
}

// --- Progressions -------------------------------------------------------
// "Just the Two of Us": IVmaj9 - III7#9 - vim9 - vm9 I13.
const JTTOU: Prog = &[
    (5, Maj9, 4),
    (4, Dom7s9, 4),
    (9, Min9, 4),
    (7, Min9, 2),
    (0, Dom13, 2),
];
const CHROMATIC_TWO_FIVES: Prog = &[
    (2, Min9, 2),
    (7, Dom13, 2),
    (1, Min9, 2),
    (6, Dom13, 2),
    (0, Maj9, 4),
    (9, Dom7alt, 4),
];
// The city-pop "royal road": IV - V - iii - vi, then ii - bII (tritone sub).
const ROYAL_ROAD: Prog = &[
    (5, Maj9, 4),
    (7, Sus9, 2),
    (7, Dom13, 2),
    (4, Min9, 2),
    (9, Min9, 2),
    (2, Min9, 2),
    (1, Dom13s11, 2),
];
const VILLAGE_B: Prog = &[
    (0, Maj9, 4),
    (10, Min9, 2),
    (3, Dom13, 2),
    (8, Maj7s11, 4),
    (7, Dom7alt, 4),
];
const EXPLORE_A: Prog = &[(0, Maj9, 8), (5, Maj7s11, 8)];
const EXPLORE_B: Prog = &[(2, Min11, 4), (7, Sus9, 4), (0, Maj69, 4), (9, Dom7alt, 4)];
const EXPLORE_HOOK: Prog = &[(5, Maj9, 4), (4, Min9, 4), (2, Min9, 4), (1, Dom13s11, 4)];
const FOREST_A: Prog = &[(0, Maj7s11, 8), (9, Min11, 8)];
// Descending lydian chords, ECM-style.
const FOREST_B: Prog = &[(5, Maj7s11, 4), (4, Min9, 4), (3, Maj7s11, 4), (2, Sus9, 4)];
// I - iii - IV - iv (the borrowed minor iv).
const MINOR_FOUR: Prog = &[(0, Maj9, 4), (4, Min9, 4), (5, Maj7s11, 4), (5, Min6, 4)];
const MAZE_A: Prog = &[(0, Min11, 8), (8, Maj7s11, 8)];
// Phrygian bII lydian, then a sus and an altered V.
const MAZE_B: Prog = &[
    (0, Min11, 4),
    (1, Maj7s11, 4),
    (10, Sus9, 4),
    (7, Dom7alt, 4),
];
const MAZE_HOOK: Prog = &[
    (5, Min9, 4),
    (10, Dom13, 4),
    (3, Maj9, 4),
    (8, Maj7s11, 2),
    (7, Dom7alt, 2),
];
// The sewers (#127): a dorian i - iv vamp, then borrowed lydian chords
// dripping down to the minor V.
const SEWER_A: Prog = &[(0, Min9, 8), (5, Min11, 8)];
const SEWER_B: Prog = &[
    (8, Maj7s11, 4),
    (3, Maj9, 4),
    (1, Maj7s11, 4),
    (7, Dom7b9, 4),
];
const SEWER_HOOK: Prog = &[
    (0, Min11, 4),
    (10, Sus9, 4),
    (8, Maj7s11, 4),
    (7, Dom7alt, 4),
];
const SKY_B: Prog = &[(9, Min11, 4), (2, Min9, 4), (7, Sus9, 4), (0, Maj69, 4)];
const SKY_HOOK: Prog = &[(5, Maj7s11, 4), (7, Sus9, 4), (4, Min9, 4), (9, Min11, 4)];
// A bolero cycle in minor: i - ii-V - i - V/iv - iv - bVII - bIII - ii-V.
const BOLERO_A: Prog = &[
    (0, Min6, 4),
    (2, HalfDim, 2),
    (7, Dom7b9, 2),
    (0, Min6, 4),
    (0, Dom7b9, 4),
    (5, Min9, 4),
    (10, Dom13, 4),
    (3, Maj9, 4),
    (2, HalfDim, 2),
    (7, Dom7b9, 2),
];
const BOLERO_B: Prog = &[
    (3, Maj9, 4),
    (8, Maj7s11, 4),
    (2, HalfDim, 4),
    (7, Dom7alt, 4),
];
const HIDE_A: Prog = &[
    (0, MinMaj9, 4),
    (1, Maj7s11, 4),
    (0, MinMaj9, 4),
    (11, Dim7, 4),
];
const HIDE_B: Prog = &[
    (8, Maj7s11, 4),
    (7, Dom7b9, 4),
    (0, Min11, 4),
    (1, Dom13s11, 4),
];
// Dorian vamp i9 - IV9.
const SEEK_A: Prog = &[(0, Min9, 8), (5, Dom9, 8)];
const SEEK_B: Prog = &[
    (8, Maj7s11, 4),
    (10, Sus9, 4),
    (0, Min11, 4),
    (7, Dom7s9, 4),
];
// bVI - bVII - i.
const SEEK_HOOK: Prog = &[(8, Maj7s11, 4), (10, Dom13, 4), (0, Min9, 8)];
const DUEL_A: Prog = &[
    (0, Min11, 4),
    (10, Sus9, 4),
    (8, Maj7s11, 4),
    (7, Dom7alt, 4),
];
const DUEL_B: Prog = &[
    (1, Maj7s11, 4),
    (0, Min11, 4),
    (1, Maj7s11, 4),
    (7, Dom7s9, 4),
];
const DUEL_HOOK: Prog = &[
    (8, Maj9, 2),
    (10, Dom13, 2),
    (0, Min9, 4),
    (5, Min9, 2),
    (7, Dom7alt, 2),
    (0, Min11, 4),
];
const LANDLORD_A: Prog = &[
    (0, MinMaj9, 4),
    (1, Dom13s11, 4),
    (0, MinMaj9, 4),
    (11, Dim7, 4),
];
// Chromatically climbing sus chords: the rent goes up.
const LANDLORD_B: Prog = &[(0, Sus9, 4), (1, Sus9, 4), (2, Sus9, 4), (7, Dom7alt, 4)];
const LANDLORD_HOOK: Prog = &[
    (8, Maj7s11, 4),
    (7, Dom7alt, 4),
    (0, MinMaj9, 4),
    (2, HalfDim, 2),
    (7, Dom7b9, 2),
];
// The victory cadence: bVI - bVII - I.
const VICTORY: Prog = &[(8, Maj7s11, 2), (10, Dom13, 2), (0, Maj9, 4)];

// --- Bass templates (one char per 16th) ----------------------------------
// R root, P octave pop, G ghost (dead note), 5/3/7 chord tones,
// A chromatic approach to the next chord, - hold, . rest.
const SLAP1: &str = "R.GR.PGR..R.PGRA";
const SLAP2: &str = "R..GP..RR.GP.5-A";
const PUMP: &str = "R.R.P.R.R.R.P.RA";
const LAID: &str = "R-----.5..R---A.";
const PEDAL: &str = "R-------5-------";
const SPARSE: &str = "R-----....R-....";
// Dub: a held root, space, and a late push back to it.
const DUB: &str = "R---..R-..5-.RA.";
const BOLERO: &str = "R-------5---3-A-";
const TENSE: &str = "R.R.R.R.R.R.R.R.";
const DRIVE: &str = "R.RP.RG.R.RP.RGA";
const SHRED: &str = "RGRPRGR5RGRPRG7A";
const MARCH: &str = "R...R...R...R.A.";

// Upbeat songs (more tunes for the wandering moods).
// "Sunny Side": I - vi - ii - V, bright and bouncy.
const SUNNY_A: Prog = &[(0, Maj9, 4), (9, Min9, 4), (2, Min9, 4), (7, Dom13, 4)];
// iii - VI7 - ii - V: the turnaround that keeps it moving.
const SUNNY_B: Prog = &[
    (4, Min9, 4),
    (9, Dom7s9, 4),
    (2, Min9, 4),
    (7, Sus9, 2),
    (7, Dom13, 2),
];
// "Night Market": vi - IV - V - I, driving.
const MARKET_A: Prog = &[(9, Min9, 4), (5, Maj9, 4), (7, Dom13, 4), (0, Maj69, 4)];
const MARKET_B: Prog = &[
    (2, Min9, 4),
    (7, Dom13s11, 4),
    (0, Maj9, 4),
    (9, Dom7alt, 4),
];
const MARKET_HOOK: Prog = &[
    (5, Maj9, 4),
    (7, Dom13, 4),
    (4, Min9, 4),
    (9, Min9, 2),
    (1, Dom13s11, 2),
];
// "Kite Day": lydian lift between I and II, sunny and open.
const KITE_A: Prog = &[(0, Maj9, 4), (2, Dom13, 4), (0, Maj9, 4), (2, Dom13, 4)];
const KITE_B: Prog = &[(9, Min11, 4), (5, Maj7s11, 4), (2, Min9, 4), (7, Sus9, 4)];
const KITE_HOOK: Prog = &[(5, Maj9, 4), (7, Dom13, 4), (9, Min9, 4), (0, Maj69, 4)];

/// How many songs a mood has to choose from.
fn songs(m: Mood) -> usize {
    match m {
        Mood::Village => 3,
        Mood::Explore => 2,
        _ => 1,
    }
}

fn style(m: Mood) -> &'static Style {
    style_song(m, 0)
}

/// Mood `m`'s song number `song` (0 = its original).
fn style_song(m: Mood, song: u8) -> &'static Style {
    const SUNNY: Style = Style {
        bpm: 118.0,
        swing: 0.08,
        key: 7,
        a: SUNNY_A,
        b: SUNNY_B,
        hook: ROYAL_ROAD,
        groove: Groove::City,
        bass: &[PUMP, SLAP2],
        comp: Comp::City,
        lead: LeadKind::Synth,
        arp: None,
        pad: false,
        intensity: 0.62,
        odd: 0.1,
        modulate: 0.45,
        kime: 0.35,
        theme: false,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Pad,
        horns: Horns::Both,
        kit: Kit {
            clap: true,
            shaker: true,
            ..KIT_NONE
        },
        answer: Some(Inst::PolyBrass),
        nylon: Nylon::None,
    };
    const MARKET: Style = Style {
        bpm: 126.0,
        swing: 0.04,
        key: 10,
        a: MARKET_A,
        b: MARKET_B,
        hook: MARKET_HOOK,
        groove: Groove::Drive,
        bass: &[DRIVE, SLAP1],
        comp: Comp::Stabs,
        lead: LeadKind::Brass,
        arp: None,
        pad: true,
        intensity: 0.68,
        odd: 0.2,
        modulate: 0.35,
        kime: 0.5,
        theme: false,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Pad,
        horns: Horns::Stabs,
        kit: Kit {
            clap: true,
            conga: true,
            ride: true,
            ..KIT_NONE
        },
        answer: Some(Inst::Kalimba),
        nylon: Nylon::None,
    };
    const KITE: Style = Style {
        bpm: 112.0,
        swing: 0.1,
        key: 0,
        a: KITE_A,
        b: KITE_B,
        hook: KITE_HOOK,
        groove: Groove::City,
        bass: &[SLAP1, PUMP],
        comp: Comp::Laid,
        lead: LeadKind::Mini,
        arp: Some(Inst::Kalimba),
        pad: true,
        intensity: 0.55,
        odd: 0.0,
        modulate: 0.3,
        kime: 0.25,
        theme: false,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Strings,
        horns: Horns::Swells,
        kit: Kit {
            shaker: true,
            conga: true,
            ..KIT_NONE
        },
        answer: Some(Inst::Flute),
        nylon: Nylon::Pick(&[Section::Intro, Section::Breakdown]),
    };
    match (m, song) {
        (Mood::Village, 1) => return &SUNNY,
        (Mood::Village, 2) => return &MARKET,
        (Mood::Explore, 1) => return &KITE,
        _ => {}
    }
    const MENU: Style = Style {
        bpm: 102.0,
        swing: 0.12,
        key: 1,
        a: JTTOU,
        b: CHROMATIC_TWO_FIVES,
        hook: JTTOU,
        groove: Groove::City,
        bass: &[SLAP1, PUMP],
        comp: Comp::City,
        lead: LeadKind::Synth,
        arp: None,
        pad: true,
        intensity: 0.55,
        odd: 0.15,
        modulate: 0.5,
        kime: 0.4,
        theme: true,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Pad,
        horns: Horns::Both,
        kit: Kit {
            clap: true,
            ride: true,
            ..KIT_NONE
        },
        answer: Some(Inst::PolyBrass),
        nylon: Nylon::None,
    };
    const EXPLORE: Style = Style {
        bpm: 92.0,
        swing: 0.18,
        key: 5,
        a: EXPLORE_A,
        b: EXPLORE_B,
        hook: EXPLORE_HOOK,
        groove: Groove::Laid,
        bass: &[LAID],
        comp: Comp::Laid,
        lead: LeadKind::Mini,
        arp: Some(Inst::Kalimba),
        pad: true,
        intensity: 0.35,
        odd: 0.0,
        modulate: 0.25,
        kime: 0.15,
        theme: false,
        bass_inst: Inst::Fretless,
        pad_inst: Inst::Strings,
        horns: Horns::Swells,
        kit: Kit {
            shaker: true,
            conga: true,
            ..KIT_NONE
        },
        answer: Some(Inst::Flute),
        nylon: Nylon::Pick(&[Section::Intro, Section::A, Section::Breakdown]),
    };
    const VILLAGE: Style = Style {
        bpm: 104.0,
        swing: 0.14,
        key: 3,
        a: ROYAL_ROAD,
        b: VILLAGE_B,
        hook: JTTOU,
        groove: Groove::City,
        bass: &[SLAP1, SLAP2, PUMP],
        comp: Comp::City,
        lead: LeadKind::Brass,
        arp: None,
        pad: false,
        intensity: 0.55,
        odd: 0.2,
        modulate: 0.4,
        kime: 0.45,
        theme: true,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Pad,
        horns: Horns::Both,
        kit: Kit {
            clap: true,
            shaker: true,
            conga: true,
            ..KIT_NONE
        },
        answer: None,
        nylon: Nylon::None,
    };
    const FOREST: Style = Style {
        bpm: 84.0,
        swing: 0.08,
        key: 2,
        a: FOREST_A,
        b: FOREST_B,
        hook: MINOR_FOUR,
        groove: Groove::Brush,
        bass: &[PEDAL, LAID],
        comp: Comp::Long,
        lead: LeadKind::Flute,
        arp: Some(Inst::Harp),
        pad: true,
        intensity: 0.3,
        odd: 0.0,
        modulate: 0.2,
        kime: 0.0,
        theme: false,
        bass_inst: Inst::Fretless,
        pad_inst: Inst::Strings,
        horns: Horns::None,
        kit: Kit {
            shaker: true,
            ..KIT_NONE
        },
        answer: Some(Inst::Harp),
        nylon: Nylon::Bossa,
    };
    const MAZE: Style = Style {
        bpm: 76.0,
        swing: 0.1,
        key: 0,
        a: MAZE_A,
        b: MAZE_B,
        hook: MAZE_HOOK,
        groove: Groove::Sparse,
        bass: &[SPARSE],
        comp: Comp::Long,
        lead: LeadKind::Bell,
        arp: None,
        pad: true,
        intensity: 0.3,
        odd: 0.0,
        modulate: 0.15,
        kime: 0.0,
        theme: false,
        bass_inst: Inst::Fretless,
        pad_inst: Inst::Pad,
        horns: Horns::Swells,
        kit: KIT_NONE,
        answer: Some(Inst::Strings),
        nylon: Nylon::None,
    };
    const UNDERGROUND: Style = Style {
        bpm: 84.0,
        swing: 0.16,
        key: 2,
        a: SEWER_A,
        b: SEWER_B,
        hook: SEWER_HOOK,
        groove: Groove::Laid,
        bass: &[DUB],
        comp: Comp::Stabs,
        lead: LeadKind::Flute,
        arp: Some(Inst::Kalimba),
        pad: true,
        intensity: 0.35,
        odd: 0.0,
        modulate: 0.15,
        kime: 0.0,
        theme: false,
        bass_inst: Inst::Fretless,
        pad_inst: Inst::Strings,
        horns: Horns::None,
        kit: Kit {
            conga: true,
            ..KIT_NONE
        },
        answer: Some(Inst::EPiano),
        nylon: Nylon::None,
    };
    const SKY: Style = Style {
        bpm: 70.0,
        swing: 0.15,
        key: 8,
        a: MINOR_FOUR,
        b: SKY_B,
        hook: SKY_HOOK,
        groove: Groove::Dream,
        bass: &[PEDAL],
        comp: Comp::Long,
        lead: LeadKind::Mini,
        arp: Some(Inst::Bell),
        pad: true,
        intensity: 0.25,
        odd: 0.0,
        modulate: 0.3,
        kime: 0.0,
        theme: false,
        bass_inst: Inst::Fretless,
        pad_inst: Inst::Strings,
        horns: Horns::Swells,
        kit: Kit {
            shaker: true,
            ..KIT_NONE
        },
        answer: Some(Inst::Kalimba),
        nylon: Nylon::Pick(&[Section::Intro, Section::B, Section::Breakdown]),
    };
    const ABUELA: Style = Style {
        bpm: 84.0,
        swing: 0.05,
        key: 9,
        a: BOLERO_A,
        b: BOLERO_B,
        hook: BOLERO_A,
        groove: Groove::Bolero,
        bass: &[BOLERO],
        comp: Comp::Bolero,
        lead: LeadKind::Flute,
        arp: None,
        pad: true,
        intensity: 0.4,
        odd: 0.0,
        modulate: 0.1,
        kime: 0.0,
        theme: false,
        bass_inst: Inst::Fretless,
        pad_inst: Inst::Strings,
        horns: Horns::Swells,
        kit: Kit {
            shaker: true,
            ..KIT_NONE
        },
        answer: Some(Inst::Guitar),
        nylon: Nylon::None,
    };
    const HIDING: Style = Style {
        bpm: 96.0,
        swing: 0.0,
        key: 6,
        a: HIDE_A,
        b: HIDE_B,
        hook: HIDE_A,
        groove: Groove::Tense,
        bass: &[TENSE],
        comp: Comp::Tense,
        lead: LeadKind::Silent,
        arp: None,
        pad: true,
        intensity: 0.35,
        odd: 0.0,
        modulate: 0.3,
        kime: 0.0,
        theme: false,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Pad,
        horns: Horns::None,
        kit: KIT_NONE,
        answer: None,
        nylon: Nylon::None,
    };
    const SEEKING: Style = Style {
        bpm: 128.0,
        swing: 0.06,
        key: 4,
        a: SEEK_A,
        b: SEEK_B,
        hook: SEEK_HOOK,
        groove: Groove::Drive,
        bass: &[DRIVE, SLAP2],
        comp: Comp::Stabs,
        lead: LeadKind::Mini,
        arp: None,
        pad: false,
        intensity: 0.75,
        odd: 0.25,
        modulate: 0.3,
        kime: 0.5,
        theme: false,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Pad,
        horns: Horns::Stabs,
        kit: Kit {
            clap: true,
            conga: true,
            ..KIT_NONE
        },
        answer: Some(Inst::EPiano),
        nylon: Nylon::None,
    };
    const DUEL: Style = Style {
        bpm: 150.0,
        swing: 0.0,
        key: 11,
        a: DUEL_A,
        b: DUEL_B,
        hook: DUEL_HOOK,
        groove: Groove::Shred,
        bass: &[SHRED, DRIVE],
        comp: Comp::Stabs,
        lead: LeadKind::Mini,
        arp: None,
        pad: false,
        intensity: 0.95,
        odd: 0.35,
        modulate: 0.35,
        kime: 0.7,
        theme: false,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Pad,
        horns: Horns::Stabs,
        kit: Kit {
            clap: true,
            ride: true,
            ..KIT_NONE
        },
        answer: None,
        nylon: Nylon::None,
    };
    const LANDLORD: Style = Style {
        bpm: 108.0,
        swing: 0.08,
        key: 1,
        a: LANDLORD_A,
        b: LANDLORD_B,
        hook: LANDLORD_HOOK,
        groove: Groove::March,
        bass: &[MARCH],
        comp: Comp::Tense,
        lead: LeadKind::Brass,
        arp: None,
        pad: true,
        intensity: 0.5,
        odd: 0.15,
        modulate: 0.6,
        kime: 0.3,
        theme: false,
        bass_inst: Inst::Bass,
        pad_inst: Inst::Strings,
        horns: Horns::Both,
        kit: KIT_NONE,
        answer: Some(Inst::Strings),
        nylon: Nylon::None,
    };
    match m {
        Mood::Menu | Mood::Still => &MENU,
        Mood::Explore => &EXPLORE,
        Mood::Village => &VILLAGE,
        Mood::Forest => &FOREST,
        Mood::Maze => &MAZE,
        Mood::Sky => &SKY,
        Mood::Abuela => &ABUELA,
        Mood::Hiding => &HIDING,
        Mood::Seeking => &SEEKING,
        Mood::Duel => &DUEL,
        Mood::Landlord => &LANDLORD,
        Mood::Underground => &UNDERGROUND,
    }
}

/// The signature hook, written out (over "Just the Two of Us" in Db):
/// per bar, (16th step, length in 16ths, MIDI note).
const THEME: [&[(u8, u8, u8)]; 4] = [
    &[(0, 2, 70), (2, 2, 73), (4, 5, 77), (10, 2, 75), (12, 4, 73)],
    &[(2, 2, 69), (4, 2, 72), (6, 4, 75), (10, 2, 73), (12, 4, 72)],
    &[(0, 3, 73), (3, 3, 77), (6, 2, 80), (8, 4, 77)],
    &[
        (0, 2, 75),
        (2, 2, 78),
        (4, 2, 80),
        (6, 2, 78),
        (8, 2, 77),
        (10, 2, 75),
        (12, 4, 70),
    ],
];
const THEME_KEY: u8 = 1;

/// Syncopated one-bar rhythm cells for motifs: (step, length).
const CELLS: [&[(u8, u8)]; 8] = [
    &[(0, 2), (2, 2), (4, 4), (10, 2), (12, 4)],
    &[(2, 1), (3, 1), (4, 2), (6, 4), (12, 3)],
    &[(0, 3), (3, 3), (6, 2), (8, 6)],
    &[(0, 1), (1, 1), (2, 1), (3, 3), (6, 1), (7, 5)],
    &[(4, 2), (6, 2), (8, 2), (10, 6)],
    &[(0, 6), (6, 2), (8, 2), (10, 2), (12, 4)],
    &[(1, 2), (3, 3), (6, 3), (9, 3), (12, 4)],
    &[(0, 2), (3, 2), (6, 2), (8, 8)],
];

/// Unison hit rhythms (steps) for kimes, 4/4 and 7/8.
const KIMES: [&[u8]; 3] = [&[0, 3, 6, 10, 12], &[0, 2, 5, 8, 11, 14], &[0, 3, 6, 8, 12]];
const KIME_78: &[u8] = &[0, 2, 4, 7, 10, 12];

const LEAD_LO: u8 = 62;
const LEAD_HI: u8 = 84;

#[derive(Clone, Copy, Debug, Default)]
struct MNote {
    step: u8,
    len: u8,
    /// Scale steps from the motif's anchor.
    c: i8,
}

#[derive(Clone, Copy, Debug, Default)]
struct Motif {
    n: usize,
    notes: [MNote; 10],
}

/// A line of absolute notes for one bar: (step, length, MIDI note).
#[derive(Clone, Copy, Debug)]
struct Line {
    n: usize,
    notes: [(u8, u8, u8); 20],
}

impl Line {
    fn new() -> Self {
        Self {
            n: 0,
            notes: [(0, 0, 0); 20],
        }
    }
    fn push(&mut self, step: u8, len: u8, note: u8) {
        if self.n < self.notes.len() {
            self.notes[self.n] = (step, len, note);
            self.n += 1;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Plan {
    kind: Section,
    bars: u16,
    bar: u16,
    key: u8,
    prog: Prog,
    fixed: Option<Chord>,
    /// Approach chord over the second half of the last bar.
    tail: Option<Chord>,
}

impl Plan {
    fn prog_beats(&self) -> u32 {
        self.prog.iter().map(|c| c.2 as u32).sum::<u32>().max(1)
    }

    fn chord_at(&self, bar: u16, step: u32) -> Chord {
        if let Some(c) = self.fixed {
            return c;
        }
        if bar + 1 >= self.bars
            && step >= 8
            && let Some(t) = self.tail
        {
            return t;
        }
        let beat = (bar as u32 * 4 + step / 4) % self.prog_beats();
        let mut acc = 0;
        for &(off, q, b) in self.prog {
            if beat < acc + b as u32 {
                return Chord::new(self.key + off, q);
            }
            acc += b as u32;
        }
        Chord::new(self.key, Maj9)
    }

    fn first(&self) -> Chord {
        self.chord_at(0, 0)
    }
}

/// What the director reads from the game, once per bar.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Cue {
    pub mood: Mood,
    pub intensity: f32,
    /// Bumped by the game for every victory (round won, chapter done...).
    pub victories: u32,
    /// How high the player has climbed, in storeys (0 on the ground, up to
    /// [`LIFT_MAX`]): the key climbs a step of the scale for each.
    pub lift: u8,
    /// Which way the player is moving vertically, -1 (falling, heading
    /// down) .. 1 (climbing): melodies and arpeggios lean that way.
    pub climb: f32,
}

/// The key ladder: semitones above the tune's key for each storey climbed
/// (up the major scale: do, re, mi, fa, sol).
const LIFT: [u8; 5] = [0, 2, 4, 5, 7];
/// The highest storey the key follows.
pub const LIFT_MAX: u8 = LIFT.len() as u8 - 1;

/// One planned bar: its length and what to play in it.
pub struct Bar {
    pub len: u32,
    pub events: Vec<Ev>,
    pub bus: [f32; BUSES],
    /// Master gain for the bar (linear), see [`Mood::level_db`].
    pub gain: f32,
    /// How bright the band plays (0..1): rises through a tune's sections
    /// and across each one, so the sounds open up and settle back.
    pub bright: f32,
    /// Reverb and echo send scales for the bar.
    pub wet: f32,
    pub echo: f32,
    /// The mood's colour and feel.
    pub tone: Tone,
}

impl Bar {
    pub fn new() -> Self {
        Self {
            len: RATE,
            events: Vec::with_capacity(EVENT_CAP),
            bus: [0.0; BUSES],
            gain: 1.0,
            bright: 0.5,
            wet: 1.0,
            echo: 1.0,
            tone: Tone::NEUTRAL,
        }
    }
}

impl Default for Bar {
    fn default() -> Self {
        Self::new()
    }
}

const EVENT_CAP: usize = 384;

/// The composer and director (lives on the audio thread).
pub struct Composer {
    rng: u64,
    /// The mood being played (changes only at bar lines).
    pub mood: Mood,
    /// The style used while still (the mood before the stillness).
    style_mood: Mood,
    /// Which song each mood is playing (see [`songs`]).
    song: [u8; 16],
    pub section: Section,
    intensity: f32,
    bpm: f32,
    plan: Plan,
    next: Option<Plan>,
    /// Mood the transition bar now playing leads into.
    switching: Option<Mood>,
    /// The keys anticipated this bar's downbeat at the end of the last one.
    pushed: bool,
    /// Mood to return to after a victory.
    key_shift: u8,
    ep: Voicing,
    pad: Voicing,
    horn: Voicing,
    /// Which comping/ostinato variant this section uses.
    comp_var: usize,
    /// The horns held a swell this bar (the pad sits out).
    swelled: bool,
    motif: Motif,
    hook: Motif,
    last_note: u8,
    /// Where this phrase's motif started (restated later).
    anchor: u8,
    /// The lead's last notes (for the answering instrument) and where its
    /// notes in this bar end.
    frag: Line,
    lead_end: u32,
    lead_this_bar: bool,
    victories: u32,
    /// The storey the key is on, and the one this bar's second half is
    /// modulating to (over `bridge`, the new key's dominant).
    lift: u8,
    lift_to: Option<u8>,
    bridge: Chord,
    /// A modulation landed on this bar's downbeat.
    landed: bool,
    climb: f32,
    /// The nylon guitar's left hand.
    gtr: Voicing,
    pub bars_played: u64,
    // Per-bar scratch.
    steps: u32,
    step_len: f32,
    swing: f32,
    /// Every lead note written on a strong beat with its chord (tests).
    #[cfg(test)]
    pub strong_log: Vec<(u8, Chord)>,
}

impl Composer {
    pub fn new(seed: u64) -> Self {
        let mood = Mood::Menu;
        let s = style(mood);
        let mut c = Self {
            rng: seed | 1,
            mood,
            style_mood: mood,
            song: [0; 16],
            section: Section::Intro,
            intensity: s.intensity,
            bpm: s.bpm,
            plan: Plan {
                kind: Section::Intro,
                bars: 4,
                bar: 0,
                key: s.key,
                prog: s.a,
                fixed: None,
                tail: None,
            },
            next: None,
            switching: None,
            pushed: false,
            key_shift: 0,
            ep: [53, 57, 60, 64],
            pad: [60, 64, 67, 71],
            horn: [62, 65, 69, 72],
            comp_var: 0,
            swelled: false,
            motif: Motif::default(),
            hook: Motif::default(),
            last_note: 72,
            anchor: 72,
            frag: Line::new(),
            lead_end: 0,
            lead_this_bar: false,
            victories: 0,
            lift: 0,
            lift_to: None,
            bridge: Chord::new(7, Dom7alt),
            landed: false,
            climb: 0.0,
            gtr: [52, 57, 60, 64],
            bars_played: 0,
            steps: 16,
            step_len: 1.0,
            swing: 0.0,
            #[cfg(test)]
            strong_log: Vec::new(),
        };
        c.motif = c.new_motif();
        c.hook = c.new_motif();
        c
    }

    /// The tempo of the bar just written.
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn chance(&mut self, p: f32) -> bool {
        self.rand() < p
    }

    fn pick(&mut self, n: usize) -> usize {
        ((self.rand() * n as f32) as usize).min(n.saturating_sub(1))
    }

    fn style(&self) -> &'static Style {
        if self.mood == Mood::Still {
            style_song(self.style_mood, self.song[self.style_mood as usize % 16])
        } else {
            style_song(self.mood, self.song[self.mood as usize % 16])
        }
    }

    /// The chord `step` 16ths after this bar's start (may run into later
    /// bars or the next planned section). While the key is climbing, the
    /// second half of the bar is the new key's dominant and everything
    /// after it is heard in the new key.
    fn chord(&self, step: u32) -> Chord {
        if let Some(to) = self.lift_to {
            if step >= 16 {
                let c = self.raw_chord(step);
                let d = LIFT[to as usize] as i32 - LIFT[self.lift as usize] as i32;
                return Chord::new((c.root as i32 + d).rem_euclid(12) as u8, c.q);
            }
            if step >= 8 {
                return self.bridge;
            }
        }
        self.raw_chord(step)
    }

    fn raw_chord(&self, step: u32) -> Chord {
        let bar = self.plan.bar as u32 + step / 16;
        if bar >= self.plan.bars as u32 {
            if let Some(n) = &self.next {
                return n.chord_at((bar - self.plan.bars as u32) as u16, step % 16);
            }
            return self.plan.chord_at(self.plan.bars - 1, 15);
        }
        self.plan.chord_at(bar as u16, step % 16)
    }

    /// Plan the section after `prev` in mood `m`.
    fn plan_section(&mut self, m: Mood, prev: Section, fresh: bool) -> Plan {
        // A new visit picks a song; after a breakdown the band may move on
        // to another one.
        let n = songs(m);
        if n > 1 && fresh {
            self.song[m as usize % 16] = self.pick(n) as u8;
        } else if n > 1 && prev == Section::Breakdown && self.chance(0.6) {
            let now = self.song[m as usize % 16] as usize;
            self.song[m as usize % 16] = ((now + 1 + self.pick(n - 1)) % n) as u8;
        }
        let s = style_song(m, self.song[m as usize % 16]);
        let i = self.intensity;
        let kind = if m == Mood::Still {
            Section::Still
        } else if fresh {
            if i < 0.6 && !m.urgent() {
                Section::Intro
            } else {
                Section::A
            }
        } else {
            match prev {
                Section::Intro | Section::Breakdown | Section::Victory | Section::Still => {
                    Section::A
                }
                Section::A => Section::Hook,
                Section::Hook => {
                    if self.chance(0.6) {
                        Section::B
                    } else if i > 0.45 && s.lead != LeadKind::Silent {
                        Section::Solo
                    } else {
                        Section::A
                    }
                }
                Section::B => {
                    if i > 0.45 && s.lead != LeadKind::Silent {
                        Section::Solo
                    } else if i < 0.35 {
                        Section::Breakdown
                    } else {
                        Section::Hook
                    }
                }
                Section::Solo => {
                    if self.chance(0.3) {
                        Section::Breakdown
                    } else {
                        Section::Hook
                    }
                }
            }
        };
        if fresh {
            self.key_shift = 0;
        } else if kind != Section::Still && self.chance(s.modulate) {
            self.key_shift += if self.chance(0.5) { 1 } else { 2 };
            if self.key_shift > 4 {
                // Back down home (a fourth up is the same pitch classes).
                self.key_shift = 0;
            }
        }
        let key = (s.key + self.key_shift) % 12;
        let (prog, bars) = match kind {
            Section::Intro => (s.a, 4),
            Section::A => (s.a, 8),
            Section::Hook => (s.hook, 8),
            Section::B => (s.b, 8),
            Section::Solo => (if self.chance(0.5) { s.a } else { s.b }, 8),
            Section::Breakdown => (s.a, 4),
            Section::Victory => (VICTORY, 2),
            Section::Still => (s.a, 2),
        };
        let fixed = (kind == Section::Still).then(|| self.chord(0));
        if matches!(kind, Section::B | Section::Solo) || fresh {
            self.motif = self.new_motif();
        }
        self.comp_var = self.pick(4);
        if fresh {
            self.hook = self.new_motif();
        }
        Plan {
            kind,
            bars,
            bar: 0,
            key,
            prog,
            fixed,
            tail: None,
        }
    }

    /// An approach chord into `to`: its V7alt, its tritone sub, or (for
    /// minor targets) a V7b9.
    fn approach(&mut self, to: Chord) -> Chord {
        if to.q.is_minor() {
            Chord::new(to.root + 7, Dom7b9)
        } else if self.chance(0.5) {
            Chord::new(to.root + 1, Dom13s11)
        } else {
            Chord::new(to.root + 7, Dom7alt)
        }
    }

    /// Decide and write the next bar. Called exactly at every bar line.
    pub fn next_bar(&mut self, cue: Cue, bar: &mut Bar) {
        bar.events.clear();
        self.intensity += (cue.intensity.clamp(0.0, 1.0) - self.intensity) * 0.5;
        self.climb = cue.climb.clamp(-1.0, 1.0);
        if self.bars_played > 0 {
            self.plan.bar += 1;
        }
        let mut new_mood = false;

        if cue.victories != self.victories {
            let fresh_victory = cue.victories > self.victories
                && !matches!(self.mood, Mood::Still | Mood::Menu)
                && self.plan.kind != Section::Victory;
            self.victories = cue.victories;
            if fresh_victory {
                let s = self.style();
                self.next = None;
                self.plan = Plan {
                    kind: Section::Victory,
                    bars: 2,
                    bar: 0,
                    key: (s.key + self.key_shift) % 12,
                    prog: VICTORY,
                    fixed: None,
                    tail: None,
                };
            }
        }

        if cue.mood != self.mood && self.plan.kind != Section::Victory {
            let target = cue.mood;
            if self.switching.is_some() || target.urgent() || self.mood == Mood::Still {
                // Cut in at this bar line.
                self.switching = None;
                if target == Mood::Still {
                    self.style_mood = self.mood;
                }
                let fresh = target != Mood::Still && self.mood != Mood::Still;
                let from_still = self.mood == Mood::Still;
                self.mood = target;
                let prev = if from_still {
                    Section::Still
                } else {
                    self.plan.kind
                };
                self.plan = self.plan_section(target, prev, fresh);
                self.next = None;
                new_mood = true;
            } else {
                // This bar is the transition: fill over an approach chord.
                self.switching = Some(target);
                let next = self.plan_section(target, self.plan.kind, true);
                self.plan.bar = self.plan.bar.min(self.plan.bars.saturating_sub(1));
                self.plan.bars = self.plan.bar + 1;
                let first = next.first();
                self.plan.tail = Some(self.approach(first));
                self.next = Some(next);
            }
        } else if cue.mood == self.mood {
            self.switching = None;
        }

        if self.plan.bar >= self.plan.bars {
            if let Some(m) = self.switching.take() {
                self.mood = m;
                new_mood = true;
            }
            let prev = self.plan.kind;
            let m = self.mood;
            let next = match self.next.take() {
                Some(n) => n,
                None => self.plan_section(m, prev, false),
            };
            self.plan = next;
        }
        // Entering the last bar of a section: plan the next one now so the
        // turnaround can lead into it.
        if self.plan.bar + 1 == self.plan.bars
            && self.next.is_none()
            && self.switching.is_none()
            && !matches!(self.plan.kind, Section::Victory | Section::Still)
        {
            let m = self.mood;
            let kind = self.plan.kind;
            let next = self.plan_section(m, kind, false);
            if next.key != self.plan.key || self.chance(0.5) {
                let first = next.first();
                self.plan.tail = Some(self.approach(first));
            }
            self.next = Some(next);
        }
        self.section = self.plan.kind;
        self.lift_step(cue.lift.min(LIFT_MAX), new_mood);

        let s = self.style();
        // Tempo eases towards the style's (an accelerando into a duel).
        let target_bpm = s.bpm * if self.mood == Mood::Still { 0.9 } else { 1.0 };
        let d = target_bpm - self.bpm;
        self.bpm = if d.abs() < 1.5 {
            target_bpm
        } else {
            self.bpm + d * 0.5
        };
        self.swing = s.swing;
        self.step_len = 60.0 / self.bpm / 4.0 * RATE as f32;
        let last = self.plan.bar + 1 == self.plan.bars;
        let fancy = matches!(self.plan.kind, Section::Hook | Section::Solo | Section::B);
        self.steps = if last
            && fancy
            && self.switching.is_none()
            && self.lift_to.is_none()
            && self.chance(s.odd)
        {
            14
        } else {
            16
        };
        bar.len = (self.steps as f32 * self.step_len) as u32;
        self.bars_played += 1;

        self.layers(&mut bar.bus);
        bar.gain = 10f32.powf(self.mood.level_db() / 20.0);
        self.colour(bar);
        self.write(bar, new_mood);
        self.transpose(bar);
        self.humanize(bar);
        bar.events.sort_unstable_by_key(|e| e.at);
    }

    /// Players aren't sequencers: every phrase breathes (builds to its third
    /// bar, eases off on the fourth), downbeats lean in, no two notes land at
    /// quite the same strength, and chord tones spread by a few milliseconds
    /// instead of striking as one. Drums only get the small touch variation.
    fn humanize(&mut self, bar: &mut Bar) {
        const BREATH: [f32; 4] = [0.97, 1.0, 1.05, 0.96];
        let breath = BREATH[usize::from(self.plan.bar) % 4];
        let tone = self.mood.tone();
        let step_len = self.step_len.max(1.0);
        let last = bar.len.saturating_sub(1);
        for i in 0..bar.events.len() {
            let r1 = self.rand();
            let r2 = self.rand();
            let e = &mut bar.events[i];
            let bus = e.inst.bus();
            if bus == Bus::Drums || matches!(e.inst, Inst::Shaker | Inst::Rim | Inst::Clap) {
                // A drummer leans on the kick and snare, and rides the
                // cymbals and shakers well under them.
                let touch = match e.inst {
                    Inst::Hat | Inst::OpenHat | Inst::Shaker | Inst::Ride => 0.8,
                    Inst::Kick | Inst::Snare => 0.92,
                    _ => 1.0,
                };
                e.vel *= touch * (1.0 + 0.1 * (r1 - 0.5));
                // Feel (per mood): the backbeat drags behind the grid, and
                // the hats wander a little and lean on the beat, lighter on
                // the off-16ths. The kick stays put: it's the anchor.
                let ms = RATE as f32 / 1000.0;
                let step = (e.at as f32 / step_len).round() as u32;
                match e.inst {
                    Inst::Snare | Inst::Clap | Inst::Rim | Inst::Ghost => {
                        let lag = tone.lay_back_ms * (0.7 + 0.6 * r2);
                        e.at = (e.at + (lag * ms) as u32).min(last);
                    }
                    Inst::Hat | Inst::OpenHat | Inst::Shaker | Inst::Ride => {
                        let off = tone.loose_ms * (2.0 * r2 - 1.0) * ms;
                        e.at = ((e.at as f32 + off).max(0.0) as u32).min(last);
                        if step % 2 == 1 {
                            e.vel *= 0.86;
                        } else if step.is_multiple_of(4) {
                            e.vel *= 1.06;
                        }
                    }
                    _ => {}
                }
                continue;
            }
            let r3 = self.rand();
            // Releases don't all line up: each note lets go a little early or
            // late (sustained pads and strings less so).
            let hold = match bus {
                Bus::Pad => 0.9 + 0.2 * r3,
                _ => 0.72 + 0.36 * r3,
            };
            e.len = ((e.len as f32 * hold) as u32).max(1);
            // Play inside the range rather than at the top of it: the same
            // shape, with headroom above (0.73 -> 0.68, 1.0 -> 0.9).
            e.vel = 0.18 + 0.72 * e.vel.clamp(0.0, 1.0).powf(1.15);
            if e.inst == Inst::Flute {
                // The bottle sits back, a breath behind the band.
                e.vel *= 0.85;
            }
            let step = (e.at as f32 / step_len).round() as u32;
            let accent = match step {
                0 => 1.07,
                s if s % 4 == 0 => 1.03,
                s if s % 2 == 1 => 0.96,
                _ => 1.0,
            };
            e.vel = (e.vel * breath * accent * (1.0 + 0.16 * (r1 - 0.5))).clamp(0.05, 1.0);
            if !e.glide {
                // Keys and guitar spread their chords most; bass sits tight.
                let spread = match bus {
                    Bus::Keys | Bus::Guitar | Bus::Pad => 180.0,
                    Bus::Bass => 50.0,
                    _ => 90.0,
                };
                e.at = (e.at + (r2 * spread) as u32).min(last);
            }
        }
    }

    /// Follow the player up and down: land a modulation prepared in the
    /// last bar, then, if the player is on another storey, prepare the next
    /// one (a step at a time) over this bar's second half.
    fn lift_step(&mut self, want: u8, new_mood: bool) {
        self.landed = false;
        if let Some(to) = self.lift_to.take() {
            self.lift = to;
            self.landed = true;
        }
        let last = self.plan.bar + 1 >= self.plan.bars;
        let free = !matches!(self.plan.kind, Section::Victory | Section::Still)
            && !matches!(self.mood, Mood::Menu | Mood::Still)
            && self.switching.is_none()
            && !new_mood
            && !(last && self.plan.tail.is_some());
        if want == self.lift || !free {
            return;
        }
        let to = if want > self.lift {
            self.lift + 1
        } else {
            self.lift - 1
        };
        let d = LIFT[to as usize] as i32 - LIFT[self.lift as usize] as i32;
        let next = self.raw_chord(16);
        let target = Chord::new((next.root as i32 + d).rem_euclid(12) as u8, next.q);
        self.bridge = self.approach(target);
        self.lift_to = Some(to);
    }

    /// Play the bar in the key of the storey the player is on. Past a
    /// fourth up the bass goes down instead, to keep its weight, but never
    /// below a bass guitar's low E.
    fn transpose(&self, bar: &mut Bar) {
        const LOW_E: u8 = 28;
        let up = LIFT[self.lift as usize];
        if up == 0 {
            return;
        }
        for e in bar.events.iter_mut() {
            match e.inst.bus() {
                Bus::Drums => {}
                Bus::Bass if up >= 5 && e.note + up >= LOW_E + 12 => e.note = e.note + up - 12,
                _ => e.note = (e.note + up).min(108),
            }
        }
    }

    /// How the band's sound moves through the tune: brightness climbs from
    /// the intro to the hooks and solos and across each section, and the
    /// room gets wetter in intros, bridges and breakdowns.
    fn colour(&self, bar: &mut Bar) {
        bar.tone = self.mood.tone();
        let (base, wet, echo) = match self.plan.kind {
            Section::Intro => (0.25, 1.35, 1.2),
            Section::A => (0.4, 1.0, 1.0),
            Section::Hook => (0.62, 0.95, 1.0),
            Section::B => (0.5, 1.2, 1.15),
            Section::Solo => (0.75, 1.05, 1.45),
            Section::Breakdown => (0.2, 1.5, 1.4),
            Section::Victory => (0.9, 1.2, 1.0),
            Section::Still => (0.15, 1.6, 1.3),
        };
        let through = self.plan.bar as f32 / self.plan.bars.max(1) as f32;
        let calm = 1.0 - self.intensity;
        bar.bright = (base + 0.18 * through + 0.25 * (self.intensity - 0.5)).clamp(0.05, 1.0);
        bar.wet = 1.3 * wet * (1.0 + 0.3 * calm);
        bar.echo = 1.2 * echo;
        // Higher up the air is clearer and the room bigger.
        let up = self.lift as f32;
        bar.bright = (bar.bright + 0.05 * up).min(1.0);
        bar.wet *= 1.0 + 0.07 * up;
        // Down in the tunnels everything rings off wet brick (#127).
        if self.mood == Mood::Underground {
            bar.wet *= 1.6;
            bar.echo *= 1.6;
        }
    }

    /// Bus levels: which instruments are in for this bar.
    fn layers(&self, bus: &mut [f32; BUSES]) {
        let s = self.style();
        let i = self.intensity;
        let kind = self.plan.kind;
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        *bus = [0.0; BUSES];
        if kind == Section::Still {
            bus[Bus::Pad as usize] = 1.0;
            bus[Bus::Perc as usize] = 0.6;
            return;
        }
        let drums = match kind {
            Section::Intro => on(i > 0.5) * 0.6,
            _ => on(i > 0.12) * if s.groove == Groove::Dream { 0.7 } else { 1.0 },
        };
        bus[Bus::Drums as usize] = drums;
        bus[Bus::Bass as usize] = on(kind != Section::Intro || i > 0.4);
        bus[Bus::Keys as usize] = 1.0;
        bus[Bus::Lead as usize] = on(s.lead != LeadKind::Silent);
        bus[Bus::Pad as usize] = if s.pad {
            1.0
        } else {
            on(kind == Section::Breakdown || i < 0.3) * 0.8
        };
        bus[Bus::Perc as usize] = 1.0;
        bus[Bus::Brass as usize] = on(s.horns != Horns::None);
        bus[Bus::Guitar as usize] =
            on(s.nylon != Nylon::None || s.comp == Comp::Bolero || s.answer == Some(Inst::Guitar));
    }

    /// Is the nylon guitar picking (instead of the keys) in this section?
    fn picking(&self) -> bool {
        match self.style().nylon {
            Nylon::Pick(sections) => sections.contains(&self.plan.kind),
            _ => false,
        }
    }

    /// Sample offset of 16th `step`, with the off-16ths swung late.
    fn at(&self, step: u32) -> u32 {
        let swung = step as f32 + if step % 2 == 1 { self.swing } else { 0.0 };
        (swung * self.step_len).max(0.0) as u32
    }

    fn len(&self, steps: f32) -> u32 {
        (steps * self.step_len) as u32
    }

    fn push(bar: &mut Bar, ev: Ev) {
        if bar.events.len() < EVENT_CAP {
            let ev = Ev {
                at: ev.at.min(bar.len.saturating_sub(1)),
                ..ev
            };
            bar.events.push(ev);
        }
    }

    fn write(&mut self, bar: &mut Bar, new_mood: bool) {
        let kind = self.plan.kind;
        let last = self.plan.bar + 1 == self.plan.bars;
        let i = self.intensity;
        let s = self.style();
        if kind == Section::Still {
            self.write_still(bar);
            return;
        }
        let phrase_end = (self.plan.bar % 4) == 3 || last;
        let kime = self.steps == 14
            || (kind == Section::Victory && self.plan.bar == 0)
            || (last && self.switching.is_none() && i > 0.5 && self.chance(s.kime * 0.6));
        let crash = (self.plan.bar == 0 && i > 0.4 && kind != Section::Intro)
            || (self.landed && i > 0.3)
            || new_mood && i > 0.5
            || (kind == Section::Victory && self.plan.bar == 1);
        self.swelled = false;
        // A reversed cymbal rising into a bigger section or a new mood.
        let into_big = self.switching.is_some()
            || self.lift_to.is_some_and(|t| t > self.lift)
            || (last
                && self
                    .next
                    .is_some_and(|n| matches!(n.kind, Section::Hook | Section::Solo)));
        if into_big && (i > 0.35 || self.lift_to.is_some()) {
            let swell = (super::drums::SWELL_SECS * RATE as f32) as u32;
            if bar.len > swell {
                Self::push(bar, hit(bar.len - swell, Inst::Swell, 57, 0.8 + 0.3 * i));
            }
        }
        if kime {
            self.pushed = false;
            self.write_kime(bar, crash);
        } else {
            let fill =
                (phrase_end && i > 0.3 && self.plan.bar % 8 >= 3) || self.switching.is_some();
            self.write_drums(bar, fill, crash);
            self.write_bass(bar);
            self.write_comp(bar);
            // A note held over the bar line still counts as the lead playing.
            self.lead_this_bar = false;
            self.lead_end = self.lead_end.saturating_sub(16);
            self.write_lead(bar);
            self.write_answer(bar);
            self.write_horns(bar);
        }
        self.write_pad(bar);
        self.write_arp(bar);
    }

    fn write_still(&mut self, bar: &mut Bar) {
        let c = self.chord(0);
        let pad_inst = self.style().pad_inst;
        if self.plan.bar.is_multiple_of(2) {
            self.pad = voice_lead(c, &self.pad, 55, 67);
            let len = self.len(self.steps as f32 * 2.2);
            for n in self.pad {
                Self::push(
                    bar,
                    Ev {
                        at: 0,
                        inst: pad_inst,
                        note: n,
                        vel: 0.6,
                        len,
                        glide: false,
                    },
                );
            }
        }
        if self.plan.bar % 4 == 1 {
            // A single high bell on the 9th, very soft.
            let n = c.root_from(84) + 2;
            let at = self.at(4);
            Self::push(
                bar,
                Ev {
                    at,
                    inst: Inst::Chime,
                    note: n.min(100),
                    vel: 0.35,
                    len: 0,
                    glide: false,
                },
            );
        }
    }

    // --- Drums ----------------------------------------------------------

    fn write_drums(&mut self, bar: &mut Bar, fill: bool, crash: bool) {
        let s = self.style();
        let i = self.intensity;
        let kind = self.plan.kind;
        let odd_bar = self.plan.bar % 2 == 1;
        let (kicks, snare, hat, extra): ([&str; 2], &str, &str, &[(Inst, u8, [&str; 2])]) =
            match s.groove {
                Groove::City => (
                    ["x.....x...x.....", "x.....x.o.x..o.."],
                    "....x..g.g..x..g",
                    "xoxoxoxoxoxoxOxo",
                    &[],
                ),
                Groove::Laid => (
                    ["x.......x.x.....", "x.......x...o..."],
                    "....x.......x..g",
                    "x.o.x.o.x.o.x.oo",
                    &[],
                ),
                Groove::Brush => (
                    ["x.........x.....", "x.......o......."],
                    "....g.......g...",
                    "o.g.o.g.o.g.o.gg",
                    &[],
                ),
                Groove::Sparse => (
                    ["x.........x.....", "x..............."],
                    "................",
                    "..o...o...o...o.",
                    &[(Inst::Rim, 37, ["......o.......o.", "......o....o...."])],
                ),
                Groove::Dream => (
                    ["x...............", "................"],
                    "................",
                    "....g.......g...",
                    &[],
                ),
                Groove::Bolero => (
                    ["x.......x.......", "x.......x.....o."],
                    "................",
                    "ogggogggogggoggg",
                    &[
                        // 3-2 son clave across two bars, and bongos.
                        (Inst::Rim, 37, ["x..x..x.........", "....x...x......."]),
                        (Inst::Conga, 64, ["......og......og", "......og....o.og"]),
                    ],
                ),
                Groove::Tense => (
                    ["x..o............", "x..o............"],
                    "................",
                    "o.g.o.g.o.g.o.g.",
                    &[(Inst::Rim, 37, ["..........o.....", "..........o..o.."])],
                ),
                Groove::Drive => (
                    ["x...x...x...x.o.", "x...x...x..ox..."],
                    "....x..g....x..g",
                    "xoOoxoOoxoOoxoOo",
                    &[],
                ),
                Groove::Shred => (
                    ["x..x..x...x..x..", "x.xx..x.x.x..xx."],
                    "....x.g.g...x.gx",
                    "xoxoxoxoxoxoxoxo",
                    &[],
                ),
                Groove::March => (
                    ["x...x...x...x...", "x...x...x...x.x."],
                    "............x...",
                    "x.o.x.o.x.o.x.o.",
                    &[(Inst::Snare, 38, ["..g.......g.g.g.", "..g...g...g.ggg."])],
                ),
            };
        let soft = match s.groove {
            Groove::Brush | Groove::Dream => 0.6,
            Groove::Sparse | Groove::Tense => 0.75,
            _ => 1.0,
        };
        // Where the backbeat sits: laid-back grooves drag it a hair.
        let lay = match s.groove {
            Groove::Laid | Groove::Brush | Groove::Bolero => 110,
            Groove::Drive | Groove::Shred => 0,
            _ => 50,
        };
        let fill_from = if fill {
            if i > 0.6 { 8 } else { 12 }
        } else {
            99
        };
        let kick = kicks[usize::from(odd_bar && i > 0.3)];
        let steps = self.steps.min(16);
        // The ride takes over from the hats in solos and flat-out duels.
        let ride = s.kit.ride && (kind == Section::Solo || (s.groove == Groove::Shred && i > 0.85));
        let clap = s.kit.clap && (kind == Section::Hook || i > 0.75);
        let shaker = s.kit.shaker && i > 0.2 && kind != Section::Intro;
        let conga = s.kit.conga && i > 0.3 && kind != Section::Intro;
        // Tumbao: heel-toe, a slap on two, open tones on four and its "and".
        const TUMBAO: [&str; 2] = ["h.t.S.t.h.t.o.o.", "h.t.S.t.h.tSl.l."];
        for st in 0..steps {
            let k = st as usize;
            let at = self.at(st);
            if st >= fill_from {
                self.fill_step(bar, st, fill_from, steps);
                continue;
            }
            if let Some(v) = vel(kick, k) {
                Self::push(bar, hit(at, Inst::Kick, 36, v * soft));
            }
            match snare.as_bytes().get(k) {
                Some(b'g') if i > 0.4 => {
                    let v = 0.5 + 0.3 * self.rand();
                    Self::push(bar, hit(at + 40, Inst::Ghost, 38, v * soft));
                }
                Some(b'x') | Some(b'o') => {
                    let v = vel(snare, k).unwrap_or(0.8);
                    Self::push(bar, hit(at + lay, Inst::Snare, 38, v * soft));
                    if clap && st % 8 == 4 {
                        Self::push(bar, hit(at + lay + 25, Inst::Clap, 39, 0.75));
                    }
                }
                _ => {}
            }
            let jitter = (self.rand() * 80.0) as u32;
            match hat.as_bytes().get(k) {
                Some(_) if ride => {
                    // Ride: quarters and the "and"s, bell-ish accents on the
                    // beat, foot hat on two and four.
                    if st % 2 == 0 {
                        let v = if st % 4 == 0 { 0.9 } else { 0.62 } * (0.9 + 0.2 * self.rand());
                        Self::push(bar, hit(at + jitter, Inst::Ride, 51, v));
                    }
                    if st % 8 == 4 {
                        Self::push(bar, hit(at, Inst::Hat, 42, 0.35));
                    }
                }
                Some(b'O') if i > 0.5 => {
                    Self::push(bar, hit(at, Inst::OpenHat, 46, 0.7 * soft));
                }
                Some(c) if *c != b'.' => {
                    // Below mid intensity, 16th hats thin to 8ths.
                    let thin =
                        st % 2 == 1 && i < 0.45 && hat.len() == 16 && hat.as_bytes()[1] != b'.';
                    if !thin {
                        let v = vel(hat, k).unwrap_or(0.6) * (0.85 + 0.3 * self.rand());
                        Self::push(bar, hit(at + jitter, Inst::Hat, 42, v * soft));
                    }
                }
                _ => {}
            }
            for (inst, note, pats) in extra {
                if let Some(v) = vel(pats[usize::from(odd_bar)], k) {
                    let note = match inst {
                        Inst::Conga if v < 0.5 => 71,
                        _ => *note,
                    };
                    Self::push(bar, hit(at, *inst, note, v * soft));
                }
            }
            if shaker && (i > 0.5 || st % 2 == 0) {
                // Accent the "and"s; the beads lean a touch ahead.
                let v = match st % 4 {
                    2 => 0.85,
                    0 => 0.55,
                    _ => 0.38,
                } * (0.85 + 0.3 * self.rand());
                Self::push(
                    bar,
                    hit(at.saturating_sub(60) + jitter, Inst::Shaker, 70, v),
                );
            }
            if conga && let Some(c) = TUMBAO[usize::from(odd_bar)].as_bytes().get(k) {
                let (note, v) = match c {
                    b'h' => (58, 0.22),
                    b't' => (58, 0.3),
                    b'S' => (64, 0.85),
                    b'o' => (58, 0.75),
                    b'l' => (53, 0.8),
                    _ => (0, 0.0),
                };
                if note > 0 {
                    Self::push(bar, hit(at + jitter / 2, Inst::Conga, note, v * soft));
                }
            }
        }
        if crash {
            Self::push(bar, hit(0, Inst::Crash, 49, 0.9));
            Self::push(bar, hit(0, Inst::Kick, 36, 1.0));
        }
    }

    /// One step of a drum fill: snare 16ths swelling, or toms rolling down.
    fn fill_step(&mut self, bar: &mut Bar, st: u32, from: u32, steps: u32) {
        let s = self.style();
        let span = (steps - from).max(1) as f32;
        let p = (st - from) as f32 / span;
        let at = self.at(st);
        match s.groove {
            Groove::Brush | Groove::Dream | Groove::Sparse | Groove::Tense => {
                if st.is_multiple_of(2) {
                    Self::push(bar, hit(at, Inst::Ghost, 38, 0.5 + 0.4 * p));
                }
            }
            _ => {
                let kind = (self.plan.bar / 4 + self.bars_played as u16 / 8) % 3;
                match kind {
                    0 => Self::push(bar, hit(at, Inst::Snare, 38, 0.45 + 0.55 * p)),
                    1 => {
                        let note = [55u8, 55, 50, 50, 45, 45, 45, 45][((p * 8.0) as usize).min(7)];
                        Self::push(bar, hit(at, Inst::Tom, note, 0.7 + 0.3 * p));
                    }
                    _ => {
                        if st.is_multiple_of(2) {
                            Self::push(bar, hit(at, Inst::Snare, 38, 0.8));
                            Self::push(bar, hit(at, Inst::Kick, 36, 0.8));
                        } else {
                            Self::push(bar, hit(at, Inst::Tom, 50, 0.6));
                        }
                    }
                }
                if st.is_multiple_of(4) {
                    Self::push(bar, hit(at, Inst::Kick, 36, 0.7));
                }
            }
        }
    }

    // --- Bass -----------------------------------------------------------

    fn write_bass(&mut self, bar: &mut Bar) {
        let s = self.style();
        let inst = s.bass_inst;
        let i = self.intensity;
        let pick = if s.bass.len() > 1 && self.chance(0.35) {
            self.pick(s.bass.len())
        } else {
            (self.plan.bar as usize / 4) % s.bass.len()
        };
        let pat = s.bass[pick].as_bytes();
        let steps = self.steps.min(16) as usize;
        // At the end of a phrase the bass sometimes fills the last beat.
        let fill = steps == 16
            && self.plan.bar % 4 == 3
            && i > 0.45
            && self.switching.is_none()
            && self.chance(0.45);
        let fill_at = if fill { 12 } else { steps };
        let mut prev: Option<u8> = None;
        for st in 0..fill_at {
            let c = pat[st];
            if matches!(c, b'.' | b'-') {
                if c == b'.' {
                    prev = None;
                }
                continue;
            }
            let hold = pat[st + 1..steps]
                .iter()
                .take_while(|&&b| b == b'-')
                .count();
            let ch = self.chord(st as u32);
            let root = ch.root_from(31);
            let (note, vel, len) = match c {
                b'R' => (root, 0.8, if hold > 0 { hold as f32 + 0.9 } else { 0.7 }),
                b'P' => (root + 12, 1.0, 0.6),
                b'G' => (root, 0.25, 0.25),
                b'5' => (root + ch.q.fifth(), 0.75, hold as f32 + 0.8),
                b'3' => (root + ch.q.third(), 0.75, hold as f32 + 0.8),
                b'7' => (root + ch.q.seventh(), 0.8, 0.8),
                b'A' => {
                    let next = self.chord(st as u32 + 1).root_from(31);
                    let n = if self.chance(0.5) { next - 1 } else { next + 1 };
                    (n, 0.7, hold as f32 + 0.8)
                }
                _ => continue,
            };
            // A fretless slides into nearby notes instead of re-plucking.
            let glide = inst == Inst::Fretless
                && prev.is_some_and(|p| (p as i32 - note as i32).abs() <= 5 && p != note)
                && self.chance(0.35);
            prev = Some(note);
            let at = self.at(st as u32);
            let len = self.len(len);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note,
                    vel: vel * (0.85 + 0.25 * i),
                    len,
                    glide,
                },
            );
        }
        if fill {
            // Four 16ths: up the chord to the octave, or down from it, with
            // the last a half step from the next root.
            let ch = self.chord(12);
            let root = ch.root_from(31);
            let next = self.chord(16).root_from(31);
            let line = if self.chance(0.5) {
                [
                    root + ch.q.fifth(),
                    root + ch.q.seventh(),
                    root + 12,
                    next + 11,
                ]
            } else {
                [
                    root + 12,
                    root + ch.q.seventh(),
                    root + ch.q.fifth(),
                    next + 1,
                ]
            };
            for (k, n) in line.into_iter().enumerate() {
                let st = 12 + k as u32;
                let at = self.at(st);
                let len = self.len(0.85);
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst,
                        note: n,
                        vel: if k == 0 { 0.85 } else { 0.7 },
                        len,
                        glide: inst == Inst::Fretless && k == 3,
                    },
                );
            }
        }
    }

    // --- Keys -----------------------------------------------------------

    fn write_comp(&mut self, bar: &mut Bar) {
        let s = self.style();
        let i = self.intensity;
        if s.comp == Comp::Bolero {
            self.write_guitar(bar);
            return;
        }
        if s.nylon == Nylon::Bossa {
            self.write_bossa(bar);
            return;
        }
        if self.picking() {
            self.write_picking(bar);
            return;
        }
        // A few rhythms per comping style; each phrase picks one, so the
        // keys don't hammer the same figure for minutes.
        type Rh = &'static [(u8, u8, bool)];
        let pats: &[Rh] = match s.comp {
            Comp::City => &[
                &[(0, 3, false), (6, 2, false), (10, 1, false), (14, 2, true)],
                &[(2, 2, false), (6, 1, false), (8, 3, false), (14, 2, true)],
                &[
                    (0, 1, false),
                    (3, 2, false),
                    (7, 1, false),
                    (10, 2, false),
                    (14, 2, true),
                ],
                &[
                    (0, 2, false),
                    (4, 1, false),
                    (7, 3, false),
                    (11, 1, false),
                    (12, 2, false),
                ],
            ],
            Comp::Laid => &[
                &[(0, 6, false), (7, 1, false), (10, 6, false)],
                &[(0, 10, false), (11, 4, false)],
                &[(2, 4, false), (8, 6, false), (14, 2, true)],
            ],
            Comp::Long => &[&[(0, 16, false)], &[(0, 10, false), (10, 6, false)]],
            Comp::Stabs => &[
                &[
                    (0, 1, false),
                    (3, 1, false),
                    (6, 2, false),
                    (10, 1, false),
                    (12, 1, false),
                    (14, 2, true),
                ],
                &[
                    (0, 1, false),
                    (2, 1, false),
                    (6, 1, false),
                    (8, 1, false),
                    (11, 2, false),
                    (14, 2, true),
                ],
                &[
                    (0, 2, false),
                    (3, 1, false),
                    (7, 1, false),
                    (10, 2, false),
                    (13, 1, false),
                ],
            ],
            Comp::Tense | Comp::Bolero => &[&[(0, 12, false), (14, 2, true)]],
        };
        let pat = pats[(self.plan.bar as usize / 4 + self.comp_var) % pats.len()];
        let steps = self.steps;
        // Every chord change in the bar needs a hit at or just before it.
        let mut hits = [(0u8, 0u8, false); 12];
        let mut n = 0;
        for &h in pat {
            if (h.0 as u32) < steps && n < hits.len() {
                hits[n] = h;
                n += 1;
            }
        }
        for change in [4u32, 8, 12] {
            if self.chord(change) != self.chord(change - 1)
                && !hits[..n]
                    .iter()
                    .any(|h| (h.0 as u32) <= change && h.0 as u32 + 2 >= change)
                && n < hits.len()
            {
                hits[n] = (change as u8, 4, false);
                n += 1;
            }
        }
        // A chord anticipated on the last "and of 4" already rings.
        let pushed = std::mem::take(&mut self.pushed);
        for (k, &(st, ln, push)) in hits[..n].iter().enumerate() {
            if st == 0 && pushed {
                continue;
            }
            if k > 0 && i < 0.45 && st % 4 != 0 && self.chance(0.5 - i) {
                continue; // sparser when calm
            }
            if push && st as u32 + 2 >= steps {
                self.pushed = true;
            }
            let ch = if push {
                self.chord(st as u32 + 2)
            } else {
                self.chord(st as u32)
            };
            self.ep = voice_lead(ch, &self.ep, 50, 62);
            let accent = if st % 4 == 0 { 0.75 } else { 0.62 };
            let vel = accent * (0.75 + 0.35 * i);
            let base = self.at(st as u32);
            let mut len = ln as f32 * 0.92;
            if push {
                // An anticipation rings over the bar line.
                len += 2.0;
            }
            let len = self.len(len);
            let strum = if matches!(s.comp, Comp::Laid | Comp::Long) {
                90
            } else {
                12
            };
            for (j, &note) in self.ep.iter().enumerate() {
                Self::push(
                    bar,
                    Ev {
                        at: base + j as u32 * strum,
                        inst: Inst::EPiano,
                        note,
                        vel,
                        len,
                        glide: false,
                    },
                );
            }
        }
    }

    /// Bolero guitar: a strum on one, then arpeggiated 8ths.
    fn write_guitar(&mut self, bar: &mut Bar) {
        let steps = self.steps;
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            self.ep = voice_lead(ch, &self.ep, 50, 62);
            let at = self.at(st);
            if st % 8 == 0 {
                let len = self.len(4.0);
                for (j, &n) in self.ep.iter().enumerate() {
                    Self::push(
                        bar,
                        Ev {
                            at: at + j as u32 * 300,
                            inst: Inst::Guitar,
                            note: n,
                            vel: 0.8,
                            len,
                            glide: false,
                        },
                    );
                }
            } else {
                let order = [0usize, 2, 1, 3, 2, 1, 3, 2];
                let n = self.ep[order[(st / 2) as usize % 8]] + 12;
                let len = self.len(2.0);
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst: Inst::Guitar,
                        note: n,
                        vel: 0.55,
                        len,
                        glide: false,
                    },
                );
            }
            st += 2;
        }
    }

    /// The guitar's thumb: the chord's root on the low strings (E2 up).
    fn thumb(ch: Chord) -> u8 {
        ch.root_from(40)
    }

    /// The thumb's alternate bass: the fifth below the root's octave (the
    /// root again on an altered dominant, whose fifth is bent).
    fn thumb_fifth(ch: Chord) -> u8 {
        let root = Self::thumb(ch);
        if ch.q == Dom7alt {
            return root;
        }
        let f = root + ch.q.fifth();
        if f > 55 { f - 12 } else { f }
    }

    /// Bossa nova: thumb on one and three (root, then fifth), the fingers
    /// pinching a shell on the off-beats, a two-bar figure.
    fn write_bossa(&mut self, bar: &mut Bar) {
        const FINGERS: [&[u8]; 2] = [&[0, 3, 6, 10, 13], &[2, 5, 8, 12, 14]];
        let i = self.intensity;
        let steps = self.steps;
        for st in [0u32, 8] {
            if st >= steps {
                continue;
            }
            let ch = self.chord(st);
            let note = if st == 0 || self.chord(0) != ch {
                Self::thumb(ch)
            } else {
                Self::thumb_fifth(ch)
            };
            let at = self.at(st);
            let len = self.len(7.5);
            Self::push(bar, ev(at, Inst::Guitar, note, 0.62 + 0.2 * i, len));
        }
        let second = self.plan.bar % 2 == 1;
        let fig = FINGERS[self.plan.bar as usize % 2];
        for (k, &st) in fig.iter().enumerate() {
            let st = st as u32;
            if st >= steps {
                continue;
            }
            // The last pinch before a new chord anticipates it.
            let ch = if st % 4 == 3 || st >= 14 {
                self.chord(st + 1)
            } else {
                self.chord(st)
            };
            self.gtr = voice_lead(ch, &self.gtr, 52, 60);
            let at = self.at(st);
            let len = self.len(2.5);
            let vel = 0.42 + 0.12 * i + if st.is_multiple_of(4) { 0.06 } else { 0.0 };
            // The top string moves in the figure's second bar (a little
            // melody in the comping), back home for the next chord.
            let mut shell = [self.gtr[1], self.gtr[2], self.gtr[3]];
            if second && k >= 3 {
                shell[2] = ch.scale_step(shell[2], if k == 3 { 1 } else { -1 });
            }
            // Fingers together, but not quite.
            for (j, &n) in shell.iter().enumerate() {
                Self::push(bar, ev(at + j as u32 * 40, Inst::Guitar, n, vel, len));
            }
        }
    }

    /// Fingerpicking (p-i-m-a): the thumb on the beat, the fingers rolling
    /// up and down the chord in eighths, each string left to ring.
    fn write_picking(&mut self, bar: &mut Bar) {
        let i = self.intensity;
        let steps = self.steps;
        // Which finger plays each eighth: 0 = thumb, 1..=3 = strings up.
        const ROLL: [[u8; 8]; 3] = [
            [0, 1, 2, 3, 0, 2, 1, 2],
            [0, 2, 1, 3, 0, 3, 2, 1],
            [0, 1, 3, 2, 0, 1, 2, 3],
        ];
        let roll = ROLL[(self.plan.bar as usize / 2 + self.comp_var) % ROLL.len()];
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            if st % 8 == 0 || self.chord(st.saturating_sub(2)) != ch {
                self.gtr = voice_lead(ch, &self.gtr, 52, 60);
            }
            let finger = roll[(st / 2) as usize % 8];
            // Rests now and then when calm, so it breathes.
            if finger != 0 && i < 0.4 && self.chance(0.18) {
                st += 2;
                continue;
            }
            let (note, vel, ring) = if finger == 0 {
                let n = if st % 8 == 4 && self.chord(st & !7) == ch {
                    Self::thumb_fifth(ch)
                } else {
                    Self::thumb(ch)
                };
                (n, 0.6 + 0.2 * i, 8.0)
            } else {
                (self.gtr[finger as usize], 0.42 + 0.15 * i, 6.0)
            };
            let at = self.at(st);
            let len = self.len(ring);
            Self::push(bar, ev(at, Inst::Guitar, note, vel, len));
            // A hammered sixteenth pickup into the next beat, when there's
            // some energy.
            if finger == 3 && i > 0.45 && self.chance(0.25) && st + 1 < steps {
                let n2 = ch.scale_step(self.gtr[2], 1);
                Self::push(
                    bar,
                    ev(self.at(st + 1), Inst::Guitar, n2, vel * 0.8, self.len(3.0)),
                );
            }
            st += 2;
        }
    }

    fn write_pad(&mut self, bar: &mut Bar) {
        let s = self.style();
        if self.swelled || (!s.pad && self.plan.kind != Section::Breakdown && self.intensity >= 0.3)
        {
            return;
        }
        let steps = self.steps;
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            // Only re-articulate when the chord actually changes.
            let changed = st > 0 || self.plan.bar == 0 || self.prev_chord() != ch;
            let mut end = st + 4;
            while end < st + 64 && self.chord(end) == ch {
                end += 4;
            }
            let mut nx = st + 4;
            while nx < steps && self.chord(nx) == ch {
                nx += 4;
            }
            if changed {
                self.pad = voice_lead(ch, &self.pad, 55, 67);
                let at = self.at(st);
                let len = self.len((end - st) as f32 + 1.0);
                for &n in &{ self.pad } {
                    Self::push(
                        bar,
                        Ev {
                            at,
                            inst: s.pad_inst,
                            note: n + 12,
                            vel: 0.5,
                            len,
                            glide: false,
                        },
                    );
                }
            }
            st = nx;
        }
    }

    fn prev_chord(&self) -> Chord {
        if self.plan.bar == 0 {
            return Chord::new(0, Maj9);
        }
        self.plan.chord_at(self.plan.bar - 1, 15)
    }

    /// Bells, harp or chimes arpeggiating the chord.
    fn write_arp(&mut self, bar: &mut Bar) {
        let s = self.style();
        let Some(inst) = s.arp else {
            if self.plan.kind == Section::Victory && self.plan.bar == 1 {
                self.write_victory_bells(bar);
            }
            return;
        };
        if self.picking() {
            return;
        }
        if inst == Inst::Kalimba {
            self.write_ostinato(bar);
            return;
        }
        let i = self.intensity;
        let every = if i > 0.5 { 2 } else { 4 };
        let steps = self.steps;
        let mut st = 0;
        let up = if self.climb.abs() > 0.3 {
            self.climb > 0.0
        } else {
            self.plan.bar.is_multiple_of(2)
        };
        let mut k = 0usize;
        while st < steps {
            if self.chance(0.25 - 0.2 * i) {
                st += every;
                k += 1;
                continue;
            }
            let ch = self.chord(st);
            let tones = ch.q.voicing();
            let idx = if up { k % 4 } else { 3 - k % 4 };
            let n = ch.root_from(60) + tones[idx] + 12 * (k as u8 / 4 % 2);
            let at = self.at(st);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note: n.min(96),
                    vel: 0.5 + 0.2 * self.rand(),
                    len: 0,
                    glide: false,
                },
            );
            st += every;
            k += 1;
        }
    }

    fn write_victory_bells(&mut self, bar: &mut Bar) {
        let ch = self.chord(0);
        for (k, t) in ch.q.voicing().iter().enumerate() {
            let at = self.at(k as u32 * 2);
            let n = ch.root_from(72) + t;
            Self::push(
                bar,
                Ev {
                    at,
                    inst: Inst::Chime,
                    note: n.min(96),
                    vel: 0.6,
                    len: 0,
                    glide: false,
                },
            );
        }
    }

    /// An interlocking kalimba/marimba ostinato: a two-bar syncopated
    /// figure through the chord's tones that keeps its shape while the
    /// chords move under it.
    fn write_ostinato(&mut self, bar: &mut Bar) {
        // (16th step, tone: 0 = root, 1..=4 = the voicing's tones).
        type Fig = &'static [(u8, u8)];
        const FIGS: [[Fig; 2]; 3] = [
            [
                &[(0, 0), (3, 2), (6, 1), (8, 3), (10, 2), (13, 4), (14, 1)],
                &[(0, 0), (3, 2), (6, 3), (8, 4), (11, 2), (14, 3)],
            ],
            [
                &[(0, 1), (2, 3), (4, 0), (7, 2), (10, 4), (12, 3)],
                &[(0, 1), (2, 3), (4, 2), (7, 4), (9, 3), (12, 1), (14, 0)],
            ],
            [
                &[(0, 2), (2, 0), (5, 3), (8, 2), (10, 4), (13, 1)],
                &[(0, 2), (2, 0), (5, 3), (8, 4), (11, 3), (14, 1)],
            ],
        ];
        let fig = FIGS[self.comp_var % 3][usize::from(self.plan.bar % 2 == 1)];
        let i = self.intensity;
        for &(st, t) in fig {
            if st as u32 >= self.steps || (i < 0.3 && st % 4 != 0 && self.chance(0.3)) {
                continue;
            }
            let ch = self.chord(st as u32);
            let v = ch.q.voicing();
            let tones = [0, v[0], v[1], v[2], v[3]];
            let mut n = ch.root_from(65) + tones[t as usize];
            while n > 86 {
                n -= 12;
            }
            let at = self.at(st as u32);
            let vel = if st % 4 == 0 { 0.8 } else { 0.6 } * (0.85 + 0.3 * self.rand());
            Self::push(
                bar,
                Ev {
                    at,
                    inst: Inst::Kalimba,
                    note: n,
                    vel,
                    len: self.len(1.5),
                    glide: false,
                },
            );
        }
    }

    // --- Answers ----------------------------------------------------------

    /// Call and response: when the lead leaves a gap, a second voice echoes
    /// its last few notes in the same rhythm, a third or a sixth lower and
    /// fitted to the chords now sounding.
    fn write_answer(&mut self, bar: &mut Bar) {
        let s = self.style();
        let Some(inst) = s.answer else {
            return;
        };
        let kind = self.plan.kind;
        if !matches!(kind, Section::A | Section::Hook | Section::B) || self.frag.n < 2 {
            return;
        }
        let steps = self.steps;
        let from = if self.lead_end > 0 {
            self.lead_end + 1
        } else {
            0
        };
        let from = from.next_multiple_of(2);
        if from + 5 > steps || !self.chance(0.45 + 0.35 * self.intensity) {
            return;
        }
        let (lo, hi) = match inst {
            Inst::PolyBrass | Inst::EPiano => (60, 76),
            Inst::Strings => (57, 74),
            // Blown bottles live lower than a flute.
            Inst::Flute => (60, 79),
            _ => (62, 86),
        };
        let k = self.frag.n.min(3);
        let notes = self.frag.notes;
        let first = notes[self.frag.n - k];
        let down = if self.chance(0.6) { -2 } else { -5 };
        let mut prev = 0u8;
        for &(st, ln, n) in &notes[self.frag.n - k..self.frag.n] {
            let at_step = from + (st - first.0) as u32;
            if at_step >= steps {
                break;
            }
            let ch = self.chord(at_step);
            let mut note = ch.scale_step(n, down);
            while note > hi {
                note -= 12;
            }
            while note < lo {
                note += 12;
            }
            if at_step.is_multiple_of(4) || ln >= 3 {
                note = ch.nearest_strong(note);
            }
            if note == prev {
                note = ch.scale_step(note, -1);
            }
            prev = note;
            let at = self.at(at_step);
            let len = self.len((ln as f32).min((steps - at_step) as f32) * 0.95);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note,
                    vel: 0.62 + 0.15 * self.intensity,
                    len,
                    glide: false,
                },
            );
        }
    }

    // --- Horns ------------------------------------------------------------

    /// The horn section: shout figures and pushes in hooks and hot solos,
    /// long blooming chords on the changes in bridges and calm sections.
    fn write_horns(&mut self, bar: &mut Bar) {
        let s = self.style();
        if s.horns == Horns::None {
            return;
        }
        let i = self.intensity;
        let kind = self.plan.kind;
        let pb = self.plan.bar;
        let stabs = matches!(s.horns, Horns::Stabs | Horns::Both);
        let swells = matches!(s.horns, Horns::Swells | Horns::Both);
        let stab_now = stabs
            && match kind {
                Section::Hook | Section::Victory => true,
                Section::Solo => i > 0.55 && pb % 2 == 1,
                Section::A => i > 0.7 && pb % 8 >= 4,
                _ => false,
            };
        let swell_now = swells
            && !stab_now
            && match kind {
                Section::B | Section::Breakdown | Section::Hook => true,
                Section::Intro => pb >= 2,
                Section::A => i < 0.5 && pb % 8 >= 4,
                _ => false,
            };
        if kind == Section::Victory {
            self.horn_chord(bar, 0, 15.0, 1.0, false);
        } else if stab_now {
            self.horn_stabs(bar);
        } else if swell_now {
            self.horn_swells(bar);
        }
    }

    /// One section chord at `st`, `len` 16ths long, voice-led from the
    /// last (`top`: block-voice under this melody note instead).
    fn horn_chord(&mut self, bar: &mut Bar, st: u32, len: f32, vel: f32, push: bool) {
        self.horn_hit(bar, st, len, vel, push, None);
    }

    fn horn_hit(
        &mut self,
        bar: &mut Bar,
        st: u32,
        len: f32,
        vel: f32,
        push: bool,
        top: Option<u8>,
    ) {
        let ch = if push {
            self.chord(st + 2)
        } else {
            self.chord(st)
        };
        self.horn = match top {
            Some(t) => block_voicing(ch, t),
            None => no_top_rub(voice_lead(ch, &self.horn, 58, 67)),
        };
        let at = self.at(st);
        let len = self.len(len);
        for &n in &{ self.horn } {
            Self::push(
                bar,
                Ev {
                    at,
                    inst: Inst::PolyBrass,
                    note: n,
                    vel,
                    len,
                    glide: false,
                },
            );
        }
    }

    /// 16th steps where the lead starts a note this bar (horns answer
    /// around them rather than on top of them).
    fn lead_onsets(&self, bar: &Bar) -> u32 {
        let mut mask = 0u32;
        for e in &bar.events {
            if matches!(
                e.inst,
                Inst::Lead | Inst::Brass | Inst::Mini | Inst::Flute | Inst::Bell
            ) {
                let st = (e.at as f32 / self.step_len).round() as u32;
                mask |= 1 << st.min(31);
            }
        }
        mask
    }

    fn horn_stabs(&mut self, bar: &mut Bar) {
        // Shouts answer the lead in the bars where it holds or rests;
        // pushes just kick the off-beats elsewhere.
        type Fig = &'static [(u8, u8)];
        const SHOUTS: [Fig; 4] = [
            &[(4, 1), (6, 1), (8, 2), (11, 1), (14, 2)],
            &[(2, 1), (3, 1), (6, 2), (10, 1), (12, 3)],
            &[(0, 2), (3, 1), (6, 1), (8, 1), (10, 4)],
            &[(6, 1), (7, 1), (10, 2), (14, 2)],
        ];
        // The shout's top line, in chord-tone steps from where it starts.
        const CONTOURS: [&[i8]; 3] = [&[0, 1, 2, 1, 3], &[2, 1, 0, 1, -1], &[0, 2, 1, 3, 2]];
        const PUSHES: [Fig; 3] = [&[(14, 2)], &[(6, 1), (14, 2)], &[(10, 1), (11, 3)]];
        let phrase = self.plan.bar % 4;
        let busy = self.lead_onsets(bar);
        let free = |st: u8| busy & (1 << st) == 0 && (st == 0 || busy & (1 << (st - 1)) == 0);
        let mut hits = [(0u8, 0u8); 6];
        let mut n = 0;
        let mut shout = false;
        if phrase % 2 == 1 {
            let fig = SHOUTS[(self.comp_var + phrase as usize / 2) % SHOUTS.len()];
            for &h in fig.iter().filter(|h| free(h.0)) {
                hits[n] = h;
                n += 1;
            }
            shout = n >= 3;
        }
        if !shout {
            n = 0;
            for &h in PUSHES[self.comp_var % PUSHES.len()]
                .iter()
                .filter(|h| free(h.0))
            {
                hits[n] = h;
                n += 1;
            }
        }
        let i = self.intensity;
        let contour = CONTOURS[(self.comp_var + self.plan.bar as usize) % CONTOURS.len()];
        let mut top = self.chord(hits[0].0 as u32).nearest_strong(70);
        for (k, &(st, ln)) in hits[..n].iter().enumerate() {
            let st = st as u32;
            if st >= self.steps {
                continue;
            }
            let push = st + ln as u32 >= 16 && st >= 14;
            let last = k + 1 == n;
            let vel = (if last { 0.95 } else { 0.78 }) * (0.8 + 0.25 * i);
            // Short notes are clipped tight, the last one rings.
            let len = if last {
                ln as f32 + 0.5
            } else {
                ln as f32 * 0.6
            };
            if shout {
                let ch = self.chord(if push { st + 2 } else { st });
                let step = contour[k % contour.len()] as i32
                    - if k > 0 {
                        contour[(k - 1) % contour.len()] as i32
                    } else {
                        0
                    };
                top = chord_tone_step(ch, top, step).clamp(65, 80);
                let t = Some(ch.nearest_strong(top));
                self.horn_hit(bar, st, len, vel, push, t);
            } else {
                self.horn_chord(bar, st, len, vel, push);
            }
        }
    }

    fn horn_swells(&mut self, bar: &mut Bar) {
        let steps = self.steps;
        let mut st = 0;
        let i = self.intensity;
        while st < steps {
            let ch = self.chord(st);
            let mut end = st + 4;
            while end < st + 32 && self.chord(end) == ch {
                end += 4;
            }
            let changed = st > 0 || self.plan.bar.is_multiple_of(2) || self.prev_chord() != ch;
            if changed {
                let len = (end - st) as f32 - 0.5;
                self.horn_chord(bar, st, len, 0.5 + 0.25 * i, false);
                self.swelled = true;
            }
            let mut nx = st + 4;
            while nx < steps && self.chord(nx) == ch {
                nx += 4;
            }
            st = nx;
        }
    }

    // --- Kimes ----------------------------------------------------------

    /// A unison hit figure: bass, keys, lead and kit together, ending on a
    /// held chord.
    fn write_kime(&mut self, bar: &mut Bar, crash: bool) {
        let rhythm = if self.steps == 14 {
            KIME_78
        } else {
            KIMES[self.pick(KIMES.len())]
        };
        let lead = self.lead_inst();
        // The lead line walks down the chord tones from high up.
        let mut top = self
            .chord(rhythm[0] as u32)
            .nearest_strong(self.last_note.max(74));
        let steps = self.steps;
        for (k, &st) in rhythm.iter().enumerate() {
            let st = st as u32;
            if st >= steps {
                break;
            }
            let lastk = k + 1 == rhythm.len() || rhythm[k + 1] as u32 >= steps;
            let next = if lastk { steps } else { rhythm[k + 1] as u32 };
            let ch = self.chord(st);
            let at = self.at(st);
            let len_steps = if lastk {
                (steps - st) as f32 + 1.0
            } else {
                ((next - st) as f32 * 0.6).max(0.6)
            };
            let len = self.len(len_steps);
            // Kit.
            Self::push(bar, hit(at, Inst::Kick, 36, 1.0));
            Self::push(bar, hit(at, Inst::Snare, 38, 0.9));
            if k == 0 && crash || lastk {
                Self::push(bar, hit(at, Inst::Crash, 49, 0.85));
            }
            // Bass in octaves with the lead's pitch class, else the root.
            let root = ch.root_from(31);
            Self::push(
                bar,
                Ev {
                    at,
                    inst: Inst::Bass,
                    note: if k % 2 == 1 { root + 12 } else { root },
                    vel: 1.0,
                    len,
                    glide: false,
                },
            );
            // Keys stab.
            self.ep = voice_lead(ch, &self.ep, 50, 62);
            for &n in &{ self.ep } {
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst: Inst::EPiano,
                        note: n,
                        vel: 0.9,
                        len,
                        glide: false,
                    },
                );
            }
            // Horns hit with the band.
            if self.style().horns != Horns::None {
                self.horn = no_top_rub(voice_lead(ch, &self.horn, 58, 67));
                for &n in &{ self.horn } {
                    Self::push(
                        bar,
                        Ev {
                            at,
                            inst: Inst::PolyBrass,
                            note: n,
                            vel: 0.95,
                            len,
                            glide: false,
                        },
                    );
                }
            }
            // Lead.
            top = ch.nearest_strong(top);
            while top > LEAD_HI {
                top -= 12;
            }
            #[cfg(test)]
            if st.is_multiple_of(4) {
                self.strong_log.push((top, ch));
            }
            if let Some(inst) = lead {
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst,
                        note: top,
                        vel: 1.0,
                        len,
                        glide: false,
                    },
                );
            }
            self.last_note = top;
            // Next hit: a chord tone a step or two below.
            let next_ch = self.chord(next.min(steps.saturating_sub(1)));
            top = next_ch.nearest_strong(top.saturating_sub(2).max(LEAD_LO + 2));
            if top == self.last_note {
                top = next_ch.nearest_strong(top.saturating_sub(3));
            }
        }
    }

    // --- Lead -----------------------------------------------------------

    fn lead_inst(&self) -> Option<Inst> {
        // In solos the band's two synth players trade every two bars.
        let trade = self.plan.kind == Section::Solo && (self.plan.bar / 2) % 2 == 1;
        match self.style().lead {
            LeadKind::Synth if trade => Some(Inst::Mini),
            LeadKind::Mini | LeadKind::Brass if trade => Some(Inst::Lead),
            LeadKind::Synth => Some(Inst::Lead),
            LeadKind::Brass => Some(Inst::Brass),
            LeadKind::Flute => Some(Inst::Flute),
            LeadKind::Bell => Some(Inst::Bell),
            LeadKind::Mini => Some(Inst::Mini),
            LeadKind::Silent => None,
        }
    }

    fn new_motif(&mut self) -> Motif {
        // A motif needs a shape you can hear: retry flat ones.
        let mut m = self.motif_try();
        for _ in 0..4 {
            let cs = m.notes[..m.n].iter().map(|x| x.c);
            let range = cs.clone().max().unwrap_or(0) - cs.min().unwrap_or(0);
            if range >= 3 {
                break;
            }
            m = self.motif_try();
        }
        m
    }

    fn motif_try(&mut self) -> Motif {
        let cell = CELLS[self.pick(CELLS.len())];
        let mut m = Motif::default();
        let mut c: i8 = 0;
        let n = cell.len().min(m.notes.len());
        for (k, &(step, len)) in cell.iter().take(n).enumerate() {
            m.notes[k] = MNote { step, len, c };
            // Arch contour: rise through the first half, fall after.
            let rising = k * 2 < n;
            let leap = [1i8, 1, 2, 2, 3][self.pick(5)];
            let dir = if self.chance(if rising { 0.75 } else { 0.25 }) {
                1
            } else {
                -1
            };
            c = (c + dir * leap).clamp(-5, 6);
        }
        m.n = n;
        m
    }

    /// Play a motif against the current chords, starting near `from`.
    fn realize(&mut self, m: &Motif, from: u8, shift: i8, invert: bool) -> Line {
        let mut line = Line::new();
        if m.n == 0 {
            return line;
        }
        let first = self.chord(m.notes[0].step as u32);
        let anchor = first.nearest_strong(fold(from));
        for k in 0..m.n {
            let mn = m.notes[k];
            let step = mn.step as i32 + shift as i32;
            if !(0..self.steps as i32).contains(&step) {
                continue;
            }
            let ch = self.chord(step as u32);
            let c = if invert { -mn.c } else { mn.c };
            let note = ch.scale_step(anchor, c as i32);
            line.push(step as u8, mn.len, note);
        }
        line
    }

    fn write_lead(&mut self, bar: &mut Bar) {
        let Some(inst) = self.lead_inst() else {
            return;
        };
        let s = self.style();
        let i = self.intensity;
        let kind = self.plan.kind;
        let pb = self.plan.bar;
        let phrase = pb % 4;
        let last = self.last_note;
        let line = match kind {
            Section::Intro | Section::Breakdown | Section::Still => return,
            Section::Victory => {
                // Hold the 9th of the tonic, swelling in.
                let ch = self.chord(0);
                let mut l = Line::new();
                let nine = fold(ch.root_from(72) + 2);
                l.push(0, 2, ch.nearest_strong(nine.saturating_sub(2)));
                l.push(2, 14, ch.nearest_strong(nine));
                l
            }
            Section::Hook if s.theme => {
                let mut l = Line::new();
                let shift = (self.plan.key + 12 - THEME_KEY) % 12;
                let shift = if shift > 6 {
                    shift as i32 - 12
                } else {
                    shift as i32
                };
                if pb == 7 && i > 0.4 {
                    self.pent_run(&mut l, false);
                } else {
                    for &(st, ln, n) in THEME[pb as usize % 4] {
                        l.push(st, ln, (n as i32 + shift).clamp(0, 127) as u8);
                    }
                }
                l
            }
            Section::Solo => {
                let r = self.rand();
                let mut l = Line::new();
                if s.groove == Groove::Shred || r < 0.25 + 0.3 * i {
                    let up = if self.climb.abs() > 0.3 {
                        self.climb > 0.0
                    } else {
                        phrase % 2 == 1
                    };
                    self.pent_run(&mut l, up);
                } else if r < 0.6 {
                    self.bebop_line(&mut l);
                } else if phrase == 3 {
                    self.cadence(&mut l);
                } else {
                    let m = self.motif;
                    let shift = if self.chance(0.5) { 2 } else { 0 };
                    let invert = self.chance(0.3);
                    l = self.realize(&m, last + 3, shift, invert);
                }
                l
            }
            Section::A | Section::Hook | Section::B => {
                // Calm music breathes: whole phrases left out.
                if kind == Section::A && i < 0.4 && (pb / 4).is_multiple_of(2) {
                    return;
                }
                let m = if kind == Section::Hook {
                    self.hook
                } else {
                    self.motif
                };
                let second_half = pb % 8 >= 4;
                // The story of eight bars: state an idea, answer it, develop
                // it, come home; then state it again a step higher, climb to
                // a peak in bar seven and resolve.
                const ARC: [u8; 8] = [71, 72, 74, 72, 73, 76, 79, 72];
                // Climbing lifts the line a little higher each phrase.
                let lean = (self.climb * 3.0).round() as i32;
                let height = (ARC[pb as usize % 8] as i32 + (i * 2.0) as i32 + lean) as u8;
                match phrase {
                    // Call: the motif, restated (sequenced up) in the second
                    // half so the listener recognises it.
                    0 => {
                        let from = if second_half {
                            self.anchor + if self.chance(0.6) { 2 } else { 0 }
                        } else {
                            self.anchor = height;
                            height
                        };
                        self.realize(&m, from, 0, false)
                    }
                    // Response: a short answer, or rest.
                    1 => {
                        if self.chance(0.45 - 0.2 * i) {
                            return;
                        }
                        self.response(&m)
                    }
                    // Development, heading for the phrase's peak.
                    2 if second_half && i > 0.5 && self.chance(0.4) => {
                        let mut l = Line::new();
                        self.pent_run(&mut l, self.climb > -0.3);
                        l
                    }
                    // On the move up or down: a run that way.
                    2 if self.climb.abs() > 0.5 && self.chance(0.5) => {
                        let mut l = Line::new();
                        self.pent_run(&mut l, self.climb > 0.0);
                        l
                    }
                    2 => match self.pick(4) {
                        0 => self.realize(&m, height.max(last + 2), 0, false), // sequence up
                        1 => self.realize(&m, height, 2, false),               // displaced
                        2 => self.realize(&m, height, 0, true),                // inverted
                        _ => {
                            let mut half = m;
                            half.n = m.n.div_ceil(2);
                            let mut l = self.realize(&half, height, 0, false);
                            self.tail_into(&mut l, 8);
                            l
                        }
                    },
                    // Cadence.
                    _ => {
                        let mut l = Line::new();
                        if i > 0.65 && kind != Section::Hook && self.chance(0.4) {
                            self.pent_run(&mut l, true);
                        } else {
                            self.cadence(&mut l);
                        }
                        l
                    }
                }
            }
        };
        let approach = !(kind == Section::Hook && s.theme);
        self.emit_line(bar, inst, line, approach);
    }

    /// An answer to the call: two or three notes from the motif's end,
    /// inverted, landing on a guide tone.
    fn response(&mut self, m: &Motif) -> Line {
        let mut l = Line::new();
        let start = [4u8, 6, 8][self.pick(3)];
        let ch = self.chord(start as u32);
        // The answer moves against the call: a rising call is answered
        // falling, stepwise or by thirds.
        let rose = m.n > 1 && m.notes[m.n - 1].c > m.notes[0].c;
        let dir = if rose { -1 } else { 1 };
        let mut n = ch.scale_step(self.last_note, dir);
        let k = m.n.clamp(2, 3);
        for j in 0..k {
            let st = start + j as u8 * 2;
            let len = if j + 1 == k { 6 } else { 2 };
            l.push(st, len, n);
            let by = 1 + self.pick(2) as i32;
            n = self.chord(st as u32 + 2).scale_step(n, dir * by);
        }
        l
    }

    /// Finish a fragment with two notes into a long chord tone at `at`.
    fn tail_into(&mut self, l: &mut Line, at: u8) {
        let ch = self.chord(at as u32);
        let target = ch.nearest_strong(self.last_note);
        l.push(at.saturating_sub(2), 1, ch.scale_step(target, 2));
        l.push(at.saturating_sub(1), 1, target.saturating_sub(1));
        l.push(at, 6, target);
    }

    /// The end of a phrase: a short pickup, then a long guide tone, then rest.
    fn cadence(&mut self, l: &mut Line) {
        let ch = self.chord(4);
        let g = ch.q.guide();
        let pick = g[self.pick(2)];
        let target = nearest_pc(self.last_note, (ch.root + pick) % 12);
        let pre = self.chord(2);
        l.push(0, 2, pre.scale_step(target, 2));
        l.push(2, 2, pre.scale_step(target, 1));
        l.push(4, 9, target);
    }

    /// A bebop line: 8ths descending through the scale, chromatic passing
    /// tones on the off-beats so chord tones land on the beats.
    fn bebop_line(&mut self, l: &mut Line) {
        let start = if self.chance(0.5) { 0 } else { 2 };
        let ch0 = self.chord(start);
        let mut n = ch0.nearest_strong(fold(self.last_note + 7));
        let mut st = start;
        while st < self.steps.min(16) {
            l.push(st as u8, 2, n);
            let ch = self.chord(st + 2);
            let down = ch.scale_step(n, -1);
            // Passing tone between two scale tones a whole step apart; at the
            // bottom of the range, turn back up by a chord tone.
            n = if down < LEAD_LO + 1 {
                ch.nearest_strong(n + 4)
            } else if (st + 2) % 4 != 0 && n - down == 2 && self.chance(0.6) {
                n - 1
            } else {
                down
            };
            st += 2;
        }
    }

    /// A pentatonic run in 16ths, up or down, ending on a chord tone.
    fn pent_run(&mut self, l: &mut Line, up: bool) {
        let mut up = up;
        let pent_of = |ch: Chord| -> [u8; 5] {
            if ch.q.is_minor() {
                [0, 3, 5, 7, 10]
            } else if matches!(ch.q, Dom7alt | Dom7s9) {
                [1, 4, 6, 8, 11] // minor pentatonic a half step up: all altered tones
            } else {
                [0, 2, 4, 7, 9]
            }
        };
        let start = if self.chance(0.5) { 0 } else { 4 };
        let len = 8 + self.pick(5) as u32;
        let mut n = fold(if up {
            self.last_note.saturating_sub(5)
        } else {
            self.last_note + 7
        });
        for k in 0..len {
            let st = start + k;
            if st >= self.steps {
                break;
            }
            let ch_here = self.chord(st);
            let ln = if k + 1 == len { 4 } else { 1 };
            l.push(st as u8, ln, n);
            // Next pentatonic tone of the chord (re-rooted as chords change);
            // a run that reaches the edge of the range turns around rather
            // than jumping an octave.
            let r = ch_here.root;
            let pent = pent_of(ch_here);
            if (up && n + 2 > LEAD_HI) || (!up && n < LEAD_LO + 2) {
                up = !up;
            }
            let mut m = n as i32;
            loop {
                m += if up { 1 } else { -1 };
                let pc = ((m - r as i32).rem_euclid(12)) as u8;
                if pent.contains(&pc) || !(30..110).contains(&m) {
                    break;
                }
            }
            n = m.clamp(0, 127) as u8;
        }
    }

    /// Enforce the rules and write a line out: strong beats land on chord
    /// tones, weak notes on scale tones, chromatic approach tones before
    /// strong targets, everything in the lead's range.
    fn emit_line(&mut self, bar: &mut Bar, inst: Inst, mut line: Line, approach: bool) {
        let n = line.n;
        if n == 0 {
            return;
        }
        line.notes[..n].sort_by_key(|x| x.0);
        // One note per step (a later idea at the same step wins).
        let mut m = 0;
        for k in 0..n {
            if m > 0 && line.notes[m - 1].0 == line.notes[k].0 {
                line.notes[m - 1] = line.notes[k];
            } else {
                line.notes[m] = line.notes[k];
                m += 1;
            }
        }
        let n = m;
        line.n = n;
        // Move the whole line by octaves into range, so its shape survives.
        let hi = line.notes[..n].iter().map(|x| x.2).max().unwrap_or(LEAD_HI);
        let lo = line.notes[..n].iter().map(|x| x.2).min().unwrap_or(LEAD_LO);
        let shift: i32 = if hi > LEAD_HI + 2 {
            -12 * ((hi - LEAD_HI + 9) / 12) as i32
        } else if lo + 2 < LEAD_LO {
            12 * ((LEAD_LO - lo + 9) / 12) as i32
        } else {
            0
        };
        let mut prev: Option<u8> = None;
        for k in 0..n {
            let (st, _, raw) = line.notes[k];
            let ch = self.chord(st as u32);
            let note = soft_fold((raw as i32 + shift).clamp(0, 127) as u8);
            let mut out = if st % 4 == 0 {
                ch.nearest_strong(note)
            } else if approach {
                ch.nearest_scale(note)
            } else {
                note
            };
            // What the ear takes as the harmony is the long notes and the
            // first note on a new chord: make those chord tones, and on a
            // change prefer a nearby third or seventh (the guide tones).
            if approach {
                let ln = line.notes[k].1;
                let new_chord = k == 0 || self.chord(line.notes[k - 1].0 as u32) != ch;
                if new_chord && let Some(g) = nearest_guide(ch, out, 2) {
                    out = g;
                } else if ln >= 3 && !ch.is_strong(out) {
                    out = ch.nearest_strong(out);
                }
            }
            // Snapping mustn't stall a moving line on a repeated note: keep
            // going the way the idea went, to the next chord tone.
            if let Some(p) = prev
                && out == p
                && note != p
                && st % 4 == 0
            {
                let up = note > p;
                let mut t = out;
                for _ in 0..12 {
                    t = if up { t + 1 } else { t - 1 };
                    if ch.is_strong(t) {
                        break;
                    }
                }
                if ch.is_strong(t) {
                    out = t;
                }
            }
            line.notes[k].2 = out;
            prev = Some(out);
        }
        if approach {
            // One chromatic approach per bar is spice; more is a trill.
            let mut approached = false;
            for k in 0..n.saturating_sub(1) {
                let (st, ln, _) = line.notes[k];
                let (nst, _, target) = line.notes[k + 1];
                if !approached
                    && st % 4 != 0
                    && ln <= 2
                    && nst % 4 == 0
                    && nst - st <= 2
                    && self.chance(0.35)
                {
                    approached = true;
                    let below = target.saturating_sub(1);
                    let prev = if k > 0 { line.notes[k - 1].2 } else { 0 };
                    line.notes[k].2 = if prev == below { target + 1 } else { below };
                }
            }
        }
        let i = self.intensity;
        // Dynamics follow the phrase's arc: swell into the peak, ease off
        // into the cadence.
        const SHAPE: [f32; 8] = [0.9, 0.92, 0.97, 0.9, 0.95, 1.0, 1.1, 0.9];
        let shape = SHAPE[self.plan.bar as usize % 8];
        self.frag = Line::new();
        self.lead_this_bar = true;
        for k in 0..n {
            let (st, ln, note) = line.notes[k];
            let ch = self.chord(st as u32);
            let note = soft_fold(note);
            self.frag.push(st, ln, note);
            self.lead_end = self.lead_end.max(st as u32 + ln as u32);
            if st % 4 == 0 {
                debug_assert!(ch.is_strong(note));
                #[cfg(test)]
                self.strong_log.push((note, ch));
            }
            let next_st = if k + 1 < n { line.notes[k + 1].0 } else { 99 };
            let legato = next_st as u32 <= st as u32 + ln as u32;
            let strong = st % 4 == 0 || ln >= 4;
            let vel = (if strong { 0.85 } else { 0.68 }) * (0.8 + 0.3 * i) * shape;
            let jitter = (self.rand() * 120.0) as u32;
            let at = self.at(st as u32) + jitter;
            let len = self.len(ln as f32 * if legato { 1.02 } else { 0.85 });
            let prev = self.last_note;
            let glide = matches!(inst, Inst::Lead | Inst::Mini)
                && k > 0
                && (prev as i32 - note as i32).abs() <= 5
                && self.chance(0.3);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note,
                    vel,
                    len,
                    glide,
                },
            );
            self.last_note = note;
        }
    }
}

/// Fold a note into the lead's range by octaves.
fn fold(mut n: u8) -> u8 {
    while n > LEAD_HI {
        n -= 12;
    }
    while n < LEAD_LO {
        n += 12;
    }
    n
}

/// Fold only notes well outside the lead's range, so a line that pokes a
/// little over the edge keeps its shape instead of jumping an octave.
fn soft_fold(mut n: u8) -> u8 {
    while n > LEAD_HI + 3 {
        n -= 12;
    }
    while n + 2 < LEAD_LO {
        n += 12;
    }
    n
}

/// The note of pitch class `pc` nearest `near`.
fn nearest_pc(near: u8, pc: u8) -> u8 {
    let base = near - near % 12 + pc;
    let cands = [base.saturating_sub(12), base, base + 12];
    let mut best = base;
    for c in cands {
        if (c as i32 - near as i32).abs() < (best as i32 - near as i32).abs() {
            best = c;
        }
    }
    best
}

/// The guide tone (third or seventh) of `ch` within `max` semitones of
/// `note`, nearest first.
fn nearest_guide(ch: Chord, note: u8, max: u8) -> Option<u8> {
    let g = ch.q.guide();
    let is_guide = |n: u8| g.contains(&((n + 12 - ch.root % 12) % 12));
    for d in 0..=max {
        if is_guide(note.saturating_sub(d)) {
            return Some(note.saturating_sub(d));
        }
        if is_guide(note + d) {
            return Some(note + d);
        }
    }
    None
}

/// Move `steps` chord tones up or down from `note`.
fn chord_tone_step(ch: Chord, note: u8, steps: i32) -> u8 {
    let mut n = ch.nearest_strong(note) as i32;
    for _ in 0..steps.abs() {
        loop {
            n += steps.signum();
            if !(24..108).contains(&n) || ch.is_strong(n as u8) {
                break;
            }
        }
    }
    n.clamp(24, 107) as u8
}

/// Four-way close "block" voicing under a melody note: the next chord tones
/// down from `top`, never a semitone just under the melody (it rubs).
fn block_voicing(ch: Chord, top: u8) -> Voicing {
    let pcs = ch.q.voicing().map(|t| (ch.root + t) % 12);
    let ok = |n: u8| pcs.contains(&(n % 12)) || n % 12 == ch.root;
    let mut v = [top; 4];
    let mut n = top;
    for k in (0..3).rev() {
        n -= 1;
        while !ok(n) || (k == 2 && top - n == 1) || n == v[k + 1] {
            n -= 1;
        }
        v[k] = n;
    }
    v
}

/// Horns don't like semitone rubs at the edges of a voicing: lift a
/// bottom-note rub an octave, drop a top-note rub an octave.
fn no_top_rub(mut v: Voicing) -> Voicing {
    if v[1] - v[0] == 1 {
        v[0] += 12;
        v.sort_unstable();
    }
    if v[3] - v[2] == 1 {
        v[2] -= 12;
        v.sort_unstable();
    }
    v
}

fn vel(pat: &str, k: usize) -> Option<f32> {
    match pat.as_bytes().get(k) {
        Some(b'x') => Some(1.0),
        Some(b'o') => Some(0.72),
        Some(b'O') => Some(0.8),
        Some(b'g') => Some(0.38),
        _ => None,
    }
}

fn ev(at: u32, inst: Inst, note: u8, vel: f32, len: u32) -> Ev {
    Ev {
        at,
        inst,
        note,
        vel,
        len,
        glide: false,
    }
}

fn hit(at: u32, inst: Inst, note: u8, vel: f32) -> Ev {
    Ev {
        at,
        inst,
        note,
        vel,
        len: 0,
        glide: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(mood: Mood) -> Cue {
        Cue {
            mood,
            intensity: mood.base_intensity(),
            ..Cue::default()
        }
    }

    #[test]
    fn strong_beats_land_on_chord_tones() {
        let mut c = Composer::new(7);
        let mut bar = Bar::new();
        for (k, m) in Mood::ALL
            .iter()
            .cycle()
            .take(Mood::ALL.len() * 30)
            .enumerate()
        {
            let mut q = cue(*m);
            q.intensity = (k % 7) as f32 / 6.0;
            c.next_bar(q, &mut bar);
            assert!(bar.events.windows(2).all(|w| w[0].at <= w[1].at));
            assert!(bar.events.iter().all(|e| e.at < bar.len));
        }
        let mut c = Composer::new(99);
        for m in Mood::ALL {
            for _ in 0..24 {
                c.next_bar(cue(m), &mut bar);
            }
        }
        assert!(c.strong_log.len() > 100, "{}", c.strong_log.len());
        for (n, ch) in &c.strong_log {
            assert!(ch.is_strong(*n), "{n} over {ch:?}");
        }
    }

    /// Climbing storeys walks the key up the ladder a step at a time, each
    /// step prepared by a dominant in the bar before; coming down walks it
    /// back.
    /// The village and explore moods have several songs: every one comes
    /// up, and each plays notes at its own tempo.
    #[test]
    fn every_song_gets_played() {
        for (m, n) in [(Mood::Village, 3), (Mood::Explore, 2)] {
            let mut tempos = std::collections::BTreeSet::new();
            for seed in 0..40 {
                let mut c = Composer::new(seed);
                let mut bar = Bar::new();
                c.next_bar(cue(Mood::Menu), &mut bar);
                for _ in 0..6 {
                    c.next_bar(cue(m), &mut bar);
                }
                assert!(!bar.events.is_empty());
                tempos.insert(c.bpm() as u32);
            }
            assert_eq!(tempos.len(), n, "{m:?}: {tempos:?}");
        }
    }

    #[test]
    fn climbing_lifts_the_key() {
        let mut c = Composer::new(7);
        let mut bar = Bar::new();
        let mut run = |c: &mut Composer, lift: u8, bars: usize| {
            let mut seen = Vec::new();
            for _ in 0..bars {
                c.next_bar(
                    Cue {
                        lift,
                        climb: 1.0,
                        ..cue(Mood::Explore)
                    },
                    &mut bar,
                );
                if c.lift_to.is_some() {
                    assert!(c.bridge.q.is_dominant(), "bridge {:?}", c.bridge);
                    assert_eq!(c.chord(8), c.bridge);
                }
                seen.push(c.lift);
            }
            seen
        };
        run(&mut c, 0, 6);
        let up = run(&mut c, LIFT_MAX, 14);
        assert_eq!(*up.last().unwrap_or(&0), LIFT_MAX, "{up:?}");
        assert!(
            up.windows(2).all(|w| w[1] == w[0] || w[1] == w[0] + 1),
            "{up:?}"
        );
        let down = run(&mut c, 0, 14);
        assert_eq!(*down.last().unwrap_or(&9), 0, "{down:?}");
    }

    #[test]
    fn every_mood_writes_music() {
        let mut bar = Bar::new();
        for m in Mood::ALL {
            let mut c = Composer::new(3);
            let mut notes = 0;
            for _ in 0..16 {
                c.next_bar(cue(m), &mut bar);
                notes += bar.events.len();
            }
            assert!(notes > 10, "{m:?} wrote {notes}");
            assert_eq!(c.mood, m);
        }
    }

    #[test]
    fn victories_play_a_cadence_then_resume() {
        let mut c = Composer::new(5);
        let mut bar = Bar::new();
        for _ in 0..6 {
            c.next_bar(cue(Mood::Village), &mut bar);
        }
        let mut q = cue(Mood::Village);
        q.victories = 1;
        c.next_bar(q, &mut bar);
        assert_eq!(c.section, Section::Victory);
        c.next_bar(q, &mut bar);
        assert_eq!(c.section, Section::Victory);
        c.next_bar(q, &mut bar);
        assert_eq!(c.section, Section::A);
    }

    fn name(n: u8) -> String {
        const N: [&str; 12] = [
            "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
        ];
        format!("{}{}", N[n as usize % 12], (n / 12) as i32 - 1)
    }

    /// Print a score of 28 bars, to read what the composer writes:
    /// `PEEK=duel cargo test -p fusion peek_score -- --ignored --nocapture`
    /// (duel, abuela, menu, forest, explore, seeking; else village).
    #[test]
    #[ignore = "prints a score"]
    fn peek_score() {
        let want = std::env::var("PEEK").unwrap_or_default();
        let mood = Mood::ALL
            .into_iter()
            .find(|m| format!("{m:?}").eq_ignore_ascii_case(&want))
            .unwrap_or(Mood::Village);
        // PEEK_CLIMB=1: up a storey every two bars from bar 4, then down.
        let climbing = std::env::var_os("PEEK_CLIMB").is_some();
        let mut c = Composer::new(42);
        let mut bar = Bar::new();
        for b in 0..28u8 {
            let lift = if climbing {
                (b.saturating_sub(4) / 2).min(LIFT_MAX) - (b.saturating_sub(18) / 2).min(LIFT_MAX)
            } else {
                0
            };
            c.next_bar(
                Cue {
                    mood,
                    intensity: 0.7,
                    lift,
                    climb: if climbing { 1.0 } else { 0.0 },
                    ..Cue::default()
                },
                &mut bar,
            );
            let sl = c.step_len;
            // As heard: in the storey's key.
            let up = LIFT[c.lift as usize];
            let chords: Vec<String> = (0..4)
                .map(|k| {
                    let ch = c.chord(k * 4);
                    format!("{}{:?}", N12[((ch.root + up) % 12) as usize], ch.q)
                })
                .collect();
            println!(
                "{b:2} {:?}/{:?} bar{} storey {}{} [{}] steps {}",
                c.mood,
                c.section,
                c.plan.bar,
                c.lift,
                c.lift_to.map_or(String::new(), |t| format!("->{t}")),
                chords.join(" "),
                c.steps
            );
            for inst in [
                Inst::Guitar,
                Inst::Lead,
                Inst::Mini,
                Inst::Brass,
                Inst::Flute,
                Inst::PolyBrass,
                Inst::Bass,
                Inst::Fretless,
                Inst::EPiano,
                Inst::Kalimba,
            ] {
                let notes: Vec<String> = bar
                    .events
                    .iter()
                    .filter(|e| e.inst == inst)
                    .map(|e| format!("{:.1}:{}", e.at as f32 / sl, name(e.note)))
                    .collect();
                if !notes.is_empty() {
                    println!("   {inst:?}: {}", notes.join(" "));
                }
            }
        }
    }

    const N12: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
}
