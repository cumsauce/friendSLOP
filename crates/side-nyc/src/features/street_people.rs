//! Street people (#358, from catherine): "add random preachers, street
//! performers and tweakers to the city and especially to Penn Station and the
//! subway station. tweakers should be screaming at no one non stop having one
//! side of an argument, preachers should be pointing out people and telling
//! them they're going to hell, and street performers should play bangin music
//! on instruments or occasionally just one scale over and over on a drum pad."
//!
//! The crowd is thickest at the stations: Penn Station's concourse (#357),
//! the PATH platform under the Oculus and the Fulton St entrance. Everyone
//! stands where `data/side/street_people.ron` puts them (a city sidewalk
//! spot, in Penn Station, beside the Fulton St entrance, or along a train
//! stop's platform):
//!
//! - a **tweaker** paces a few steps back and forth and screams one side of
//!   an argument, nonstop, in a voice from the `samiam` speech synth;
//! - a **preacher** turns to the nearest player in range, points (an arm
//!   held out) and tells them they're going to hell;
//! - a **performer** stands at a drum (`models/props/street-drum.gltf`) and
//!   plays a loop: usually a banging groove, sometimes just one scale up and
//!   down on the pad, over and over.
//!
//! It's all local dressing (like Miami's dancers): every peer spawns the same
//! people from the data file and hears them where they stand. Nothing here is
//! replicated and nothing depends on the model file.

use bevy::prelude::*;
use serde::Deserialize;

use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player};

/// What someone is.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Tweaker,
    Preacher,
    Performer,
}

/// Where someone stands.
#[derive(Deserialize, Debug, Clone)]
pub enum Spot {
    /// On the city's floor (x, z).
    City(f32, f32),
    /// Beside the Fulton St entrance: this far (x, z) from where you come back up.
    Fulton(f32, f32),
    /// In Penn Station's concourse (x, z in its stop's frame).
    Penn(f32, f32),
    /// On a train stop's platform (the stop's name), this far along it (m,
    /// + towards the tunnel).
    Platform(String, f32),
}

impl Default for Spot {
    fn default() -> Self {
        Spot::City(0.0, 0.0)
    }
}

/// One street person.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Person {
    pub kind: Kind,
    pub at: Spot,
}

/// Tuning: `assets/data/side/street_people.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Everyone, and where they stand.
    pub people: Vec<Person>,
    /// Only speak or play while a player is this close (m).
    pub hear_range: f32,
    /// Seconds between a tweaker's yells, and a preacher's sermons (min, max).
    pub tweaker_every: (f32, f32),
    pub preacher_every: (f32, f32),
    /// Speech voices (pitch Hz, speed).
    pub tweaker_voice: (f32, f32),
    pub preacher_voice: (f32, f32),
    /// How far a tweaker paces each way (m) and how fast (m/s).
    pub pace: f32,
    pub pace_speed: f32,
    /// How close a preacher picks someone out (m).
    pub point_range: f32,
    /// A performer's tempo (beats per minute) and the bars in one loop.
    pub bpm: f32,
    pub bars: u32,
    /// The chance a loop is just the scale on the pad, not the groove.
    pub scale_chance: f32,
    /// How loud the people are.
    pub voice_volume: f32,
    pub music_volume: f32,
    /// What tweakers scream (one side of an argument with nobody).
    pub tweaker_lines: Vec<String>,
    /// What preachers tell the person they point at.
    pub preacher_lines: Vec<String>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/street_people.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The performer's drum (from the modeler); they still play without it.
pub const DRUM_MODEL: &str = "models/props/street-drum.gltf";

