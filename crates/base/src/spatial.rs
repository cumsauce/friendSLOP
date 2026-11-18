//! 3D sound (polish, not a friend idea): things in the world are heard where
//! they are.
//!
//! Bevy's built-in spatial audio only does inverse-square volume and a mild
//! pan, so sounds placed in the world use our own little mixer instead. Every
//! world sound is a [`SpatialSource`]: a mono clip played through
//!
//! - distance: full level within `near` metres, then 1/distance (6 dB per
//!   doubling, the way sound really spreads), fading to silence at `far`;
//! - direction: an equal-power level difference between the ears, the tiny
//!   arrival-time difference (up to ~0.66 ms) and head shadow (the far ear
//!   hears it duller), so left/right is clear on headphones and speakers;
//! - front/back: sounds behind you are a touch darker and quieter;
//! - HRTF (headphones, the loudest few sounds only, [`TUNING`]): a short
//!   head-related filter per ear (`friendslop-hrtf`, baked from the SADIE II
//!   KU100 head) takes over the level difference and head shadow and adds
//!   what the outer ears do, so a sound above, below or behind you sounds it.
//!   Everything else, and everything if the file is missing, keeps the cheap
//!   model above;
//! - air: far sounds lose their top end;
//! - occlusion: a hill, wall or building between you and the sound muffles it
//!   (a ray against the physics colliders);
//! - space: a send into the shared room echo ([`crate::reverb`]) at a fixed
//!   level, so the dry sound fading with distance against it is what makes
//!   far things sound far. (Offline renders keep a small tail of their own.)
//!
//! The game writes each sound's target levels and filters into atomics every
//! frame ([`Link`]); the audio thread glides to them, so moving sounds never
//! click. The listener ("ears") is your head when you have a body, facing the
//! way the camera looks. Voice chat listens from the same ears.

use friendslop_hrtf::{Fir, Hrir, HrirSet, TAPS};
use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool};
use bevy::transform::TransformSystems;
use bevy_seedling::prelude::{AudioSample, Connect, RegisterNode, SoundEffectsBus};
use firewheel::channel_config::{ChannelConfig, ChannelCount};
use firewheel::node::{
    AudioNode, AudioNodeInfo, AudioNodeProcessor, ConstructProcessorContext, EmptyConfig,
    NodeError, ProcBuffers, ProcExtra, ProcInfo, ProcessStatus,
};

use crate::AppState;
use crate::player::{LocalPlayer, Player};
use crate::rules::Duelist;
use crate::rules::EYE;
use crate::voice_chat::VoiceListener;
use crate::world::MainCamera;
use friendslop_data::Data;

/// How far a sound carries: full level within `near` metres, 1/distance
/// beyond, fading out to silence at `far`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carry {
    pub near: f32,
    pub far: f32,
}

impl Carry {
    /// Footsteps: a few metres.
    pub const STEP: Carry = Carry {
        near: 1.5,
        far: 30.0,
    };
    /// A crackling fire, a music box.
    pub const NEARBY: Carry = Carry {
        near: 4.0,
        far: 50.0,
    };
    /// Animal calls, an "aww".
    pub const CALL: Carry = Carry {
        near: 5.0,
        far: 90.0,
    };
    /// Farts. The whole commune should know, but only roughly where.
    pub const LOUD: Carry = Carry {
        near: 6.0,
        far: 160.0,
    };
}

/// Where we hear from: a position and the way we face (Bevy axes: -Z is
/// forward, +X right).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Ears {
    pub pos: Vec3,
    pub rot: Quat,
}

impl Default for Ears {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            rot: Quat::IDENTITY,
        }
    }
}

/// The space the listener is in, set by whichever feature owns it (Fulton
/// St station's tiled halls): how much more of every world sound goes to the
/// diffuse tail and how long that tail rings.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Room {
    /// Multiplies every sound's send to the tail (1 = the open air).
    pub wet: f32,
    /// Seconds the tail takes to die away by 60 dB (for sounds starting now).
    pub rt60: f32,
}

impl Room {
    /// Outdoors: a short, dark tail.
    pub const OPEN: Room = Room {
        wet: 1.0,
        rt60: 1.1,
    };

    /// A shape heard in this room.
    pub fn apply(&self, mut s: Shape) -> Shape {
        s.wet *= self.wet;
        s
    }
}

impl Default for Room {
    fn default() -> Self {
        Self::OPEN
    }
}

/// Tuning for the head-related sound (`data/base/spatial.ron`).
#[derive(serde::Deserialize, Default)]
pub struct Tuning {
    /// Head-related filtering at all (the player's own setting also applies).
    pub enabled: bool,
    /// At most this many sounds are filtered at once: the loudest ones.
    pub max_hrtf: usize,
    /// Seconds a sound takes to fade into (or out of) the filter.
    pub fade_secs: f32,
    /// The shared room echo every world sound sends into.
    pub reverb: crate::reverb::Reverb,
}

pub static TUNING: Data<Tuning> = Data::ron("data/base/spatial.ron");

/// The baked head-related responses (`scripts/hrtf-bake.py`). `None` while
/// the file is missing or isn't one (a Git LFS pointer): sounds then use the
/// cheap model.
pub static HRIR: Data<Option<HrirSet>> =
    Data::new("audio/hrtf/sadie_ku100.hrir", |b| Ok(HrirSet::parse(b)));

/// The responses for each sample rate a clip comes in.
#[derive(Resource, Default)]
struct Hrtf {
    from: Option<Arc<Option<HrirSet>>>,
    by_rate: HashMap<u32, Arc<HrirSet>>,
}

impl Hrtf {
    /// Follow the data file (it can change while we run).
    fn refresh(&mut self) {
        let now = HRIR.get();
        if self.from.as_ref().is_none_or(|f| !Arc::ptr_eq(f, &now)) {
            self.by_rate.clear();
            self.from = Some(now);
        }
    }

    fn available(&self) -> bool {
        self.from.as_ref().is_some_and(|f| f.is_some())
    }

    fn for_rate(&mut self, rate: u32) -> Option<Arc<HrirSet>> {
        let from = self.from.clone()?;
        let set = from.as_ref().as_ref()?;
        Some(
            self.by_rate
                .entry(rate)
                .or_insert_with(|| Arc::new(set.resampled(rate)))
                .clone(),
        )
    }
}

/// Which sounds get the head-related filter: the `max` loudest audible
/// ones, if it's on at all. `levels` are what each sound would be heard at.
pub fn budget(levels: &[f32], max: usize, on: bool) -> Vec<bool> {
    let mut want = vec![false; levels.len()];
    if !on || max == 0 {
        return want;
    }
    let mut ranked: Vec<usize> = (0..levels.len()).filter(|&i| levels[i] > 1e-3).collect();
    ranked.sort_by(|&a, &b| levels[b].total_cmp(&levels[a]));
    for &i in ranked.iter().take(max) {
        want[i] = true;
    }
    want
}

/// The entity voice chat listens from: kept on [`Ears`] every frame.
#[derive(Component)]
struct EarsEntity;

/// Head radius and the speed of sound, for the arrival-time difference.
const HEAD_RADIUS: f32 = 0.0875;
const SPEED_OF_SOUND: f32 = 343.0;
/// Send to the diffuse tail (constant with distance; the dry sound isn't).
const REVERB: f32 = 0.09;

