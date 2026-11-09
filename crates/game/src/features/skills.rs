//! Skills that grow with use, like Rune Factory (#176, catherine: "everything,
//! even walking and sleeping and pissing and farting should have a skill that
//! improves it over time").
//!
//! Every player has replicated [`Skills`]: XP in each [`Skill`], earned by
//! doing it, with a level from 1 to [`MAX_LEVEL`] ([`level`]). Levels make
//! the thing better:
//! - **Walking** (per metre): you walk and run up to 20% faster.
//! - **Farting** (per fart): your gas fills up to 50% faster.
//! - **Sleeping** (per second asleep; Z lies you down and gets you up): you
//!   can't move while asleep, and you sleep... more skilfully.
//! - **Pissing** (per piss; X): a puddle at your feet, bigger the higher your
//!   level, that everyone sees for a while.
//! - **Smoking** (per smoke) and **Dueling** (a duel fought: more for
//!   winning) are tracked and levelled too.
//!
//! Level-ups show a toast. The server owns all XP.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, WalkSet, Walker};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// The top level.
pub const MAX_LEVEL: u32 = 100;
/// XP for level `n` grows with the square: `XP_STEP * (n - 1)^2`.
pub const XP_STEP: f32 = 10.0;
/// How long a puddle lies there.
pub const PUDDLE_SECS: f32 = 20.0;

/// What you can get better at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skill {
    Walking,
    Farting,
    Sleeping,
    Pissing,
    Smoking,
    Dueling,
}

impl Skill {
    pub const ALL: [Skill; 6] = [
        Skill::Walking,
        Skill::Farting,
        Skill::Sleeping,
        Skill::Pissing,
        Skill::Smoking,
        Skill::Dueling,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Skill::Walking => "Walking",
            Skill::Farting => "Farting",
            Skill::Sleeping => "Sleeping",
            Skill::Pissing => "Pissing",
            Skill::Smoking => "Smoking",
            Skill::Dueling => "Dueling",
        }
    }
}

/// A player's XP in each skill (in [`Skill::ALL`] order). Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Skills(pub [f32; 6]);

impl Skills {
    pub fn xp(&self, s: Skill) -> f32 {
        self.0[s as usize]
    }

    pub fn level(&self, s: Skill) -> u32 {
        level(self.xp(s))
    }

    pub fn gain(&mut self, s: Skill, xp: f32) {
        self.0[s as usize] += xp;
    }

    /// Walking and running speed multiplier: up to +20% at the top level.
    pub fn walk_speed(&self) -> f32 {
        1.0 + 0.2 * (self.level(Skill::Walking) - 1) as f32 / (MAX_LEVEL - 1) as f32
    }

    /// Gas fill rate multiplier: up to +50% at the top level.
    pub fn gas_rate(&self) -> f32 {
        1.0 + 0.5 * (self.level(Skill::Farting) - 1) as f32 / (MAX_LEVEL - 1) as f32
    }
}

/// The level for `xp`: 1 at none, then each level needs more.
pub fn level(xp: f32) -> u32 {
    (1 + (xp.max(0.0) / XP_STEP).sqrt() as u32).min(MAX_LEVEL)
}

/// On a player while they're asleep. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asleep;

/// Client -> server: lie down, or get up.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ToggleSleep;

/// Client -> server: piss here.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Piss;

/// Server -> everyone: `player` pissed at `at`, a puddle of `size` metres.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pissed {
    pub player: u64,
    pub at: Vec3,
    pub size: f32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Skills>()
        .replicate::<Asleep>()
        .add_client_event::<ToggleSleep>(Channel::Ordered)
        .add_client_event::<Piss>(Channel::Ordered)
        .add_server_event::<Pissed>(Channel::Ordered)
        .add_observer(give_skills)
        .add_observer(toggle_sleep)
        .add_observer(piss)
        .add_observer(fart_xp)
        .add_observer(duel_xp)
        .add_systems(
            Update,
            (
                hold_sleepers.before(WalkSet),
                (walk_xp, sleep_xp, smoke_xp).after(WalkSet),
            )
                .run_if(in_state(ServerState::Running)),
        );
}

