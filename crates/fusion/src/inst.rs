//! The band's instruments as data: `assets/data/music/instruments.ron`.
//!
//! An [`Inst`] is an instrument's name, interned: the score and the composer
//! name instruments, and the table here says how each one sounds (which
//! synthesis engine in [`crate::synth`] with which settings, its bus, pan,
//! sends and how it is played). A name the file doesn't list plays nothing.
//!
//! In the browser the file arrives after start-up: until then the table is
//! empty and the band plays silence.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::synth::{BUSES, BlowPatch, Bus, ChoirPatch, Patch, StringPatch, SubPatch};

/// An instrument, by name (see the module docs). Cheap to copy and compare;
/// `{:?}` prints its name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Inst(u16);

/// Every name ever asked for, in order: an [`Inst`] is an index here.
static NAMES: RwLock<Vec<&'static str>> = RwLock::new(Vec::new());

impl Inst {
    /// No instrument (an idle voice).
    pub const NONE: Inst = Inst(u16::MAX);

    /// The instrument called `name` (interned: the same name is always the
    /// same `Inst`, whether or not the instruments file lists it).
    pub fn named(name: &str) -> Inst {
        if let Ok(names) = NAMES.read()
            && let Some(i) = names.iter().position(|n| *n == name)
        {
            return Inst(i as u16);
        }
        let Ok(mut names) = NAMES.write() else {
            return Inst(u16::MAX);
        };
        if let Some(i) = names.iter().position(|n| *n == name) {
            return Inst(i as u16);
        }
        if names.len() >= u16::MAX as usize {
            return Inst(u16::MAX);
        }
        names.push(Box::leak(name.to_owned().into_boxed_str()));
        Inst(names.len() as u16 - 1)
    }

    /// Its name ("?" for none).
    pub fn name(self) -> &'static str {
        NAMES
            .read()
            .ok()
            .and_then(|n| n.get(self.0 as usize).copied())
            .unwrap_or("?")
    }

    /// The bus it plays through (the drums for an unknown instrument).
    pub fn bus(self) -> Bus {
        table().get(self).map_or(Bus::Drums, |s| Bus::ALL[s.bus])
    }

    /// Every instrument in the file, in its order.
    pub fn all() -> Vec<Inst> {
        table().order.clone()
    }

    /// Look one up by name, ignoring case (tools).
    pub fn find(name: &str) -> Option<Inst> {
        Inst::all()
            .into_iter()
            .find(|i| i.name().eq_ignore_ascii_case(name))
    }
}

impl Default for Inst {
    /// No instrument.
    fn default() -> Self {
        Inst::NONE
    }
}

impl std::fmt::Debug for Inst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl serde::Serialize for Inst {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

/// Reads a bare name (`Fretless`, as moods.ron writes them).
impl<'de> serde::Deserialize<'de> for Inst {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Name;
        impl serde::de::Visitor<'_> for Name {
            type Value = Inst;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an instrument name")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Inst, E> {
                Ok(Inst::named(v))
            }
        }
        d.deserialize_identifier(Name)
    }
}

// --- The file -------------------------------------------------------------

/// `data/music/instruments.ron`, as written.
#[derive(serde::Deserialize, Default)]
pub struct Instruments {
    pub instruments: Vec<Def>,
}

/// The instruments, read at run time (see [`friendslop_data`]).
pub static INSTRUMENTS: friendslop_data::Data<Instruments> =
    friendslop_data::Data::ron("data/music/instruments.ron");

/// One instrument (see the file's header for what each field does).
#[derive(serde::Deserialize, Clone, Debug)]
pub struct Def {
    pub name: String,
    pub bus: String,
    pub send: f32,
    pub echo: f32,
    pub pan: Pan,
    pub voice: Voice,
    pub glide: Option<f32>,
    pub bright: f32,
    pub engine: Engine,
}

/// Where a note sits: `at + (note - c) / w + (random - 0.5) * rand`, then
/// clamped to `±clamp`, then times `scale`.
#[derive(serde::Deserialize, Clone, Copy, Debug)]
pub struct Pan {
    pub at: f32,
    pub note: Option<(f32, f32)>,
    pub rand: f32,
    pub clamp: Option<f32>,
    pub scale: f32,
}

/// Which voice of its engine plays it.
#[derive(serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Voice {
    /// Any free voice of the engine's pool.
    Poly,
    /// The engine's one mono lead voice (FM and subtractive).
    Lead,
    /// One note at a time, never stolen for another part (FM).
    Bass,
}

/// How an instrument makes its sound.
#[derive(serde::Deserialize, Clone, Debug)]
pub enum Engine {
    Fm(Patch),
    Sub(SubPatch),
    String(StringPatch),
    Blown(BlowPatch),
    Choir(ChoirPatch),
    /// Synthesized drum samples, one per root note.
    Kit {
        samples: Vec<KitSample>,
        hit: Hit,
    },
    /// `foley-synth` note-block samples.
    Foley {
        recipe: Foley,
        hit: Hit,
    },
}

/// One drum sample: rendered by `make`, peaking at `peak`, played for `root`.
#[derive(serde::Deserialize, Clone, Copy, Debug)]
pub struct KitSample {
    pub root: u8,
    pub make: DrumGen,
    pub peak: f32,
}

/// The drum generators in [`crate::drums`].
#[derive(serde::Deserialize, Clone, Copy, Debug)]
pub enum DrumGen {
    Kick,
    Snare(f32, f32),
    Hat,
    OpenHat,
    Crash,
    Ride,
    Clap,
    Shaker,
    Membrane(f32, f32, f32, f32),
    Rim,
    Cowbell,
    Swell,
    Timbale(f32),
    Cajon(f32),
    Tabla(bool),
    Tambourine,
    Guiro,
    Claves,
    Woodblock(f32),
}