/// Where a spot is in the world (feet), or `None` if it names no stop.
pub fn spot_at(s: &Spot) -> Option<Vec3> {
    match s {
        Spot::City(x, z) => Some(Vec3::new(*x, crate::rules::nyc::CITY_Y, *z)),
        Spot::Fulton(x, z) => {
            Some(super::fulton::street_at(&super::fulton::tuning()) + Vec3::new(*x, 0.0, *z))
        }
        Spot::Penn(x, z) => super::penn::frame().map(|f| f.transform_point(Vec3::new(*x, 0.0, *z))),
        Spot::Platform(name, along) => {
            let t = super::trains::tuning();
            let stop = t
                .lines
                .iter()
                .flat_map(|l| l.stops.iter())
                .find(|s| &s.name == name)?;
            let f = super::trains::stop_frame(stop);
            Some(f.translation + f.rotation * Vec3::new(*along, PLATFORM_Y, PLATFORM_MID))
        }
    }
}

/// A platform's top over the car's origin, and its middle across (the
/// `path-platform` model's).
const PLATFORM_Y: f32 = 0.26;
const PLATFORM_MID: f32 = 3.6;

/// A small deterministic hash, so every peer picks the same.
pub fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

/// A number in `lo..hi` from a hash.
pub fn between((lo, hi): (f32, f32), h: u32) -> f32 {
    lo + (hi - lo) * (h % 10_000) as f32 / 10_000.0
}

/// A tweaker's offset along their pacing line at `secs` (m): back and forth,
/// stopping at each end.
pub fn pacing(secs: f32, pace: f32, speed: f32, i: u32) -> f32 {
    if pace <= 0.0 || speed <= 0.0 {
        return 0.0;
    }
    let period = 4.0 * pace / speed + 2.0;
    let p = ((secs + i as f32 * 1.7) % period) / period;
    // Half the time walking one way, half the other, a pause at each end.
    let walk = (4.0 * pace / speed) / period / 2.0;
    let rest = (1.0 - 2.0 * walk) / 2.0;
    if p < walk {
        -pace + 2.0 * pace * p / walk
    } else if p < walk + rest {
        pace
    } else if p < 2.0 * walk + rest {
        pace - 2.0 * pace * (p - walk - rest) / walk
    } else {
        -pace
    }
}

/// The nearest of `players` within `range` of `at`.
pub fn nearest(at: Vec3, players: &[Vec3], range: f32) -> Option<Vec3> {
    players
        .iter()
        .copied()
        .filter(|p| p.distance(at) <= range)
        .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))
}

/// The notes of the scale on the pad (MIDI): a major scale up and back down.
pub fn scale_notes() -> Vec<u8> {
    let up = [60u8, 62, 64, 65, 67, 69, 71, 72];
    let mut v = up.to_vec();
    v.extend(up.iter().rev().skip(1).take(6));
    v
}

