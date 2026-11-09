//! Punishment: get caught farting too often and the commune turns on you.
//! From issue #123 by Kori.exe.
//!
//! The server counts how many times each player has been caught farting by
//! an NPC (every `Farted { shamed: true }`, see `fart.rs`) in a replicated
//! [`Strikes`] on their `Player`. On the [`STRIKES_TO_ATTACK`]th catch the
//! count resets and the server spawns a replicated [`AttackMode`] for
//! [`ATTACK_SECS`]: every villager (the walking NPCs) drops what it's doing
//! and chases that player's body (see `villagers::think`). While it lasts,
//! everyone hears high-pitched, frantic string music (generated here; the
//! band goes quiet underneath) and the chasing villagers jingle like a pocket
//! full of keys. Catches don't count while you're already being chased.

use bevy::audio::AudioSource;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fart::Farted;
use crate::features::fps::Walker;
use crate::features::synth_audio::play_synth;
use crate::features::villagers::Villager;
use crate::player::{LocalPlayer, Player};

/// Catches that set the commune on you.
pub const STRIKES_TO_ATTACK: u8 = 4;
/// How long attack mode lasts.
pub const ATTACK_SECS: f32 = 60.0;
/// How fast villagers chase (walking is 5 m/s, so you can just stay ahead).
pub const CHASE_SPEED: f32 = 4.0;
/// Chasers stop this close to you (they crowd you rather than stack up).
pub const CROWD_GAP: f32 = 1.1;

/// Times this player has been caught farting since the last attack.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Strikes(pub u8);

/// Server -> everyone: the villagers are after `target` for `secs_left` more seconds.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackMode {
    pub target: u64,
    pub secs_left: u8,
}

/// Server only: the precise time left on an attack.
#[derive(Component, Debug)]
struct AttackClock(f32);

pub fn protocol(app: &mut App) {
    app.replicate::<Strikes>()
        .replicate::<AttackMode>()
        .add_observer(give_strikes)
        .add_observer(count_catch)
        .add_systems(Update, run_attacks.run_if(in_state(ServerState::Running)));
}

fn give_strikes(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Strikes::default());
    }
}

/// Server: a shameful fart is a strike; the last strike sets the commune on you.
fn count_catch(
    farted: On<Farted>,
    server: Res<State<ServerState>>,
    mut commands: Commands,
    mut players: Query<(&Player, &mut Strikes)>,
    attacks: Query<&AttackMode>,
    odds: Query<&crate::features::gamble::HouseOdds>,
) {
    if *server.get() != ServerState::Running || !farted.shamed {
        return;
    }
    if attacks.iter().any(|a| a.target == farted.player) {
        return; // already being punished
    }
    let Some((_, mut strikes)) = players.iter_mut().find(|(p, _)| p.id == farted.player) else {
        return;
    };
    strikes.0 += 1;
    // Marmalade's wheel (#173) sets how patient the commune is.
    if strikes.0 < crate::features::gamble::odds(&odds).strikes_to_attack() {
        return;
    }
    strikes.0 = 0;
    start_attack(&mut commands, farted.player);
}

/// Server: set the commune on `target` for a full attack.
pub fn start_attack(commands: &mut Commands, target: u64) {
    commands.spawn((
        AttackMode {
            target,
            secs_left: ATTACK_SECS as u8,
        },
        AttackClock(ATTACK_SECS),
        Replicated,
    ));
}

/// Server: count attacks down (paused during the stillness, like the NPCs).
fn run_attacks(
    time: Res<Time>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut attacks: Query<(Entity, &mut AttackMode, &mut AttackClock)>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    for (entity, mut attack, mut clock) in &mut attacks {
        clock.0 -= time.delta_secs();
        if clock.0 <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let secs = clock.0.ceil() as u8;
        if attack.secs_left != secs {
            attack.secs_left = secs;
        }
    }
}