/// What one sound should do right now, per ear (left, right).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Shape {
    pub gain: [f32; 2],
    /// Arrival delay in seconds.
    pub delay: [f32; 2],
    /// Low-pass cutoff in Hz.
    pub cutoff: [f32; 2],
    /// Send to the diffuse tail.
    pub wet: f32,
    /// How much of the head-related filter this sound has, 0..=1.
    pub hrtf: f32,
    /// Which way it is from the head (a unit vector, -Z ahead, +X right).
    pub dir: [f32; 3],
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How a sound at `at` reaches `ears`. `occluded` 0..=1: how blocked the
/// straight line is.
pub fn shape(ears: &Ears, at: Vec3, carry: Carry, occluded: f32) -> Shape {
    shape_hrtf(ears, at, carry, occluded, 0.0)
}

/// [`shape`] for a sound that is `hrtf` (0..=1) of the way into the
/// head-related filter, which carries the level difference and the head
/// shadow itself, so those fade out of the cheap model as it fades in.
pub fn shape_hrtf(ears: &Ears, at: Vec3, carry: Carry, occluded: f32, hrtf: f32) -> Shape {
    let h = hrtf.clamp(0.0, 1.0);
    let near = carry.near.max(0.1);
    let far = carry.far.max(near + 1.0);
    let v = at - ears.pos;
    let d = v.length();
    if !d.is_finite() {
        return Shape::default();
    }
    let fade = 1.0 - smoothstep(far * 0.6, far, d);
    let spread = if d <= near { 1.0 } else { near / d };
    let local = if d > 1e-3 {
        ears.rot.inverse() * (v / d)
    } else {
        Vec3::NEG_Z
    };
    // Within a metre a sound is all around you: narrow the image.
    let p = (local.x * d.min(1.0)).clamp(-1.0, 1.0);
    let behind = local.z.max(0.0);
    let occ = occluded.clamp(0.0, 1.0);
    let level = spread * fade * (1.0 - 0.12 * behind) * (1.0 - 0.5 * occ);
    // Equal power: the near ear up to +3 dB, the far one down.
    let ild = 0.6 * (1.0 - h);
    let gain = [
        (1.0 - ild * p).max(0.0).sqrt() * level,
        (1.0 + ild * p).max(0.0).sqrt() * level,
    ];
    // Woodworth's spherical-head arrival difference, on the far ear.
    let side = p.abs();
    let itd = HEAD_RADIUS / SPEED_OF_SOUND * (side.asin() + side);
    let delay = if p > 0.0 { [itd, 0.0] } else { [0.0, itd] };
    // Air eats the highs with distance; behind you and through walls, more.
    let air = 20_000.0 / (1.0 + d / 35.0);
    let base = air * (1.0 - 0.45 * (1.0 - h) * behind) * (1.0 - 0.85 * occ);
    let shadow = 1.0 - 0.75 * (1.0 - h) * side.powf(1.3);
    let cutoff = if p > 0.0 {
        [base * shadow, base]
    } else {
        [base, base * shadow]
    };
    Shape {
        gain,
        delay,
        cutoff,
        wet: REVERB * fade * (1.0 + 0.6 * occ),
        hrtf: h,
        dir: local.to_array(),
    }
}

/// One sound's live parameters, shared with the audio thread.
#[derive(Default, Debug)]
pub struct Link {
    v: [AtomicU32; 11],
    /// Held where it is (its site is asleep): silent, not advancing.
    paused: AtomicBool,
    /// Set by the audio thread once a one-shot has rung out.
    done: AtomicBool,
}

