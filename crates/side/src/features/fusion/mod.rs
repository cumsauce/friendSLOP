//! Generated, adaptive 90s jazz-fusion soundtrack (asked for in chat: "write
//! some jazz fusion music, in the 90s/FM synth vibe ... that adapts to the
//! game state smoothly"). Replaces the old looping "Catsong" track.
//!
//! The music itself (composer, synths, mixer) lives in the `fusion` crate,
//! which has no Bevy dependency so it builds and auditions in seconds:
//! `cargo run -p fusion --bin fusion-render -- help`.
//!
//! The game side (this file) only picks a [`Mood`], an intensity and counts
//! victories each frame and stores them in atomics; the audio thread reads
//! them at the next bar line, so changes are always quantized to the bar.
//! Which mood is the score's rules (`moods.ron`, see [`fusion::direct`])
//! over the [`MusicSignals`] features publish, the camera and its zone: this
//! file names no feature.
//!
//! It also follows the local player: each storey climbed above the ground
//! moves the key up a step of the scale (prepared by a dominant, so it
//! sounds like a gear change, not a glitch), melodies lean the way you're
//! heading, running lifts the band, standing about lets it lay back, friends
//! nearby fill it out, and it ducks under voice chat.
//! The music is synthesized live inside a custom Bevy audio source (a few
//! hundred floats of work per sample, no allocation), so the main thread
//! never waits on it. Music is local: every peer hears its own take.

pub use fusion::{compose, drums, dsp, engine, inst, synth, theory};

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, AtomicU32, Ordering};

use bevy::prelude::*;
use bevy_seedling::prelude::{Connect, RegisterNode};
use firewheel::channel_config::{ChannelConfig, ChannelCount};
use firewheel::node::{
    AudioNode, AudioNodeInfo, AudioNodeProcessor, ConstructProcessorContext, EmptyConfig,
    NodeError, ProcBuffers, ProcExtra, ProcInfo, ProcessStatus,
};

use crate::AppState;
use crate::features::fps::Walker;
use crate::music::{MusicSignalSet, MusicSignals};
use crate::player::{LocalPlayer, Player};
use crate::sound::{MusicBus, Volume, VolumeNode, audible};

pub use compose::{Cue, LIFT_MAX, Mood};
use engine::Engine;
use synth::{Bank, RATE};

/// How loud the music plays (before the master volume, which is on the main bus): under SFX and
/// voice chat.
pub const MUSIC_LEVEL: f32 = 0.5;

/// What the game wants, shared with the audio thread.
#[derive(Debug, Default)]
pub struct MusicControl {
    mood: AtomicU16,
    intensity: AtomicU32,
    victories: AtomicU32,
    /// Stingers fired so far, and which one (`Score::stinger` index) was last.
    stingers: AtomicU32,
    stinger: AtomicU32,
    lift: AtomicU8,
    climb: AtomicU32,
    /// Ducking level (f32 bits); 0 bits reads as "no ducking".
    duck: AtomicU32,
    /// Live signals songs follow (f32 bits), in `fusion::live::names` order.
    live: [AtomicU32; fusion::live::LIVE],
}

