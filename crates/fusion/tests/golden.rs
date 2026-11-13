//! Golden lock on the soundtrack: what the composer writes for every mood,
//! hashed, so refactors of the engine (patterns, theory and instruments moving
//! into data) can prove today's songs still play exactly the same.
//!
//! `golden.ron` beside this file holds the hashes. A deliberate change to a
//! song re-records them: `GOLDEN_RECORD=1 cargo test -p fusion --test golden`.
//!
//! `events` runs in CI: note numbers, timing and lengths are integers, and
//! velocities and mix targets are rounded coarsely enough that maths library
//! differences between platforms don't show. `audio` hashes rendered samples
//! (local refactor guard only, `--ignored`: float maths differs by platform).

use std::collections::BTreeMap;

use fusion::compose::{Bar, Composer, Cue, Mood};

const BARS: usize = 32;
const SEEDS: [u64; 4] = [1, 7, 2024, 99];
const INTENSITIES: [f32; 3] = [0.2, 0.55, 0.9];

/// FNV-1a, 64 bit.
struct Fnv(u64);
impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn str(&mut self, s: &str) {
        for b in s.bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

fn hash_bar(h: &mut Fnv, c: &Composer, bar: &Bar) {
    h.str(&format!(
        "{:?}|{:?}|{}|{:.2}|{:.2}|{:.2}|{:.2}|",
        c.mood, c.section, bar.len, bar.gain, bar.bright, bar.wet, bar.echo
    ));
    for b in bar.bus {
        h.str(&format!("{b:.2},"));
    }
    for e in &bar.events {
        h.str(&format!(
            "{}:{:?}:{}:{:.2}:{}:{};",
            e.at, e.inst, e.note, e.vel, e.len, e.glide
        ));
    }
}

/// One run: `bars` bars asking for `cue(bar)`.
fn run(seed: u64, bars: usize, cue: impl Fn(usize) -> Cue) -> String {
    let mut c = Composer::new(seed);
    let mut bar = Bar::new();
    let mut h = Fnv::new();
    for i in 0..bars {
        c.next_bar(cue(i), &mut bar);
        hash_bar(&mut h, &c, &bar);
    }
    format!("{:016x}", h.0)
}

fn events() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for mood in Mood::ALL {
        for seed in SEEDS {
            for i in INTENSITIES {
                let key = format!("{mood:?}/{seed}/{i}");
                out.insert(
                    key,
                    run(seed, BARS, |_| Cue {
                        mood,
                        intensity: i,
                        ..Cue::default()
                    }),
                );
            }
        }
        // A victory in the middle, and a climb up the storeys and back.
        out.insert(
            format!("{mood:?}/victory"),
            run(5, 24, |b| Cue {
                mood,
                intensity: 0.6,
                victories: u32::from(b >= 8),
                ..Cue::default()
            }),
        );
        out.insert(
            format!("{mood:?}/climb"),
            run(11, 28, |b| Cue {
                mood,
                intensity: 0.5,
                lift: [0, 0, 1, 2, 3, 4, 4, 3, 2, 1, 0][(b / 3).min(10)],
                climb: if b < 18 { 1.0 } else { -1.0 },
                ..Cue::default()
            }),
        );
    }
    // Moving between moods (the one-bar transitions).
    out.insert(
        "tour".into(),
        run(3, 96, |b| Cue {
            mood: Mood::ALL[(b / 6) % Mood::ALL.len()],
            intensity: 0.3 + 0.1 * ((b / 6) % 6) as f32,
            victories: (b / 40) as u32,
            ..Cue::default()
        }),
    );
    out
}

fn audio() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for mood in Mood::ALL {
        let (samples, _) = fusion::render::render_score(7, 6.0, 0, |_| Cue {
            mood,
            intensity: mood.base_intensity(),
            ..Cue::default()
        });
        let mut h = Fnv::new();
        for s in samples {
            h.str(&format!("{s:.4},"));
        }
        out.insert(format!("{mood:?}"), format!("{:016x}", h.0));
    }
    out
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Golden {
    events: BTreeMap<String, String>,
    audio: BTreeMap<String, String>,
}

fn path() -> String {
    format!("{}/tests/golden.ron", env!("CARGO_MANIFEST_DIR"))
}

fn load() -> Golden {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| ron::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(g: &Golden) {
    let Ok(text) = ron::ser::to_string_pretty(g, ron::ser::PrettyConfig::default()) else {
        panic!("can't write the golden file");
    };
    if std::fs::write(path(), text).is_err() {
        panic!("can't write {}", path());
    }
}

fn recording() -> bool {
    std::env::var("GOLDEN_RECORD").is_ok_and(|v| v == "1")
}

fn compare(what: &str, want: &BTreeMap<String, String>, got: &BTreeMap<String, String>) {
    let diff: Vec<_> = got
        .iter()
        .filter(|(k, v)| want.get(*k) != Some(*v))
        .map(|(k, _)| k.as_str())
        .chain(
            want.keys()
                .filter(|k| !got.contains_key(*k))
                .map(String::as_str),
        )
        .collect();
    assert!(
        diff.is_empty(),
        "{what} changed for {} runs (first: {:?}). If the change is deliberate, re-record with \
         GOLDEN_RECORD=1 cargo test -p fusion --test golden",
        diff.len(),
        &diff[..diff.len().min(8)]
    );
}

#[test]
fn every_mood_still_writes_the_same_music() {
    let got = events();
    let mut g = load();
    if recording() {
        g.events = got;
        save(&g);
        return;
    }
    compare("the composer's notes", &g.events, &got);
}

#[test]
#[ignore = "local refactor guard: float maths differs by platform"]
fn every_mood_still_sounds_the_same() {
    let got = audio();
    let mut g = load();
    if recording() {
        g.audio = got;
        save(&g);
        return;
    }
    compare("the rendered audio", &g.audio, &got);
}
