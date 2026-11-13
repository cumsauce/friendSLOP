//! The score's rules: which mood plays when, as data (`moods.ron`'s
//! `rules`, `hush` and `silence`). No game code: the game hands over what
//! it knows through [`Facts`] (the signals its features publish, where the
//! camera is, which zone that is) and gets a mood and an intensity back.
//!
//! - [`Cond`]: `Signal("duel", Gt, 0.0)`, `InGame`, `InZone("Forest")`,
//!   `InBox([x, z], [x, z])`, `Near("landlord", 12.0)`, `All`, `Any`, `Not`.
//! - [`Expr`]: a little arithmetic over signals and numbers
//!   (`"0.65 + 0.35 * (1 - clamp(secs_left / seek_secs, 0, 1))"`).

use crate::compose::Mood;

/// What the game knows this frame.
pub trait Facts {
    /// A signal's value (0 when nothing set it).
    fn signal(&self, name: &str) -> f32;
    /// Playing (not at the title screen).
    fn in_game(&self) -> bool;
    /// Where the camera is.
    fn at(&self) -> Option<[f32; 3]>;
    /// Is the camera in the zone called `zone`?
    fn in_zone(&self, zone: &str) -> bool;
    /// Where the map spot `name` is (x, z).
    fn spot(&self, name: &str) -> Option<[f32; 2]>;
}

/// How a signal compares with a value.
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

/// When a rule applies.
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub enum Cond {
    Signal(String, Op, f32),
    InGame,
    InZone(String),
    /// The camera inside a box: `[x, z]` corners, or `[x, y, z]` to bound
    /// the height too.
    InBox(Vec<f32>, Vec<f32>),
    /// The camera within this many metres (across the ground) of a map spot.
    Near(String, f32),
    All(Vec<Cond>),
    Any(Vec<Cond>),
    Not(Box<Cond>),
}

impl Cond {
    pub fn holds(&self, f: &dyn Facts) -> bool {
        match self {
            Cond::Signal(name, op, v) => {
                let s = f.signal(name);
                match op {
                    Op::Gt => s > *v,
                    Op::Ge => s >= *v,
                    Op::Lt => s < *v,
                    Op::Le => s <= *v,
                    Op::Eq => s == *v,
                    Op::Ne => s != *v,
                }
            }
            Cond::InGame => f.in_game(),
            Cond::InZone(z) => f.in_zone(z),
            Cond::InBox(lo, hi) => f.at().is_some_and(|[x, y, z]| {
                let p: &[f32] = if lo.len() >= 3 && hi.len() >= 3 {
                    &[x, y, z]
                } else {
                    &[x, z]
                };
                p.len() <= lo.len().min(hi.len())
                    && p.iter()
                        .zip(lo.iter().zip(hi.iter()))
                        .all(|(v, (a, b))| *v >= a.min(*b) && *v <= a.max(*b))
            }),
            Cond::Near(spot, r) => match (f.at(), f.spot(spot)) {
                (Some([x, _, z]), Some([sx, sz])) => (x - sx).hypot(z - sz) < *r,
                _ => false,
            },
            Cond::All(cs) => cs.iter().all(|c| c.holds(f)),
            Cond::Any(cs) => cs.iter().any(|c| c.holds(f)),
            Cond::Not(c) => !c.holds(f),
        }
    }
}

/// One rule: in `mood` at `intensity` (else the mood's own) when `when`.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct Rule {
    pub when: Cond,
    pub mood: Mood,
    #[serde(default)]
    pub intensity: Option<Expr>,
}

/// The first rule that holds: its mood and intensity (the fallback mood if
/// none does).
pub fn pick(rules: &[Rule], f: &dyn Facts) -> (Mood, f32) {
    match rules.iter().find(|r| r.when.holds(f)) {
        Some(r) => (
            r.mood,
            r.intensity
                .as_ref()
                .map_or_else(|| r.mood.base_intensity(), |e| e.eval(f)),
        ),
        None => {
            let m = Mood::default();
            (m, m.base_intensity())
        }
    }
}

/// An arithmetic expression over signals (see the module docs). Written in
/// the file as a string, parsed as it's read.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f32),
    Signal(String),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

impl Expr {
    pub fn eval(&self, f: &dyn Facts) -> f32 {
        match self {
            Expr::Num(n) => *n,
            Expr::Signal(s) => f.signal(s),
            Expr::Neg(e) => -e.eval(f),
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(f), b.eval(f));
                match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    _ => a / b,
                }
            }
            Expr::Call(name, args) => {
                let v: Vec<f32> = args.iter().map(|a| a.eval(f)).collect();
                match (name.as_str(), v.as_slice()) {
                    ("min", [a, b]) => a.min(*b),
                    ("max", [a, b]) => a.max(*b),
                    ("clamp", [x, lo, hi]) => x.clamp(*lo, hi.max(*lo)),
                    _ => 0.0,
                }
            }
        }
    }

    /// Parse `text` (see the module docs).
    pub fn parse(text: &str) -> Result<Expr, String> {
        let toks = lex(text)?;
        let mut p = Parser { toks, at: 0 };
        let e = p.sum()?;
        match p.toks.get(p.at) {
            None => Ok(e),
            Some(t) => Err(format!("unexpected {t:?} in {text:?}")),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Expr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Expr::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f32),
    Name(String),
    Sym(char),
}

