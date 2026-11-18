//! Footsteps and landing thuds (polish, not a friend idea): yours, and
//! everyone else's heard from where they walk.
//!
//! Purely local: each peer watches the replicated `Walker` positions, so
//! nothing new goes over the network. The sounds are synthesized once at
//! startup (soft noise crunch over a low thump), a few variants so steps
//! don't sound like a machine gun. Dirt paths sound a little crisper.

use crate::sound::AudioSample;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::duel::Duelist;
use crate::features::fps::{Walker, my_walker};
use crate::features::synth_audio::RATE;
use crate::player::{LocalPlayer, Player};

/// Metres between footfalls while walking (running strides scale with speed).
const STRIDE: f32 = 1.7;
/// Footfalls never come faster than one per this many seconds, however fast
/// you go (bhops, surf landings).
const FASTEST_STEP: f32 = 0.25;
/// Falling faster than this, then touching ground, is a landing.
const LANDING_VY: f32 = -4.0;
/// A landing needs at least this long in the air (s): losing the ground for a
/// frame as the body settles isn't a fall.
const MIN_AIR: f32 = 0.15;
/// Moving faster than this (m/s) between updates is a teleport, not a step.
const TELEPORT_SPEED: f32 = 20.0;

#[derive(Resource)]
struct StepSounds {
    grass: Vec<Handle<AudioSample>>,
    path: Vec<Handle<AudioSample>>,
    land: Handle<AudioSample>,
}

#[derive(Default)]
struct Gait {
    last: Option<Vec3>,
    travelled: f32,
    vy: f32,
    /// The last airborne frames were a real fall (fast down): the next
    /// grounded frame is its landing, once.
    fell: bool,
    /// Seconds since the feet last touched the ground.
    air: f32,
    next: usize,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, make_sounds).add_systems(
        Update,
        (step_sounds, others_steps).run_if(in_state(AppState::InGame)),
    );
}

fn make_sounds(mut commands: Commands, audio: Option<ResMut<Assets<AudioSample>>>) {
    let Some(mut audio) = audio else { return };
    let mut add =
        |samples: Vec<f32>| audio.add(crate::sound::wav_or_silence(&wav_bytes(&samples, RATE)));
    let grass = (0..4).map(|i| add(step(0x1234 + i, 900.0, 0.09))).collect();
    let path = (0..4)
        .map(|i| add(step(0x9876 + i, 2400.0, 0.07)))
        .collect();
    let land = add(step(0x4242, 700.0, 0.16));
    commands.insert_resource(StepSounds { grass, path, land });
}

/// One footfall: low-passed noise (the crunch) over a short sine thump.
pub fn step(seed: u32, cutoff: f32, secs: f32) -> Vec<f32> {
    let n = (RATE as f32 * secs) as usize;
    let mut rng = seed.max(1);
    let a = (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
    let mut lp = 0.0;
    let thump_hz = 70.0 + (seed % 7) as f32 * 6.0;
    (0..n)
        .map(|i| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            lp = lp * a + noise * (1.0 - a);
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.004).min(1.0);
            let env = attack * (-t / (secs * 0.3)).exp() * (1.0 - t / secs);
            let thump = (std::f32::consts::TAU * thump_hz * t).sin() * (-t / 0.025).exp();
            ((lp * 2.5 + thump * 0.5) * env).clamp(-1.0, 1.0)
        })
        .collect()
}

/// What a body's latest move sounds like.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Footfall {
    Land,
    Step { on_path: bool },
}