fn midi_hz(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// One performer loop: `bars` bars at `bpm`, either the groove (kick,
/// snare, hats and a bass line) or the scale on a drum pad (a pitched tom
/// hit on every eighth). Mono samples at `rate`.
pub fn performance(bpm: f32, bars: u32, scale: bool, seed: u32, rate: u32) -> Vec<f32> {
    let beat = 60.0 / bpm.max(30.0);
    let eighths = bars.max(1) * 8;
    let step = beat / 2.0;
    let len = (eighths as f32 * step * rate as f32) as usize;
    let mut out = vec![0.0f32; len];
    let r = rate as f32;
    let mut add = |at: f32, dur: f32, f: &dyn Fn(f32) -> f32| {
        let s0 = (at * r) as usize;
        let n = (dur * r) as usize;
        for i in 0..n {
            if let Some(o) = out.get_mut(s0 + i) {
                *o += f(i as f32 / r);
            }
        }
    };
    let tau = std::f32::consts::TAU;
    if scale {
        let notes = scale_notes();
        for e in 0..eighths {
            let hz = midi_hz(notes[e as usize % notes.len()] as f32 - 12.0);
            add(e as f32 * step, step, &|t| {
                let bend = hz * (1.0 + 0.6 * (-t * 40.0).exp());
                (tau * bend * t).sin() * (-t * 9.0).exp() * 0.6
            });
        }
    } else {
        // A bass line from a seeded pick of roots.
        let roots = [36.0, 36.0, 43.0, 41.0, 39.0, 36.0, 34.0, 41.0];
        for e in 0..eighths {
            let t0 = e as f32 * step;
            let on_beat = e % 2 == 0;
            if e % 4 == 0 || (e % 8 == 7 && hash(seed, e).is_multiple_of(2)) {
                add(t0, 0.35, &|t| {
                    let f = 50.0 + 110.0 * (-t * 30.0).exp();
                    (tau * f * t).sin() * (-t * 8.0).exp() * 0.9
                });
            }
            if e % 8 == 4 {
                add(t0, 0.25, &|t| {
                    let n =
                        ((hash(seed ^ 0x51, (t * 48_000.0) as u32) % 2001) as f32 / 1000.0) - 1.0;
                    (n * 0.5 + (tau * 190.0 * t).sin() * 0.3) * (-t * 14.0).exp()
                });
            }
            let hat = if on_beat { 0.12 } else { 0.2 };
            add(t0, 0.05, &|t| {
                let n = ((hash(seed ^ 0xa7, (t * 48_000.0) as u32) % 2001) as f32 / 1000.0) - 1.0;
                n * hat * (-t * 80.0).exp()
            });
            let root = roots[(e / 2 % 8) as usize] + if e % 2 == 1 { 12.0 } else { 0.0 };
            let hz = midi_hz(root);
            add(t0, step * 0.9, &|t| {
                let ph = (hz * t).fract();
                (ph * 2.0 - 1.0) * 0.25 * (-t * 4.0).exp()
            });
        }
    }
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-3);
    for s in &mut out {
        *s = (*s / peak * 0.8).tanh();
    }
    out
}

/// A street person's own state (local).
#[derive(Component)]
struct StreetPerson {
    i: u32,
    kind: Kind,
    home: Vec3,
    /// Seconds until they next speak or start a loop.
    next: f32,
    /// How many times they've spoken or played.
    said: u32,
}

/// A preacher's pointing arm.
#[derive(Component)]
struct PointingArm;

pub fn plugin(app: &mut App) {
    // They're only there while you're in the city (#412): they come out when
    // you arrive and are put away when you leave.
    app.add_systems(
        Update,
        (spawn.run_if(crate::sites::arrived("nyc")), (act, sound))
            .chain()
            .run_if(crate::sites::here("nyc"))
            .run_if(in_state(AppState::InGame)),
    )
    .add_systems(
        Update,
        crate::sites::put_away::<StreetPerson>.run_if(crate::sites::left("nyc")),
    );
}

fn spawn(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = tuning();
    let arm = meshes.add(Capsule3d::new(0.06, 0.55));
    let sleeve = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.1, 0.1),
        perceptual_roughness: 0.9,
        ..default()
    });
    for (i, p) in t.people.iter().enumerate() {
        let i = i as u32;
        let Some(at) = spot_at(&p.at) else {
            continue;
        };
        let mut e = commands.spawn((
            StreetPerson {
                i,
                kind: p.kind,
                home: at,
                next: between((0.5, 4.0), hash(i, 1)),
                said: 0,
            },
            crate::people::Gait::default(),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(between(
                (0.0, std::f32::consts::TAU),
                hash(i, 2),
            ))),
            Visibility::default(),
            crate::people::CharacterLook::random(0x0358_0000 + i as u64 * 31),
            DespawnOnExit(AppState::InGame),
        ));
        match p.kind {
            Kind::Preacher => {
                // The arm held out at whoever they're damning.
                e.with_child((
                    PointingArm,
                    Mesh3d(arm.clone()),
                    MeshMaterial3d(sleeve.clone()),
                    Transform::from_xyz(0.25, 1.35, 0.35)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
            }
            Kind::Performer => {
                e.with_child((
                    WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(DRUM_MODEL))),
                    Transform::from_xyz(0.0, 0.0, 0.55),
                ));
            }
            Kind::Tweaker => {}
        }
    }
}

