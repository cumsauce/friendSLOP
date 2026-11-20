//! Fulton St's ambience (issue by doomy: "create some environmental sounds for
//! fulton station and wire them up ... modify/change depending on the level
//! (lower is more creepy)"). Local decoration only, nothing replicated.
//!
//! - Two room-tone loops: a lit fluorescent hum that fades out as you go down
//!   (and is silent on a pitch-dark level, whose lights are off) and a low sub
//!   drone that fades in.
//! - Noises now and then around you: drips and a far train near the top, pipe
//!   clanks throughout, metal groans and something breathing near the bottom.
//!
//! The mix follows your *depth* (0 on the top level, 1 on the J platform); the
//! numbers are in `data/side/fulton_ambience.ron`. Missing sound files stay
//! silent.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

use super::fulton::{Station, dark_level, level_at, tuning as station_tuning};
use crate::AppState;
use crate::rules::nyc::in_station;
use crate::sound::{AudioSample, SoundVolume, play_loop};
use crate::spatial::{Carry, Ears, play_at};

/// One kind of noise.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Noise {
    /// Names its takes: `audio/sfx/fulton/fulton_<id>_1..3.wav`.
    pub id: String,
    /// Weight on the top level.
    pub top: f32,
    /// Weight on the bottom level.
    pub bottom: f32,
    pub volume: f32,
    /// Closest and furthest it's placed from you, metres.
    pub near: f32,
    pub far: f32,
    /// Carries far (else it's a nearby sound).
    pub loud: bool,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub hum_volume: f32,
    pub deep_volume: f32,
    pub hum_gone_at: f32,
    pub deep_from: f32,
    pub dark_boost: f32,
    pub fade: f32,
    pub gap_top: f32,
    pub gap_bottom: f32,
    pub events: Vec<Noise>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/fulton_ambience.ron");

/// How deep a level is: 0 on the top level, 1 on the last.
pub fn depth(level: usize, levels: usize) -> f32 {
    if levels <= 1 {
        return 0.0;
    }
    (level as f32 / (levels - 1) as f32).clamp(0.0, 1.0)
}

/// The (hum, drone) volumes at `depth`. A pitch-dark level has no lights to
/// hum and a heavier drone.
pub fn bed_levels(t: &Tuning, depth: f32, dark: bool) -> (f32, f32) {
    let hum = if dark {
        0.0
    } else {
        t.hum_volume * (1.0 - depth / t.hum_gone_at.max(1e-3)).clamp(0.0, 1.0)
    };
    let ramp = (depth - t.deep_from) / (1.0 - t.deep_from).max(1e-3);
    let boost = if dark { t.dark_boost } else { 0.0 };
    let deep = t.deep_volume * (ramp.clamp(0.0, 1.0) + boost).min(1.0);
    (hum, deep)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Seconds until the next noise (`roll` 0..1 spreads it a little).
pub fn next_gap(t: &Tuning, depth: f32, roll: f32) -> f32 {
    lerp(t.gap_top, t.gap_bottom, depth) * (0.6 + 0.8 * roll)
}

/// Which noise comes next (`roll` 0..1), by the weights at this depth.
pub fn pick(t: &Tuning, depth: f32, roll: f32) -> Option<&Noise> {
    let weight = |n: &Noise| lerp(n.top, n.bottom, depth).max(0.0);
    let total: f32 = t.events.iter().map(weight).sum();
    if total <= 0.0 {
        return None;
    }
    let mut at = roll.clamp(0.0, 0.9999) * total;
    for n in &t.events {
        let w = weight(n);
        if at < w {
            return Some(n);
        }
        at -= w;
    }
    t.events.last()
}

/// A cheap seeded roll in 0..1 (the next of a hash chain).
fn roll(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let x = (*state ^ (*state >> 15)).wrapping_mul(0x2c1b_3c6d);
    ((x >> 8) as f32) / ((1u32 << 24) as f32)
}

#[derive(Resource)]
struct Sounds {
    hum: Handle<AudioSample>,
    deep: Handle<AudioSample>,
    takes: HashMap<String, [Handle<AudioSample>; 3]>,
}

impl Sounds {
    fn load(assets: &AssetServer, t: &Tuning) -> Self {
        let at = |name: &str| assets.load(format!("audio/sfx/fulton/{name}.wav"));
        Self {
            hum: at("fulton_hum"),
            deep: at("fulton_deep"),
            takes: t
                .events
                .iter()
                .map(|n| {
                    (
                        n.id.clone(),
                        [1, 2, 3].map(|i| at(&format!("fulton_{}_{i}", n.id))),
                    )
                })
                .collect(),
        }
    }
}

/// A looping room-tone player.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Bed {
    Hum,
    Deep,
}

#[derive(Default)]
struct Pace {
    /// Seconds until the next noise.
    gap: f32,
    seed: u32,
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        ambience
            .run_if(in_state(AppState::InGame))
            .run_if(resource_exists::<Station>),
    );
}