impl Gait {
    /// Follow a body to `pos`: a footfall every stride while it stands on
    /// the ground (`grounded`, from the walker's controller), a thud when a
    /// real fall ends, nothing in the air or on a teleport.
    fn advance(&mut self, pos: Vec3, grounded: bool, dt: f32) -> Option<Footfall> {
        let last = self.last.replace(pos)?;
        if dt <= 0.0 {
            return None;
        }
        let moved = pos - last;
        // Teleports (respawn, fate) aren't steps: judge by the ground speed,
        // so the fastest real running never reads as one, and by an absurd
        // fall speed (a long real fall stays under it; a respawn drop
        // doesn't, and mustn't count as a landing).
        let ground_speed = moved.xz().length() / dt;
        if ground_speed > TELEPORT_SPEED || moved.y / dt < -100.0 {
            self.travelled = 0.0;
            self.vy = 0.0;
            self.fell = false;
            return None;
        }
        // Replicated positions come in steps, so smooth the vertical speed
        // (only used to spot a landing).
        self.vy += (moved.y / dt - self.vy) * (dt * 20.0).min(1.0);
        // A real fall (fast down while airborne) ends in one thud when the
        // feet touch ground again.
        let landed = grounded && self.fell;
        self.air = if grounded { 0.0 } else { self.air + dt };
        self.fell = !grounded && self.vy < LANDING_VY && self.air >= MIN_AIR;
        if landed {
            self.travelled = 0.0; // the thud starts the stride count over
            // On the ground now: the smoothed speed mustn't still say falling.
            self.vy = 0.0;
        }
        if grounded {
            // Strides bank on the ground only: air time (a bhop's apex, a
            // fall) banks nothing, so landing doesn't fire a burst.
            self.travelled += moved.xz().length();
            let stride = STRIDE.max(moved.xz().length() / dt * FASTEST_STEP);
            if self.travelled >= stride && !landed {
                self.travelled = 0.0;
                return Some(Footfall::Step {
                    on_path: crate::terrain::is_path(pos.x, pos.z),
                });
            }
        } else {
            self.travelled = 0.0;
        }
        landed.then_some(Footfall::Land)
    }

    /// Which sound a footfall makes (stepping through the variants).
    fn sound<'a>(
        &mut self,
        sounds: &'a StepSounds,
        fall: Footfall,
    ) -> Option<&'a Handle<AudioSample>> {
        match fall {
            Footfall::Land => Some(&sounds.land),
            Footfall::Step { on_path } => {
                let set = if on_path { &sounds.path } else { &sounds.grass };
                self.next = (self.next + 1) % set.len().max(1);
                set.get(self.next)
            }
        }
    }
}

fn step_sounds(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Option<Res<StepSounds>>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
    mut gait: Local<Gait>,
) {
    let Some(sounds) = sounds else { return };
    let Some(w) = my_walker(me.as_deref(), &walkers, &duelists) else {
        gait.last = None;
        return;
    };
    let Some(fall) = gait.advance(w.pos, w.grounded, time.delta_secs()) else {
        return;
    };
    // Surfing a ramp slides along the ground for metres on end: that's not
    // walking, and the strides it would bank don't count.
    if matches!(fall, Footfall::Step { .. }) && crate::rules::surf::unbounded(w.pos) {
        return;
    }
    let volume = if fall == Footfall::Land { 0.5 } else { 0.22 };
    if let Some(handle) = gait.sound(&sounds, fall) {
        let e = crate::sound::play_sfx(&mut commands, handle, volume);
        commands.entity(e).insert(DespawnOnExit(AppState::InGame));
    }
}

