//! Parameter automation: a song part's `auto` lanes move a sound's settings
//! over time (filter sweeps, wobbles, risers, gated chops), written as data in
//! the song files. The composer hands each bar the lanes that are on; the band
//! evaluates them every block.
//!
//! Positions are in bars from the start of the current section, so a ramp
//! with `bars: 8` sweeps across the first eight bars of every section it
//! plays in (and starts again at nine).

use crate::compose::Section;
use crate::inst::Inst;

/// How a lane's value meets the field it moves.
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum How {
    /// Added to the patch's value.
    #[default]
    Add,
    /// The patch's value times it.
    Times,
    /// The patch's value times 2^it (cutoffs, rates: octaves).
    Octaves,
    /// Replaces the patch's value.
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

/// A patch whose numbers lanes can move while its notes sound: `field` is
/// every automatable setting by its name in instruments.ron.
pub trait Automate {
    fn field(&mut self, name: &str) -> Option<&mut f32>;
    /// Every name `field` knows.
    const FIELDS: &'static [&'static str];

    /// Move `name` by `v` (false: no such setting).
    fn automate(&mut self, name: &str, how: How, v: f32) -> bool {
        let Some(f) = self.field(name) else {
            return false;
        };
        *f = how.apply(*f, v);
        true
    }
}

/// Implements [`Automate`] for a patch from its plain `f32` fields.
#[macro_export]
macro_rules! automate {
    ($t:ty { $($f:ident),* $(,)? }) => {
        impl $crate::auto::Automate for $t {
            const FIELDS: &'static [&'static str] = &[$(stringify!($f)),*];
            fn field(&mut self, name: &str) -> Option<&mut f32> {
                match name {
                    $(stringify!($f) => Some(&mut self.$f),)*
                    _ => None,
                }
            }
        }
    };
}

/// Lanes on this name set the voice's level in dB (every pitched engine;
/// not drum or foley samples), whatever the engine.
pub const GAIN_DB: &str = "level_db";

/// An LFO's shape (one cycle 0..1).
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum Shape {
    #[default]
    Sine,
    Triangle,
    /// Rising saw (0 → 1, then jumps back).
    Saw,
    /// Falling saw (1 → 0): a sidechain-ish duck.
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
    /// One value per cycle, in turn (a gate, a stepped sweep).
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

/// One lane as written in a song part.
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct AutoFile {
    /// The setting it moves: any number in the instrument's engine settings
    /// in instruments.ron (`cut_base`, `k`, `env_oct`, `i1_sus`, `vibrato`...;
    /// Fm and Sub instruments), or `level_db` on any pitched instrument.
    pub param: String,
    #[serde(default)]
    pub how: How,
    pub curve: Curve,
    /// One cycle's length in bars (fractions work: 0.25 is a beat in 4/4).
    #[serde(default = "one")]
    pub bars: f32,
    /// Only in these sections (all of the part's if empty), never in `not`.
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub not: Vec<Section>,
}

impl AutoFile {
    pub fn on(&self, s: Section) -> bool {
        (self.sections.is_empty() || self.sections.contains(&s)) && !self.not.contains(&s)
    }
}

/// A lane playing in this bar, for one instrument.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub inst: Inst,
    pub param: String,
    pub how: How,
    pub curve: Curve,
    pub bars: f32,
}

impl Lane {
    /// Its value `pos` bars into the section.
    pub fn at(&self, pos: f32) -> f32 {
        self.curve.at(pos / self.bars.max(1e-3))
    }
}

/// The voice's level from its `level_db` lanes at `p0` and `p1` bars into
/// the section (linear gains, 1 with none).
pub fn gain(lanes: &[Lane], inst: Inst, p0: f32, p1: f32) -> (f32, f32) {
    let (mut a, mut b) = (0.0, 0.0);
    for l in lanes
        .iter()
        .filter(|l| l.inst == inst && l.param == GAIN_DB)
    {
        a = l.how.apply(a, l.at(p0));
        b = l.how.apply(b, l.at(p1));
    }
    (db(a), db(b))
}

/// Lay every lane on `inst` (but `level_db`) over `p`, a fresh copy of its
/// patch, at `pos` bars into the section. Lanes apply in the order written.
pub fn apply<P: Automate>(lanes: &[Lane], inst: Inst, pos: f32, p: &mut P) {
    for l in lanes
        .iter()
        .filter(|l| l.inst == inst && l.param != GAIN_DB)
    {
        p.automate(&l.param, l.how, l.at(pos));
    }
}

fn db(d: f32) -> f32 {
    if d <= -60.0 {
        0.0
    } else {
        10f32.powf(d / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_move_as_written() {
        let r = Curve::Ramp(-2.0, 2.0);
        assert_eq!(r.at(0.0), -2.0);
        assert!((r.at(0.5)).abs() < 1e-6);
        assert_eq!(r.at(1.0), -2.0);
        let s = Curve::Steps(vec![0.0, -60.0]);
        assert_eq!(s.at(0.2), 0.0);
        assert_eq!(s.at(1.7), -60.0);
        assert_eq!(s.at(2.1), 0.0);
        let l = Curve::Lfo {
            shape: Shape::Square,
            center: 1.0,
            depth: 0.5,
        };
        assert_eq!(l.at(0.1), 1.5);
        assert_eq!(l.at(0.6), 0.5);
    }

    #[derive(Default)]
    struct P {
        cut: f32,
        k: f32,
    }
    crate::automate!(P { cut, k });

    #[test]
    fn lanes_only_touch_their_instrument_and_field() {
        let a = Inst::named("SuperSaw");
        let b = Inst::named("Reese");
        let lane = |param: &str, how, curve| Lane {
            inst: a,
            param: param.into(),
            how,
            curve,
            bars: 4.0,
        };
        let lanes = [
            lane("cut", How::Octaves, Curve::Ramp(0.0, 2.0)),
            lane("k", How::Set, Curve::Steps(vec![0.5])),
            lane(GAIN_DB, How::Add, Curve::Steps(vec![-6.0])),
        ];
        let mut p = P { cut: 100.0, k: 2.0 };
        apply(&lanes, b, 2.0, &mut p);
        assert_eq!((p.cut, p.k), (100.0, 2.0));
        apply(&lanes, a, 2.0, &mut p);
        assert!((p.cut - 200.0).abs() < 0.1 && p.k == 0.5);
        assert!((gain(&lanes, a, 0.0, 1.0).0 - 0.501).abs() < 0.01);
        assert_eq!(gain(&lanes, b, 0.0, 1.0), (1.0, 1.0));
        assert!(!P::default().automate("nope", How::Add, 1.0));
    }

    #[test]
    fn a_lane_parses_from_a_song() {
        let a: AutoFile =
            ron::from_str("(param: \"level_db\", curve: Steps([0.0, -60.0]), bars: 0.125)")
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.bars, 0.125);
        let b: AutoFile = ron::from_str(
            "(param: \"cut_base\", how: Octaves, curve: Lfo(shape: Triangle, center: 0.0, depth: 1.5), sections: [Hook])",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(b.how, How::Octaves);
        assert!(b.on(Section::Hook) && !b.on(Section::A));
    }
}