fn give_skills(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Skills::default());
    }
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

fn toggle_sleep(
    ev: On<FromClient<ToggleSleep>>,
    mut commands: Commands,
    players: Query<(Entity, &PlayerOwner, Has<Asleep>)>,
) {
    let owner = owner_of(ev.client_id);
    let Some((e, _, asleep)) = players.iter().find(|(_, o, _)| o.0 == owner) else {
        return;
    };
    if asleep {
        commands.entity(e).remove::<Asleep>();
    } else {
        commands.entity(e).insert(Asleep);
    }
}

fn piss(
    ev: On<FromClient<Piss>>,
    mut commands: Commands,
    mut players: Query<(&Player, &PlayerOwner, &Walker, &mut Skills, Has<Asleep>)>,
) {
    let owner = owner_of(ev.client_id);
    let Some((p, _, w, mut skills, asleep)) = players.iter_mut().find(|(_, o, ..)| o.0 == owner)
    else {
        return;
    };
    if asleep {
        return; // not in bed, please
    }
    skills.gain(Skill::Pissing, 3.0);
    let lv = skills.level(Skill::Pissing) as f32;
    let ahead = Vec3::new(-w.yaw.sin(), 0.0, -w.yaw.cos()) * 0.5;
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Pissed {
            player: p.id,
            at: w.pos + ahead,
            size: 0.35 + 0.012 * lv,
        },
    });
}

fn fart_xp(ev: On<super::fart::Farted>, mut players: Query<(&Player, &mut Skills)>) {
    if let Some((_, mut s)) = players.iter_mut().find(|(p, _)| p.id == ev.player) {
        s.gain(Skill::Farting, 5.0);
    }
}

fn duel_xp(ev: On<super::duel::DuelFought>, mut players: Query<(&Player, &mut Skills)>) {
    for (p, mut s) in &mut players {
        if p.id == ev.winner {
            s.gain(Skill::Dueling, 25.0);
        } else if p.id == ev.loser {
            s.gain(Skill::Dueling, 8.0);
        }
    }
}

/// Server: sleepers don't walk.
fn hold_sleepers(mut players: Query<&mut WalkInput, With<Asleep>>) {
    for mut input in &mut players {
        input.stick = Vec2::ZERO;
        input.jump = false;
    }
}

/// Server: a metre walked is one Walking XP (teleports don't count).
fn walk_xp(
    mut last: Local<std::collections::HashMap<u64, Vec3>>,
    mut players: Query<(&Player, &Walker, &mut Skills)>,
) {
    for (p, w, mut s) in &mut players {
        let prev = last.insert(p.id, w.pos).unwrap_or(w.pos);
        let d = prev.xz().distance(w.pos.xz());
        if d > 0.0 && d < 2.0 {
            s.bypass_change_detection().gain(Skill::Walking, d);
            // Replicate when the shown level changes, not every step.
            if level(s.xp(Skill::Walking) - d) != s.level(Skill::Walking) {
                s.set_changed();
            }
        }
    }
}

/// Server: a second asleep is one Sleeping XP.
fn sleep_xp(time: Res<Time>, mut players: Query<&mut Skills, With<Asleep>>) {
    let dt = time.delta_secs();
    for mut s in &mut players {
        let before = s.level(Skill::Sleeping);
        s.bypass_change_detection().gain(Skill::Sleeping, dt);
        if s.level(Skill::Sleeping) != before {
            s.set_changed();
        }
    }
}

/// Server: each smoke lit (see `smoking.rs`) is Smoking XP.
fn smoke_xp(mut players: Query<&mut Skills, Added<super::smoking::Smoking>>) {
    for mut s in &mut players {
        s.gain(Skill::Smoking, 5.0);
    }
}

