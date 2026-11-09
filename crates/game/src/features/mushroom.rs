//! A voice-altering mushroom. From issue #6 by corvy.
//!
//! The server keeps one replicated `Mushroom` somewhere on the floor. Click it
//! to eat it: the server picks a random "much higher" or "much lower" pitch
//! and tells only the eater, whose own outgoing voice then runs through a
//! small pitch shifter (so everyone hears the squeaky or deep version) for a
//! while. A new mushroom sprouts somewhere else a little later.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_voice_chat::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::assets::{Sfx, play_sfx};
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, in_reach};
use crate::features::morality::{Act, MoralAct};
use crate::player::PlayerOwner;

/// A mushroom on the floor at (x, z).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Mushroom {
    pub at: Vec2,
}

/// Client -> server: "I'm eating the mushroom."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct EatMushroom;

/// Server -> the eater only: your voice is now `pitch` times its normal pitch
/// for `secs` seconds.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct MushroomVoice {
    pub pitch: f32,
    pub secs: f32,
}

/// How long a mushroom's effect lasts.
pub const EFFECT_SECS: f32 = 45.0;
/// How long until a new mushroom sprouts after one is eaten.
const REGROW_SECS: f32 = 10.0;

pub fn protocol(app: &mut App) {
    app.replicate::<Mushroom>()
        .add_client_event::<EatMushroom>(Channel::Ordered)
        .add_server_event::<MushroomVoice>(Channel::Ordered)
        .init_resource::<MushroomPatch>()
        .add_observer(eat_mushroom)
        .add_systems(OnEnter(ServerState::Running), sprout_first)
        .add_systems(OnExit(ServerState::Running), clear_patch)
        .add_systems(Update, regrow.run_if(in_state(ServerState::Running)));
}

/// Server only: a tiny deterministic random source and the regrow timer.
#[derive(Resource, Default)]
struct MushroomPatch {
    seed: u32,
    regrow_in: Option<f32>,
    /// The mushroom eaten this tick (its despawn is still queued).
    eaten: Option<Entity>,
}

impl MushroomPatch {
    fn next(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    fn spot(&mut self) -> Vec2 {
        let angle = self.next() * std::f32::consts::TAU;
        let dist = 3.0 + self.next() * 9.0;
        Vec2::from_angle(angle) * dist
    }
}

/// Much higher (1.5x-1.9x) or much lower (0.55x-0.7x), at random.
pub fn pick_pitch(coin: f32, amount: f32) -> f32 {
    if coin < 0.5 {
        1.5 + amount * 0.4
    } else {
        0.55 + amount * 0.15
    }
}

/// Session over: no mushroom (even one sprouted in the very last frame) or
/// half-run regrow timer may carry into the next one.
fn clear_patch(
    mut commands: Commands,
    mut patch: ResMut<MushroomPatch>,
    mushrooms: Query<Entity, With<Mushroom>>,
) {
    for e in &mushrooms {
        commands.entity(e).despawn();
    }
    patch.regrow_in = None;
    patch.eaten = None;
}

fn sprout_first(mut commands: Commands, mut patch: ResMut<MushroomPatch>) {
    let at = patch.spot();
    patch.eaten = None;
    commands.spawn((Mushroom { at }, Replicated));
}

fn regrow(
    time: Res<Time>,
    mut commands: Commands,
    mut patch: ResMut<MushroomPatch>,
    mushrooms: Query<(), With<Mushroom>>,
) {
    let Some(left) = patch.regrow_in else { return };
    let left = left - time.delta_secs();
    if left > 0.0 {
        patch.regrow_in = Some(left);
        return;
    }
    patch.regrow_in = None;
    if mushrooms.is_empty() {
        let at = patch.spot();
        patch.eaten = None;
        commands.spawn((Mushroom { at }, Replicated));
    }
}

/// Server: whoever asks first gets the mushroom.
fn eat_mushroom(
    eat: On<FromClient<EatMushroom>>,
    mut commands: Commands,
    mut patch: ResMut<MushroomPatch>,
    mushrooms: Query<(Entity, &Mushroom)>,
    players: Query<(Entity, &PlayerOwner)>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    still: Query<&super::stillness::Stillness>,
    mut fate: super::fate::Fate,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Some((mushroom, m)) = mushrooms.iter().next() else {
        return;
    };
    // The despawn below is deferred: remember the eaten one now so a second
    // click in the same tick can't eat (and score) it again.
    if patch.eaten == Some(mushroom) {
        return;
    }
    if !in_reach(
        &mut commands,
        eat.client_id,
        &walkers,
        Vec3::new(m.at.x, 0.0, m.at.y),
    ) {
        return;
    }
    if fate.fails(eat.client_id) {
        return;
    }
    commands.entity(mushroom).despawn();
    patch.eaten = Some(mushroom);
    MoralAct::report(&mut commands, &players, eat.client_id, Act::EatMushroom);
    let (coin, amount) = (patch.next(), patch.next());
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(eat.client_id),
        message: MushroomVoice {
            pitch: pick_pitch(coin, amount),
            secs: EFFECT_SECS,
        },
    });
    patch.regrow_in = Some(REGROW_SECS);
}

