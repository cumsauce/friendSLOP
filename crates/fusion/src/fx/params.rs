//! Automating bus effects: a song's `mix_auto` lanes move a bus's settings
//! (send levels, echo feedback, chorus mix, EQ gains, and any effect added
//! to `BusMix`) over time, the way a part's lanes move its instrument.
//!
//! A lane names a bus (or `"master"`: `wet`, `echo` and `feedback`, the
//! reverb and echo sends and the echo's feedback) and a setting by path:
//! `trim`, `pump`, `chorus.mix`, `autopan.depth`, `eq.1.db` (the second EQ
//! band's gain; `hz` and `q` too). Positions are in bars from the start of
//! the current section, as for instrument lanes.
//!
//! The curves are the same as `crate::auto`'s instrument lanes (written
//! here too until the two land together; then this file uses those).

use crate::compose::Section;
use crate::dsp::Eq;

/// Something with automatable numbers, reached by a dotted path.
pub trait Params {
    /// The number at `path` (`""` is the value itself).
    fn param(&mut self, path: &str) -> Option<&mut f32>;
    /// The `i`th element, then `rest` inside it (tuples, lists).
    fn index(&mut self, _i: usize, _rest: &str) -> Option<&mut f32> {
        None
    }
}

impl Params for f32 {
    fn param(&mut self, path: &str) -> Option<&mut f32> {
        path.is_empty().then_some(self)
    }
}

impl Params for bool {
    fn param(&mut self, _: &str) -> Option<&mut f32> {
        None
    }
}

impl<T: Params> Params for Option<T> {
    fn param(&mut self, path: &str) -> Option<&mut f32> {
        self.as_mut()?.param(path)
    }
    fn index(&mut self, i: usize, rest: &str) -> Option<&mut f32> {
        self.as_mut()?.index(i, rest)
    }
}

/// `head.rest` split, with the head read as an index.
fn split_index(path: &str) -> Option<(usize, &str)> {
    let (h, t) = path.split_once('.').unwrap_or((path, ""));
    Some((h.parse().ok()?, t))
}

impl<T: Params> Params for Vec<T> {
    fn param(&mut self, path: &str) -> Option<&mut f32> {
        let (i, t) = split_index(path)?;
        self.index(i, t)
    }
    fn index(&mut self, i: usize, rest: &str) -> Option<&mut f32> {
        self.get_mut(i)?.param(rest)
    }
}

macro_rules! tuple_params {
    ($($t:ident $i:tt),*) => {
        impl<$($t: Params),*> Params for ($($t,)*) {
            fn param(&mut self, path: &str) -> Option<&mut f32> {
                let (i, t) = split_index(path)?;
                self.index(i, t)
            }
            fn index(&mut self, i: usize, rest: &str) -> Option<&mut f32> {
                match i {
                    $($i => self.$i.param(rest),)*
                    _ => None,
                }
            }
        }
    };
}
tuple_params!(A 0, B 1);
tuple_params!(A 0, B 1, C 2);
tuple_params!(A 0, B 1, C 2, D 3);
tuple_params!(A 0, B 1, C 2, D 3, E 4);
tuple_params!(A 0, B 1, C 2, D 3, E 4, F 5);

/// An EQ band: `hz`, `q` (shelves have none) and `db` (not on the passes).
impl Params for Eq {
    fn param(&mut self, path: &str) -> Option<&mut f32> {
        match (self, path) {
            (
                Eq::Hp(f, _)
                | Eq::Lp(f, _)
                | Eq::Bp(f, _)
                | Eq::Peak(f, _, _)
                | Eq::LowShelf(f, _)
                | Eq::HighShelf(f, _),
                "hz",
            ) => Some(f),
            (Eq::Hp(_, q) | Eq::Lp(_, q) | Eq::Bp(_, q) | Eq::Peak(_, q, _), "q") => Some(q),
            (Eq::Peak(_, _, g) | Eq::LowShelf(_, g) | Eq::HighShelf(_, g), "db") => Some(g),
            _ => None,
        }
    }
}

/// Implements [`Params`] for a settings struct from its plain number
/// fields (or any field that is itself `Params`): `params!(Drive { amount, tone })`.
#[macro_export]
macro_rules! params {
    ($t:ty { $($f:ident),* $(,)? }) => {
        impl $crate::fx::params::Params for $t {
            fn param(&mut self, path: &str) -> Option<&mut f32> {
                let (h, t) = path.split_once('.').unwrap_or((path, ""));
                match h {
                    $(stringify!($f) => $crate::fx::params::Params::param(&mut self.$f, t),)*
                    _ => None,
                }
            }
        }
    };
}

/// The pseudo-bus whose settings are the shared effects.
pub const MASTER: &str = "master";

/// The shared effects' automatable settings (scales on the mood's own).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Master {
    /// Reverb send scale (1 as mixed).
    pub wet: f32,
    /// Echo send scale (1 as mixed).
    pub echo: f32,
    /// The echo's feedback (how many repeats).
    pub feedback: f32,
}