impl MusicControl {
    pub fn set(&self, mood: Mood, intensity: f32) {
        self.mood.store(mood.index(), Ordering::Relaxed);
        self.intensity
            .store(intensity.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn victory(&self) {
        self.victories.fetch_add(1, Ordering::Relaxed);
    }

    /// Fire the stinger numbered `which` (see `Score::stinger_names`): the
    /// audio thread plays it at once and cuts to the mood set before it.
    pub fn stinger(&self, which: usize) {
        self.stinger.store(which as u32, Ordering::Relaxed);
        self.stingers.fetch_add(1, Ordering::Release);
    }

    /// The stinger count and which one fired last.
    pub fn stingers(&self) -> (u32, usize) {
        let seq = self.stingers.load(Ordering::Acquire);
        (seq, self.stinger.load(Ordering::Relaxed) as usize)
    }

    /// Where the player is heading: the storey they're on and which way
    /// they're moving vertically (-1..1).
    pub fn set_motion(&self, lift: u8, climb: f32) {
        self.lift.store(lift.min(LIFT_MAX), Ordering::Relaxed);
        self.climb
            .store(climb.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Music level under voice chat (1 = full).
    pub fn set_duck(&self, level: f32) {
        self.duck
            .store(level.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// A live signal's value (slot from `fusion::live::names`).
    pub fn set_live(&self, slot: usize, value: f32) {
        if let Some(s) = self.live.get(slot) {
            s.store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
        }
    }

    pub fn live(&self) -> [f32; fusion::live::LIVE] {
        std::array::from_fn(|i| f32::from_bits(self.live[i].load(Ordering::Relaxed)))
    }

    pub fn duck(&self) -> f32 {
        match self.duck.load(Ordering::Relaxed) {
            0 => 1.0,
            bits => f32::from_bits(bits),
        }
    }

    pub fn cue(&self) -> Cue {
        Cue {
            mood: Mood::from_index(self.mood.load(Ordering::Relaxed)),
            intensity: f32::from_bits(self.intensity.load(Ordering::Relaxed)),
            victories: self.victories.load(Ordering::Relaxed),
            lift: self.lift.load(Ordering::Relaxed),
            climb: f32::from_bits(self.climb.load(Ordering::Relaxed)),
        }
    }
}

/// The game's handle on the music.
#[derive(Resource, Clone)]
pub struct MusicLink(pub Arc<MusicControl>);

/// The soundtrack in the audio graph: an endless stereo stream, no inputs.
/// Its processor owns the [`Engine`] (fixed at 44.1 kHz) and resamples it to
/// the graph's rate.
#[derive(Component, Clone)]
pub struct FusionNode {
    bank: Arc<Bank>,
    control: Arc<MusicControl>,
}

impl AudioNode for FusionNode {
    type Configuration = EmptyConfig;

    fn info(&self, _: &EmptyConfig) -> Result<AudioNodeInfo, NodeError> {
        Ok(AudioNodeInfo::new()
            .debug_name("fusion music")
            .channel_config(ChannelConfig {
                num_inputs: ChannelCount::ZERO,
                num_outputs: ChannelCount::STEREO,
            }))
    }

    fn construct_processor(
        &self,
        _: &EmptyConfig,
        cx: ConstructProcessorContext,
    ) -> Result<impl AudioNodeProcessor, NodeError> {
        Ok(FusionProcessor(FusionStream::new(
            self.bank.clone(),
            self.control.clone(),
            cx.stream_info.sample_rate.get(),
        )))
    }
}

struct FusionProcessor(FusionStream);

impl AudioNodeProcessor for FusionProcessor {
    fn process(
        &mut self,
        info: &ProcInfo,
        ProcBuffers { outputs, .. }: ProcBuffers,
        _: &mut ProcExtra,
    ) -> ProcessStatus {
        let [l, r, ..] = outputs else {
            return ProcessStatus::ClearAllOutputs;
        };
        let n = info.frames.min(l.len()).min(r.len());
        self.0.fill(&mut l[..n], &mut r[..n]);
        ProcessStatus::OutputsModified
    }
}

const CHUNK: usize = 512;

/// The live music stream at a chosen rate: renders the engine 256 stereo
/// frames at a time and resamples it (cubic Hermite, streaming) to `rate`.
/// No allocation after `new`.
pub struct FusionStream {
    engine: Engine,
    control: Arc<MusicControl>,
    buf: [f32; CHUNK],
    /// Next unread frame of `buf`.
    i: usize,
    /// Four consecutive source frames; the output lies between the middle two.
    hist: [[f32; 2]; 4],
    /// Position between `hist[1]` and `hist[2]`, and source frames per output frame.
    frac: f64,
    step: f64,
}

impl FusionStream {
    pub fn new(bank: Arc<Bank>, control: Arc<MusicControl>, rate: u32) -> Self {
        let seed = crate::settings::since_epoch().as_nanos() as u64;
        Self {
            engine: Engine::new(bank, seed),
            control,
            buf: [0.0; CHUNK],
            i: CHUNK / 2,
            hist: [[0.0; 2]; 4],
            frac: 0.0,
            step: f64::from(RATE) / f64::from(rate.max(1)),
        }
    }

    fn next_source(&mut self) -> [f32; 2] {
        if self.i >= CHUNK / 2 {
            let control = &self.control;
            self.engine.set_duck(control.duck());
            self.engine.set_live(&control.live());
            let (seq, which) = control.stingers();
            self.engine.set_stinger(seq, which);
            self.engine.render(&mut self.buf, &mut || control.cue());
            self.i = 0;
        }
        let f = [self.buf[self.i * 2], self.buf[self.i * 2 + 1]];
        self.i += 1;
        f
    }

    /// Fill `l` and `r` (same length) with the next frames.
    pub fn fill(&mut self, l: &mut [f32], r: &mut [f32]) {
        for (l, r) in l.iter_mut().zip(r.iter_mut()) {
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                let f = self.next_source();
                self.hist.rotate_left(1);
                self.hist[3] = f;
            }
            let t = self.frac as f32;
            let at = |c: usize| {
                let (y0, y1, y2, y3) = (
                    self.hist[0][c],
                    self.hist[1][c],
                    self.hist[2][c],
                    self.hist[3][c],
                );
                let c1 = 0.5 * (y2 - y0);
                let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                ((c3 * t + c2) * t + c1) * t + y1
            };
            *l = at(0);
            *r = at(1);
            self.frac += self.step;
        }
    }
}

/// Marks the music's audio entity.
#[derive(Component)]
struct MusicPlayer;

pub fn plugin(app: &mut App) {
    let control = Arc::new(MusicControl::default());
    let start = Mood::default();
    control.set(start, start.base_intensity());
    app.insert_resource(MusicLink(control))
        .init_resource::<Director>()
        .init_resource::<Motion>()
        .init_resource::<MusicSignals>()
        .add_systems(
            Update,
            (
                follow_player,
                direct,
                duck_for_voice,
                live_signals,
                music_volume,
            )
                .chain()
                .after(MusicSignalSet),
        );
    // Headless apps (tests) have no audio graph: just keep the director.
    if !app.is_plugin_added::<bevy_seedling::SeedlingCorePlugin>() {
        return;
    }
    app.register_simple_node::<FusionNode>()
        .add_systems(Startup, start_bank.run_if(audible))
        .add_systems(
            Update,
            finish_bank
                .run_if(resource_exists::<BankBuild>)
                .run_if(audible),
        );
}

/// The sample bank being rendered (once, at startup).
#[derive(Resource)]
struct BankBuild {
    #[cfg(not(target_arch = "wasm32"))]
    task: bevy::tasks::Task<Bank>,
    #[cfg(target_arch = "wasm32")]
    bank: Bank,
    #[cfg(target_arch = "wasm32")]
    step: usize,
}

fn start_bank(mut commands: Commands) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use bevy::tasks::{AsyncComputeTaskPool, TaskPool};
        let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
        commands.insert_resource(BankBuild {
            task: pool.spawn(async { Bank::build() }),
        });
    }
    // No threads in the browser: render one sample per frame instead.
    #[cfg(target_arch = "wasm32")]
    commands.insert_resource(BankBuild {
        bank: Bank::default(),
        step: 0,
    });
}

fn finish_bank(mut commands: Commands, mut build: ResMut<BankBuild>, link: Res<MusicLink>) {
    #[cfg(not(target_arch = "wasm32"))]
    let Some(bank) = bevy::tasks::futures::check_ready(&mut build.task) else {
        return;
    };
    #[cfg(target_arch = "wasm32")]
    let bank = {
        // The kit comes from the instruments file: wait until it's fetched.
        if !inst::INSTRUMENTS.loaded() {
            return;
        }
        if build.step < synth::bank_steps() {
            let step = build.step;
            build.bank.build_step(step);
            build.step += 1;
            return;
        }
        std::mem::take(&mut build.bank)
    };
    commands.remove_resource::<BankBuild>();
    // Silent until `music_volume` fades it in; the volume node feeds the music bus.
    commands
        .spawn(FusionNode {
            bank: Arc::new(bank),
            control: link.0.clone(),
        })
        .chain_node((
            VolumeNode {
                volume: Volume::Linear(0.0),
                ..default()
            },
            MusicPlayer,
        ))
        .connect(MusicBus);
}

/// The music's level, live (master is on the main bus).
/// The score's `hush` fades the band out (an attack's strings, the dark of
/// the warehouse, Fulton St station) and its `silence` cuts it dead (the
/// endless hallway, #206).
fn music_volume(
    time: Res<Time>,
    mut fade_in: Local<f32>,
    mut hush: Local<f32>,
    here: Facts,
    mut nodes: Query<&mut VolumeNode, With<MusicPlayer>>,
) {
    let score = compose::SCORE.get();
    let here = here.get();
    let holds = |c: &Option<fusion::direct::Cond>| c.as_ref().is_some_and(|c| c.holds(&here));
    let quiet = holds(&score.hush);
    let k = 1.0 - (-3.0 * time.delta_secs()).exp();
    *hush += (f32::from(u8::from(quiet)) - *hush) * k;
    if holds(&score.silence) {
        *hush = 1.0;
    }
    for mut node in &mut nodes {
        *fade_in = (*fade_in + time.delta_secs() / 2.0).min(1.0);
        let level = MUSIC_LEVEL * *fade_in * (1.0 - *hush);
        node.volume = Volume::Linear(level);
    }
}

/// Storeys: metres above the ground under your feet where the key steps
/// up (a floor of a building, the next floor, a tall spire, the islands).
const STOREYS: [f32; LIFT_MAX as usize] = [2.4, 5.4, 9.0, 15.0];
/// Going back down needs this much below a threshold (no flapping on a
/// landing).
const STOREY_SLACK: f32 = 0.8;

/// The storey for height `h` above the ground, from storey `now`.
pub fn storey(h: f32, now: u8) -> u8 {
    let mut s = now.min(LIFT_MAX) as usize;
    while s < STOREYS.len() && h > STOREYS[s] {
        s += 1;
    }
    while s > 0 && h < STOREYS[s - 1] - STOREY_SLACK {
        s -= 1;
    }
    s as u8
}

/// How the local player is moving, smoothed.
#[derive(Resource, Debug, Default)]
struct Motion {
    last: Option<Vec3>,
    /// Height above the ground (smoothed), its storey and vertical speed.
    height: f32,
    storey: u8,
    vy: f32,
    /// Horizontal speed (m/s, smoothed) and how long you've stood still.
    speed: f32,
    idle: f32,
    /// Other players within earshot.
    friends: usize,
}

/// Players this close count as being together.
const FRIEND_RANGE: f32 = 15.0;

fn follow_player(
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    mut motion: ResMut<Motion>,
    link: Res<MusicLink>,
) {
    let dt = time.delta_secs().max(1e-4);
    let my_id = me.map(|me| me.0);
    let mine = walkers
        .iter()
        .find(|(p, _)| Some(p.id) == my_id)
        .map(|(_, w)| w.pos);
    let Some(at) = mine else {
        *motion = Motion::default();
        link.0.set_motion(0, 0.0);
        return;
    };
    let ease = |tau: f32| 1.0 - (-dt / tau).exp();
    let ground = crate::terrain::height_at(at.x, at.z);
    let h = (at.y - ground).max(0.0);
    let (vy, speed) = match motion.last {
        // A teleport (respawn, a new round) isn't movement.
        Some(last) if last.distance(at) < 20.0 * dt + 1.0 => {
            let d = (at - last) / dt;
            (d.y, d.xz().length())
        }
        _ => {
            motion.height = h;
            (0.0, 0.0)
        }
    };
    motion.last = Some(at);
    // A jump is up and down within a second: it barely moves these.
    motion.height += (h - motion.height) * ease(0.6);
    motion.vy += (vy - motion.vy) * ease(0.9);
    motion.speed += (speed - motion.speed) * ease(0.5);
    motion.storey = storey(motion.height, motion.storey);
    motion.idle = if motion.speed < 0.4 {
        motion.idle + dt
    } else {
        0.0
    };
    motion.friends = walkers
        .iter()
        .filter(|(p, w)| Some(p.id) != my_id && w.pos.distance(at) < FRIEND_RANGE)
        .count();
    link.0.set_motion(motion.storey, motion.vy / 1.5);
}

/// The signals songs follow live (a part's `follow:`), copied to the audio
/// thread every frame.
fn live_signals(signals: Res<MusicSignals>, link: Res<MusicLink>) {
    for (slot, name) in fusion::live::names().iter().enumerate() {
        link.0.set_live(slot, signals.get(name));
    }
}

/// The band backs off while someone's talking (a little for your own
/// voice, so speakers don't feed the music into your mic).
fn duck_for_voice(activity: Option<Res<crate::voice_chat::VoiceActivity>>, link: Res<MusicLink>) {
    let level = match activity {
        Some(a) if !a.heard.is_empty() => 0.6,
        Some(a) if a.transmitting => 0.8,
        _ => 1.0,
    };
    link.0.set_duck(level);
}

/// How the player's own movement and company nudge the band in the open
/// world: running lifts it, standing about lets it lay back, friends
/// nearby fill it out.
fn movement_nudge(m: &Motion) -> f32 {
    let pace = (m.speed / crate::kcc::RUN_SPEED).clamp(0.0, 1.0);
    let settle = ((m.idle - 8.0) / 10.0).clamp(0.0, 1.0);
    0.18 * (pace - 0.35) - 0.12 * settle + 0.05 * m.friends.min(3) as f32
}

/// The director's view of the game, from the main thread.
#[derive(Resource, Debug, Default)]
struct Director {
    /// The mood being asked for, and a candidate waiting out its hysteresis.
    mood: Mood,
    candidate: Mood,
    held: f32,
    /// Victory cues counted so far.
    victories: u32,
    /// Each stinger's cue count so far (`Score::stinger_names` order).
    stingers: Vec<u32>,
}

/// What the score's rules see: the signals, whether we're playing, and
/// where the camera is (and so its zone).
pub struct Here<'a> {
    pub signals: &'a MusicSignals,
    pub in_game: bool,
    pub at: Option<Vec3>,
}

impl fusion::direct::Facts for Here<'_> {
    fn signal(&self, name: &str) -> f32 {
        self.signals.get(name)
    }
    fn in_game(&self) -> bool {
        self.in_game
    }
    fn at(&self) -> Option<[f32; 3]> {
        self.at.map(|a| a.to_array())
    }
    fn in_zone(&self, zone: &str) -> bool {
        self.at
            .is_some_and(|at| format!("{:?}", crate::zones::zone_of(at.xz())) == zone)
    }
    fn spot(&self, name: &str) -> Option<[f32; 2]> {
        friendslop_world::places::spot(name).map(|p| p.to_array())
    }
}

/// The game state the rules read, as a system parameter.
#[derive(bevy::ecs::system::SystemParam)]
struct Facts<'w, 's> {
    state: Option<Res<'w, State<AppState>>>,
    signals: Res<'w, MusicSignals>,
    camera:
        Query<'w, 's, &'static GlobalTransform, (With<Camera3d>, Without<crate::world::MapCamera>)>,
}

impl Facts<'_, '_> {
    fn get(&self) -> Here<'_> {
        Here {
            signals: &self.signals,
            in_game: self
                .state
                .as_ref()
                .is_some_and(|s| *s.get() == AppState::InGame),
            at: crate::music::camera_at(&self.camera),
        }
    }
}