/// Where the hunted players' bodies are (x, z), for the villager brains.
#[derive(SystemParam)]
pub struct Hunted<'w, 's> {
    attacks: Query<'w, 's, &'static AttackMode>,
    walkers: Query<'w, 's, (&'static Player, &'static Walker)>,
}

impl Hunted<'_, '_> {
    pub fn spots(&self) -> Vec<Vec2> {
        self.attacks
            .iter()
            .filter_map(|a| {
                self.walkers
                    .iter()
                    .find(|(p, _)| p.id == a.target)
                    .map(|(_, w)| w.pos.xz())
            })
            .collect()
    }
}

/// One chase step for a villager at `pos`: run at the nearest of `prey`.
/// Returns the new position and the way to face, or `None` if nobody is hunted.
pub fn chase_step(pos: Vec2, prey: &[Vec2], dt: f32) -> Option<(Vec2, Vec2)> {
    let target = prey
        .iter()
        .copied()
        .min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)))?;
    let to = target - pos;
    let d = to.length();
    if d < 1e-4 {
        return Some((pos, Vec2::NEG_Y));
    }
    let dir = to / d;
    let step = (CHASE_SPEED * dt).min((d - CROWD_GAP).max(0.0));
    Some((pos + dir * step, dir))
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Punishment>()
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(OnExit(AppState::InGame), stop_all)
        .add_systems(
            Update,
            (
                attack_music,
                jingle_keys,
                show_attack.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// This peer's attack-mode sounds.
#[derive(Resource, Default)]
struct Punishment {
    /// The attack it's playing music for, and the music's entity.
    music: Option<(Entity, Entity)>,
    /// A few jingles to pick from, made once.
    jingles: Vec<Handle<AudioSource>>,
    /// Seconds until each villager jingles again.
    next_jingle: [f32; super::villagers::COUNT],
    dice: u32,
}

impl Punishment {
    fn roll(&mut self) -> f32 {
        self.dice = self
            .dice
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.dice >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn stop_all(mut commands: Commands, mut p: ResMut<Punishment>) {
    if let Some((_, music)) = p.music.take() {
        commands.entity(music).try_despawn();
    }
}

#[derive(Component)]
struct AttackText;

#[derive(Component)]
struct StrikesText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::ROSE_RED),
        AttackText,
        crate::hud::slot(crate::hud::HudSlot::Toasts),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        StrikesText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(format!(
            "punishment: get caught farting {STRIKES_TO_ATTACK} times and the villagers \
             chase you for {} s.",
            ATTACK_SECS as u32
        )),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Is any attack on? The band goes quiet under the strings.
pub fn attack_on(attacks: &Query<&AttackMode>) -> bool {
    !attacks.is_empty()
}

/// Start the strings when an attack begins (for as long as it has left, so
/// late joiners hear the end of it), stop them when it ends.
fn attack_music(
    mut commands: Commands,
    mut p: ResMut<Punishment>,
    attacks: Query<(Entity, &AttackMode)>,
    audio: Option<Res<Assets<AudioSource>>>,
) {
    let current = attacks.iter().next();
    let playing_for = p.music.map(|(a, _)| a);
    if current.map(|(e, _)| e) == playing_for {
        return;
    }
    if let Some((_, music)) = p.music.take() {
        commands.entity(music).try_despawn();
    }
    let Some((entity, attack)) = current else {
        return;
    };
    if audio.is_none() {
        return;
    }
    let secs = attack.secs_left.max(1) as f32;
    let music = play_synth(&mut commands, move || {
        wav_bytes(&attack_song(SONG_RATE, secs), SONG_RATE)
    });
    p.music = Some((entity, music));
}

/// Every chasing villager jingles like a pocket full of keys, now and then.
fn jingle_keys(
    mut commands: Commands,
    time: Res<Time>,
    mut p: ResMut<Punishment>,
    attacks: Query<&AttackMode>,
    villagers: Query<&Villager>,
    audio: Option<ResMut<Assets<AudioSource>>>,
) {
    if attacks.is_empty() {
        return;
    }
    let Some(mut audio) = audio else { return };
    if p.jingles.is_empty() {
        p.jingles = (0..4)
            .map(|i| {
                audio.add(AudioSource {
                    bytes: wav_bytes(&jingle(JINGLE_RATE, 0x6e7 + i * 977), JINGLE_RATE).into(),
                })
            })
            .collect();
    }
    let dt = time.delta_secs();
    for v in &villagers {
        let i = v.id as usize % super::villagers::COUNT;
        p.next_jingle[i] -= dt;
        if p.next_jingle[i] > 0.0 {
            continue;
        }
        p.next_jingle[i] = 0.45 + p.roll() * 0.7;
        let pick = (p.roll() * p.jingles.len() as f32) as usize % p.jingles.len();
        let at = Vec3::new(
            v.pos.x,
            crate::terrain::height_at(v.pos.x, v.pos.y) + 0.9,
            v.pos.y,
        );
        let sound = p.jingles[pick].clone();
        crate::spatial::play_at(
            &mut commands,
            &sound,
            at,
            0.55,
            crate::spatial::Carry::NEARBY,
        );
    }
}

fn show_attack(
    me: Option<Res<LocalPlayer>>,
    attacks: Query<&AttackMode>,
    players: Query<(&Player, &Strikes)>,
    mut attack_text: Query<&mut Text, (With<AttackText>, Without<StrikesText>)>,
    mut strikes_text: Query<&mut Text, (With<StrikesText>, Without<AttackText>)>,
    odds: Query<&crate::features::gamble::HouseOdds>,
) {
    let me = me.map(|m| m.0);
    let want = attacks
        .iter()
        .map(|a| {
            if Some(a.target) == me {
                format!(
                    "ATTACK MODE: the whole commune is after YOU. Run! ({})",
                    a.secs_left
                )
            } else {
                format!(
                    "ATTACK MODE: the commune is chasing player {} ({})",
                    a.target, a.secs_left
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    for mut text in &mut attack_text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
    let mine = players
        .iter()
        .find(|(p, _)| Some(p.id) == me)
        .map_or(0, |(_, s)| s.0);
    let want = if mine > 0 {
        format!(
            "caught farting {mine}/{}",
            crate::features::gamble::odds(&odds).strikes_to_attack()
        )
    } else {
        String::new()
    };
    for mut text in &mut strikes_text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

const SONG_RATE: u32 = 22_050;
const JINGLE_RATE: u32 = 32_000;

/// Band-limited sawtooth (polyBLEP), phase 0..1, `dt` = freq / rate.
fn saw(phase: f32, dt: f32) -> f32 {
    let mut v = 2.0 * phase - 1.0;
    if phase < dt {
        let t = phase / dt;
        v -= t + t - t * t - 1.0;
    } else if phase > 1.0 - dt {
        let t = (phase - 1.0) / dt;
        v -= t * t + t + t + 1.0;
    }
    v
}

/// How a note is bowed.
#[derive(Clone, Copy)]
enum Bow {
    /// Short, bouncing 16ths.
    Spiccato,
    /// A hard, shrieking down-bow stab.
    Stab,
    /// A sustained, trembling note that swells.
    Tremolo,
}

/// Add one bowed string note (a small section: two detuned saws, vibrato
/// and a little bow noise) into `out`.
fn bow_note(out: &mut [f32], rate: u32, start: f32, len: f32, midi: f32, vel: f32, bow: Bow) {
    let r = rate as f32;
    let first = (start * r) as usize;
    let tail = match bow {
        Bow::Spiccato => 0.05,
        Bow::Stab => 0.12,
        Bow::Tremolo => 0.2,
    };
    let n = ((len + tail) * r) as usize;
    let base = 440.0 * 2f32.powf((midi - 69.0) / 12.0);
    let (mut p1, mut p2) = (0.0f32, 0.37f32);
    let mut noise = (midi as u32).wrapping_mul(2_654_435_761) | 1;
    for k in 0..n {
        let Some(slot) = out.get_mut(first + k) else {
            break;
        };
        let t = k as f32 / r;
        let env = match bow {
            Bow::Spiccato => (t / 0.006).min(1.0) * (-t / 0.045).exp(),
            Bow::Stab => (t / 0.004).min(1.0) * (0.35 + 0.65 * (-t / 0.05).exp()),
            Bow::Tremolo => {
                let swell = (t / len).clamp(0.0, 1.0);
                let shake = 0.55 + 0.45 * (t * std::f32::consts::TAU * 13.0).sin().abs();
                (t / 0.08).min(1.0) * (0.3 + 0.7 * swell) * shake
            }
        };
        let release = if t > len {
            (1.0 - (t - len) / tail).max(0.0)
        } else {
            1.0
        };
        let vib_depth = 0.006 * ((t - 0.08) / 0.2).clamp(0.0, 1.0);
        let vib = 1.0 + vib_depth * (t * std::f32::consts::TAU * 6.2).sin();
        let f1 = base * vib;
        let f2 = base * vib * 1.004;
        p1 = (p1 + f1 / r).fract();
        p2 = (p2 + f2 / r).fract();
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        let hiss = (noise as f32 / u32::MAX as f32 - 0.5) * 0.12;
        let tone = 0.5 * (saw(p1, f1 / r) + saw(p2, f2 / r)) + hiss;
        *slot += tone * env * release * vel;
    }
}

/// The attack music: `secs` of high, frantic strings. Bouncing chromatic
/// 16ths up top that climb a semitone every four bars, shrieking
/// minor-second stabs, and a trembling cluster that swells in later, all
/// ending on one long scream.
pub fn attack_song(rate: u32, secs: f32) -> Vec<f32> {
    const BPM: f32 = 152.0;
    let sixteenth = 60.0 / BPM / 4.0;
    let bar = sixteenth * 16.0;
    // Semitones above A5: a nervous, chromatic figure.
    const FIGURE: [f32; 16] = [
        0.0, 1.0, 0.0, 4.0, 0.0, 1.0, 7.0, 6.0, 0.0, 1.0, 0.0, 4.0, 3.0, 4.0, 7.0, 8.0,
    ];
    const STABS: [usize; 5] = [0, 3, 6, 10, 12];
    let n = (secs * rate as f32) as usize;
    let mut out = vec![0.0f32; n];
    let end = (secs - 1.5).max(0.0);
    let bars = (end / bar).ceil() as usize;
    for b in 0..bars {
        let t0 = b as f32 * bar;
        let climb = ((b / 4) % 6) as f32;
        let drive = (b as f32 / bars.max(1) as f32).min(1.0);
        for (s, step) in FIGURE.iter().enumerate() {
            let at = t0 + s as f32 * sixteenth;
            if at >= end {
                break;
            }
            let accent = if s % 4 == 0 { 1.0 } else { 0.7 };
            bow_note(
                &mut out,
                rate,
                at,
                sixteenth * 0.6,
                81.0 + climb + step,
                0.32 * accent * (0.8 + 0.2 * drive),
                Bow::Spiccato,
            );
        }
        if b % 2 == 1 || b >= bars / 2 {
            for s in STABS {
                let at = t0 + s as f32 * sixteenth;
                if at >= end {
                    break;
                }
                for midi in [93.0 + climb, 94.0 + climb] {
                    bow_note(&mut out, rate, at, 0.14, midi, 0.22, Bow::Stab);
                }
            }
        }
        if b >= 4 && b % 4 == 0 {
            let len = (bar * 4.0).min(end - t0);
            for midi in [88.0 + climb, 89.0 + climb] {
                bow_note(&mut out, rate, t0, len, midi, 0.18, Bow::Tremolo);
            }
        }
    }
    // The scream: everyone saws at the top for the last stretch.
    for midi in [96.0, 97.0, 101.0] {
        bow_note(&mut out, rate, end, 1.3, midi, 0.2, Bow::Tremolo);
    }
    // Take the fizz off the top, add a little room, then soft-clip.
    let (mut lp1, mut lp2) = (0.0f32, 0.0f32);
    let a = 1.0 - (-std::f32::consts::TAU * 5_500.0 / rate as f32).exp();
    for x in out.iter_mut() {
        lp1 += a * (*x - lp1);
        lp2 += a * (lp1 - lp2);
        *x = lp2;
    }
    let echo = (0.11 * rate as f32) as usize;
    for i in echo..n {
        out[i] += out[i - echo] * 0.28;
    }
    let fade_in = (0.03 * rate as f32).max(1.0);
    let fade_out = (0.2 * rate as f32).max(1.0);
    for (i, x) in out.iter_mut().enumerate() {
        let fade = (i as f32 / fade_in).min(1.0) * ((n - i) as f32 / fade_out).min(1.0);
        *x = ((*x * 1.4).tanh() * 0.5 * fade).clamp(-1.0, 1.0);
    }
    out
}

/// A jingle of keys: a handful of little metal clinks, each a few
/// inharmonic partials that ring for a moment.
pub fn jingle(rate: u32, seed: u32) -> Vec<f32> {
    let r = rate as f32;
    let n = (0.45 * r) as usize;
    let mut out = vec![0.0f32; n];
    let mut s = seed | 1;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s as f32 / u32::MAX as f32
    };
    const PARTIALS: [f32; 4] = [1.0, 1.47, 2.09, 2.76];
    for _ in 0..7 {
        let start = (rnd() * 0.3 * r) as usize;
        let base = 2_300.0 + rnd() * 2_000.0;
        let vel = 0.3 + rnd() * 0.7;
        let decay = 0.02 + rnd() * 0.04;
        for (j, mult) in PARTIALS.iter().enumerate() {
            let f = base * mult;
            if f > r * 0.45 {
                continue;
            }
            let phase0 = rnd();
            let amp = vel / (1.0 + j as f32);
            for k in 0..((decay * 5.0 * r) as usize) {
                let Some(slot) = out.get_mut(start + k) else {
                    break;
                };
                let t = k as f32 / r;
                let env = (-t / (decay / (1.0 + 0.4 * j as f32))).exp();
                *slot += amp * env * (std::f32::consts::TAU * (f * t + phase0)).sin();
            }
        }
    }
    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-6);
    for x in out.iter_mut() {
        *x = *x / peak * 0.7;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chasers_run_at_the_nearest_hunted_player_and_crowd_them() {
        assert_eq!(chase_step(Vec2::ZERO, &[], 0.2), None);
        let prey = [Vec2::new(10.0, 0.0), Vec2::new(-30.0, 0.0)];
        let (p, dir) = chase_step(Vec2::ZERO, &prey, 0.2).unwrap_or_default();
        assert!((p - Vec2::new(CHASE_SPEED * 0.2, 0.0)).length() < 1e-4);
        assert!((dir - Vec2::X).length() < 1e-4);
        // Close up, they stop short instead of standing on you.
        let (p, _) = chase_step(Vec2::new(9.5, 0.0), &prey, 0.2).unwrap_or_default();
        assert!((p.x - 9.5).abs() < 1e-4, "{p}");
    }

    #[test]
    fn attack_song_is_as_long_as_asked_and_sane() {
        let s = attack_song(SONG_RATE, 8.0);
        assert_eq!(s.len(), 8 * SONG_RATE as usize);
        assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        let rms = (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        assert!(rms > 0.02, "rms {rms}");
    }

    #[test]
    fn jingles_ring_and_differ() {
        let a = jingle(JINGLE_RATE, 1);
        let b = jingle(JINGLE_RATE, 2);
        assert!(a.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        assert!(a.iter().any(|x| x.abs() > 0.5));
        assert_ne!(a, b);
    }
}