impl Master {
    pub const DEFAULT: Master = Master {
        wet: 1.0,
        echo: 1.0,
        feedback: crate::fx::echo::FEEDBACK,
    };
}

impl Default for Master {
    fn default() -> Self {
        Self::DEFAULT
    }
}

crate::params!(Master {
    wet,
    echo,
    feedback
});

/// How a lane's value meets the setting it moves.
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum How {
    /// Added to the mix's value.
    #[default]
    Add,
    /// The mix's value times it.
    Times,
    /// The mix's value times 2^it (frequencies, rates: octaves).
    Octaves,
    /// Replaces the mix's value.
    Set,
}

impl How {
    pub fn apply(self, base: f32, v: f32) -> f32 {
        match self {
            How::Add => base + v,
            How::Times => base * v,
            How::Octaves => base * 2f32.powf(v.clamp(-10.0, 10.0)),
            How::Set => v,
        }
    }
}

/// An LFO's shape (one cycle 0..1).
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum Shape {
    #[default]
    Sine,
    Triangle,
    /// Rising saw (0 → 1, then jumps back).
    Saw,
    /// Falling saw (1 → 0).
    Down,
    Square,
}

impl Shape {
    /// -1..1 at phase `t` (0..1).
    pub fn at(self, t: f32) -> f32 {
        let t = t - t.floor();
        match self {
            Shape::Sine => (t * std::f32::consts::TAU).sin(),
            Shape::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
            Shape::Saw => 2.0 * t - 1.0,
            Shape::Down => 1.0 - 2.0 * t,
            Shape::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        }
    }
}

/// How the value moves through each `bars`-long cycle.
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub enum Curve {
    /// A straight line from `from` to `to`, then back to `from`.
    Ramp(f32, f32),
    /// Swings `depth` either side of `center`.
    Lfo {
        shape: Shape,
        center: f32,
        depth: f32,
    },
    /// One value per cycle, in turn.
    Steps(Vec<f32>),
}

impl Curve {
    /// The value at `p` cycles in.
    pub fn at(&self, p: f32) -> f32 {
        match self {
            Curve::Ramp(a, b) => a + (b - a) * (p - p.floor()),
            Curve::Lfo {
                shape,
                center,
                depth,
            } => center + depth * shape.at(p),
            Curve::Steps(v) if v.is_empty() => 0.0,
            Curve::Steps(v) => v[(p.max(0.0) as usize) % v.len()],
        }
    }
}

fn one() -> f32 {
    1.0
}

/// One bus lane as written in a song's `mix_auto`.
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct BusLaneFile {
    /// A bus by name (`Bus::NAMES`) or `"master"`.
    pub bus: String,
    /// The setting's path on that bus (see the module docs).
    pub param: String,
    #[serde(default)]
    pub how: How,
    pub curve: Curve,
    /// One cycle's length in bars.
    #[serde(default = "one")]
    pub bars: f32,
    /// Only in these sections (all if empty), never in `not`.
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub not: Vec<Section>,
}

impl BusLaneFile {
    pub fn on(&self, s: Section) -> bool {
        (self.sections.is_empty() || self.sections.contains(&s)) && !self.not.contains(&s)
    }

    /// This lane for the band: its bus as an index (`None`: master).
    pub fn lane(&self) -> Option<BusLane> {
        let bus = if self.bus == MASTER {
            None
        } else {
            Some(
                crate::synth::Bus::NAMES
                    .iter()
                    .position(|n| *n == self.bus)?,
            )
        };
        Some(BusLane {
            bus,
            param: self.param.clone(),
            how: self.how,
            curve: self.curve.clone(),
            bars: self.bars,
        })
    }
}

/// A bus lane playing in this bar.
#[derive(Clone, Debug, PartialEq)]
pub struct BusLane {
    /// The bus (`None`: the master's shared effects).
    pub bus: Option<usize>,
    pub param: String,
    pub how: How,
    pub curve: Curve,
    pub bars: f32,
}

impl BusLane {
    /// Its value `pos` bars into the section.
    pub fn at(&self, pos: f32) -> f32 {
        self.curve.at(pos / self.bars.max(1e-3))
    }
}

