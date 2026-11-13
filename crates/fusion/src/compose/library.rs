//! The library (`assets/data/music/library.ron`): every named pattern,
//! chord quality, scale, form, feel and band the composer plays from.
//!
//! The file is parsed once per version and leaked, like the score: the
//! composer reads `&'static` entries. Names the file doesn't have resolve to
//! nothing (a part that needs them is left out, a quality plays as a plain
//! major chord), so a half-written library never takes the music down.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use serde::Deserialize;

use super::Section;
use crate::synth::{Bus, Inst};
use crate::theory::Quality;

/// `data/music/library.ron`, as written (see its header).
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Library {
    pub scales: BTreeMap<String, Vec<u8>>,
    pub qualities: BTreeMap<String, QualityFile>,
    pub kits: BTreeMap<String, Kit>,
    pub grooves: BTreeMap<String, Groove>,
    pub lanes: BTreeMap<String, Lane>,
    pub hands: BTreeMap<Inst, Hand>,
    pub comps: BTreeMap<String, Comp>,
    pub leads: BTreeMap<String, LeadStyle>,
    pub horns: BTreeMap<String, HornStyle>,
    pub ostinatos: BTreeMap<String, Vec<Vec<Vec<(u8, u8)>>>>,
    pub kimes: BTreeMap<String, Kimes>,
    pub themes: BTreeMap<String, Theme>,
    pub feels: BTreeMap<String, Feel>,
    pub forms: BTreeMap<String, Form>,
    pub bands: BTreeMap<String, Band>,
}

/// The library, read at run time (see [`friendslop_data`]).
pub static LIBRARY: friendslop_data::Data<Library> =
    friendslop_data::Data::ron("data/music/library.ron");

// --- Conditions -------------------------------------------------------------

/// When something plays (see the file's header). `()` always holds.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct When {
    pub sections: Vec<Section>,
    pub not: Vec<Section>,
    pub above: Option<f32>,
    pub at_least: Option<f32>,
    pub below: Option<f32>,
    pub every: Option<(u16, u16)>,
    pub from_bar: Option<u16>,
    pub lead: Option<bool>,
    pub urgent: Option<bool>,
}

/// What a [`When`] looks at.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    pub section: Section,
    pub intensity: f32,
    pub bar: u16,
    pub lead: bool,
    pub urgent: bool,
}

impl When {
    pub fn holds(&self, c: &Ctx) -> bool {
        (self.sections.is_empty() || self.sections.contains(&c.section))
            && !self.not.contains(&c.section)
            && self.above.is_none_or(|a| c.intensity > a)
            && self.at_least.is_none_or(|a| c.intensity >= a)
            && self.below.is_none_or(|b| c.intensity < b)
            && self.every.is_none_or(|(n, k)| c.bar % n.max(1) >= k)
            && self.from_bar.is_none_or(|b| c.bar >= b)
            && self.lead.is_none_or(|l| l == c.lead)
            && self.urgent.is_none_or(|u| u == c.urgent)
    }
}

/// Does any of `list` hold? (An empty list never does.)
pub fn any(list: &[When], c: &Ctx) -> bool {
    list.iter().any(|w| w.holds(c))
}

/// The first rule that holds gives the level (0 if none does).
pub fn level(rules: &[(When, f32)], c: &Ctx) -> f32 {
    rules
        .iter()
        .find(|(w, _)| w.holds(c))
        .map_or(0.0, |(_, g)| *g)
}

// --- Theory -----------------------------------------------------------------

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct QualityFile {
    pub voicing: Vec<u8>,
    pub scale: String,
    pub pent: String,
    pub strong: Vec<u8>,
    pub guide: Vec<u8>,
    pub third: u8,
    pub fifth: u8,
    pub seventh: u8,
    pub minor: bool,
    pub dominant: bool,
    pub altered: bool,
}

/// A chord quality ready to play: its scales resolved.
#[derive(Clone, Debug)]
pub struct QualityDef {
    pub voicing: [u8; 4],
    pub scale: Vec<u8>,
    pub pent: Vec<u8>,
    pub strong: Vec<u8>,
    pub guide: [u8; 2],
    pub third: u8,
    pub fifth: u8,
    pub seventh: u8,
    pub minor: bool,
    pub dominant: bool,
    pub altered: bool,
}