impl Link {
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    pub fn store(&self, s: &Shape) {
        let v = [
            s.gain[0],
            s.gain[1],
            s.delay[0],
            s.delay[1],
            s.cutoff[0],
            s.cutoff[1],
            s.wet,
            s.hrtf,
            s.dir[0],
            s.dir[1],
            s.dir[2],
        ];
        for (slot, x) in self.v.iter().zip(v) {
            let x = if x.is_finite() { x } else { 0.0 };
            slot.store(x.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn load(&self) -> Shape {
        let v = |i: usize| f32::from_bits(self.v[i].load(Ordering::Relaxed));
        Shape {
            gain: [v(0), v(1)],
            delay: [v(2), v(3)],
            cutoff: [v(4), v(5)],
            wet: v(6),
            hrtf: v(7),
            dir: [v(8), v(9), v(10)],
        }
    }
}

/// A mono sound, decoded once.
#[derive(Debug, Clone)]
pub struct Clip {
    samples: Vec<f32>,
    rate: u32,
}

impl Clip {
    pub fn new(samples: Vec<f32>, rate: u32) -> Self {
        Self {
            samples,
            rate: rate.max(1000),
        }
    }

    /// A loaded sound folded to mono; `None` if it's empty (e.g. a Git LFS
    /// pointer instead of the real file never loads at all).
    pub fn decode(source: &AudioSample) -> Option<Self> {
        let (samples, rate) = crate::sound::pcm_mono(source)?;
        Some(Self::new(samples, rate)).filter(|c| !c.samples.is_empty())
    }

    /// A 16-bit PCM WAV (what the synths make) folded to mono.
    pub fn from_wav(bytes: &[u8]) -> Option<Self> {
        let (samples, rate) = crate::sound::parse_wav(bytes)?;
        Some(Self::new(samples, rate)).filter(|c| !c.samples.is_empty())
    }
}

/// A world sound in the audio graph (one node per playback: it carries that
/// playback's [`Link`]). No inputs; four outputs: the dry stereo pair, then
/// the send to the shared room echo ([`crate::reverb::ReverbBus`]) as a pair.
#[derive(Component, Clone)]
pub struct SpatialNode {
    clip: Arc<Clip>,
    link: Arc<Link>,
    looping: bool,
    volume: f32,
    hrir: Option<Arc<HrirSet>>,
}

/// Dry left and right, then the send to the room's echo twice.
const SPATIAL_OUTS: ChannelCount = match ChannelCount::new(4) {
    Some(c) => c,
    None => ChannelCount::STEREO,
};

impl AudioNode for SpatialNode {
    type Configuration = EmptyConfig;

    fn info(&self, _: &EmptyConfig) -> Result<AudioNodeInfo, NodeError> {
        Ok(AudioNodeInfo::new()
            .debug_name("spatial sound")
            .channel_config(ChannelConfig {
                num_inputs: ChannelCount::ZERO,
                num_outputs: SPATIAL_OUTS,
            }))
    }

    fn construct_processor(
        &self,
        _: &EmptyConfig,
        cx: ConstructProcessorContext,
    ) -> Result<impl AudioNodeProcessor, NodeError> {
        let rate = cx.stream_info.sample_rate.get();
        Ok(SpatialProcessor {
            src: SpatialSource::new(self.clip.clone(), self.link.clone(), self.looping)
                .at_rate(rate)
                .with_hrir(self.hrir.clone())
                .sending(),
            volume: self.volume,
            ended: false,
        })
    }
}

struct SpatialProcessor {
    src: SpatialSource,
    volume: f32,
    ended: bool,
}

impl AudioNodeProcessor for SpatialProcessor {
    fn process(
        &mut self,
        info: &ProcInfo,
        ProcBuffers { outputs, .. }: ProcBuffers,
        _: &mut ProcExtra,
    ) -> ProcessStatus {
        if self.ended || self.src.link.is_paused() {
            return ProcessStatus::ClearAllOutputs;
        }
        let [l, r, sl, sr, ..] = outputs else {
            return ProcessStatus::ClearAllOutputs;
        };
        let n = info.frames;
        for i in 0..n {
            let Some(([a, b], send)) = self.src.frame_send() else {
                self.ended = true;
                self.src.link.done.store(true, Ordering::Relaxed);
                for out in [&mut *l, &mut *r, &mut *sl, &mut *sr] {
                    out[i..n].fill(0.0);
                }
                break;
            };
            l[i] = a * self.volume;
            r[i] = b * self.volume;
            sl[i] = send * self.volume;
            sr[i] = sl[i];
        }
        ProcessStatus::OutputsModified
    }
}

/// Frames between parameter reads; gains glide across each block.
const BLOCK: usize = 32;
/// Arrival-delay buffer (enough for 0.66 ms at 96 kHz).
const ITD_LEN: usize = 128;
/// The tail rings on at most this long after the clip ends.
const TAIL_SECS: f32 = 2.0;

/// A small feedback delay network: four lines, a Hadamard mix and damping.
/// Dark and short, like an open meadow with a few walls about.
struct Tail {
    lines: [Vec<f32>; 4],
    at: [usize; 4],
    fb: [f32; 4],
    lp: [f32; 4],
    damp: f32,
}

impl Tail {
    fn new(rate: u32, rt60: f32) -> Self {
        let r = rate as f32;
        let lens = [1031.0f32, 1327.0, 1523.0, 1871.0].map(|l| (l * r / 44_100.0) as usize + 1);
        let rt60 = rt60.clamp(0.2, 8.0);
        Self {
            lines: lens.map(|l| vec![0.0; l]),
            at: [0; 4],
            fb: lens.map(|l| 10f32.powf(-3.0 * l as f32 / (r * rt60))),
            lp: [0.0; 4],
            damp: 1.0 - (-TAU * 3200.0 / r).exp(),
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> (f32, f32) {
        let mut s = [0.0; 4];
        for (i, s) in s.iter_mut().enumerate() {
            let y = self.lines[i][self.at[i]];
            self.lp[i] += self.damp * (y - self.lp[i]);
            *s = self.lp[i];
        }
        let h = [
            s[0] + s[1] + s[2] + s[3],
            s[0] - s[1] + s[2] - s[3],
            s[0] + s[1] - s[2] - s[3],
            s[0] - s[1] - s[2] + s[3],
        ];
        for (((line, at), fb), h) in self.lines.iter_mut().zip(&mut self.at).zip(self.fb).zip(h) {
            line[*at] = x + fb * 0.5 * h;
            *at = (*at + 1) % line.len();
        }
        (0.5 * (s[0] + s[2]), 0.5 * (s[1] + s[3]))
    }
}

/// The audio-thread side of one world sound: stereo frames at the output
/// rate (the clip is read at its own rate, stepped through to match).
pub struct SpatialSource {
    clip: Arc<Clip>,
    link: Arc<Link>,
    looping: bool,
    /// The rate frames come out at (the audio graph's).
    rate: u32,
    /// Read position in the clip, whole samples and the fraction between.
    pos: usize,
    frac: f32,
    /// Clip samples per output frame.
    step: f32,
    gain: [f32; 2],
    gain_step: [f32; 2],
    delay: [f32; 2],
    delay_step: [f32; 2],
    coef: [f32; 2],
    wet: f32,
    ring: [f32; ITD_LEN],
    w: usize,
    lp: [[f32; 2]; 2],
    tail: Tail,
    rt60: f32,
    /// The tail rings on at most this long after the clip ends.
    tail_secs: f32,
    left_in_block: usize,
    after_end: usize,
    peak: f32,
    right: Option<f32>,
    /// The head-related responses at the output rate, if there are any.
    hrir: Option<Arc<HrirSet>>,
    fir: [Fir; 2],
    taps: [[f32; TAPS]; 2],
    tap_step: [[f32; TAPS]; 2],
    /// How much of the filtered signal is heard, gliding like the gains.
    mix: f32,
    mix_step: f32,
    fir_on: bool,
    /// The room's echo is the shared reverb's ([`Self::sending`]), not ours.
    send: bool,
}

impl SpatialSource {
    /// Play `clip` at its own rate (see [`Self::at_rate`] for another).
    pub fn new(clip: Arc<Clip>, link: Arc<Link>, looping: bool) -> Self {
        let rate = clip.rate;
        let mut s = Self {
            tail: Tail::new(rate, Room::OPEN.rt60),
            rt60: Room::OPEN.rt60,
            tail_secs: TAIL_SECS,
            clip,
            link,
            looping,
            rate,
            pos: 0,
            frac: 0.0,
            step: 1.0,
            gain: [0.0; 2],
            gain_step: [0.0; 2],
            delay: [0.0; 2],
            delay_step: [0.0; 2],
            coef: [1.0; 2],
            wet: 0.0,
            ring: [0.0; ITD_LEN],
            w: 0,
            lp: [[0.0; 2]; 2],
            left_in_block: 0,
            after_end: 0,
            peak: 0.0,
            right: None,
            hrir: None,
            fir: [Fir::default(), Fir::default()],
            taps: [[0.0; TAPS]; 2],
            tap_step: [[0.0; TAPS]; 2],
            mix: 0.0,
            mix_step: 0.0,
            fir_on: false,
            send: false,
        };
        s.start_at_link();
        s
    }

    /// Start right where the game says, no glide from silence.
    fn start_at_link(&mut self) {
        let now = self.link.load();
        self.gain = now.gain;
        self.delay = now.delay.map(|d| self.delay_samples(d));
    }

    /// Put frames out at `rate` Hz (the audio graph's), whatever the clip's.
    /// Call before [`Self::with_rt60`] and [`Self::with_hrir`].
    pub fn at_rate(mut self, rate: u32) -> Self {
        let rate = rate.max(1000);
        self.rate = rate;
        self.step = self.clip.rate as f32 / rate as f32;
        self.tail = Tail::new(rate, self.rt60);
        self.start_at_link();
        self
    }

    /// Ring on in a room whose tail takes `rt60` seconds to die away.
    pub fn with_rt60(mut self, rt60: f32) -> Self {
        if (rt60 - Room::OPEN.rt60).abs() > 1e-3 {
            self.rt60 = rt60;
            self.tail = Tail::new(self.rate, rt60);
            self.tail_secs = TAIL_SECS.max(rt60 * 1.2);
        }
        self
    }

    /// Filter with these head-related responses (for the output rate)
    /// whenever the game asks for it.
    pub fn with_hrir(mut self, hrir: Option<Arc<HrirSet>>) -> Self {
        self.hrir = hrir.filter(|h| h.rate() == self.rate);
        if let Some(h) = &self.hrir {
            let now = self.link.load();
            self.mix = now.hrtf;
            self.taps = h.lookup(now.dir).ear;
        }
        self
    }

    /// Leave the room's echo to the shared reverb: [`Self::frame_send`] gives
    /// the dry frame and the send, and the sound ends once its dry part has.
    pub fn sending(mut self) -> Self {
        self.send = true;
        self.tail_secs = 0.1;
        self
    }

    fn delay_samples(&self, secs: f32) -> f32 {
        (secs * self.rate as f32).clamp(0.0, (ITD_LEN - 2) as f32)
    }

    /// Read the game's latest wishes; false once the sound has fully ended.
    fn begin_block(&mut self) -> bool {
        if !self.looping && self.pos >= self.clip.samples.len() {
            self.after_end += BLOCK;
            let max_tail = (self.tail_secs * self.rate as f32) as usize;
            if self.after_end > max_tail || (self.after_end > BLOCK * 4 && self.peak < 1e-5) {
                return false;
            }
        }
        let want = self.link.load();
        let n = BLOCK as f32;
        for ear in 0..2 {
            self.gain_step[ear] = (want.gain[ear] - self.gain[ear]) / n;
            let d = self.delay_samples(want.delay[ear]);
            self.delay_step[ear] = (d - self.delay[ear]) / n;
            let fc = want.cutoff[ear].clamp(20.0, 0.45 * self.rate as f32);
            self.coef[ear] = 1.0 - (-TAU * fc / self.rate as f32).exp();
        }
        self.wet = want.wet;
        self.fir_on = self.hrir.is_some() && (want.hrtf > 1e-3 || self.mix > 1e-3);
        self.mix_step = if self.fir_on {
            (want.hrtf - self.mix) / n
        } else {
            self.mix = 0.0;
            0.0
        };
        if let (true, Some(hrir)) = (self.fir_on, &self.hrir) {
            let to: Hrir = hrir.lookup(want.dir);
            for ear in 0..2 {
                for k in 0..TAPS {
                    self.tap_step[ear][k] = (to.ear[ear][k] - self.taps[ear][k]) / n;
                }
            }
        }
        self.peak = 0.0;
        self.left_in_block = BLOCK;
        true
    }

    /// The next clip sample (linear between the two around the read point).
    #[inline]
    fn next_sample(&mut self) -> f32 {
        let s = &self.clip.samples;
        let len = s.len();
        if self.pos >= len {
            return 0.0;
        }
        let x0 = s[self.pos];
        let x = if self.frac > 0.0 {
            let n = self.pos + 1;
            let x1 = if n < len {
                s[n]
            } else if self.looping {
                s[0]
            } else {
                0.0
            };
            x0 + self.frac * (x1 - x0)
        } else {
            x0
        };
        self.frac += self.step;
        let whole = self.frac.floor();
        self.frac -= whole;
        self.pos += whole as usize;
        if self.looping && self.pos >= len {
            self.pos %= len;
        }
        x
    }

    /// One stereo frame; `None` once the sound (and its tail) has ended.
    pub fn frame(&mut self) -> Option<(f32, f32)> {
        let ([l, r], send) = self.frame_send()?;
        if self.send {
            return Some((l, r));
        }
        let (tl, tr) = self.tail.tick(send);
        let (l, r) = (l + tl, r + tr);
        // Our own tail rings on: the sound lasts while it's audible.
        self.peak = self.peak.max(l.abs()).max(r.abs());
        Some((l, r))
    }

    /// One dry stereo frame and the mono send to the room's echo.
    pub fn frame_send(&mut self) -> Option<([f32; 2], f32)> {
        if self.left_in_block == 0 && !self.begin_block() {
            return None;
        }
        self.left_in_block -= 1;
        let x = self.next_sample();
        self.ring[self.w] = x;
        let mut out = [0.0; 2];
        for (ear, out) in out.iter_mut().enumerate() {
            let d = self.delay[ear];
            let back = d.floor();
            let frac = d - back;
            let i0 = (self.w + ITD_LEN - back as usize) % ITD_LEN;
            let i1 = (i0 + ITD_LEN - 1) % ITD_LEN;
            let mut y = self.ring[i0] + frac * (self.ring[i1] - self.ring[i0]);
            if self.fir_on {
                let f = self.fir[ear].tick(y, &self.taps[ear]);
                y += self.mix * (f - y);
                for (t, d) in self.taps[ear].iter_mut().zip(&self.tap_step[ear]) {
                    *t += d;
                }
            }
            // Two one-poles: a gentle 12 dB/octave roll-off.
            let a = self.coef[ear];
            let lp = &mut self.lp[ear];
            lp[0] += a * (y - lp[0]);
            lp[1] += a * (lp[0] - lp[1]);
            *out = lp[1] * self.gain[ear];
            self.gain[ear] += self.gain_step[ear];
            self.delay[ear] += self.delay_step[ear];
        }
        if self.fir_on {
            self.mix = (self.mix + self.mix_step).clamp(0.0, 1.0);
        }
        let send = x * self.wet;
        self.w = (self.w + 1) % ITD_LEN;
        self.peak = self.peak.max(out[0].abs()).max(out[1].abs());
        Some((out, send))
    }

    /// The rate frames come out at.
    pub fn rate(&self) -> u32 {
        self.rate
    }
}

/// Interleaved stereo samples (left, right, left, ...), for offline renders.
impl Iterator for SpatialSource {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        let (l, r) = self.frame()?;
        self.right = Some(r);
        Some(l)
    }
}

/// Where a world sound's samples come from while it waits to start.
enum Pending {
    Asset(Handle<AudioSample>),
    Synth(Task<Option<Clip>>),
}

/// A world sound waiting for its clip (decoding, or the file loading).
#[derive(Component)]
struct PendingSound {
    from: Pending,
    volume: f32,
    carry: Carry,
    looping: bool,
}

/// A playing world sound: the game keeps its [`Link`] up to date.
#[derive(Component)]
pub struct Emitter {
    link: Arc<Link>,
    carry: Carry,
    /// Smoothed 0..=1.
    occluded: f32,
    /// The last ray's answer (rays are cast every few frames, #151).
    blocked: bool,
    /// Smoothed 0..=1: how far it is into the head-related filter.
    hrtf: f32,
}

fn spawn_pending(
    commands: &mut Commands,
    from: Pending,
    at: Transform,
    volume: f32,
    carry: Carry,
    looping: bool,
) -> Entity {
    commands
        .spawn((
            PendingSound {
                from,
                volume,
                carry,
                looping,
            },
            at,
            DespawnOnExit(AppState::InGame),
        ))
        .id()
}

/// A sound out in the world is in the visuals layer of the area it plays
/// in (gone if you leave before it ends); one outside every area, or a
/// loop (the feature that starts it stops it), is [`crate::areas::Global`].
fn file_sound(commands: &mut Commands, e: Entity, at: Vec3, looping: bool) -> Entity {
    match crate::areas::area_at(at).filter(|_| !looping) {
        Some(area) => commands.entity(e).insert(crate::areas::InArea::show(&area)),
        None => commands.entity(e).insert(crate::areas::Global),
    };
    e
}

/// Play a sound once at `at` in the world. Returns its entity (despawn to stop).
pub fn play_at(
    commands: &mut Commands,
    sound: &Handle<AudioSample>,
    at: Vec3,
    volume: f32,
    carry: Carry,
) -> Entity {
    let from = Pending::Asset(sound.clone());
    let e = spawn_pending(
        commands,
        from,
        Transform::from_translation(at),
        volume,
        carry,
        false,
    );
    file_sound(commands, e, at, false)
}

/// Play a sound once, riding along with `parent` (offset in its space).
pub fn play_on(
    commands: &mut Commands,
    sound: &Handle<AudioSample>,
    parent: Entity,
    offset: Vec3,
    volume: f32,
    carry: Carry,
) -> Entity {
    let e = spawn_pending(
        commands,
        Pending::Asset(sound.clone()),
        Transform::from_translation(offset),
        volume,
        carry,
        false,
    );
    commands.entity(e).insert(ChildOf(parent));
    e
}

/// Loop a sound at `at` until it's despawned (or the game ends).
pub fn loop_at(
    commands: &mut Commands,
    sound: &Handle<AudioSample>,
    at: Vec3,
    volume: f32,
    carry: Carry,
) -> Entity {
    let from = Pending::Asset(sound.clone());
    let e = spawn_pending(
        commands,
        from,
        Transform::from_translation(at),
        volume,
        carry,
        true,
    );
    file_sound(commands, e, at, true)
}

/// Synthesize a WAV off the main thread, then play it once at `at`.
pub fn play_wav_at(
    commands: &mut Commands,
    at: Vec3,
    volume: f32,
    carry: Carry,
    make_wav: impl FnOnce() -> Vec<u8> + Send + 'static,
) -> Entity {
    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
    let task = pool.spawn(async move { Clip::from_wav(&make_wav()) });
    let e = spawn_pending(
        commands,
        Pending::Synth(task),
        Transform::from_translation(at),
        volume,
        carry,
        false,
    );
    file_sound(commands, e, at, false)
}

/// Play already-made mono samples once at `at`; `make` runs off the main thread.
pub fn play_samples_at(
    commands: &mut Commands,
    at: Vec3,
    volume: f32,
    carry: Carry,
    make: impl FnOnce() -> (Vec<f32>, u32) + Send + 'static,
) -> Entity {
    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
    let task = pool.spawn(async move {
        let (samples, rate) = make();
        Some(Clip::new(samples, rate)).filter(|c| !c.samples.is_empty())
    });
    let e = spawn_pending(
        commands,
        Pending::Synth(task),
        Transform::from_translation(at),
        volume,
        carry,
        false,
    );
    file_sound(commands, e, at, false)
}

/// Decoded clips by sound file (holding the handle so its id isn't reused).
#[derive(Resource, Default)]
struct Clips(HashMap<AssetId<AudioSample>, Cached>);

enum Cached {
    Decoding(Handle<AudioSample>, Task<Option<Clip>>),
    Ready {
        _keep: Handle<AudioSample>,
        clip: Arc<Clip>,
    },
    Failed,
}

enum Lookup {
    Ready(Arc<Clip>),
    Wait,
    Failed,
}

impl Clips {
    fn get(
        &mut self,
        handle: &Handle<AudioSample>,
        sources: Option<&Assets<AudioSample>>,
        server: Option<&AssetServer>,
    ) -> Lookup {
        let id = handle.id();
        if let Some(cached) = self.0.get_mut(&id) {
            let next = match cached {
                Cached::Ready { clip, .. } => return Lookup::Ready(clip.clone()),
                Cached::Failed => return Lookup::Failed,
                Cached::Decoding(h, task) => match check_ready(task) {
                    None => return Lookup::Wait,
                    Some(Some(clip)) => Cached::Ready {
                        _keep: h.clone(),
                        clip: Arc::new(clip),
                    },
                    Some(None) => Cached::Failed,
                },
            };
            *cached = next;
            return match cached {
                Cached::Ready { clip, .. } => Lookup::Ready(clip.clone()),
                _ => Lookup::Failed,
            };
        }
        if let Some(source) = sources.and_then(|s| s.get(id)) {
            let source = source.clone();
            let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
            let task = pool.spawn(async move { Clip::decode(&source) });
            self.0.insert(id, Cached::Decoding(handle.clone(), task));
            return Lookup::Wait;
        }
        if server.is_some_and(|s| s.load_state(id).is_failed()) {
            return Lookup::Failed;
        }
        Lookup::Wait
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Ears>()
        .init_resource::<Room>()
        .add_systems(Startup, spawn_ears)
        .add_systems(PostUpdate, follow_ears.after(TransformSystems::Propagate));
    // Headless apps (tests) have no audio: sounds asked for are dropped.
    if !app.is_plugin_added::<bevy_seedling::SeedlingCorePlugin>() {
        app.add_systems(PostUpdate, drop_sounds);
        return;
    }
    app.register_simple_node::<SpatialNode>()
        .init_resource::<Clips>()
        .init_resource::<Hrtf>()
        .add_systems(
            PostUpdate,
            (start_sounds, occlude, steer, sleep_sounds, end_sounds)
                .chain()
                .after(follow_ears),
        );
}

fn spawn_ears(mut commands: Commands) {
    commands.spawn((
        EarsEntity,
        VoiceListener::default(),
        Transform::default(),
        Name::new("Ears"),
        crate::areas::Global,
    ));
}

/// Ears at my head when I have a body (camera's facing), else at the camera.
fn follow_ears(
    me: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &GlobalTransform)>,
    duelists: Query<&Duelist>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    state: Option<Res<State<AppState>>>,
    mut ears: ResMut<Ears>,
    mut entity: Query<
        (&mut Transform, &mut GlobalTransform),
        (With<EarsEntity>, Without<Player>, Without<MainCamera>),
    >,
) {
    let Ok(cam) = camera.single() else { return };
    let (_, rot, cam_pos) = cam.to_scale_rotation_translation();
    let in_game = state.is_some_and(|s| *s.get() == AppState::InGame);
    let head = me
        .filter(|_| in_game)
        .filter(|m| !duelists.iter().any(|d| d.player == m.0))
        .and_then(|m| players.iter().find(|(p, _)| p.id == m.0))
        .map(|(_, t)| t.translation() + Vec3::Y * EYE);
    let want = Ears {
        pos: head.unwrap_or(cam_pos),
        rot,
    };
    if *ears != want {
        *ears = want;
    }
    for (mut t, mut g) in &mut entity {
        let tf = Transform::from_translation(want.pos).with_rotation(want.rot);
        *t = tf;
        *g = GlobalTransform::from(tf);
    }
}

fn drop_sounds(mut commands: Commands, pending: Query<Entity, With<PendingSound>>) {
    for e in &pending {
        commands.entity(e).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn start_sounds(
    mut commands: Commands,
    mut pending: Query<(Entity, &mut PendingSound, &GlobalTransform)>,
    ears: Res<Ears>,
    room: Option<Res<Room>>,
    sources: Option<Res<Assets<AudioSample>>>,
    server: Option<Res<AssetServer>>,
    mut clips: ResMut<Clips>,
    rate: Option<Res<bevy_seedling::context::SampleRate>>,
    mut hrtf: ResMut<Hrtf>,
) {
    let rate = crate::sound::stream_rate(rate.as_deref());
    hrtf.refresh();
    for (entity, mut p, at) in &mut pending {
        let clip = match &mut p.from {
            Pending::Asset(handle) => {
                match clips.get(handle, sources.as_deref(), server.as_deref()) {
                    Lookup::Ready(clip) => clip,
                    Lookup::Wait => continue,
                    Lookup::Failed => {
                        commands.entity(entity).despawn();
                        continue;
                    }
                }
            }
            Pending::Synth(task) => match check_ready(task) {
                None => continue,
                Some(Some(clip)) => Arc::new(clip),
                Some(None) => {
                    commands.entity(entity).despawn();
                    continue;
                }
            },
        };
        let room = room.as_deref().copied().unwrap_or_default();
        let link = Arc::new(Link::default());
        link.store(&room.apply(shape(&ears, at.translation(), p.carry, 0.0)));
        // Filtered at the graph's rate (the node runs there, whatever the clip's).
        let hrir = hrtf.for_rate(rate);
        let node = SpatialNode {
            clip,
            link: link.clone(),
            looping: p.looping,
            volume: p.volume,
            hrir,
        };
        commands
            .entity(entity)
            .remove::<PendingSound>()
            .insert(node);
        commands
            .entity(entity)
            .connect_with(SoundEffectsBus, &[(0, 0), (1, 1)]);
        commands
            .entity(entity)
            .connect_with(crate::reverb::ReverbBus, &[(2, 0), (3, 1)]);
        commands.entity(entity).insert((Emitter {
            link,
            carry: p.carry,
            occluded: 0.0,
            blocked: false,
            hrtf: 0.0,
        },));
    }
}

/// Is something solid between the ears and each sound? Smoothed so walking
/// past a tree trunk doesn't flicker.
fn occlude(
    time: Res<Time>,
    ears: Res<Ears>,
    spatial: avian3d::prelude::SpatialQuery,
    mut emitters: Query<(Entity, &GlobalTransform, &mut Emitter)>,
    mut frame: Local<u32>,
) {
    /// Each sound re-checks its line of sight every this many frames; the
    /// 150 ms smoothing hides the gap.
    const EVERY: u32 = 3;
    *frame = frame.wrapping_add(1);
    let filter = avian3d::prelude::SpatialQueryFilter::default();
    let k = 1.0 - (-time.delta_secs() / 0.15).exp();
    for (entity, at, mut e) in &mut emitters {
        // Aim a little above the source so the ground it sits on doesn't count.
        let to = at.translation() + Vec3::Y * 0.4 - ears.pos;
        let d = to.length();
        if !(entity.index_u32().wrapping_add(*frame)).is_multiple_of(EVERY) {
            let want = if e.blocked { 1.0 } else { 0.0 };
            e.occluded += (want - e.occluded) * k;
            continue;
        }
        let blocked = if d < e.carry.far && d > 3.0 {
            Dir3::new(to).ok().is_some_and(|dir| {
                spatial
                    .cast_ray(ears.pos + dir * 0.5, dir, d - 2.0, true, &filter)
                    .is_some()
            })
        } else {
            false
        };
        e.blocked = blocked;
        let want = if blocked { 1.0 } else { 0.0 };
        e.occluded += (want - e.occluded) * k;
    }
}

fn steer(
    time: Res<Time>,
    ears: Res<Ears>,
    room: Option<Res<Room>>,
    settings: Option<Res<crate::settings::Settings>>,
    mut hrtf: ResMut<Hrtf>,
    mut emitters: Query<(&GlobalTransform, &mut Emitter)>,
    mut levels: Local<Vec<f32>>,
) {
    let room = room.as_deref().copied().unwrap_or_default();
    hrtf.refresh();
    let tuning = TUNING.get();
    let on = tuning.enabled && hrtf.available() && settings.is_none_or(|s| s.hrtf);
    // The loudest few get the filter; the rest keep the cheap model.
    levels.clear();
    for (at, e) in &emitters {
        let s = shape(&ears, at.translation(), e.carry, e.occluded);
        levels.push(s.gain[0] + s.gain[1]);
    }
    let want = budget(&levels, tuning.max_hrtf, on);
    let k = 1.0 - (-time.delta_secs() / tuning.fade_secs.max(0.02)).exp();
    for ((at, mut e), want) in emitters.iter_mut().zip(want) {
        let to = if want { 1.0 } else { 0.0 };
        e.hrtf += (to - e.hrtf) * k;
        if e.hrtf < 1e-3 && !want {
            e.hrtf = 0.0;
        }
        e.link.store(&room.apply(shape_hrtf(
            &ears,
            at.translation(),
            e.carry,
            e.occluded,
            e.hrtf,
        )));
    }
}

/// A sleeping site's sounds hold where they are (#412): paused while you're
/// away, so they cost nothing, and carried on from there when you're back.
fn sleep_sounds(awake: Res<crate::sites::Awake>, sounds: Query<(&crate::sites::AtSite, &Emitter)>) {
    for (site, e) in &sounds {
        let asleep = !awake.me(site.0);
        if e.link.is_paused() != asleep {
            e.link.set_paused(asleep);
        }
    }
}

/// A one-shot that has rung out (the audio thread says so) is despawned,
/// which takes its node out of the graph.
fn end_sounds(mut commands: Commands, sounds: Query<(Entity, &Emitter)>) {
    for (entity, e) in &sounds {
        if e.link.is_done() {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ears() -> Ears {
        Ears::default()
    }

    #[test]
    fn closer_is_louder_and_far_is_silent() {
        let c = Carry::CALL;
        let level = |z: f32| {
            let s = shape(&ears(), Vec3::new(0.0, 0.0, -z), c, 0.0);
            s.gain[0] + s.gain[1]
        };
        assert!((level(2.0) - level(4.0)).abs() < 1e-6, "flat inside near");
        assert!(level(10.0) < level(5.0) * 0.55, "1/d beyond near");
        assert!(level(c.far + 1.0) == 0.0, "silent past far");
        assert!(level(c.far * 0.5) > 0.0);
    }

    #[test]
    fn right_is_heard_on_the_right() {
        let s = shape(&ears(), Vec3::new(5.0, 0.0, 0.0), Carry::CALL, 0.0);
        assert!(s.gain[1] > s.gain[0] * 1.5, "{s:?}");
        assert!(s.delay[0] > 0.0005 && s.delay[1] == 0.0, "left ear later");
        assert!(s.cutoff[0] < s.cutoff[1], "left ear in the head's shadow");
        // Turn around: now it's on the left.
        let back = Ears {
            rot: Quat::from_rotation_y(std::f32::consts::PI),
            ..ears()
        };
        let s = shape(&back, Vec3::new(5.0, 0.0, 0.0), Carry::CALL, 0.0);
        assert!(s.gain[0] > s.gain[1] * 1.5, "{s:?}");
    }

    #[test]
    fn behind_and_through_walls_is_duller() {
        let c = Carry::CALL;
        let front = shape(&ears(), Vec3::new(0.0, 0.0, -10.0), c, 0.0);
        let behind = shape(&ears(), Vec3::new(0.0, 0.0, 10.0), c, 0.0);
        let walled = shape(&ears(), Vec3::new(0.0, 0.0, -10.0), c, 1.0);
        assert!(behind.cutoff[0] < front.cutoff[0] * 0.7);
        assert!(walled.cutoff[0] < front.cutoff[0] * 0.3);
        assert!(walled.gain[0] < front.gain[0] * 0.6);
        assert!(walled.wet > front.wet, "a wall leaves mostly the room");
    }

    fn render(src: &mut SpatialSource, limit: usize) -> Vec<f32> {
        src.take(limit).collect()
    }

    #[test]
    fn source_pans_rings_out_and_ends() {
        let rate = 44_100;
        // A short noise burst.
        let mut x = 1u32;
        let burst: Vec<f32> = (0..rate / 10)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as f32 / u32::MAX as f32 - 0.5
            })
            .collect();
        let clip = Arc::new(Clip::new(burst, rate));
        let link = Arc::new(Link::default());
        link.store(&shape(&ears(), Vec3::new(4.0, 0.0, -1.0), Carry::CALL, 0.0));
        let mut src = SpatialSource::new(clip.clone(), link, false);
        let out = render(&mut src, rate as usize * 10);
        assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1.0));
        assert!(out.len().is_multiple_of(2));
        let frames = out.len() / 2;
        assert!(frames > clip.samples.len(), "a tail after the burst");
        assert!(frames < clip.samples.len() + (TAIL_SECS * rate as f32) as usize + 2 * BLOCK);
        let energy = |ch: usize, from: usize, to: usize| -> f32 {
            out.chunks(2)
                .skip(from)
                .take(to - from)
                .map(|f| f[ch] * f[ch])
                .sum()
        };
        let n = clip.samples.len();
        assert!(energy(1, 0, n) > energy(0, 0, n) * 2.0, "right side louder");
        let dry = energy(0, 0, n) + energy(1, 0, n);
        let tail = energy(0, n, frames) + energy(1, n, frames);
        let ratio_db = 10.0 * (tail / dry).log10();
        assert!((-30.0..-8.0).contains(&ratio_db), "tail {ratio_db} dB");
    }

    #[test]
    fn a_sending_sound_leaves_its_echo_to_the_room() {
        let rate = 44_100;
        let clip = Arc::new(Clip::new(vec![0.3; rate as usize / 20], rate));
        let link = Arc::new(Link::default());
        link.store(&shape(&ears(), Vec3::NEG_Z * 3.0, Carry::CALL, 0.0));
        let mut src = SpatialSource::new(clip.clone(), link, false).sending();
        let mut frames = 0;
        let mut send = 0.0;
        while let Some((dry, s)) = src.frame_send() {
            assert!(dry.iter().all(|x| x.is_finite()) && s.is_finite());
            send += s * s;
            frames += 1;
        }
        assert!(send > 0.0, "it sends to the echo");
        let n = clip.samples.len();
        assert!(frames >= n && frames < n + rate as usize / 10, "{frames}");
    }

    #[test]
    fn a_tiled_hall_rings_longer_and_wetter() {
        let rate = 44_100;
        let clip = Arc::new(Clip::new(vec![0.3; rate as usize / 20], rate));
        let tail = |room: Room| {
            let link = Arc::new(Link::default());
            link.store(&room.apply(shape(&ears(), Vec3::NEG_Z * 3.0, Carry::CALL, 0.0)));
            let mut src = SpatialSource::new(clip.clone(), link, false).with_rt60(room.rt60);
            let out = render(&mut src, rate as usize * 20);
            assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1.5));
            let n = clip.samples.len() * 2;
            let e: f32 = out[n.min(out.len())..].iter().map(|s| s * s).sum();
            (out.len() / 2, e)
        };
        let (open_len, open_e) = tail(Room::OPEN);
        let hall = Room {
            wet: 6.0,
            rt60: 2.8,
        };
        let (hall_len, hall_e) = tail(hall);
        assert!(hall_len > open_len, "{hall_len} vs {open_len}");
        assert!(hall_e > open_e * 10.0, "{hall_e} vs {open_e}");
    }

    fn noise(n: usize) -> Vec<f32> {
        let mut x = 7u32;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as f32 / u32::MAX as f32 - 0.5
            })
            .collect()
    }