/// Where the players are (feet).
fn player_spots(walkers: &Query<&Walker, With<Player>>) -> Vec<Vec3> {
    walkers.iter().map(|w| w.pos).collect()
}

/// Move them: tweakers pace, preachers face their target, performers bob.
fn act(
    time: Res<Time>,
    walkers: Query<&Walker, With<Player>>,
    mut people: Query<(&StreetPerson, &mut Transform)>,
) {
    let t = tuning();
    let secs = time.elapsed_secs();
    let players = player_spots(&walkers);
    for (p, mut tf) in &mut people {
        match p.kind {
            Kind::Tweaker => {
                let yaw = between((0.0, std::f32::consts::TAU), hash(p.i, 2));
                let dir = Quat::from_rotation_y(yaw) * Vec3::X;
                let x = pacing(secs, t.pace, t.pace_speed, p.i);
                let was = tf.translation;
                tf.translation = p.home + dir * x;
                let step = tf.translation - was;
                // Face the way they walk, twitching.
                let twitch = (secs * 13.0 + p.i as f32).sin() * 0.25;
                if step.length_squared() > 1e-6 {
                    let look = step.normalize();
                    tf.rotation = Quat::from_rotation_y(look.x.atan2(look.z) + twitch);
                } else {
                    tf.rotation = Quat::from_rotation_y(yaw + twitch);
                }
            }
            Kind::Preacher => {
                if let Some(at) = nearest(p.home, &players, t.point_range) {
                    let d = at - p.home;
                    tf.rotation = Quat::from_rotation_y(d.x.atan2(d.z));
                }
            }
            Kind::Performer => {
                let beat = secs * t.bpm / 60.0 * std::f32::consts::PI;
                tf.translation = p.home + Vec3::Y * (beat.sin().abs() * 0.04);
                tf.rotation = Quat::from_rotation_y(
                    between((0.0, std::f32::consts::TAU), hash(p.i, 2)) + (beat * 0.5).sin() * 0.15,
                );
            }
        }
    }
}