impl Default for QualityDef {
    /// What an unknown quality plays: a plain major seventh chord.
    fn default() -> Self {
        Self {
            voicing: [4, 7, 11, 14],
            scale: vec![0, 2, 4, 5, 7, 9, 11],
            pent: vec![0, 2, 4, 7, 9],
            strong: vec![0, 4, 7, 11, 2],
            guide: [4, 11],
            third: 4,
            fifth: 7,
            seventh: 11,
            minor: false,
            dominant: false,
            altered: false,
        }
    }
}

/// Every quality, indexed by [`Quality`].
#[derive(Default)]
pub struct Qualities {
    by_id: Vec<Option<QualityDef>>,
    fallback: QualityDef,
}

impl Qualities {
    pub fn get(&self, q: Quality) -> &QualityDef {
        self.by_id
            .get(q.index())
            .and_then(Option::as_ref)
            .unwrap_or(&self.fallback)
    }

    pub fn has(&self, q: Quality) -> bool {
        self.by_id.get(q.index()).is_some_and(Option::is_some)
    }

    fn build(lib: &Library) -> Self {
        let mut t = Qualities::default();
        let scale = |n: &str| lib.scales.get(n).cloned();
        for (name, f) in &lib.qualities {
            let k = Quality::named(name).index();
            if t.by_id.len() <= k {
                t.by_id.resize(k + 1, None);
            }
            let d = QualityDef::default();
            let mut voicing = d.voicing;
            for (slot, v) in voicing.iter_mut().zip(&f.voicing) {
                *slot = *v;
            }
            let mut guide = d.guide;
            for (slot, v) in guide.iter_mut().zip(&f.guide) {
                *slot = *v;
            }
            t.by_id[k] = Some(QualityDef {
                voicing,
                scale: scale(&f.scale).unwrap_or(d.scale),
                pent: scale(&f.pent).unwrap_or(d.pent),
                strong: if f.strong.is_empty() {
                    d.strong
                } else {
                    f.strong.clone()
                },
                guide,
                third: f.third,
                fifth: f.fifth,
                seventh: f.seventh,
                minor: f.minor,
                dominant: f.dominant,
                altered: f.altered,
            });
        }
        t
    }
}

/// The library as the composer plays it: the file, leaked, and its
/// qualities resolved. Rebuilt when the file changes.
pub struct Lib {
    pub file: &'static Library,
    pub qualities: Qualities,
}

/// The library now (empty until the file has loaded).
pub fn lib() -> &'static Lib {
    static TABLE: RwLock<Option<(usize, &'static Lib)>> = RwLock::new(None);
    let file = LIBRARY.get();
    // Until the file is in, `get` hands out a fresh default each time: don't
    // rebuild (and leak) for every one of those.
    let key = if LIBRARY.loaded() {
        Arc::as_ptr(&file) as usize
    } else {
        0
    };
    if let Ok(t) = TABLE.read()
        && let Some((k, t)) = *t
        && k == key
    {
        return t;
    }
    let file: &'static Library = Box::leak(Box::new(file));
    let t: &'static Lib = Box::leak(Box::new(Lib {
        file,
        qualities: Qualities::build(file),
    }));
    if let Ok(mut slot) = TABLE.write() {
        *slot = Some((key, t));
    }
    t
}

/// A key for the library's current version (rebuild what was made from it
/// when this changes).
pub fn version() -> usize {
    std::ptr::from_ref(lib()) as usize
}

// --- Drums ------------------------------------------------------------------

/// A drum kit: which instrument and note plays each piece.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Kit {
    pub kick: (Inst, u8),
    pub snare: (Inst, u8),
    pub ghost: (Inst, u8),
    pub hat: (Inst, u8),
    pub open_hat: (Inst, u8),
    pub ride: (Inst, u8),
    pub clap: (Inst, u8),
    pub crash: (Inst, u8),
    pub swell: (Inst, u8),
    pub tom: Inst,
    pub rolls: Vec<u8>,
    pub tom_mid: u8,
    /// Pattern letters' velocities.
    pub vel: BTreeMap<char, f32>,
    pub ride_when: Vec<When>,
    pub clap_when: Vec<When>,
}