/// What the music should be playing (the score's rules, first match).
pub fn pick_mood(here: &Here) -> (Mood, f32) {
    fusion::direct::pick(&compose::SCORE.get().rules, here)
}

fn direct(
    time: Res<Time>,
    link: Res<MusicLink>,
    mut dir: ResMut<Director>,
    here: Facts,
    motion: Res<Motion>,
) {
    let here = here.get();
    // Victories: whatever the features cued (a round, a duel, a chapter...).
    let score = compose::SCORE.get();
    let victories: u32 = score
        .victory_cues
        .iter()
        .map(|c| here.signals.cues(c))
        .sum();
    for _ in dir.victories..victories {
        link.0.victory();
    }
    dir.victories = victories;

    let (want, intensity) = pick_mood(&here);

    // Places need to be meant (walking along a zone edge shouldn't flip the
    // band every bar); game events take over at once.
    if want != dir.candidate {
        dir.candidate = want;
        dir.held = 0.0;
    }
    dir.held += time.delta_secs();
    if want != dir.mood && (want.instant() || dir.held > 2.0) {
        dir.mood = want;
    }
    let mood = dir.mood;
    let intensity = if mood == want {
        intensity
    } else {
        mood.base_intensity()
    };
    let intensity = if here.in_game && mood.wandering() {
        (intensity + movement_nudge(&motion)).clamp(0.05, 0.9)
    } else {
        intensity
    };
    link.0.set(mood, intensity);
    // Stingers go after the mood, so the band cuts into the new one.
    let fired = stingers_fired(&score.stingers, &here, &mut dir.stingers);
    for which in fired {
        link.0.stinger(which);
    }
}

