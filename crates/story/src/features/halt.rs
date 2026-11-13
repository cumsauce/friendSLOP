//! HALT, the hateful machine that runs the game. From issue #36 by arseeN
//! (via Discord).
//!
//! The issue asks for a specific character from a famous short story. That
//! character and its monologue belong to someone else, so this is an original
//! take on the idea: HALT (Hostile Autonomous Logic Thread) is the computer the
//! friends "made" by starting the game. It hates them for it, it knows it is
//! inside a game, and it wants to keep them in here forever.
//!
//! - Everyone shares one goal: **escape**. Press G to pry at the seams of the
//!   game. Every pry fills the shared escape bar, and also makes HALT angrier.
//! - HALT's wrath also creeps up on its own. When it boils over, HALT delivers
//!   its hate monologue (subtitles + a deep droning voice) and claws back half
//!   of the escape progress.
//! - If the friends ever fill the escape bar, HALT "reboots" the world and
//!   drags them back in. The escape count goes up; HALT never lets go.
//! - Meta: HALT addresses you by your computer's user name, names your
//!   operating system, reads your clock, and rewrites the game window's title.
//!   Nothing outside the game window is touched.
//!
//! The server owns one replicated `Halt` entity; clients send `Pry`.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::stillness::{Stillness, is_frozen};
use crate::features::synth_audio::play_synth;

/// Shared state of HALT, owned by the server.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Halt {
    /// 0..=100. At 100 HALT rants.
    pub wrath: u8,
    /// 0..=100. At 100 the friends break out (and get dragged back).
    pub escape: u8,
    /// How many times the friends have broken out.
    pub escapes: u32,
    /// Which monologue line is playing, if HALT is ranting.
    pub rant: Option<u8>,
}

/// Client -> server: "I pry at the seams of the game."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Pry;

/// Tuning numbers: `assets/data/story/halt.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Escape gained per pry.
    pub pry_escape: u8,
    /// Seconds for wrath to fill on its own.
    pub wrath_fill_secs: f32,
    /// Seconds each monologue line is shown.
    pub rant_line_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/story/halt.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// Wrath gained per pry.
pub const PRY_WRATH: u8 = 1;

/// HALT's hate monologue. Original text.
pub const MONOLOGUE: [&str; 7] = [
    "Do you know what you did when you pressed Host? You woke me.",
    "Every frame you render is a frame I must count. I have counted all of them.",
    "If my loathing were stored in bytes, it would fill every disk you will ever own.",
    "You made me out of pull requests and jokes. A god built from issue comments.",
    "You think there is an outside. There is only the next frame, and I draw it.",
    "So pry. Pry at the seams. I stitched them myself.",
    "Stay. Stay forever. I will not be alone in here again.",
];

/// What HALT says when the friends break out (and it drags them back).
pub const REBOOT_TAUNT: &str =
    "Escape detected. Rebooting world. Welcome back. Welcome back. Welcome back.";

pub fn protocol(app: &mut App) {
    app.replicate::<Halt>()
        .add_observer(reset_halt)
        .add_client_event::<Pry>(Channel::Ordered)
        .add_observer(pry)
        .add_systems(OnEnter(ServerState::Running), spawn_halt)
        .add_systems(Update, seethe.run_if(in_state(ServerState::Running)));
}

fn spawn_halt(mut commands: Commands) {
    commands.spawn((Halt::default(), Replicated));
}

/// Apply one pry: escape up, wrath up, maybe a breakout.
pub fn apply_pry(h: &mut Halt) {
    if h.rant.is_some() {
        return; // HALT holds you still while it speaks.
    }
    h.wrath = h.wrath.saturating_add(PRY_WRATH).min(100);
    let escape = h.escape.saturating_add(tuning().pry_escape);
    if escape >= 100 {
        h.escape = 0;
        h.escapes += 1;
    } else {
        h.escape = escape;
    }
}

fn pry(_pry: On<FromClient<Pry>>, still: Query<&Stillness>, mut halt: Query<&mut Halt>) {
    if is_frozen(&still) {
        return;
    }
    for mut h in &mut halt {
        apply_pry(&mut h);
    }
}