impl Kit {
    pub fn vel(&self, pat: &str, k: usize) -> Option<f32> {
        pat.as_bytes()
            .get(k)
            .and_then(|&c| self.vel.get(&(c as char)))
            .copied()
    }
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fill {
    /// Snare swells and tom rolls.
    #[default]
    Rolls,
    /// Soft ghost notes on the 8ths.
    Ghost,
}

fn sixteen() -> u32 {
    16
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Groove {
    #[serde(default = "sixteen")]
    pub steps: u32,
    /// Two bars (the second once the band is busy).
    pub kick: Vec<String>,
    pub snare: String,
    pub hat: String,
    pub extra: Vec<(Inst, u8, Vec<String>)>,
    pub soft: f32,
    pub lay: u32,
    pub fill: Fill,
    pub layer: Option<Vec<(When, f32)>>,
    pub ride_when: Option<Vec<When>>,
    pub clap_when: Option<Vec<When>>,
    pub solo_runs: bool,
}

/// A hand-percussion lane (see the file's header).
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Lane {
    #[serde(default = "sixteen")]
    pub steps: u32,
    pub inst: Option<Inst>,
    pub bars: Vec<String>,
    pub hits: BTreeMap<char, (u8, f32)>,
    /// The note off the beat (a woodblock's low block).
    pub off_note: Option<u8>,
    /// Off-16ths only play above this intensity.
    pub odd_above: Option<f32>,
    pub when: Vec<When>,
    /// Samples ahead of the grid.
    pub early: u32,
    /// The kit's timing wander, divided by this (0: none).
    pub jitter: u32,
    pub rand_vel: bool,
    /// Scaled by the groove's `soft`.
    #[serde(default = "yes")]
    pub soft: bool,
    /// Played on the song's hand drum.
    pub hand: bool,
    /// The notes at and above which a stroke is a high head / a slap.
    pub heads: (u8, u8),
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct Hand {
    /// (low, high, slap) heads; none: the strokes' own notes.
    pub notes: Option<(u8, u8, u8)>,
    /// The note a soft groove hit plays.
    pub quiet: Option<u8>,
}

// --- Chords -----------------------------------------------------------------

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Writer {
    /// Keys comping in rhythms.
    #[default]
    Block,
    /// Bolero guitar: a strum, then arpeggiated 8ths.
    Strum,
    /// Bossa nova guitar.
    Bossa,
    /// Fingerpicked guitar.
    Pick,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Comp {
    #[serde(default = "sixteen")]
    pub steps: u32,
    pub writer: Writer,
    pub rhythms: Vec<Vec<(u8, u8, bool)>>,
    pub spread: u32,
    pub order: Vec<usize>,
    pub fingers: Vec<Vec<u8>>,
    pub rolls: Vec<Vec<u8>>,
}

// --- Melody -----------------------------------------------------------------

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct LeadStyle {
    pub cells: Vec<Vec<(u8, u8)>>,
    pub arc: Vec<u8>,
    pub shape: Vec<f32>,
    pub leaps: Vec<i8>,
    pub rise: f32,
    pub fall: f32,
    pub contour: (i8, i8),
    pub min_span: i8,
    pub range: (u8, u8),
    pub trade: BTreeMap<Inst, Inst>,
    pub glides: Vec<Inst>,
    pub glide: f32,
    pub solo_run: (f32, f32),
    pub solo_bebop: f32,
    pub solo_shift: f32,
    pub solo_invert: f32,
    pub calm: f32,
    pub restate: f32,
    pub rest: (f32, f32),
    pub dev_run: f32,
    pub climb_run: f32,
    pub cadence_run: (f32, f32),
    pub approach: f32,
    pub bebop_pass: f32,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct HornStyle {
    pub inst: Inst,
    pub shouts: Vec<Vec<(u8, u8)>>,
    pub contours: Vec<Vec<i8>>,
    pub pushes: Vec<Vec<(u8, u8)>>,
    pub stabs: Vec<When>,
    pub swells: Vec<When>,
    pub avoid: Vec<Inst>,
    pub top: (u8, u8, u8),
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Kimes {
    pub rhythms: Vec<Vec<u8>>,
    pub odd: Vec<u8>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Theme {
    pub key: u8,
    pub bars: Vec<Vec<(u8, u8, u8)>>,
}

// --- Feel -------------------------------------------------------------------

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Feel {
    pub humanize: Humanize,
    pub registers: Registers,
    pub colours: BTreeMap<Section, (f32, f32, f32)>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Humanize {
    pub breath: Vec<f32>,
    pub kit: Vec<Inst>,
    pub touch: BTreeMap<Inst, f32>,
    pub touch_var: f32,
    pub backbeat: Vec<Inst>,
    pub loose: Vec<Inst>,
    pub lay: (f32, f32),
    pub loose_vel: (f32, f32),
    pub hold: BTreeMap<String, (f32, f32)>,
    pub hold_default: (f32, f32),
    pub curve: (f32, f32, f32),
    pub quiet: BTreeMap<Inst, f32>,
    pub accent: (f32, f32, f32),
    pub wobble: f32,
    pub spread: BTreeMap<String, f32>,
    pub spread_default: f32,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Registers {
    pub keys: (u8, u8),
    pub guitar: (u8, u8),
    pub horn: (u8, u8),
    pub pad: (u8, u8),
    pub bass: u8,
    pub thumb: u8,
    pub thumb_top: u8,
    pub arp: (u8, u8),
    pub ostinato: (u8, u8),
    pub bells: (u8, u8),
    pub lead: (u8, u8),
    pub answer: BTreeMap<Inst, (u8, u8)>,
    pub answer_default: (u8, u8),
}

// --- Form -------------------------------------------------------------------

/// Which of a song's progressions a section plays.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    A,
    B,
    Hook,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct SectionPlan {
    pub bars: u16,
    pub prog: Vec<Slot>,
    /// A new motif for the lead.
    pub motif: bool,
    /// May end on an odd (7/8) bar.
    pub odd: bool,
}

#[derive(Deserialize, Clone)]
pub struct Edge {
    pub to: Section,
    #[serde(default)]
    pub chance: Option<f32>,
    #[serde(default)]
    pub when: When,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Approach {
    pub minor: Vec<(u8, Quality)>,
    pub other: Vec<(u8, Quality)>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Hold {
    pub tempo: f32,
    pub layers: Vec<(String, f32)>,
    pub every: u16,
    pub len: f32,
    pub vel: f32,
    pub bell: Inst,
    pub bell_every: u16,
    pub bell_bar: u16,
    pub bell_from: u8,
    pub bell_up: u8,
    pub bell_max: u8,
    pub bell_vel: f32,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Form {
    pub sections: BTreeMap<Section, SectionPlan>,
    pub start: Vec<Edge>,
    pub next: BTreeMap<Section, Vec<Edge>>,
    /// A fixed order of sections instead of the graph (empty: the graph).
    pub cycle: Vec<Section>,
    pub victory: Vec<(u8, Quality, u8)>,
    pub victory_bars: u16,
    pub approach: Approach,
    pub lift: Vec<u8>,
    pub modulate_half: f32,
    pub modulate_top: u8,
    pub variants: usize,
    pub new_song: f32,
    pub hold: Hold,
}

impl Form {
    /// Semitones above the key on storey `n` (the top rung past the top).
    pub fn lift(&self, n: u8) -> u8 {
        self.lift
            .get(n as usize)
            .or(self.lift.last())
            .copied()
            .unwrap_or(0)
    }
}

// --- Bands ------------------------------------------------------------------

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Band {
    pub form: String,
    pub feel: String,
    pub kit: String,
    pub lead: String,
    pub horns: String,
    pub kimes: String,
    pub ostinato: String,
    pub theme: String,
    pub bossa: String,
    pub pick: String,
    pub shaker: String,
    pub tumbao: String,
    pub leads: BTreeMap<String, Inst>,
    pub horn_modes: BTreeMap<String, (bool, bool)>,
    pub ostinato_insts: Vec<Inst>,
    pub keys: Inst,
    pub hand: Inst,
    pub bass: Inst,
    pub pad: Inst,
    pub guitar: Inst,
    pub kime_bass: Inst,
    pub bells: Inst,
    pub glides: Vec<Inst>,
    pub glide: f32,
    pub layers: BTreeMap<String, Vec<(When, f32)>>,
}

/// A bus by its `mix.ron` name.
pub fn bus(name: &str) -> Option<usize> {
    Bus::NAMES.iter().position(|b| *b == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn read() -> Library {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/music/library.ron"
        ))
        .unwrap_or_default();
        match ron::from_str(&text) {
            Ok(f) => f,
            Err(e) => panic!("library.ron: {e}"),
        }
    }

    /// Every quality has a voicing and its scales.
    #[test]
    fn qualities_resolve() {
        let lib = read();
        assert!(lib.qualities.len() >= 16);
        for (n, q) in &lib.qualities {
            assert!(
                q.voicing.len() == 4 && q.voicing.iter().all(|&v| v > 0),
                "{n}: voicing"
            );
            assert_eq!(q.guide.len(), 2, "{n}: guide");
            assert!(lib.scales.contains_key(&q.scale), "{n}: scale {}", q.scale);
            assert!(lib.scales.contains_key(&q.pent), "{n}: pent {}", q.pent);
            assert!(!q.strong.is_empty(), "{n}: strong tones");
            assert!(q.strong.iter().all(|&t| t < 12), "{n}: strong tones");
        }
        for (n, s) in &lib.scales {
            assert!(!s.is_empty() && s.windows(2).all(|w| w[0] < w[1]), "{n}");
        }
    }

    fn fits(what: &str, pat: &str, steps: u32) {
        assert_eq!(
            pat.len() as u32,
            steps,
            "{what}: {pat:?} isn't {steps} steps"
        );
    }

    /// Every pattern fits its meter.
    #[test]
    fn patterns_fit_their_meter() {
        let lib = read();
        for (n, g) in &lib.grooves {
            assert!(!g.kick.is_empty(), "{n}: kick");
            for p in g.kick.iter().chain([&g.snare, &g.hat]) {
                fits(n, p, g.steps);
            }
            for (_, _, bars) in &g.extra {
                for p in bars {
                    fits(n, p, g.steps);
                }
            }
        }
        for (n, l) in &lib.lanes {
            assert!(!l.bars.is_empty(), "{n}");
            for p in &l.bars {
                fits(n, p, l.steps);
            }
        }
        for (n, c) in &lib.comps {
            for r in &c.rhythms {
                assert!(r.iter().all(|h| (h.0 as u32) < c.steps), "{n}");
            }
            for f in &c.fingers {
                assert!(f.iter().all(|&s| (s as u32) < c.steps), "{n}");
            }
        }
        let in_bar = |what: &str, steps: &mut dyn Iterator<Item = u8>| {
            for s in steps {
                assert!(s < 16, "{what}: step {s}");
            }
        };
        for (n, l) in &lib.leads {
            for c in &l.cells {
                in_bar(n, &mut c.iter().map(|x| x.0));
            }
        }
        for (n, h) in &lib.horns {
            for f in h.shouts.iter().chain(&h.pushes) {
                in_bar(n, &mut f.iter().map(|x| x.0));
            }
        }
        for (n, o) in &lib.ostinatos {
            for f in o.iter().flatten() {
                in_bar(n, &mut f.iter().map(|x| x.0));
            }
        }
        for (n, k) in &lib.kimes {
            for r in k.rhythms.iter().chain([&k.odd]) {
                in_bar(n, &mut r.iter().copied());
            }
        }
        for (n, t) in &lib.themes {
            for b in &t.bars {
                in_bar(n, &mut b.iter().map(|x| x.0));
            }
        }
    }

    /// Every name the library uses resolves within it, and every instrument
    /// is in instruments.ron.
    #[test]
    fn library_names_resolve() {
        let lib = read();
        let insts = crate::inst::table();
        let inst =
            |what: &str, i: Inst| assert!(insts.get(i).is_some(), "{what}: no instrument {i:?}");
        for (n, k) in &lib.kits {
            for (i, _) in [
                k.kick, k.snare, k.ghost, k.hat, k.open_hat, k.ride, k.clap, k.crash, k.swell,
            ] {
                inst(n, i);
            }
            inst(n, k.tom);
            assert_eq!(k.rolls.len(), 8, "{n}: rolls");
        }
        for (n, g) in &lib.grooves {
            for (i, _, _) in &g.extra {
                inst(n, *i);
            }
        }
        for (n, l) in &lib.lanes {
            if let Some(i) = l.inst {
                inst(n, i);
            }
        }
        for i in lib.hands.keys() {
            inst("hands", *i);
        }
        for (n, l) in &lib.leads {
            for (a, b) in &l.trade {
                inst(n, *a);
                inst(n, *b);
            }
            assert!(
                !l.cells.is_empty() && l.arc.len() == 8 && l.shape.len() == 8,
                "{n}"
            );
        }
        for (n, h) in &lib.horns {
            inst(n, h.inst);
        }
        for (n, f) in &lib.feels {
            for i in f.humanize.touch.keys().chain(f.registers.answer.keys()) {
                inst(n, *i);
            }
            for b in f.humanize.hold.keys().chain(f.humanize.spread.keys()) {
                assert!(bus(b).is_some(), "{n}: bus {b}");
            }
        }
        let quality = |what: &str, q: Quality| {
            assert!(
                lib.qualities.contains_key(q.name()),
                "{what}: no quality {q:?}"
            );
        };
        for (n, f) in &lib.forms {
            for (_, q, _) in &f.victory {
                quality(n, *q);
            }
            for (_, q) in f.approach.minor.iter().chain(&f.approach.other) {
                quality(n, *q);
            }
            for e in f.next.values().flatten().chain(&f.start) {
                assert!(
                    f.sections.contains_key(&e.to) || e.to == Section::Victory,
                    "{n}: {:?}",
                    e.to
                );
            }
            inst(n, f.hold.bell);
            for (b, _) in &f.hold.layers {
                assert!(bus(b).is_some(), "{n}: bus {b}");
            }
        }
        for (n, b) in &lib.bands {
            assert!(lib.forms.contains_key(&b.form), "{n}: form");
            assert!(lib.feels.contains_key(&b.feel), "{n}: feel");
            assert!(lib.kits.contains_key(&b.kit), "{n}: kit");
            assert!(lib.leads.contains_key(&b.lead), "{n}: lead");
            assert!(lib.horns.contains_key(&b.horns), "{n}: horns");
            assert!(lib.kimes.contains_key(&b.kimes), "{n}: kimes");
            assert!(lib.ostinatos.contains_key(&b.ostinato), "{n}: ostinato");
            assert!(lib.themes.contains_key(&b.theme), "{n}: theme");
            for c in [&b.bossa, &b.pick] {
                assert!(lib.comps.contains_key(c), "{n}: comp {c}");
            }
            for l in [&b.shaker, &b.tumbao] {
                assert!(lib.lanes.contains_key(l), "{n}: lane {l}");
            }
            for i in b.leads.values().chain(&b.ostinato_insts).chain(&b.glides) {
                inst(n, *i);
            }
            for i in [
                b.keys,
                b.hand,
                b.bass,
                b.pad,
                b.guitar,
                b.kime_bass,
                b.bells,
            ] {
                inst(n, i);
            }
            for k in b.layers.keys() {
                assert!(bus(k).is_some() || k == "pad_sparse", "{n}: bus {k}");
            }
        }
    }

    #[test]
    fn conditions() {
        let c = Ctx {
            section: Section::A,
            intensity: 0.3,
            bar: 5,
            lead: true,
            urgent: false,
        };
        let w: When = ron::from_str("()").unwrap_or_default();
        assert!(w.holds(&c));
        let w: When = ron::from_str(
            "#![enable(implicit_some)] (sections: [A], at_least: 0.3, every: (8, 4))",
        )
        .unwrap_or_default();
        assert!(w.holds(&c));
        assert!(
            !When {
                above: Some(0.3),
                ..When::default()
            }
            .holds(&c)
        );
        assert!(!any(&[], &c));
    }
}