/// Everyone else's footsteps, heard from their feet (see `crate::spatial`).
fn others_steps(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Option<Res<StepSounds>>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
    mut gaits: Local<HashMap<u64, Gait>>,
) {
    let Some(sounds) = sounds else { return };
    let mine = me.map(|m| m.0);
    let dt = time.delta_secs();
    gaits.retain(|id, _| walkers.iter().any(|(p, _)| p.id == *id));
    for (player, w) in &walkers {
        if Some(player.id) == mine || duelists.iter().any(|d| d.player == player.id) {
            gaits.remove(&player.id);
            continue;
        }
        let gait = gaits.entry(player.id).or_default();
        let Some(fall) = gait.advance(w.pos, w.grounded, dt) else {
            continue;
        };
        // Surfing isn't stepping (see `step_sounds`).
        if matches!(fall, Footfall::Step { .. }) && crate::rules::surf::unbounded(w.pos) {
            continue;
        }
        let volume = if fall == Footfall::Land { 0.6 } else { 0.35 };
        if let Some(handle) = gait.sound(&sounds, fall) {
            let feet = w.pos + Vec3::Y * 0.1;
            crate::spatial::play_at(
                &mut commands,
                handle,
                feet,
                volume,
                crate::spatial::Carry::STEP,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_footstep_is_short_quiet_and_fades_out() {
        let s = super::step(7, 900.0, 0.09);
        assert!(!s.is_empty());
        assert!(s.iter().all(|v| v.abs() <= 1.0));
        let tail = s[s.len() - 50..]
            .iter()
            .map(|v| v.abs())
            .fold(0.0, f32::max);
        assert!(tail < 0.05, "tail {tail}");
    }

    use super::{Footfall, Gait};
    use bevy::prelude::Vec3;

    const DT: f32 = 1.0 / 60.0;

    /// Walk `speed` m/s for `frames` grounded frames, returning the falls.
    fn walk(g: &mut Gait, speed: f32, frames: usize, grounded: bool) -> Vec<Footfall> {
        let mut out = Vec::new();
        let mut pos = g.last.unwrap_or(Vec3::ZERO);
        for _ in 0..frames {
            pos += Vec3::NEG_Z * (speed * DT);
            if let Some(f) = g.advance(pos, grounded, DT) {
                out.push(f);
            }
        }
        out
    }

    /// Drop `per_frame` metres for `frames` airborne frames.
    fn fall(g: &mut Gait, per_frame: f32, frames: usize) -> Vec<Footfall> {
        let mut out = Vec::new();
        let mut pos = g.last.unwrap_or(Vec3::ZERO);
        for _ in 0..frames {
            pos += Vec3::new(0.0, -per_frame, -0.1);
            if let Some(f) = g.advance(pos, false, DT) {
                out.push(f);
            }
        }
        out
    }

    /// Then touch ground and walk on for `frames` more.
    fn land_and_walk(g: &mut Gait, frames: usize) -> Vec<Footfall> {
        let mut out = Vec::new();
        let mut pos = g.last.unwrap_or(Vec3::ZERO);
        for _ in 0..frames {
            pos += Vec3::NEG_Z * 0.05;
            if let Some(f) = g.advance(pos, true, DT) {
                out.push(f);
            }
        }
        out
    }

    #[test]
    fn walking_gives_a_step_every_stride() {
        let mut g = Gait::default();
        let falls = walk(&mut g, 3.0, 120, true);
        let steps = falls
            .iter()
            .filter(|f| matches!(f, Footfall::Step { .. }))
            .count();
        // 3 m/s for 2 s covers 6 m = ~3.5 strides.
        assert!((3..=4).contains(&steps), "{falls:?}");
    }

    #[test]
    fn air_time_banks_no_strides_and_landing_is_one_thud() {
        let mut g = Gait::default();
        walk(&mut g, 3.0, 10, true);
        // A long, fast fall is silent...
        let falls = fall(&mut g, 0.35, 40);
        assert!(falls.is_empty(), "falling is silent: {falls:?}");
        // ...and touching down is exactly one thud, no step burst.
        let falls = land_and_walk(&mut g, 30);
        assert_eq!(falls, vec![Footfall::Land], "{falls:?}");
    }

    #[test]
    fn a_bumpy_landing_after_a_long_fall_is_one_thud() {
        let mut g = Gait::default();
        walk(&mut g, 3.0, 10, true);
        // The Vessel: 40-odd metres down at up to 30 m/s.
        fall(&mut g, 0.5, 90);
        // The feet touch, lose the ground for a frame now and then while the
        // body settles, and walk on.
        let mut out = Vec::new();
        let mut pos = g.last.unwrap_or(Vec3::ZERO);
        for i in 0..60 {
            pos += Vec3::NEG_Z * 0.05;
            if let Some(f) = g.advance(pos, i % 3 != 1, DT) {
                out.push(f);
            }
        }
        let thuds = out.iter().filter(|f| **f == Footfall::Land).count();
        assert_eq!(thuds, 1, "{out:?}");
    }

    #[test]
    fn running_fast_does_not_machine_gun() {
        let mut g = Gait::default();
        // 20 m/s (a bhop landing, faster than any run): a step at most four
        // times a second, and not one per frame.
        let falls = walk(&mut g, 20.0, 240, true);
        let steps = falls
            .iter()
            .filter(|f| matches!(f, Footfall::Step { .. }))
            .count();
        assert!(steps <= 20, "4 s at 20 m/s: {steps} steps");
        assert!(steps >= 3, "still audible: {steps}");
    }

    #[test]
    fn a_jump_is_a_teleport_away_from_a_step() {
        let mut g = Gait::default();
        walk(&mut g, 3.0, 10, true);
        // Fate-style: a huge hop in one update (falling stays under the
        // absurd-vertical guard, so it mustn't read as a teleport).
        let mut pos = g.last.unwrap_or(Vec3::ZERO);
        pos += Vec3::new(50.0, 0.0, 0.0);
        assert_eq!(g.advance(pos, true, DT), None);
        // And no strides were banked by it.
        assert_eq!(g.travelled, 0.0);
        assert!(!g.fell);
    }
}
