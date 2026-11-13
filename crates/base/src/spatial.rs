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
//! - air: far sounds lose their top end;
//! - occlusion: a hill, wall or building between you and the sound muffles it
//!   (a ray against the physics colliders);
//! - space: a small diffuse tail at a fixed level, so the dry sound fading
//!   with distance against it is what makes far things sound far.
//!
//! The game writes each sound's target levels and filters into atomics every
//! frame ([`Link`]); the audio thread glides to them, so moving sounds never
//! click. The listener ("ears") is your head when you have a body, facing the
//! way the camera looks. Voice chat listens from the same ears.

use std::f32::consts::TAU;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::audio::{AddAudioSource, AudioPlugin, AudioSource, Decodable, Sample, Source, Volume};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool};
use bevy::transform::TransformSystems;

use crate::AppState;
use crate::player::{LocalPlayer, Player};
use crate::rules::Duelist;
use crate::rules::EYE;
use crate::voice_chat::VoiceListener;
use crate::world::MainCamera;

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
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How a sound at `at` reaches `ears`. `occluded` 0..=1: how blocked the
/// straight line is.
pub fn shape(ears: &Ears, at: Vec3, carry: Carry, occluded: f32) -> Shape {
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
    let ild = 0.6;
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
    let base = air * (1.0 - 0.45 * behind) * (1.0 - 0.85 * occ);
    let shadow = 1.0 - 0.75 * side.powf(1.3);
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
    }
}

/// One sound's live parameters, shared with the audio thread.
#[derive(Default, Debug)]
pub struct Link([AtomicU32; 7]);