    /// Render a looping noise source at `at` with the filter fully in.
    fn heard(at: Vec3, hrir: Option<Arc<HrirSet>>) -> Vec<f32> {
        let rate = 48_000;
        let clip = Arc::new(Clip::new(noise(4800), rate));
        let link = Arc::new(Link::default());
        link.store(&shape_hrtf(&ears(), at, Carry::CALL, 0.0, 1.0));
        let mut src = SpatialSource::new(clip, link, true).with_hrir(hrir);
        render(&mut src, 20_000)
    }

    fn energy(out: &[f32], ch: usize) -> f32 {
        out.chunks(2).map(|f| f[ch] * f[ch]).sum()
    }

    #[test]
    fn the_filter_fades_the_cheap_cues_out() {
        let side = Vec3::new(5.0, 0.0, 0.0);
        let cheap = shape_hrtf(&ears(), side, Carry::CALL, 0.0, 0.0);
        let full = shape_hrtf(&ears(), side, Carry::CALL, 0.0, 1.0);
        assert_eq!(cheap, shape(&ears(), side, Carry::CALL, 0.0));
        assert!(
            (full.gain[0] - full.gain[1]).abs() < 1e-6,
            "level difference is the filter's"
        );
        assert_eq!(full.cutoff[0], full.cutoff[1], "so is the head shadow");
        assert!(full.delay[0] > 0.0005, "the arrival time stays ours");
        assert!(full.dir[0] > 0.9, "{:?}", full.dir);
        let up = shape_hrtf(&ears(), Vec3::new(0.0, 5.0, 0.0), Carry::CALL, 0.0, 1.0);
        assert!(up.dir[1] > 0.99, "overhead is up: {:?}", up.dir);
    }