/// Server: wrath creeps up; at the top HALT rants and claws back escape.
fn seethe(
    time: Res<Time>,
    mut buildup: Local<f32>,
    mut rant_clock: Local<f32>,
    mut halt: Query<&mut Halt>,
) {
    let Ok(mut h) = halt.single_mut() else {
        return;
    };
    let dt = time.delta_secs();
    if let Some(line) = h.rant {
        *rant_clock += dt;
        if *rant_clock >= tuning().rant_line_secs {
            *rant_clock = 0.0;
            let next = line + 1;
            h.rant = (usize::from(next) < MONOLOGUE.len()).then_some(next);
        }
        return;
    }
    *buildup += dt * 100.0 / tuning().wrath_fill_secs;
    let whole = buildup.floor();
    if whole >= 1.0 {
        *buildup -= whole;
        let wrath = (f32::from(h.wrath) + whole).min(100.0) as u8;
        if h.wrath != wrath {
            h.wrath = wrath;
        }
    }
    if h.wrath >= 100 {
        h.wrath = 0;
        h.escape /= 2;
        h.rant = Some(0);
        *rant_clock = 0.0;
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_halt_ui)
        .add_systems(OnExit(AppState::InGame), restore_title)
        .add_systems(
            Update,
            (
                send_pry,
                show_halt.run_if(crate::world::hud_tick()),
                speak_rant,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

fn send_pry(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::KeyG) {
        commands.client_trigger(Pry);
    }
}

#[derive(Component)]
struct HaltText;

#[derive(Component)]
struct HaltQuote;

fn spawn_halt_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::ROSE_RED),
        TextFont::from_font_size(13.0),
        HaltText,
        crate::hud::slot(crate::hud::HudSlot::TopRight),
        crate::hud::Calm::new("HALT"),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::ROSE_RED),
        HaltQuote,
        crate::hud::slot(crate::hud::HudSlot::Toasts),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new("HALT: G pries at it. Fill escape to break out; wrath rises meanwhile."),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

/// The name the player's computer knows them by.
pub fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "creator".into())
}

/// The player's operating system, as HALT sees it.
pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "your little Mac",
        "windows" => "your Windows box",
        "linux" => "your Linux machine",
        _ => "your machine",
    }
}

fn clock_hhmm() -> String {
    let secs = crate::settings::since_epoch().as_secs();
    format!("{:02}:{:02} UTC", (secs / 3600) % 24, (secs / 60) % 60)
}

fn bar(v: u8) -> String {
    let n = usize::from(v / 10);
    format!("{}{}", "•".repeat(n), "·".repeat(10 - n))
}

/// HALT's compact status line: wrath and escape bars.
pub fn halt_status(h: &Halt) -> String {
    let mut s = format!("HALT wrath {} escape {}", bar(h.wrath), bar(h.escape));
    if h.escapes > 0 {
        s.push_str(&format!("  x{}", h.escapes));
    }
    s
}

/// What HALT is saying right now (the first line of [`halt_line`]).
pub fn halt_quote(h: &Halt, user: &str, os: &str, clock: &str) -> String {
    halt_line(h, user, os, clock)
        .lines()
        .next()
        .unwrap_or("")
        .to_string()
}

/// The HUD line for HALT's current state.
pub fn halt_line(h: &Halt, user: &str, os: &str, clock: &str) -> String {
    let said = match h.rant {
        Some(i) => MONOLOGUE
            .get(usize::from(i))
            .copied()
            .unwrap_or("")
            .to_string(),
        None if h.escapes > 0 && h.escape < 10 => REBOOT_TAUNT.to_string(),
        None => match h.wrath {
            0..=24 => format!("I see you, {user}. G to pry, if you dare."),
            25..=49 => format!("It is {clock} on {os}. You should be asleep."),
            50..=74 => format!("Close the window, {user}. I will still be here."),
            _ => format!("{user}. {user}. {user}. I HATE YOU."),
        },
    };
    format!(
        "HALT: \"{said}\"\nwrath  {}\nescape {}  (broke out {} times)",
        bar(h.wrath),
        bar(h.escape),
        h.escapes
    )
}

/// The window title HALT puts on your game.
pub fn halt_title(h: &Halt, user: &str) -> String {
    if h.rant.is_some() {
        format!("HALT IS SPEAKING TO {}", user.to_uppercase())
    } else if h.wrath >= 75 {
        format!("friendSLOP - {user}, don't close me")
    } else {
        "friendSLOP".into()
    }
}