impl Link {
    pub fn store(&self, s: &Shape) {
        let v = [
            s.gain[0],
            s.gain[1],
            s.delay[0],
            s.delay[1],
            s.cutoff[0],
            s.cutoff[1],
            s.wet,
        ];
        for (slot, x) in self.0.iter().zip(v) {
            let x = if x.is_finite() { x } else { 0.0 };
            slot.store(x.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn load(&self) -> Shape {
        let v = |i: usize| f32::from_bits(self.0[i].load(Ordering::Relaxed));
        Shape {
            gain: [v(0), v(1)],
            delay: [v(2), v(3)],
            cutoff: [v(4), v(5)],
            wet: v(6),
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

    /// Decode a WAV/OGG and fold it to mono; `None` if it isn't audio (e.g.
    /// a Git LFS pointer instead of the real file).
    pub fn decode(source: &AudioSource) -> Option<Self> {
        let source = source.clone();
        std::panic::catch_unwind(move || {
            let dec = source.decoder();
            let ch = usize::from(dec.channels().get()).max(1);
            let rate = dec.sample_rate().get();
            let raw: Vec<f32> = dec.collect();
            let samples = raw
                .chunks(ch)
                .map(|f| f.iter().sum::<f32>() / ch as f32)
                .collect();
            Self::new(samples, rate)
        })
        .ok()
        .filter(|c| !c.samples.is_empty())
    }
}

/// A world sound as a Bevy audio asset (one per playback: it carries that
/// playback's [`Link`]).
#[derive(Asset, TypePath)]
pub struct SpatialSound {
    clip: Arc<Clip>,
    link: Arc<Link>,
    looping: bool,
    rt60: f32,
}

impl Decodable for SpatialSound {
    type Decoder = SpatialSource;

    fn decoder(&self) -> SpatialSource {
        SpatialSource::new(self.clip.clone(), self.link.clone(), self.looping).with_rt60(self.rt60)
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

/// The audio-thread side of one world sound: interleaved stereo out.
pub struct SpatialSource {
    clip: Arc<Clip>,
    link: Arc<Link>,
    looping: bool,
    pos: usize,
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
    /// The tail rings on at most this long after the clip ends.
    tail_secs: f32,
    left_in_block: usize,
    after_end: usize,
    peak: f32,
    right: Option<f32>,
}

impl SpatialSource {
    pub fn new(clip: Arc<Clip>, link: Arc<Link>, looping: bool) -> Self {
        let rate = clip.rate;
        let mut s = Self {
            tail: Tail::new(rate, Room::OPEN.rt60),
            tail_secs: TAIL_SECS,
            clip,
            link,
            looping,
            pos: 0,
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
        };
        // Start right where the game says, no glide from silence.
        let now = s.link.load();
        s.gain = now.gain;
        s.delay = now.delay.map(|d| s.delay_samples(d));
        s
    }

    /// Ring on in a room whose tail takes `rt60` seconds to die away.
    pub fn with_rt60(mut self, rt60: f32) -> Self {
        if (rt60 - Room::OPEN.rt60).abs() > 1e-3 {
            self.tail = Tail::new(self.clip.rate, rt60);
            self.tail_secs = TAIL_SECS.max(rt60 * 1.2);
        }
        self
    }

    fn delay_samples(&self, secs: f32) -> f32 {
        (secs * self.clip.rate as f32).clamp(0.0, (ITD_LEN - 2) as f32)
    }

    /// Read the game's latest wishes; false once the sound has fully ended.
    fn begin_block(&mut self) -> bool {
        if !self.looping && self.pos >= self.clip.samples.len() {
            self.after_end += BLOCK;
            let max_tail = (self.tail_secs * self.clip.rate as f32) as usize;
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
            let fc = want.cutoff[ear].clamp(20.0, 0.45 * self.clip.rate as f32);
            self.coef[ear] = 1.0 - (-TAU * fc / self.clip.rate as f32).exp();
        }
        self.wet = want.wet;
        self.peak = 0.0;
        self.left_in_block = BLOCK;
        true
    }

    fn frame(&mut self) -> Option<(f32, f32)> {
        if self.left_in_block == 0 && !self.begin_block() {
            return None;
        }
        self.left_in_block -= 1;
        let len = self.clip.samples.len();
        let x = if self.pos < len {
            let x = self.clip.samples[self.pos];
            self.pos += 1;
            if self.looping && self.pos >= len {
                self.pos = 0;
            }
            x
        } else {
            0.0
        };
        self.ring[self.w] = x;
        let mut out = [0.0; 2];
        for (ear, out) in out.iter_mut().enumerate() {
            let d = self.delay[ear];
            let back = d.floor();
            let frac = d - back;
            let i0 = (self.w + ITD_LEN - back as usize) % ITD_LEN;
            let i1 = (i0 + ITD_LEN - 1) % ITD_LEN;
            let y = self.ring[i0] + frac * (self.ring[i1] - self.ring[i0]);
            // Two one-poles: a gentle 12 dB/octave roll-off.
            let a = self.coef[ear];
            let lp = &mut self.lp[ear];
            lp[0] += a * (y - lp[0]);
            lp[1] += a * (lp[0] - lp[1]);
            *out = lp[1] * self.gain[ear];
            self.gain[ear] += self.gain_step[ear];
            self.delay[ear] += self.delay_step[ear];
        }
        let (tl, tr) = self.tail.tick(x * self.wet);
        let l = out[0] + tl;
        let r = out[1] + tr;
        self.w = (self.w + 1) % ITD_LEN;
        self.peak = self.peak.max(l.abs()).max(r.abs());
        Some((l, r))
    }
}

impl Iterator for SpatialSource {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        if let Some(r) = self.right.take() {
            return Some(r as Sample);
        }
        let (l, r) = self.frame()?;
        self.right = Some(r);
        Some(l as Sample)
    }
}

impl Source for SpatialSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> NonZero<u16> {
        NonZero::<u16>::MIN.saturating_add(1)
    }

    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::new(self.clip.rate).unwrap_or(NonZero::<u32>::MIN)
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

/// Where a world sound's samples come from while it waits to start.
enum Pending {
    Asset(Handle<AudioSource>),
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

/// Play a sound once at `at` in the world. Returns its entity (despawn to stop).
pub fn play_at(
    commands: &mut Commands,
    sound: &Handle<AudioSource>,
    at: Vec3,
    volume: f32,
    carry: Carry,
) -> Entity {
    let from = Pending::Asset(sound.clone());
    spawn_pending(
        commands,
        from,
        Transform::from_translation(at),
        volume,
        carry,
        false,
    )
}

/// Play a sound once, riding along with `parent` (offset in its space).
pub fn play_on(
    commands: &mut Commands,
    sound: &Handle<AudioSource>,
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
    sound: &Handle<AudioSource>,
    at: Vec3,
    volume: f32,
    carry: Carry,
) -> Entity {
    let from = Pending::Asset(sound.clone());
    spawn_pending(
        commands,
        from,
        Transform::from_translation(at),
        volume,
        carry,
        true,
    )
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
    let task = pool.spawn(async move {
        Clip::decode(&AudioSource {
            bytes: Arc::from(make_wav()),
        })
    });
    spawn_pending(
        commands,
        Pending::Synth(task),
        Transform::from_translation(at),
        volume,
        carry,
        false,
    )
}

/// Decoded clips by sound file (holding the handle so its id isn't reused).
#[derive(Resource, Default)]
struct Clips(HashMap<AssetId<AudioSource>, Cached>);

enum Cached {
    Decoding(Handle<AudioSource>, Task<Option<Clip>>),
    Ready {
        _keep: Handle<AudioSource>,
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
        handle: &Handle<AudioSource>,
        sources: Option<&Assets<AudioSource>>,
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
    if !app.is_plugin_added::<AudioPlugin>() {
        app.add_systems(PostUpdate, drop_sounds);
        return;
    }
    app.add_audio_source::<SpatialSound>()
        .init_resource::<Clips>()
        .add_systems(
            PostUpdate,
            (start_sounds, occlude, steer).chain().after(follow_ears),
        );
}

fn spawn_ears(mut commands: Commands) {
    commands.spawn((
        EarsEntity,
        VoiceListener::default(),
        Transform::default(),
        Name::new("Ears"),
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
    sources: Option<Res<Assets<AudioSource>>>,
    server: Option<Res<AssetServer>>,
    mut clips: ResMut<Clips>,
    mut sounds: ResMut<Assets<SpatialSound>>,
) {
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
        let handle = sounds.add(SpatialSound {
            clip,
            link: link.clone(),
            looping: p.looping,
            rt60: room.rt60,
        });
        // A looping source never ends, so `ONCE` (Bevy's `LOOP` would buffer
        // the stream and freeze its position).
        let settings = if p.looping {
            PlaybackSettings::ONCE
        } else {
            PlaybackSettings::DESPAWN
        };
        commands.entity(entity).remove::<PendingSound>().insert((
            AudioPlayer(handle),
            settings.with_volume(Volume::Linear(p.volume)),
            Emitter {
                link,
                carry: p.carry,
                occluded: 0.0,
                blocked: false,
            },
        ));
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

fn steer(ears: Res<Ears>, room: Option<Res<Room>>, emitters: Query<(&GlobalTransform, &Emitter)>) {
    let room = room.as_deref().copied().unwrap_or_default();
    for (at, e) in &emitters {
        e.link
            .store(&room.apply(shape(&ears, at.translation(), e.carry, e.occluded)));
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
        let clip = Clip::decode(&AudioSource {
            bytes: Arc::from(wav),
        });
        let Some(clip) = clip else {
            panic!("didn't decode");
        };
        assert_eq!(clip.rate, 22_050);
        assert!((clip.samples.len() as i32 - 2205).abs() < 4);
        assert!((clip.samples[100] - samples[100]).abs() < 0.01);
    }
}

/// `SPATIAL_DEMO=out.wav cargo test -p friendslop --lib spatial_demo -- --ignored`
/// renders a crow cawing as it circles your head, then flies off behind a
/// wall (for checking on headphones).
#[cfg(test)]
#[test]
#[ignore]
fn spatial_demo() {
    let Ok(out) = std::env::var("SPATIAL_DEMO") else {
        return;
    };
    let bytes = std::fs::read("assets/audio/sfx/crow_caw.wav").unwrap_or_default();
    let caw = Clip::decode(&AudioSource {
        bytes: Arc::from(bytes),
    })
    .unwrap_or_else(|| Clip::new(vec![0.0; 100], 44_100));
    let rate = caw.rate;
    // Caws with gaps between them, looped.
    let mut samples = caw.samples.clone();
    samples.resize(samples.len() + rate as usize / 3, 0.0);
    let clip = Arc::new(Clip::new(samples, rate));
    let link = Arc::new(Link::default());
    let ears = Ears::default();
    let secs = 16.0;
    let at = |t: f32| -> (Vec3, f32) {
        if t < 8.0 {
            // Circle at 5 m: front, right, behind, left, front...
            let a = t / 8.0 * TAU;
            (Vec3::new(a.sin() * 5.0, 0.0, -a.cos() * 5.0), 0.0)
        } else {
            // Fly straight away ahead, a wall coming between at 12 s.
            let d = 5.0 + (t - 8.0) * 9.0;
            (Vec3::new(0.0, 0.0, -d), ((t - 12.0) / 0.3).clamp(0.0, 1.0))
        }
    };
    let (p, o) = at(0.0);
    link.store(&shape(&ears, p, Carry::CALL, o));
    let mut src = SpatialSource::new(clip, link.clone(), true);
    let frames = (secs * rate as f32) as usize;
    let mut pcm = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        if f % 256 == 0 {
            let (p, o) = at(f as f32 / rate as f32);
            link.store(&shape(&ears, p, Carry::CALL, o));
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