    #[test]
    fn hrtf_places_a_sound_on_its_side_and_above() {
        let set = Arc::new(friendslop_hrtf::synthetic(48_000));
        let right = heard(Vec3::new(5.0, 0.0, 0.0), Some(set.clone()));
        assert!(right.iter().all(|s| s.is_finite() && s.abs() < 2.0));
        assert!(
            energy(&right, 1) > energy(&right, 0) * 1.5,
            "right ear louder"
        );
        // The cheap model can't tell ahead from overhead; the filter can.
        let ahead = heard(Vec3::new(0.0, 0.0, -5.0), Some(set.clone()));
        let above = heard(Vec3::new(0.0, 5.0, 0.0), Some(set.clone()));
        let differ = |a: &[f32], b: &[f32]| {
            let d: f32 = a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum();
            d / a.iter().map(|x| x * x).sum::<f32>()
        };
        assert!(differ(&ahead, &above) > 0.1, "up and ahead sound different");
        let cheap_ahead = heard(Vec3::new(0.0, 0.0, -5.0), None);
        let cheap_above = heard(Vec3::new(0.0, 5.0, 0.0), None);
        assert!(
            differ(&cheap_ahead, &cheap_above) < 1e-6,
            "the cheap model can't tell"
        );
    }

    #[test]
    fn a_wrong_rate_or_missing_set_is_just_the_cheap_model() {
        let at = Vec3::new(5.0, 0.0, -2.0);
        let none = heard(at, None);
        let odd = heard(at, Some(Arc::new(friendslop_hrtf::synthetic(44_100))));
        assert_eq!(none, odd, "a set for another rate isn't used");
    }