fn lex(text: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let cs: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let s = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            let n: String = cs[s..i].iter().collect();
            out.push(Tok::Num(
                n.parse().map_err(|_| format!("bad number {n:?}"))?,
            ));
        } else if c.is_alphabetic() || c == '_' {
            let s = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            out.push(Tok::Name(cs[s..i].iter().collect()));
        } else if "+-*/(),".contains(c) {
            out.push(Tok::Sym(c));
            i += 1;
        } else {
            return Err(format!("unexpected {c:?} in {text:?}"));
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek_sym(&self, c: char) -> bool {
        self.toks.get(self.at) == Some(&Tok::Sym(c))
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.peek_sym(c) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("expected {c:?}"))
        }
    }

    fn sum(&mut self) -> Result<Expr, String> {
        let mut e = self.product()?;
        while let Some(Tok::Sym(op @ ('+' | '-'))) = self.toks.get(self.at).cloned() {
            self.at += 1;
            e = Expr::Bin(op, Box::new(e), Box::new(self.product()?));
        }
        Ok(e)
    }

    fn product(&mut self) -> Result<Expr, String> {
        let mut e = self.unary()?;
        while let Some(Tok::Sym(op @ ('*' | '/'))) = self.toks.get(self.at).cloned() {
            self.at += 1;
            e = Expr::Bin(op, Box::new(e), Box::new(self.unary()?));
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.peek_sym('-') {
            self.at += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, String> {
        let Some(t) = self.toks.get(self.at).cloned() else {
            return Err("unexpected end".into());
        };
        self.at += 1;
        match t {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Sym('(') => {
                let e = self.sum()?;
                self.expect(')')?;
                Ok(e)
            }
            Tok::Name(name) if self.peek_sym('(') => {
                self.at += 1;
                let mut args = Vec::new();
                if !self.peek_sym(')') {
                    args.push(self.sum()?);
                    while self.peek_sym(',') {
                        self.at += 1;
                        args.push(self.sum()?);
                    }
                }
                self.expect(')')?;
                let want = match name.as_str() {
                    "min" | "max" => 2,
                    "clamp" => 3,
                    _ => return Err(format!("no function {name}")),
                };
                if args.len() != want {
                    return Err(format!("{name} takes {want} arguments"));
                }
                Ok(Expr::Call(name, args))
            }
            Tok::Name(name) => Ok(Expr::Signal(name)),
            Tok::Sym(c) => Err(format!("unexpected {c:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct World {
        signals: BTreeMap<&'static str, f32>,
        in_game: bool,
        at: Option<[f32; 3]>,
        zone: &'static str,
    }

    impl Facts for World {
        fn signal(&self, name: &str) -> f32 {
            self.signals.get(name).copied().unwrap_or(0.0)
        }
        fn in_game(&self) -> bool {
            self.in_game
        }
        fn at(&self) -> Option<[f32; 3]> {
            self.at
        }
        fn in_zone(&self, zone: &str) -> bool {
            self.zone == zone
        }
        fn spot(&self, name: &str) -> Option<[f32; 2]> {
            (name == "here").then_some([10.0, 0.0])
        }
    }

    fn eval(text: &str, w: &World) -> f32 {
        match Expr::parse(text) {
            Ok(e) => e.eval(w),
            Err(e) => panic!("{text}: {e}"),
        }
    }

    #[test]
    fn expressions_do_arithmetic() {
        let mut w = World::default();
        w.signals.insert("secs_left", 15.0);
        w.signals.insert("seek_secs", 60.0);
        assert_eq!(eval("1 + 2 * 3", &w), 7.0);
        assert_eq!(eval("(1 + 2) * 3", &w), 9.0);
        assert_eq!(eval("-2 - -3", &w), 1.0);
        assert_eq!(eval("8 / 2 / 2", &w), 2.0);
        assert_eq!(eval("min(3, max(1, 2))", &w), 2.0);
        assert_eq!(eval("clamp(5, 0, 1)", &w), 1.0);
        assert_eq!(eval("unset + 1", &w), 1.0);
        let left: f32 = 15.0 / 60.0;
        assert_eq!(
            eval("0.65 + 0.35 * (1 - clamp(secs_left / seek_secs, 0, 1))", &w),
            0.65 + 0.35 * (1.0 - left.clamp(0.0, 1.0))
        );
        for bad in ["1 +", "(1", "foo(1)", "min(1)", "1 $ 2", "1 2"] {
            assert!(Expr::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn conditions_combine() {
        let mut w = World {
            in_game: true,
            at: Some([10.0, 5.0, 3.0]),
            zone: "Forest",
            ..World::default()
        };
        w.signals.insert("duel", 1.0);
        let holds = |text: &str, w: &World| match ron::from_str::<Cond>(text) {
            Ok(c) => c.holds(w),
            Err(e) => panic!("{text}: {e}"),
        };
        assert!(holds("Signal(\"duel\", Gt, 0.0)", &w));
        assert!(!holds("Signal(\"duel\", Lt, 1.0)", &w));
        assert!(holds("All([InGame, InZone(\"Forest\")])", &w));
        assert!(holds(
            "Any([InZone(\"Ruins\"), Not(Signal(\"x\", Ne, 0.0))])",
            &w
        ));
        assert!(holds("InBox([0, 0], [20, 20])", &w));
        assert!(!holds("InBox([0, 0, 0], [20, 4, 20])", &w), "too high");
        assert!(holds("Near(\"here\", 4.0)", &w));
        assert!(!holds("Near(\"nowhere\", 400.0)", &w));
        assert!(holds("All([])", &w));
        w.at = None;
        assert!(!holds("InBox([0, 0], [20, 20])", &w));
    }
}