pub fn plugin(app: &mut App) {
    app.add_observer(show_puddle)
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            (press_keys, show_skills, level_up_toast, dry_puddles)
                .run_if(in_state(AppState::InGame)),
        );
}

fn press_keys(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyZ) {
        commands.client_trigger(ToggleSleep);
    }
    if keys.just_pressed(KeyCode::KeyX) {
        commands.client_trigger(Piss);
    }
}

#[derive(Component)]
struct SkillsText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(13.0),
        SkillsText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new("skills: everything you do levels up. Z sleeps (and wakes), X pisses"),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

fn mine<'a>(
    me: &Option<Res<LocalPlayer>>,
    players: &'a Query<(&Player, &Skills, Has<Asleep>)>,
) -> Option<(&'a Skills, bool)> {
    let me = me.as_ref()?.0;
    players
        .iter()
        .find(|(p, ..)| p.id == me)
        .map(|(_, s, a)| (s, a))
}

fn show_skills(
    me: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &Skills, Has<Asleep>)>,
    mut text: Query<&mut Text, With<SkillsText>>,
) {
    let Some((s, asleep)) = mine(&me, &players) else {
        return;
    };
    let mut want = Skill::ALL
        .iter()
        .map(|k| format!("{} {}", k.name(), s.level(*k)))
        .collect::<Vec<_>>()
        .join(" · ");
    if asleep {
        want = format!("zzz (Z to get up)\n{want}");
    }
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

fn level_up_toast(
    me: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &Skills, Has<Asleep>)>,
    mut last: Local<Option<[u32; 6]>>,
    toast: Option<ResMut<Toast>>,
) {
    let Some((s, _)) = mine(&me, &players) else {
        return;
    };
    let now = Skill::ALL.map(|k| s.level(k));
    if let Some(prev) = *last
        && let Some(i) = (0..6).find(|&i| now[i] > prev[i])
        && let Some(mut toast) = toast
    {
        toast.show(format!(
            "{} skill up! level {}",
            Skill::ALL[i].name(),
            now[i]
        ));
    }
    *last = Some(now);
}

/// A drying puddle.
#[derive(Component)]
struct Puddle(f32);

fn show_puddle(
    ev: On<Pissed>,
    mut commands: Commands,
    state: Option<Res<State<AppState>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.85, 0.2, 0.7),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.1,
        ..default()
    });
    commands.spawn((
        Puddle(PUDDLE_SECS),
        Mesh3d(meshes.add(Cylinder::new(ev.size, 0.01))),
        MeshMaterial3d(mat),
        Transform::from_translation(ev.at + Vec3::Y * 0.02),
        DespawnOnExit(AppState::InGame),
    ));
}

fn dry_puddles(
    mut commands: Commands,
    time: Res<Time>,
    mut puddles: Query<(Entity, &mut Puddle, &mut Transform)>,
) {
    for (e, mut p, mut tf) in &mut puddles {
        p.0 -= time.delta_secs();
        if p.0 <= 0.0 {
            commands.entity(e).despawn();
        } else {
            let s = (p.0 / PUDDLE_SECS).sqrt();
            tf.scale = Vec3::new(s, 1.0, s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_climb_slower_and_cap() {
        assert_eq!(level(0.0), 1);
        assert_eq!(level(10.0), 2);
        assert_eq!(level(40.0), 3);
        assert_eq!(level(1e9), MAX_LEVEL);
    }

    #[test]
    fn walking_and_farting_get_better() {
        let mut s = Skills::default();
        assert_eq!(s.walk_speed(), 1.0);
        s.gain(Skill::Walking, 1e9);
        s.gain(Skill::Farting, 1e9);
        assert!((s.walk_speed() - 1.2).abs() < 1e-5);
        assert!((s.gas_rate() - 1.5).abs() < 1e-5);
    }
}