/// Local: speak and play while the local player's near.
fn sound(
    mut commands: Commands,
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    mut people: Query<(&mut StreetPerson, &Transform)>,
) {
    let Some(me) = me else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let t = tuning();
    let dt = time.delta_secs();
    for (mut p, tf) in &mut people {
        p.next -= dt;
        if p.next > 0.0 {
            continue;
        }
        let near = tf.translation.distance(w.pos) <= t.hear_range;
        let (i, said) = (p.i, p.said);
        let h = hash(i, said + 100);
        let mouth = tf.translation + Vec3::Y * 1.6;
        match p.kind {
            Kind::Tweaker | Kind::Preacher => {
                let (lines, voice, every) = if p.kind == Kind::Tweaker {
                    (&t.tweaker_lines, t.tweaker_voice, t.tweaker_every)
                } else {
                    (&t.preacher_lines, t.preacher_voice, t.preacher_every)
                };
                p.next = between(every, h);
                if !near || lines.is_empty() {
                    continue;
                }
                let line = lines[h as usize % lines.len()].clone();
                // Each person's own pitch.
                let pitch = voice.0 * between((0.85, 1.2), hash(i, 3));
                let speed = voice.1;
                crate::spatial::play_wav_at(
                    &mut commands,
                    mouth,
                    t.voice_volume,
                    crate::spatial::Carry::CALL,
                    move || {
                        let (s, rate) = super::trains::announcer_says(&line, (pitch, speed));
                        crate::features::bear::wav_bytes(&s, rate)
                    },
                );
            }
            Kind::Performer => {
                let beat = 60.0 / t.bpm.max(30.0);
                p.next = beat * 4.0 * t.bars.max(1) as f32;
                if !near {
                    p.next = 1.0;
                    continue;
                }
                let scale = between((0.0, 1.0), h) < t.scale_chance;
                let (bpm, bars) = (t.bpm, t.bars);
                crate::spatial::play_wav_at(
                    &mut commands,
                    tf.translation + Vec3::Y * 0.8,
                    t.music_volume,
                    crate::spatial::Carry::CALL,
                    move || {
                        let rate = crate::features::synth_audio::RATE;
                        let s = performance(bpm, bars, scale, h, rate);
                        crate::features::bear::wav_bytes(&s, rate)
                    },
                );
            }
        }
        p.said += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.hear_range > 0.0 && t.point_range > 0.0);
        assert!(t.tweaker_every.0 > 0.0 && t.tweaker_every.1 >= t.tweaker_every.0);
        assert!(t.preacher_every.0 > 0.0 && t.preacher_every.1 >= t.preacher_every.0);
        assert!(t.tweaker_voice.0 > 0.0 && t.preacher_voice.1 > 0.0);
        assert!(t.pace > 0.0 && t.pace_speed > 0.0 && t.bpm > 0.0 && t.bars > 0);
        assert!((0.0..=1.0).contains(&t.scale_chance));
        assert!(t.voice_volume > 0.0 && t.music_volume > 0.0);
        assert!(!t.tweaker_lines.is_empty() && !t.preacher_lines.is_empty());
        for k in [Kind::Tweaker, Kind::Preacher, Kind::Performer] {
            assert!(t.people.iter().any(|p| p.kind == k), "no {k:?}");
        }
        // Most of them hang around the stations.
        let at_stations = t
            .people
            .iter()
            .filter(|p| !matches!(p.at, Spot::City(..)))
            .count();
        assert!(at_stations * 2 >= t.people.len());
    }

    #[test]
    fn everyone_stands_in_the_city() {
        let t = t();
        let path = |p: &str| format!("{}/../game/assets/{p}", env!("CARGO_MANIFEST_DIR"));
        for (d, p) in [
            (
                &super::super::fulton::TUNING as &dyn friendslop_data::DataFile,
                "data/side/fulton.ron",
            ),
            (&super::super::trains::TUNING, "data/side/trains.ron"),
            (&super::super::penn::TUNING, "data/side/penn.ron"),
        ] {
            let Ok(b) = std::fs::read(path(p)) else {
                panic!("no {p}");
            };
            assert!(d.set_bytes(&b));
        }
        for p in &t.people {
            let Some(at) = spot_at(&p.at) else {
                panic!("{:?} names no stop", p.at);
            };
            assert!(crate::rules::nyc::in_city(at.x, at.z), "{:?} at {at}", p.at);
        }
    }

    #[test]
    fn tweakers_pace_within_their_line() {
        for i in 0..50 {
            let x = pacing(i as f32 * 0.37, 2.0, 1.5, 3);
            assert!((-2.0..=2.0).contains(&x));
        }
        assert_ne!(pacing(0.0, 2.0, 1.5, 0), pacing(1.0, 2.0, 1.5, 0));
    }

    #[test]
    fn preachers_pick_the_nearest_in_range() {
        let ps = [Vec3::new(10.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0)];
        assert_eq!(nearest(Vec3::ZERO, &ps, 5.0), Some(ps[1]));
        assert_eq!(nearest(Vec3::ZERO, &ps, 2.0), None);
    }

    #[test]
    fn performances_make_sound() {
        for scale in [false, true] {
            let s = performance(110.0, 2, scale, 7, 22_050);
            let secs = s.len() as f32 / 22_050.0;
            assert!((secs - 2.0 * 4.0 * 60.0 / 110.0).abs() < 0.05);
            assert!(s.iter().any(|x| x.abs() > 0.3));
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
        let n = scale_notes();
        assert_eq!(n.first(), Some(&60));
        assert_eq!(n.iter().max(), Some(&72));
    }
}