impl DrumGen {
    pub fn render(self) -> Vec<f32> {
        use crate::drums as d;
        match self {
            DrumGen::Kick => d::kick(),
            DrumGen::Snare(w, t) => d::snare(w, t),
            DrumGen::Hat => d::hat(),
            DrumGen::OpenHat => d::open_hat(),
            DrumGen::Crash => d::crash(),
            DrumGen::Ride => d::ride(),
            DrumGen::Clap => d::clap(),
            DrumGen::Shaker => d::shaker(),
            DrumGen::Membrane(f, decay, drop, slap) => d::membrane(f, decay, drop, slap),
            DrumGen::Rim => d::rim(),
            DrumGen::Cowbell => d::cowbell(),
            DrumGen::Swell => d::swell(),
            DrumGen::Timbale(f) => d::timbale(f),
            DrumGen::Cajon(s) => d::cajon(s),
            DrumGen::Tabla(b) => d::tabla(b),
            DrumGen::Tambourine => d::tambourine(),
            DrumGen::Guiro => d::guiro(),
            DrumGen::Claves => d::claves(),
            DrumGen::Woodblock(f) => d::woodblock(f),
        }
    }
}

/// A foley note-block instrument to render into the bank.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct Foley {
    pub foley: u32,
    /// MIDI notes to render, each its own sample.
    pub notes: Vec<u8>,
    pub character: f32,
    pub secs: f32,
    pub gain: f32,
}

/// How a sample plays.
#[derive(serde::Deserialize, Clone, Copy, Debug)]
pub struct Hit {
    /// Plays at the note (else always at its sample's root).
    pub pitched: bool,
    /// Sidechain: how hard a full hit ducks the band.
    pub push: f32,
    /// Low-pass a whisper-soft hit is closed to (Hz).
    pub tone_floor: f32,
    /// Cuts `choked` voices (a closed hat).
    pub chokes: bool,
    /// Cut by `chokes` (an open hat).
    pub choked: bool,
}

// --- The table --------------------------------------------------------------

/// An instrument ready to play: its [`Def`] with the bus resolved.
#[derive(Clone, Debug)]
pub struct Spec {
    pub bus: usize,
    pub send: f32,
    pub echo: f32,
    pub pan: Pan,
    pub voice: Voice,
    pub glide: Option<f32>,
    pub bright: f32,
    pub engine: Engine,
}

/// Every instrument, indexed by [`Inst`].
#[derive(Default, Debug)]
pub struct Table {
    by_id: Vec<Option<Spec>>,
    pub order: Vec<Inst>,
}

impl Table {
    pub fn get(&self, i: Inst) -> Option<&Spec> {
        self.by_id.get(i.0 as usize).and_then(Option::as_ref)
    }

    fn build(file: &Instruments) -> Self {
        let mut t = Table::default();
        for d in &file.instruments {
            let Some(bus) = Bus::NAMES.iter().position(|b| *b == d.bus) else {
                continue;
            };
            let i = Inst::named(&d.name);
            let k = i.0 as usize;
            if t.by_id.len() <= k {
                t.by_id.resize(k + 1, None);
            }
            if t.by_id[k].is_none() {
                t.order.push(i);
            }
            t.by_id[k] = Some(Spec {
                bus: bus.min(BUSES - 1),
                send: d.send,
                echo: d.echo,
                pan: d.pan,
                voice: d.voice,
                glide: d.glide,
                bright: d.bright,
                engine: d.engine.clone(),
            });
        }
        t
    }
}

/// The instruments as the band plays them, rebuilt when the file changes
/// (empty until it has loaded).
pub fn table() -> Arc<Table> {
    static TABLE: RwLock<Option<(usize, Arc<Table>)>> = RwLock::new(None);
    let file = INSTRUMENTS.get();
    // Until the file is in, `get` hands out a fresh default each time: don't
    // rebuild (and allocate) for every one of those.
    let key = if INSTRUMENTS.loaded() {
        Arc::as_ptr(&file) as usize
    } else {
        0
    };
    if let Ok(t) = TABLE.read()
        && let Some((k, t)) = t.as_ref()
        && *k == key
    {
        return t.clone();
    }
    let t = Arc::new(if key == 0 {
        Table::default()
    } else {
        Table::build(&file)
    });
    if let Ok(mut slot) = TABLE.write() {
        *slot = Some((key, t.clone()));
    }
    t
}

/// The instruments by name, for tools and tests.
pub fn by_name() -> BTreeMap<String, Inst> {
    Inst::all()
        .into_iter()
        .map(|i| (i.name().to_owned(), i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_intern_and_print() {
        let a = Inst::named("EPiano");
        assert_eq!(a, Inst::named("EPiano"));
        assert_eq!(format!("{a:?}"), "EPiano");
        assert_ne!(a, Inst::named("NoSuchThing"));
    }

    #[test]
    fn instruments_file_parses_and_resolves() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/music/instruments.ron"
        ))
        .unwrap_or_default();
        let file: Instruments = match ron::from_str(&text) {
            Ok(f) => f,
            Err(e) => panic!("instruments.ron: {e}"),
        };
        assert!(!file.instruments.is_empty());
        for d in &file.instruments {
            assert!(
                Bus::NAMES.contains(&d.bus.as_str()),
                "{}: bus {}",
                d.name,
                d.bus
            );
        }
        let t = Table::build(&file);
        assert_eq!(t.order.len(), file.instruments.len(), "duplicate names");
    }

    #[test]
    fn names_read_bare() {
        let v: Vec<Inst> = ron::from_str("[Fretless, Organ]").unwrap_or_default();
        assert_eq!(v, vec![Inst::named("Fretless"), Inst::named("Organ")]);
    }
}