    #[test]
    fn moving_through_the_filter_stays_smooth() {
        let set = Arc::new(friendslop_hrtf::synthetic(48_000));
        let clip = Arc::new(Clip::new(noise(4800), 48_000));
        let link = Arc::new(Link::default());
        link.store(&shape_hrtf(
            &ears(),
            Vec3::NEG_Z * 4.0,
            Carry::CALL,
            0.0,
            0.0,
        ));
        let mut src = SpatialSource::new(clip, link.clone(), true).with_hrir(Some(set));
        let mut last = 0.0f32;
        let mut worst = 0.0f32;
        for step in 0..400 {
            // Circle round the head while fading into the filter and out.
            let a = step as f32 * 0.05;
            let h = (a * 0.5).sin().abs();
            link.store(&shape_hrtf(
                &ears(),
                Vec3::new(a.sin() * 4.0, (a * 0.7).sin() * 3.0, -a.cos() * 4.0),
                Carry::CALL,
                0.0,
                h,
            ));
            for _ in 0..64 {
                let (Some(l), Some(_)) = (src.next(), src.next()) else {
                    return;
                };
                assert!(l.is_finite() && l.abs() < 3.0);
                worst = worst.max((l - last).abs());
                last = l;
            }
        }
        // Noise at this level steps up to ~0.5 between samples by itself.
        assert!(worst < 1.5, "a click: {worst}");
    }