/// Lay every lane over `buses` (fresh copies of this song's mix) and
/// `master`, at `pos` bars into the section, in the order written. A lane
/// on a setting the bus doesn't have (an effect that's off) does nothing.
pub fn apply(lanes: &[BusLane], pos: f32, buses: &mut [crate::synth::BusMix], master: &mut Master) {
    for l in lanes {
        let f = match l.bus {
            Some(b) => buses.get_mut(b).and_then(|m| m.param(&l.param)),
            None => master.param(&l.param),
        };
        if let Some(f) = f {
            *f = l.how.apply(*f, l.at(pos));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{BUSES, Bus, BusMix};

    fn lane(bus: &str, param: &str, how: How, curve: Curve) -> BusLane {
        BusLaneFile {
            bus: bus.into(),
            param: param.into(),
            how,
            curve,
            bars: 4.0,
            sections: Vec::new(),
            not: Vec::new(),
        }
        .lane()
        .unwrap_or_else(|| panic!("no bus {bus}"))
    }

    #[test]
    fn lanes_move_effect_settings_by_name() {
        let mut buses: [BusMix; BUSES] = Default::default();
        let pad = Bus::Pad as usize;
        buses[pad] = ron::from_str(
            "(eq: [Hp(40.0, 0.7), Peak(900.0, 1.0, -2.0)], chorus: Some((0.4, 8.0, 3.0, 0.5)))",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let lanes = [
            lane("pad", "chorus.mix", How::Set, Curve::Ramp(0.0, 1.0)),
            lane("pad", "eq.1.db", How::Add, Curve::Steps(vec![3.0])),
            lane("pad", "eq.0.hz", How::Octaves, Curve::Steps(vec![1.0])),
            lane("pad", "trim", How::Times, Curve::Steps(vec![0.5])),
            // An effect that's off: nothing to move.
            lane("bass", "chorus.mix", How::Set, Curve::Steps(vec![1.0])),
            lane(MASTER, "feedback", How::Add, Curve::Steps(vec![0.2])),
            lane(MASTER, "wet", How::Times, Curve::Steps(vec![0.0])),
        ];
        let mut master = Master::DEFAULT;
        apply(&lanes, 2.0, &mut buses, &mut master);
        let p = &mut buses[pad];
        assert_eq!(p.chorus.map(|c| c.3), Some(0.5));
        assert_eq!(p.param("eq.1.db").copied(), Some(1.0));
        assert_eq!(p.param("eq.0.hz").copied(), Some(80.0));
        assert_eq!(p.trim, 0.5);
        assert!(buses[Bus::Bass as usize].chorus.is_none());
        assert!((master.feedback - (crate::fx::echo::FEEDBACK + 0.2)).abs() < 1e-6);
        assert_eq!(master.wet, 0.0);
    }

    #[test]
    fn paths_reach_named_and_numbered_fields() {
        let mut m: BusMix = ron::from_str("(autopan: Some((5.0, 0.3)), eq: [LowShelf(90.0, 2.0)])")
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(m.param("autopan.depth").copied(), Some(0.3));
        assert_eq!(m.param("autopan.0").copied(), Some(5.0));
        assert_eq!(m.param("eq.0.db").copied(), Some(2.0));
        assert!(m.param("eq.0.q").is_none());
        assert!(m.param("eq.3.db").is_none());
        assert!(m.param("chorus.mix").is_none());
        assert!(m.param("nope").is_none());
        assert!(
            BusLaneFile {
                bus: "kazoo".into(),
                param: "trim".into(),
                how: How::Add,
                curve: Curve::Steps(vec![]),
                bars: 1.0,
                sections: vec![],
                not: vec![],
            }
            .lane()
            .is_none()
        );
    }

    /// Every bus effect the mixer has can be reached by name.
    #[test]
    fn every_effect_is_automatable() {
        let mut m: BusMix = ron::from_str(
            "(drive: Some((0.5, 0.4)), comp: Some((-20.0, 4.0, 3.0, 90.0, 0.5)),
              filter: Some((Lp, 800.0, 2.0)), width: Some((1.2, 120.0)),
              crush: Some((8.0, 11025.0, 0.5)), transient: Some((0.3, -0.2)),
              phaser: Some((0.3, 0.6, 0.4, 0.5)))",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for (path, want) in [
            ("drive.amount", 0.5),
            ("drive.tone", 0.4),
            ("comp.threshold", -20.0),
            ("comp.mix", 0.5),
            ("filter.cutoff", 800.0),
            ("filter.q", 2.0),
            ("width.amount", 1.2),
            ("width.mono_below", 120.0),
            ("crush.bits", 8.0),
            ("crush.rate", 11025.0),
            ("transient.attack", 0.3),
            ("transient.sustain", -0.2),
            ("phaser.depth", 0.6),
            ("phaser.feedback", 0.4),
        ] {
            assert_eq!(m.param(path).copied(), Some(want), "{path}");
        }
        assert!(m.param("filter.kind").is_none());
        // And overridden per song, field by field.
        let o: crate::synth::BusOverride =
            ron::from_str("(drive: None, crush: Some((4.0, 8000.0, 1.0)))")
                .unwrap_or_else(|e| panic!("{e}"));
        m.merge(&o);
        assert!(m.drive.is_none() && m.comp.is_some());
        assert_eq!(m.crush, Some((4.0, 8000.0, 1.0)));
    }

    #[test]
    fn a_lane_parses_from_a_song() {
        let l: BusLaneFile = ron::from_str(
            "(bus: \"pad\", param: \"chorus.mix\", how: Set, curve: Lfo(shape: Triangle, center: 0.4, depth: 0.2), bars: 8.0, sections: [Hook])",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(l.on(Section::Hook) && !l.on(Section::A));
        assert_eq!(l.bars, 8.0);
    }
}