pub fn plugin(app: &mut App) {
    app.init_resource::<MushroomHigh>()
        .add_observer(click_mushroom)
        .add_observer(feel_mushroom)
        .add_systems(Startup, install_pitch_shifter)
        .add_systems(OnEnter(AppState::InGame), spawn_mushroom_text)
        .add_systems(OnExit(AppState::InGame), sober_up)
        .add_systems(
            Update,
            (dress_mushrooms, wear_off).run_if(in_state(AppState::InGame)),
        );
}

/// This player's own mushroom effect.
#[derive(Resource, Default, Debug)]
struct MushroomHigh {
    pitch: f32,
    remaining: f32,
}

fn install_pitch_shifter(filters: Option<ResMut<VoiceFilters>>) {
    let Some(mut filters) = filters else { return };
    filters.add(FilterStage::Outgoing, 50, PitchShifter::new(1.0));
}

fn feel_mushroom(
    voice: On<MushroomVoice>,
    mut high: ResMut<MushroomHigh>,
    mut commands: Commands,
    sfx: Option<Res<Sfx>>,
) {
    high.pitch = voice.pitch;
    high.remaining = voice.secs;
    if let Some(sfx) = sfx {
        // The server confirmed the bite: munch.
        play_sfx(&mut commands, &sfx.eat, 0.9);
    }
}

fn set_pitch(filters: Option<ResMut<VoiceFilters>>, pitch: f32) {
    let Some(mut filters) = filters else { return };
    if let Some(shifter) = filters.get_mut::<PitchShifter>() {
        shifter.ratio = pitch;
    }
}

fn wear_off(
    time: Res<Time>,
    mut high: ResMut<MushroomHigh>,
    filters: Option<ResMut<VoiceFilters>>,
    mut text: Query<&mut Text, With<MushroomText>>,
    mut applied: Local<Option<f32>>,
) {
    high.remaining = (high.remaining - time.delta_secs()).max(0.0);
    let pitch = if high.remaining > 0.0 {
        high.pitch
    } else {
        1.0
    };
    // Only touch the voice filter chain when the pitch actually changes.
    if *applied != Some(pitch) {
        set_pitch(filters, pitch);
        *applied = Some(pitch);
    }
    let want = if high.remaining <= 0.0 {
        String::new()
    } else {
        let how = if pitch > 1.0 {
            "MUCH higher"
        } else {
            "much lower"
        };
        format!("mushroom: your voice is {how} ({:.0}s)", high.remaining)
    };
    for mut text in &mut text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

fn sober_up(mut high: ResMut<MushroomHigh>, filters: Option<ResMut<VoiceFilters>>) {
    *high = MushroomHigh::default();
    set_pitch(filters, 1.0);
}

#[derive(Component)]
struct MushroomText;

fn spawn_mushroom_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        MushroomText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new("mushroom: click it to eat it and change your voice for a while"),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Give every mushroom (on every peer) a stalk and a spotty red cap.
fn dress_mushrooms(
    mut commands: Commands,
    new: Query<(Entity, &Mushroom), Added<Mushroom>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mushroom) in &new {
        let stalk = materials.add(Color::srgb(0.95, 0.92, 0.85));
        let cap = materials.add(Color::srgb(0.85, 0.12, 0.15));
        let spot_mesh = meshes.add(Sphere::new(0.05));
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(mushroom.at.x, 0.0, mushroom.at.y),
                Clickable::at_height(0.3),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Cylinder::new(0.08, 0.3))),
                    MeshMaterial3d(stalk.clone()),
                    Transform::from_xyz(0.0, 0.15, 0.0),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.25))),
                    MeshMaterial3d(cap),
                    Transform::from_xyz(0.0, 0.3, 0.0).with_scale(Vec3::new(1.0, 0.55, 1.0)),
                ));
                for i in 0..5 {
                    let a = i as f32 / 5.0 * std::f32::consts::TAU;
                    p.spawn((
                        Mesh3d(spot_mesh.clone()),
                        MeshMaterial3d(stalk.clone()),
                        Transform::from_xyz(a.cos() * 0.16, 0.39, a.sin() * 0.16),
                    ));
                }
            });
    }
}