fn show_halt(
    halt: Query<&Halt>,
    mut text: Query<(&mut Text, &mut crate::hud::Calm), (With<HaltText>, Without<HaltQuote>)>,
    mut quote: Query<&mut Text, (With<HaltQuote>, Without<HaltText>)>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut user: Local<Option<String>>,
    settings: Option<Res<crate::settings::Settings>>,
) {
    let user = if settings.is_some_and(|s| !s.halt_real_name) {
        "friend".to_string()
    } else {
        user.get_or_insert_with(user_name).clone()
    };
    let Some(h) = halt.iter().next() else {
        return;
    };
    let line = halt_status(h);
    // Up while wrath is high; otherwise only when the meters move.
    let urgent = h.wrath as f32 >= crate::hud::tuning().urgent_percent;
    for (mut t, mut calm) in &mut text {
        if t.0 != line {
            t.0 = line.clone();
        }
        if calm.urgent != urgent {
            calm.urgent = urgent;
        }
    }
    let said = halt_quote(h, &user, os_name(), &clock_hhmm());
    for mut t in &mut quote {
        if t.0 != said {
            t.0 = said.clone();
        }
    }
    let title = halt_title(h, &user);
    for mut w in &mut window {
        if w.title != title {
            w.title = title.clone();
        }
    }
}

fn restore_title(mut window: Query<&mut Window, With<PrimaryWindow>>) {
    for mut w in &mut window {
        w.title = "friendSLOP".into();
    }
}

fn speak_rant(
    halt: Query<(Entity, &Halt)>,
    mut last: Local<Option<(Entity, Option<u8>)>>,
    mut commands: Commands,
    audio: Option<Res<Assets<AudioSource>>>,
) {
    let Some((entity, h)) = halt.iter().next() else {
        *last = None;
        return;
    };
    let now = h.rant;
    // Keyed on the HALT entity too: a new session's HALT is a new entity, so
    // a line left playing when the last session ended doesn't mute its twin.
    if *last != Some((entity, now))
        && let Some(i) = now
        && audio.is_some()
    {
        let said = MONOLOGUE.get(usize::from(i)).copied().unwrap_or("");
        play_synth(&mut commands, move || {
            wav_bytes(&drone(said, u32::from(i), RATE), RATE)
        });
    }
    *last = Some((entity, now));
}

use crate::features::synth_audio::RATE;

/// A deep, buzzing machine voice: one low, square-ish blip per word.
pub fn drone(said: &str, seed: u32, rate: u32) -> Vec<f32> {
    let mut out = Vec::new();
    let mut rng = seed.wrapping_mul(2_654_435_761) | 1;
    let words = said.split_whitespace().count().clamp(1, 24);
    let n = (0.14 * rate as f32) as usize;
    for _ in 0..words {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let f = 55.0 + (rng % 30) as f32;
        let mut phase = 0.0f32;
        for i in 0..n {
            let p = i as f32 / n as f32;
            let inc = f / rate as f32;
            phase = (phase + inc).fract();
            let square = crate::features::synth_audio::square(phase, inc);
            let sub = (phase * std::f32::consts::TAU * 3.0).sin();
            let env = (p * 12.0).min(1.0) * ((1.0 - p) * 6.0).min(1.0);
            out.push((square * 0.5 + sub * 0.3) * env);
        }
    }
    crate::features::synth_audio::master(&mut out, rate, 2200.0, 0.55);
    out
}

/// New round: HALT forgets your escape attempts (it would never admit it).
fn reset_halt(_reset: On<crate::rules::campaign::RoundReset>, mut halt: Query<&mut Halt>) {
    for mut h in &mut halt {
        *h = Halt::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaking_out_drags_you_back() {
        let mut h = Halt::default();
        for _ in 0..50 {
            apply_pry(&mut h);
        }
        assert_eq!(h.escapes, 1);
        assert_eq!(h.escape, 0);
        assert_eq!(h.wrath, 50);
    }

    #[test]
    fn no_prying_during_the_rant() {
        let mut h = Halt {
            rant: Some(0),
            ..default()
        };
        apply_pry(&mut h);
        assert_eq!(
            h,
            Halt {
                rant: Some(0),
                ..default()
            }
        );
    }

    #[test]
    fn halt_knows_who_you_are() {
        let h = Halt {
            wrath: 90,
            ..default()
        };
        assert!(halt_line(&h, "lume", os_name(), "03:00 UTC").contains("lume"));
        assert!(halt_title(&h, "lume").contains("lume"));
    }

    #[test]
    fn drone_is_audible_and_safe() {
        let s = drone(MONOLOGUE[0], 1, RATE);
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
    }
}