/// The stingers whose cue fired since last time (`seen`: each one's count
/// then, updated): their numbers in the score's order. Several in one frame
/// fire in order; the cue counts of a score with fewer stingers are dropped.
pub fn stingers_fired(
    stingers: &std::collections::BTreeMap<String, Vec<fusion::compose::StingerHit>>,
    here: &Here,
    seen: &mut Vec<u32>,
) -> Vec<usize> {
    seen.resize(stingers.len(), 0);
    let mut out = Vec::new();
    for (i, name) in stingers.keys().enumerate() {
        let count = here.signals.cues(name);
        if count > seen[i] {
            out.push(i);
        }
        seen[i] = count;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_round_trips() {
        let c = MusicControl::default();
        c.set(Mood::named("Duel"), 0.7);
        c.victory();
        let cue = c.cue();
        assert_eq!(cue.mood, Mood::named("Duel"));
        assert!((cue.intensity - 0.7).abs() < 1e-6);
        assert_eq!(cue.victories, 1);
    }

    #[test]
    fn stingers_follow_their_cues() {
        let score = compose::SCORE.get();
        let name = "fulton_spotted";
        let which = score
            .stinger_names()
            .position(|n| n == name)
            .expect("the spotted stinger");
        let mut signals = MusicSignals::default();
        let mut seen = Vec::new();
        let at = |s: &MusicSignals, seen: &mut Vec<u32>| {
            let here = Here {
                signals: s,
                in_game: true,
                at: None,
            };
            stingers_fired(&score.stingers, &here, seen)
        };
        assert!(at(&signals, &mut seen).is_empty());
        signals.cue(name);
        assert_eq!(at(&signals, &mut seen), vec![which]);
        // Once per cue.
        assert!(at(&signals, &mut seen).is_empty());
        signals.cue(name);
        assert_eq!(at(&signals, &mut seen), vec![which]);
        let c = MusicControl::default();
        assert_eq!(c.stingers().0, 0);
        c.stinger(which);
        assert_eq!(c.stingers(), (1, which));
    }

    /// Fulton St: the three moods the hunter's signals pick between.
    #[test]
    fn fulton_signals_pick_its_music() {
        let m = Mood::named;
        let cam = Some(Vec3::ZERO);
        let station = [("fulton", 1.0), ("fulton_creepy", 1.0)];
        let got = |extra: &[(&str, f32)]| {
            let mut all = station.to_vec();
            all.extend_from_slice(extra);
            pick(&all, true, cam).0
        };
        assert_eq!(got(&[]), m("FultonAmbient"));
        // The kept-up top floors keep the city's music.
        assert_ne!(pick(&[("fulton", 1.0)], true, cam).0, m("FultonAmbient"));
        assert_eq!(got(&[("fulton_tense", 12.0)]), m("FultonTense"));
        assert_eq!(got(&[("fulton_chase", 1.0)]), m("FultonChase"));
        // The chase wins over the lingering tension.
        assert_eq!(
            got(&[("fulton_chase", 1.0), ("fulton_tense", 12.0)]),
            m("FultonChase")
        );
        // Never up in the street.
        assert_ne!(
            pick(&[("fulton_chase", 1.0)], true, cam).0,
            m("FultonChase")
        );
        // And the band is not hushed down there.
        let here = Here {
            signals: &{
                let mut s = MusicSignals::default();
                s.flag("fulton", true);
                s
            },
            in_game: true,
            at: cam,
        };
        let score = compose::SCORE.get();
        assert!(score.hush.as_ref().is_some_and(|h| !h.holds(&here)));
        // Chase and tension cut in at once.
        for n in ["FultonChase", "FultonTense"] {
            assert!(m(n).urgent() && m(n).instant(), "{n}");
        }
    }

    #[test]
    fn storeys_have_slack() {
        assert_eq!(storey(0.0, 0), 0);
        assert_eq!(storey(3.0, 0), 1);
        assert_eq!(storey(20.0, 0), LIFT_MAX);
        // Just under the first floor's threshold stays on it...
        assert_eq!(storey(2.0, 1), 1);
        // ...but back on the ground drops down.
        assert_eq!(storey(0.5, 1), 0);
        assert_eq!(storey(0.0, LIFT_MAX), 0);
    }

    #[test]
    fn motion_reaches_the_music() {
        let c = MusicControl::default();
        assert_eq!(c.duck(), 1.0);
        c.set_motion(9, 3.0);
        c.set_duck(0.6);
        let cue = c.cue();
        assert_eq!(cue.lift, LIFT_MAX);
        assert_eq!(cue.climb, 1.0);
        assert!((c.duck() - 0.6).abs() < 1e-6);
        let idle = Motion {
            idle: 30.0,
            ..Motion::default()
        };
        let running = Motion {
            speed: crate::kcc::RUN_SPEED,
            friends: 2,
            ..Motion::default()
        };
        assert!(movement_nudge(&idle) < -0.1);
        assert!(movement_nudge(&running) > 0.15);
    }

    /// The place signals the features would publish with the camera at `at`.
    fn place_signals(at: Vec3) -> MusicSignals {
        let mut s = MusicSignals::default();
        let wendys = crate::features::wendys::music_place(at);
        s.flag(
            "underground",
            crate::features::underground::below(at - Vec3::Y * 1.6),
        );
        s.flag("wendys", wendys.is_some());
        s.flag("wendys_attic", wendys == Some(true));
        s.flag("nyc", crate::features::nyc::in_the_city(at));
        s.flag("heaven", crate::features::abuela::by_the_chair(at));
        s.flag("sky", crate::features::climb::on_an_island(at));
        s
    }

    fn pick(signals: &[(&str, f32)], in_game: bool, at: Option<Vec3>) -> (Mood, f32) {
        let mut s = at.map_or_else(MusicSignals::default, place_signals);
        for (k, v) in signals {
            s.set(k, *v);
        }
        pick_mood(&Here {
            signals: &s,
            in_game,
            at,
        })
    }

    /// The rules in moods.ron play what the director in code used to, in
    /// the same order, at the same intensities.
    #[test]
    fn rules_match_the_old_director() {
        let m = Mood::named;
        let base = |n: &str| m(n).base_intensity();
        let cam = Some(Vec3::ZERO);
        let seek_secs = crate::features::hide_and_seek::tuning().seek_secs;
        let cases: Vec<(&str, Vec<(&str, f32)>, bool, Option<Vec3>, Mood, f32)> = vec![
            (
                "title",
                vec![("duel", 1.0)],
                false,
                cam,
                m("Menu"),
                base("Menu"),
            ),
            (
                "frozen",
                vec![("frozen", 1.0), ("duel", 1.0)],
                true,
                cam,
                m("Still"),
                0.2,
            ),
            (
                "shamed",
                vec![("shamed", 1.0), ("duel", 1.0)],
                true,
                cam,
                m("FartAlert"),
                base("FartAlert"),
            ),
            (
                "last life",
                vec![("duel", 1.0), ("duel_lives", 1.0), ("hiding", 1.0)],
                true,
                cam,
                m("Duel"),
                1.0,
            ),
            (
                "duel",
                vec![("duel", 1.0), ("duel_lives", 3.0)],
                true,
                cam,
                m("Duel"),
                0.9,
            ),
            (
                "hiding",
                vec![("hiding", 1.0), ("secs_left", 10.0), ("landlord_boss", 1.0)],
                true,
                cam,
                m("Hiding"),
                0.3 + 0.2 * (1.0 - 10.0f32 / 30.0).clamp(0.0, 1.0),
            ),
            (
                "seeking",
                vec![
                    ("seeking", 1.0),
                    ("secs_left", 20.0),
                    ("seek_secs", seek_secs),
                ],
                true,
                cam,
                m("Seeking"),
                0.65 + 0.35 * (1.0 - (20.0 / seek_secs).clamp(0.0, 1.0)),
            ),
            (
                "boss",
                vec![
                    ("landlord_boss", 1.0),
                    ("boss_hp", 6.0),
                    ("near_landlord", 1.0),
                ],
                true,
                cam,
                m("Landlord"),
                0.75 + 0.2 * (1.0 - 6.0f32 / 12.0).clamp(0.0, 1.0),
            ),
            (
                "rent",
                vec![
                    ("near_landlord", 1.0),
                    ("raises", 3.0),
                    ("underground", 1.0),
                ],
                true,
                cam,
                m("Landlord"),
                (0.4 + 0.08 * 3.0f32).min(0.8),
            ),
            (
                "rent rises",
                vec![("near_landlord", 1.0), ("raises", 9.0)],
                true,
                cam,
                m("Landlord"),
                0.8,
            ),
            (
                "won",
                vec![("near_landlord", 1.0), ("won", 1.0)],
                true,
                cam,
                m("Village"),
                base("Village"),
            ),
            (
                "tunnels",
                vec![("underground", 1.0), ("wendys", 1.0)],
                true,
                cam,
                m("Underground"),
                base("Underground"),
            ),
            (
                "attic",
                vec![("wendys", 1.0), ("wendys_attic", 1.0), ("nyc", 1.0)],
                true,
                cam,
                m("WendysAttic"),
                base("WendysAttic"),
            ),
            (
                "wendys",
                vec![("wendys", 1.0), ("nyc", 1.0)],
                true,
                cam,
                m("Wendys"),
                base("Wendys"),
            ),
            (
                "bodega",
                vec![("nyc", 1.0), ("nyc_shop_bodega", 1.0), ("heaven", 1.0)],
                true,
                cam,
                m("Bodega"),
                base("Bodega"),
            ),
            (
                "pizza",
                vec![("nyc", 1.0), ("nyc_shop_pizza", 1.0)],
                true,
                cam,
                m("Nyc"),
                base("Nyc"),
            ),
            (
                "heaven",
                vec![("heaven", 1.0), ("sky", 1.0)],
                true,
                cam,
                m("Abuela"),
                base("Abuela"),
            ),
            ("sky", vec![("sky", 1.0)], true, cam, m("Sky"), base("Sky")),
            ("commune", vec![], true, cam, m("Village"), base("Village")),
            (
                "no camera",
                vec![],
                true,
                None,
                m("Explore"),
                base("Explore"),
            ),
            (
                "outside",
                vec![],
                true,
                Some(Vec3::new(1e5, 0.0, 1e5)),
                m("Explore"),
                base("Explore"),
            ),
        ];
        for (name, signals, in_game, at, mood, intensity) in cases {
            let (got, i) = pick(&signals, in_game, at);
            assert_eq!(got, mood, "{name}");
            assert_eq!(i, intensity, "{name}: intensity");
        }
        // The zones, wherever the map puts them.
        let layout = friendslop_world::places::layout();
        for (kind, lo, hi) in &layout.districts {
            let c = (*lo + *hi) / 2.0;
            let at = Vec3::new(c.x, crate::terrain::height_at(c.x, c.y) + 1.6, c.y);
            let want = match crate::zones::zone_of(c) {
                crate::zones::Zone::Commune | crate::zones::Zone::Village => "Village",
                crate::zones::Zone::Forest | crate::zones::Zone::Valley => "Forest",
                crate::zones::Zone::Ruins => "Maze",
                _ => "Explore",
            };
            // Only the zone: no place signals.
            let plain = MusicSignals::default();
            let (got, _) = pick_mood(&Here {
                signals: &plain,
                in_game: true,
                at: Some(at),
            });
            assert_eq!(got, m(want), "{kind}");
        }
    }

    #[test]
    fn places_have_moods() {
        assert_eq!(pick(&[], true, Some(Vec3::ZERO)).0, Mood::named("Village"));
        let chair = crate::features::abuela::chair_at();
        assert_eq!(
            pick(&[], true, Some(chair + Vec3::Y)).0,
            Mood::named("Abuela")
        );
    }

    /// Every signal the rules read is one some feature publishes.
    #[test]
    fn rules_read_published_signals() {
        const PUBLISHED: &[&str] = &[
            "duel",
            "duel_lives",
            "hiding",
            "seeking",
            "secs_left",
            "seek_secs",
            "frozen",
            "shamed",
            "landlord_boss",
            "boss_hp",
            "near_landlord",
            "raises",
            "won",
            "underground",
            "wendys",
            "wendys_attic",
            "nyc",
            "nyc_shop_bodega",
            "nyc_shop_pizza",
            "nyc_shop_laundromat",
            "nyc_shop_diner",
            "heaven",
            "sky",
            "attack",
            "spooked",
            "adrift",
            "fulton",
            "fulton_chase",
            "fulton_creepy",
            "michael_theater",
            "fulton_tense",
            "miami",
            "factory_town",
            "surf",
            "airborne",
            "boba",
            "penn_njt",
            "penn_lirr",
            "arcade_fps",
        ];
        fn names(c: &fusion::direct::Cond, out: &mut Vec<String>) {
            use fusion::direct::Cond;
            match c {
                Cond::Signal(n, ..) => out.push(n.clone()),
                Cond::All(cs) | Cond::Any(cs) => cs.iter().for_each(|c| names(c, out)),
                Cond::Not(c) => names(c, out),
                _ => {}
            }
        }
        let score = compose::SCORE.get();
        assert!(!score.rules.is_empty());
        let mut used = Vec::new();
        for r in &score.rules {
            names(&r.when, &mut used);
        }
        for c in [&score.hush, &score.silence].into_iter().flatten() {
            names(c, &mut used);
        }
        for n in used {
            assert!(PUBLISHED.contains(&n.as_str()), "nothing publishes {n:?}");
        }
    }

    #[test]
    fn stream_is_stereo_endless_and_sane() {
        let bank = Arc::new(Bank::build());
        let ctl = Arc::new(MusicControl::default());
        ctl.set(Mood::named("Village"), 0.6);
        for rate in [RATE, 48_000] {
            let mut s = FusionStream::new(bank.clone(), ctl.clone(), rate);
            let (mut l, mut r) = (vec![0.0f32; 480], vec![0.0f32; 480]);
            let (mut peak, mut energy, mut n) = (0.0f32, 0.0f64, 0usize);
            // Two seconds, in graph-sized blocks.
            for _ in 0..(rate as usize * 2 / 480) {
                s.fill(&mut l, &mut r);
                for v in l.iter().chain(r.iter()) {
                    assert!(v.is_finite());
                    peak = peak.max(v.abs());
                    energy += f64::from(*v * *v);
                    n += 1;
                }
            }
            let rms = (energy / n as f64).sqrt();
            assert!(peak > 0.01 && peak < 4.0, "peak {peak} at {rate}");
            assert!(rms > 0.001, "silent at {rate}");
        }
    }
}