/// The unified click landed on a mushroom: eat it.
fn click_mushroom(
    click: On<Clicked>,
    mut commands: Commands,
    mushrooms: Query<(), With<Mushroom>>,
) {
    if mushrooms.contains(click.entity) {
        commands.client_trigger(EatMushroom);
    }
}

/// A tiny real-time pitch shifter: two read heads sweep through a short delay
/// line at a different speed than it's written, crossfaded so the jumps back
/// are hidden. Cheap and a bit warbly, which suits a mushroom.
pub struct PitchShifter {
    /// 1.0 = unchanged, 2.0 = an octave up, 0.5 = an octave down.
    pub ratio: f32,
    buf: Vec<f32>,
    write: usize,
    /// Current delay of head A, in samples, within `0..WINDOW`.
    delay: f32,
}

const RING: usize = 2048;
const WINDOW: f32 = 800.0; // 50 ms at 16 kHz

impl PitchShifter {
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio,
            buf: vec![0.0; RING],
            write: 0,
            delay: 0.0,
        }
    }

    fn read(&self, delay: f32) -> f32 {
        let pos = self.write as f32 - 1.0 - delay;
        let pos = pos.rem_euclid(RING as f32);
        let i = pos as usize % RING;
        let j = (i + 1) % RING;
        let frac = pos.fract();
        self.buf[i] * (1.0 - frac) + self.buf[j] * frac
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        self.buf[self.write] = x;
        self.write = (self.write + 1) % RING;
        if (self.ratio - 1.0).abs() < 1e-3 {
            self.delay = 0.0;
            return x;
        }
        let d1 = self.delay;
        let d2 = (d1 + WINDOW / 2.0) % WINDOW;
        let gain = |d: f32| 0.5 - 0.5 * (std::f32::consts::TAU * d / WINDOW).cos();
        let y = self.read(d1) * gain(d1) + self.read(d2) * gain(d2);
        self.delay = (self.delay + 1.0 - self.ratio).rem_euclid(WINDOW);
        y
    }
}

impl VoiceFilter for PitchShifter {
    fn name(&self) -> &str {
        "mushroom pitch"
    }

    fn process(&mut self, frame: &mut [f32], _ctx: &FilterContext) {
        for s in frame.iter_mut() {
            *s = self.tick(*s);
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.delay = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Count upward zero crossings: a rough frequency estimate.
    fn crossings(s: &[f32]) -> usize {
        s.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count()
    }

    fn shifted(ratio: f32) -> usize {
        let rate = 16_000.0;
        let mut shifter = PitchShifter::new(ratio);
        let out: Vec<f32> = (0..32_000)
            .map(|i| shifter.tick((i as f32 / rate * 200.0 * std::f32::consts::TAU).sin()))
            .collect();
        crossings(&out[4_000..])
    }

    #[test]
    fn pitch_goes_up_and_down() {
        let normal = shifted(1.0) as f32;
        let up = shifted(1.7) as f32 / normal;
        let down = shifted(0.6) as f32 / normal;
        assert!((up - 1.7).abs() < 0.15, "up ratio {up}");
        assert!((down - 0.6).abs() < 0.1, "down ratio {down}");
    }

    #[test]
    fn pitches_are_much_higher_or_much_lower() {
        for i in 0..100 {
            let coin = i as f32 / 100.0;
            let p = pick_pitch(coin, 1.0 - coin);
            assert!(p >= 1.5 || p <= 0.7, "{p} is not dramatic enough");
        }
    }
}