/// Local: the beds follow your depth, and noises come now and then.
fn ambience(
    time: Res<Time>,
    ears: Res<Ears>,
    assets: Option<Res<AssetServer>>,
    sounds: Option<Res<Sounds>>,
    mut commands: Commands,
    mut beds: Query<(Entity, &Bed, &mut SoundVolume)>,
    mut pace: Local<Pace>,
) {
    let dt = time.delta_secs();
    let t = TUNING.get();
    if t.events.is_empty() {
        return;
    }
    let Some(sounds) = sounds else {
        if let Some(assets) = assets {
            commands.insert_resource(Sounds::load(&assets, &t));
        }
        return;
    };
    let feet = ears.pos - Vec3::Y * 1.6;
    let inside = in_station(feet);
    let (want_hum, want_deep, d) = if inside {
        let level = level_at(feet.y);
        let d = depth(level, station_tuning().levels);
        let (h, p) = bed_levels(&t, d, dark_level(&station_tuning(), level));
        (h, p, d)
    } else {
        (0.0, 0.0, 0.0)
    };
    if inside && beds.is_empty() {
        for (bed, handle) in [(Bed::Hum, &sounds.hum), (Bed::Deep, &sounds.deep)] {
            let e = play_loop(&mut commands, handle, 0.0);
            commands
                .entity(e)
                .insert((bed, DespawnOnExit(AppState::InGame)));
        }
    }
    let k = (dt * t.fade).min(1.0);
    for (e, bed, mut v) in &mut beds {
        let want = if *bed == Bed::Hum {
            want_hum
        } else {
            want_deep
        };
        v.0 += (want - v.0) * k;
        if !inside && v.0 < 0.002 {
            commands.entity(e).despawn();
        }
    }
    if !inside {
        pace.gap = 0.0;
        return;
    }
    if pace.seed == 0 {
        pace.seed = 0x9e37_79b9 ^ (time.elapsed_secs() * 1000.0) as u32;
    }
    if pace.gap <= 0.0 {
        // First visit (or just arrived): a short wait before the first one.
        pace.gap = next_gap(&t, d, roll(&mut pace.seed)) * 0.5;
        return;
    }
    pace.gap -= dt;
    if pace.gap > 0.0 {
        return;
    }
    pace.gap = next_gap(&t, d, roll(&mut pace.seed));
    let Some(noise) = pick(&t, d, roll(&mut pace.seed)) else {
        return;
    };
    let Some(takes) = sounds.takes.get(&noise.id) else {
        return;
    };
    let clip = &takes[(roll(&mut pace.seed) * takes.len() as f32) as usize % takes.len()];
    let angle = roll(&mut pace.seed) * std::f32::consts::TAU;
    let dist = lerp(noise.near, noise.far, roll(&mut pace.seed));
    let at = ears.pos
        + Vec3::new(
            angle.cos() * dist,
            (roll(&mut pace.seed) - 0.5) * 2.0,
            angle.sin() * dist,
        );
    let carry = if noise.loud {
        Carry::CALL
    } else {
        Carry::NEARBY
    };
    play_at(&mut commands, clip, at, noise.volume, carry);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("no {path}"));
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_every_field() {
        let t = data();
        assert!(t.hum_volume > 0.0 && t.deep_volume > 0.0 && t.fade > 0.0);
        assert!(t.gap_top > 0.0 && t.gap_bottom > 0.0);
        for id in ["drip", "train", "clank", "groan", "breath"] {
            let n = t.events.iter().find(|n| n.id == id);
            assert!(n.is_some_and(|n| n.near < n.far && n.volume > 0.0), "{id}");
        }
    }

    #[test]
    fn depth_runs_from_the_top_to_the_j() {
        assert_eq!(depth(0, 40), 0.0);
        assert_eq!(depth(39, 40), 1.0);
        assert_eq!(depth(5, 1), 0.0);
        assert!(depth(10, 40) < depth(20, 40));
    }

    #[test]
    fn lower_is_less_lit_and_more_drone() {
        let t = data();
        let (top_hum, top_deep) = bed_levels(&t, 0.0, false);
        let (low_hum, low_deep) = bed_levels(&t, 1.0, false);
        assert!(top_hum > 0.0 && low_hum == 0.0, "the hum fades out");
        assert!(low_deep > top_deep, "the drone comes in");
        let (dark_hum, dark_deep) = bed_levels(&t, 0.3, true);
        assert_eq!(dark_hum, 0.0, "no lights, no hum");
        assert!(dark_deep > bed_levels(&t, 0.3, false).1);
    }

    #[test]
    fn the_noises_turn_creepy_with_depth() {
        let t = data();
        let groans = |d: f32| {
            (0..100)
                .filter(|i| pick(&t, d, *i as f32 / 100.0).is_some_and(|n| n.id == "groan"))
                .count()
        };
        let drips = |d: f32| {
            (0..100)
                .filter(|i| pick(&t, d, *i as f32 / 100.0).is_some_and(|n| n.id == "drip"))
                .count()
        };
        assert_eq!(groans(0.0), 0, "none at the top");
        assert!(groans(1.0) > groans(0.5));
        assert!(drips(0.0) > drips(1.0));
        assert!(next_gap(&t, 1.0, 0.5) > next_gap(&t, 0.0, 0.5));
    }

    #[test]
    fn rolls_stay_in_range() {
        let mut s = 7;
        assert!((0..1000).all(|_| (0.0..1.0).contains(&roll(&mut s))));
    }
}
