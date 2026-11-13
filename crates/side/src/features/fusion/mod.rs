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

use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, AtomicU32, Ordering};

use bevy::audio::{AddAudioSource, AudioPlugin, Decodable, Sample, Source, Volume};
use bevy::prelude::*;

use crate::AppState;
use crate::features::fps::Walker;
use crate::music::{MusicSignalSet, MusicSignals};
use crate::player::{LocalPlayer, Player};
use crate::settings::Settings;

pub use compose::{Cue, LIFT_MAX, Mood};
use engine::Engine;
use synth::{Bank, RATE};

/// How loud the music plays (before the master volume): under SFX and
/// voice chat.
pub const MUSIC_LEVEL: f32 = 0.5;

/// What the game wants, shared with the audio thread.
#[derive(Debug, Default)]
pub struct MusicControl {
    mood: AtomicU16,
    intensity: AtomicU32,
    victories: AtomicU32,
    lift: AtomicU8,
    climb: AtomicU32,
    /// Ducking level (f32 bits); 0 bits reads as "no ducking".
    duck: AtomicU32,
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

/// The soundtrack as a Bevy audio asset: playing it starts an endless
/// [`FusionSource`].
#[derive(Asset, TypePath)]
pub struct FusionMusic {
    bank: Arc<Bank>,
    control: Arc<MusicControl>,
}

impl Decodable for FusionMusic {
    type Decoder = FusionSource;

    fn decoder(&self) -> FusionSource {
        FusionSource::new(self.bank.clone(), self.control.clone())
    }
}

const CHUNK: usize = 512;

/// The live music stream: renders 256 stereo frames at a time.
pub struct FusionSource {
    engine: Engine,
    control: Arc<MusicControl>,
    buf: [f32; CHUNK],
    i: usize,
}

impl FusionSource {
    pub fn new(bank: Arc<Bank>, control: Arc<MusicControl>) -> Self {
        let seed = crate::settings::since_epoch().as_nanos() as u64;
        Self {
            engine: Engine::new(bank, seed),
            control,
            buf: [0.0; CHUNK],
            i: CHUNK,
        }
    }
}

impl Iterator for FusionSource {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        if self.i >= CHUNK {
            let control = &self.control;
            self.engine.set_duck(control.duck());
            self.engine.render(&mut self.buf, &mut || control.cue());
            self.i = 0;
        }
        let s = self.buf[self.i];
        self.i += 1;
        Some(s as Sample)
    }
}

impl Source for FusionSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> NonZero<u16> {
        NonZero::<u16>::MIN.saturating_add(1)
    }

    fn sample_rate(&self) -> NonZero<u32> {
        NonZero::<u32>::MIN.saturating_add(RATE - 1)
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
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
            (follow_player, direct, duck_for_voice, music_volume)
                .chain()
                .after(MusicSignalSet),
        );
    // Headless apps (tests) have no audio: just keep the director.
    if !app.is_plugin_added::<AudioPlugin>() {
        return;
    }
    app.add_audio_source::<FusionMusic>()
        .add_systems(Startup, start_bank)
        .add_systems(Update, finish_bank.run_if(resource_exists::<BankBuild>));
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

fn finish_bank(
    mut commands: Commands,
    mut build: ResMut<BankBuild>,
    link: Res<MusicLink>,
    music: Option<ResMut<Assets<FusionMusic>>>,
) {
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
    let Some(mut music) = music else { return };
    let handle = music.add(FusionMusic {
        bank: Arc::new(bank),
        control: link.0.clone(),
    });
    // Not `LOOP`: the stream never ends, and looping would buffer it.
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings::ONCE.with_volume(Volume::Linear(0.0)),
        MusicPlayer,
    ));
}

/// Master volume applies live (the global volume only counts at spawn).
/// The score's `hush` fades the band out (an attack's strings, the dark of
/// the warehouse, Fulton St station) and its `silence` cuts it dead (the
/// endless hallway, #206).
fn music_volume(
    settings: Option<Res<Settings>>,
    time: Res<Time>,
    mut fade_in: Local<f32>,
    mut hush: Local<f32>,
    here: Facts,
    mut sinks: Query<&mut AudioSink, With<MusicPlayer>>,
) {
    let master = settings.map_or(0.8, |s| s.master_volume);
    let score = compose::SCORE.get();
    let here = here.get();
    let holds = |c: &Option<fusion::direct::Cond>| c.as_ref().is_some_and(|c| c.holds(&here));
    let quiet = holds(&score.hush);
    let k = 1.0 - (-3.0 * time.delta_secs()).exp();
    *hush += (f32::from(u8::from(quiet)) - *hush) * k;
    if holds(&score.silence) {
        *hush = 1.0;
    }
    for mut sink in &mut sinks {
        *fade_in = (*fade_in + time.delta_secs() / 2.0).min(1.0);
        let level = MUSIC_LEVEL * master * *fade_in * (1.0 - *hush);
        sink.set_volume(Volume::Linear(level));
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
    camera: Query<'w, 's, &'static GlobalTransform, With<Camera3d>>,
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
            "miami",
            "factory_town",
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
    fn source_streams_stereo() {
        let bank = Arc::new(Bank::build());
        let ctl = Arc::new(MusicControl::default());
        ctl.set(Mood::named("Village"), 0.6);
        let mut src = FusionSource::new(bank, ctl);
        assert_eq!(src.channels().get(), 2);
        assert_eq!(src.sample_rate().get(), RATE);
        let n = (0..RATE as usize).filter_map(|_| src.next()).count();
        assert_eq!(n, RATE as usize);
    }
}