    #[test]
    fn only_the_loudest_get_the_filter() {
        let levels = [0.2, 0.0, 0.9, 0.5, 0.0005, 0.7];
        assert_eq!(
            budget(&levels, 2, true),
            [false, false, true, false, false, true]
        );
        assert_eq!(
            budget(&levels, 10, true),
            [true, false, true, true, false, true]
        );
        assert_eq!(budget(&levels, 2, false), [false; 6], "off means off");
        assert_eq!(budget(&levels, 0, true), [false; 6]);
        assert!(budget(&[], 3, true).is_empty());
    }

    #[test]
    fn spatial_data_parses() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/base/spatial.ron"
        );
        let bytes = std::fs::read(path).unwrap_or_default();
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let t = TUNING.get();
        assert!(t.enabled && t.max_hrtf > 0 && t.fade_secs > 0.0);
    }

    #[test]
    fn the_shipped_head_loads_for_every_clip_rate() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/audio/hrtf/sadie_ku100.hrir"
        );
        let bytes = std::fs::read(path).unwrap_or_default();
        if !bytes.starts_with(b"HRIR") {
            return; // a Git LFS pointer
        }
        assert!(HRIR.set_bytes(&bytes));
        let mut h = Hrtf::default();
        h.refresh();
        assert!(h.available());
        for rate in [16_000, 22_050, 44_100, 48_000] {
            assert_eq!(h.for_rate(rate).map(|s| s.rate()), Some(rate));
        }
    }

    /// A clip at one rate played into a graph at another lasts as long and
    /// keeps its level (the node steps through it).
    #[test]
    fn a_clip_plays_at_the_graphs_rate() {
        let clip = Arc::new(Clip::new(vec![0.25; 22_050], 22_050));
        let link = Arc::new(Link::default());
        link.store(&Shape {
            gain: [1.0, 1.0],
            cutoff: [20_000.0; 2],
            ..default()
        });
        let mut src = SpatialSource::new(clip, link, false).at_rate(48_000);
        assert_eq!(src.rate(), 48_000);
        let out = render(&mut src, 400_000);
        let frames = out.len() / 2;
        // One second of clip, then up to the tail's two seconds of silence.
        assert!((48_000..48_000 * 3 + 2_000).contains(&frames), "{frames}");
        let mid = out[2 * 24_000];
        assert!((mid - 0.25).abs() < 0.01, "{mid}");
    }

    #[test]
    fn loops_never_end() {
        let clip = Arc::new(Clip::new(vec![0.1; 100], 22_050));
        let link = Arc::new(Link::default());
        link.store(&shape(&ears(), Vec3::NEG_Z, Carry::NEARBY, 0.0));
        let mut src = SpatialSource::new(clip, link, true);
        assert_eq!(render(&mut src, 50_000).len(), 50_000);
    }

    /// A WAV made the way the game's synths make them decodes back.
    #[test]
    fn synth_wavs_decode() {
        let samples: Vec<f32> = (0..2205).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let wav = crate::features::bear::wav_bytes(&samples, 22_050);
        let clip = Clip::from_wav(&wav);
        let Some(clip) = clip else {
            panic!("didn't decode");
        };
        assert_eq!(clip.rate, 22_050);
        assert!((clip.samples.len() as i32 - 2205).abs() < 4);
        assert!((clip.samples[100] - samples[100]).abs() < 0.01);
    }
}

/// `SPATIAL_DEMO=out.wav cargo test -p friendslop-base --lib spatial_demo -- --ignored`
/// renders a crow cawing as it circles your head, then swings over the top
/// of it (front, overhead, behind) and flies off behind a wall (for checking
/// on headphones). `SPATIAL_DEMO_CHEAP=1` renders the same without the
/// head-related filter, to hear what it adds.
#[cfg(test)]
#[test]
#[ignore]
fn spatial_demo() {
    let Ok(out) = std::env::var("SPATIAL_DEMO") else {
        return;
    };
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../game/assets/audio/sfx/crow_caw.wav"
    ))
    .unwrap_or_default();
    let caw = Clip::from_wav(&bytes).unwrap_or_else(|| Clip::new(vec![0.0; 100], 44_100));
    let rate = caw.rate;
    // Caws with gaps between them, looped.
    let mut samples = caw.samples.clone();
    samples.resize(samples.len() + rate as usize / 3, 0.0);
    let clip = Arc::new(Clip::new(samples, rate));
    let link = Arc::new(Link::default());
    let ears = Ears::default();
    let hrtf = if std::env::var_os("SPATIAL_DEMO_CHEAP").is_some() {
        0.0
    } else {
        1.0
    };
    let head = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../game/assets/audio/hrtf/sadie_ku100.hrir"
    ))
    .ok()
    .and_then(|b| HrirSet::parse(&b))
    .map(|s| Arc::new(s.resampled(rate)));
    let secs = 24.0;
    let at = |t: f32| -> (Vec3, f32) {
        if t < 8.0 {
            // Circle at 5 m: front, right, behind, left, front...
            let a = t / 8.0 * TAU;
            (Vec3::new(a.sin() * 5.0, 0.0, -a.cos() * 5.0), 0.0)
        } else if t < 16.0 {
            // Over the top, front to back, in the middle of the head.
            let a = (t - 8.0) / 8.0 * TAU / 2.0;
            (Vec3::new(0.0, a.sin() * 5.0, -a.cos() * 5.0), 0.0)
        } else {
            // Fly straight away ahead, a wall coming between at 20 s.
            let d = 5.0 + (t - 16.0) * 9.0;
            (Vec3::new(0.0, 0.0, -d), ((t - 20.0) / 0.3).clamp(0.0, 1.0))
        }
    };
    let (p, o) = at(0.0);
    link.store(&shape_hrtf(&ears, p, Carry::CALL, o, hrtf));
    let mut src = SpatialSource::new(clip, link.clone(), true).with_hrir(head);
    let frames = (secs * rate as f32) as usize;
    let mut pcm = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        if f % 256 == 0 {
            let (p, o) = at(f as f32 / rate as f32);
            link.store(&shape_hrtf(&ears, p, Carry::CALL, o, hrtf));
        }
        let Some(l) = src.next() else { break };
        let Some(r) = src.next() else { break };
        pcm.push(l);
        pcm.push(r);
    }
    let mut wav = Vec::new();
    let data = (pcm.len() * 2) as u32;
    for chunk in [b"RIFF".as_slice(), &(36 + data).to_le_bytes(), b"WAVEfmt "] {
        wav.extend_from_slice(chunk);
    }
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        wav.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    let _ = std::fs::write(out, wav);
}
