//! Landlords. From issue #11 by Freyja Moth.
//!
//! The inciting incident: your landlord raises your rent. The story runs in
//! three acts, tracked on one replicated `Tenancy` entity the server owns:
//!
//! 1. **Legal**: click the landlord to pay rent with every organ you carry
//!    (organs are legal tender, #22). He raises the rent again every
//!    `RAISE_SECS`, and the moment it's paid off, so you can never quite keep
//!    up. Every raise, paid off or timed out, brings the tenants closer to
//!    giving up on the law: a group that pays together gets there sooner
//!    (#135: paying used to change nothing).
//! 2. **Less than legal**: after `RAISES_BEFORE_CRIME` raises you give up on the
//!    law. Clicking him now pickpockets an organ back from his stash, which
//!    counts as harming a landlord for morality (#21, corrected in #43): always virtuous.
//! 3. **Boss fight**: after `HEISTS_BEFORE_BOSS` heists he grows huge and
//!    fights back with eviction notices. Clicking hits him; at 0 health the
//!    game is over and you've won, and his stash is shared out between every
//!    tenant.
//!
//! Everything pauses during the Stillness (#17).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, in_reach, refuse};
use crate::features::morality::{Act, MoralAct, Victim};
use crate::features::organ_currency::{Organ, OrganPurse};
use crate::features::stillness::{Stillness, is_frozen};
use crate::player::PlayerOwner;
use crate::rigged::{RigClip, Rigged};

/// Where the landlord stands.
pub use friendslop_world::places::landlord_at;
/// Tuning numbers: `assets/data/story/landlord.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Rent before the inciting raise.
    pub old_rent: u32,
    /// How much each raise adds.
    pub raise_by: u32,
    /// Seconds between rent raises.
    pub raise_secs: f32,
    /// Raises (after the first) until the tenants turn to crime.
    pub raises_before_crime: u32,
    /// Heists until the landlord turns into a boss.
    pub heists_before_boss: u32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/story/landlord.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// The boss landlord's health.
pub const BOSS_HP: u32 = 12;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Act3 {
    #[default]
    Legal,
    Crime,
    Boss,
    Won,
}

/// The whole tenancy story. One per session, server-authoritative.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tenancy {
    pub act: Act3,
    /// Rent currently demanded (in organ value).
    pub rent: u32,
    /// Paid towards the current rent.
    pub paid: u32,
    /// How many times he's raised it since the inciting incident.
    pub raises: u32,
    pub heists: u32,
    pub hp: u32,
    /// Organs he's collected; heists take from here.
    pub stash: Vec<Organ>,
    /// Seconds until the next raise.
    pub next_raise: f32,
}

impl Default for Tenancy {
    /// Right after the inciting incident: the rent has just gone up.
    fn default() -> Self {
        Self {
            act: Act3::Legal,
            rent: tuning().old_rent + tuning().raise_by,
            paid: 0,
            raises: 0,
            heists: 0,
            hp: BOSS_HP,
            stash: Vec::new(),
            next_raise: tuning().raise_secs,
        }
    }
}

impl Tenancy {
    /// Time passes: in the legal act, he keeps raising the rent.
    pub fn tick(&mut self, dt: f32) {
        if self.act != Act3::Legal {
            return;
        }
        self.next_raise -= dt;
        if self.next_raise > 0.0 {
            return;
        }
        self.raise();
    }

    /// He raises the rent (and resets his clock). Enough raises and the law
    /// has failed you.
    fn raise(&mut self) {
        self.next_raise = tuning().raise_secs;
        self.raises += 1;
        self.rent += tuning().raise_by;
        if self.raises >= tuning().raises_before_crime {
            self.act = Act3::Crime;
        }
    }

    /// A tenant bothers the landlord: pay (legal), pickpocket an organ back
    /// (crime) or hit him (boss). Returns whether it harmed him.
    pub fn bother(&mut self, purse: &mut OrganPurse) -> bool {
        match self.act {
            Act3::Legal => {
                for organ in purse.0.drain(..) {
                    self.paid += organ.value();
                    self.stash.push(organ);
                }
                while self.act == Act3::Legal && self.paid >= self.rent {
                    // Paid up, so naturally he raises it.
                    self.paid -= self.rent;
                    self.raise();
                }
                false
            }
            Act3::Crime => {
                if let Some(organ) = self.stash.pop() {
                    purse.0.push(organ);
                }
                self.heists += 1;
                if self.heists >= tuning().heists_before_boss {
                    self.act = Act3::Boss;
                }
                true
            }
            Act3::Boss => {
                self.hp = self.hp.saturating_sub(1);
                if self.hp == 0 {
                    // The stash is shared out by `share_stash`.
                    self.act = Act3::Won;
                }
                true
            }
            Act3::Won => false,
        }
    }
}

/// Client -> server: "I'm dealing with the landlord" (pay, steal or hit,
/// depending on the act).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct BotherLandlord;

pub fn protocol(app: &mut App) {
    app.replicate::<Tenancy>()
        .add_observer(reset_tenancy)
        .add_client_event::<BotherLandlord>(Channel::Ordered)
        .add_observer(bother_landlord)
        .add_systems(OnEnter(ServerState::Running), raise_the_rent)
        .add_systems(Update, tick_tenancy.run_if(in_state(ServerState::Running)));
}

/// The inciting incident.
fn raise_the_rent(mut commands: Commands) {
    commands.spawn((Tenancy::default(), Replicated));
}

fn tick_tenancy(
    time: Res<Time>,
    still: Query<&Stillness>,
    mut tenancy: Query<&mut Tenancy>,
    mut pending: Local<f32>,
) {
    if is_frozen(&still) {
        return;
    }
    // Tick in whole seconds: touching `next_raise` every frame would resend
    // the whole replicated `Tenancy` (stash and all) every frame.
    *pending += time.delta_secs();
    if *pending < 1.0 {
        return;
    }
    let dt = std::mem::take(&mut *pending);
    for mut t in &mut tenancy {
        t.tick(dt);
    }
}

fn bother_landlord(
    ev: On<FromClient<BotherLandlord>>,
    mut commands: Commands,
    still: Query<&Stillness>,
    mut tenancy: Query<&mut Tenancy>,
    players: Query<(Entity, &PlayerOwner)>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut purses: Query<&mut OrganPurse>,
) {
    if is_frozen(&still) {
        return;
    }
    if !in_reach(&mut commands, ev.client_id, &walkers, landlord_at()) {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((player, _)) = players.iter().find(|(_, o)| o.0 == owner) else {
        return;
    };
    let Ok(mut purse) = purses.get_mut(player) else {
        return;
    };
    let Ok(mut t) = tenancy.single_mut() else {
        return;
    };
    if t.bother(&mut purse) {
        commands.trigger(MoralAct {
            player,
            act: Act::Harm(Victim::Landlord),
        });
    }
    if t.act == Act3::Won && !t.stash.is_empty() {
        let stash = &mut t.stash;
        share_stash(stash, purses.iter_mut().map(|p| &mut p.into_inner().0));
    }
}

/// The landlord is beaten: his stash goes back to every tenant, an organ each
/// in turn, starting with the most valuable.
pub fn share_stash<'a>(stash: &mut Vec<Organ>, purses: impl Iterator<Item = &'a mut Vec<Organ>>) {
    let mut purses: Vec<_> = purses.collect();
    if purses.is_empty() {
        return;
    }
    stash.sort_by_key(|o| std::cmp::Reverse(o.value()));
    for (i, organ) in stash.drain(..).enumerate() {
        let n = purses.len();
        purses[i % n].push(organ);
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
    app.add_observer(click_landlord)
        .add_systems(OnEnter(AppState::InGame), spawn_landlord)
        .add_systems(Update, show_tenancy.run_if(in_state(AppState::InGame)));
}

#[derive(Component)]
struct LandlordBody;

/// His rigged model.
const LANDLORD_MODEL: &str = "models/creatures/landlord.gltf";

/// The clip he plays in each act: shaking his fists as the boss.
pub fn clip_for(act: Act3) -> &'static str {
    match act {
        Act3::Boss => "boss",
        _ => "idle",
    }
}

#[derive(Component)]
struct TenancyText;

fn spawn_landlord(mut commands: Commands) {
    // Modeler-made, rigged (source `.modeler/prefabs/np-landlord.json`):
    // suit, top hat, moustache and watch chain, with `idle` and `boss` clips.
    // A missing file only makes him invisible; clicking aims at the
    // entity's own pick point, not at his meshes.
    commands.spawn((
        LandlordBody,
        Transform::from_translation(landlord_at()),
        Clickable::at_height(1.2)
            .radius(60.0)
            .labelled("pay the landlord"),
        Visibility::default(),
        Rigged {
            path: LANDLORD_MODEL,
        },
        RigClip::looping("idle"),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        TenancyText,
        crate::hud::slot(crate::hud::HudSlot::TopRight),
        DespawnOnExit(AppState::InGame),
    ));
}

/// What the HUD says about the tenancy.
pub fn describe(t: &Tenancy) -> String {
    match t.act {
        Act3::Legal => format!(
            "RENT {}  paid {}  raise in {:.0}s\nclick the landlord to pay",
            t.rent, t.paid, t.next_raise
        ),
        Act3::Crime => format!(
            "the law has failed you  |  rent {}\nclick him to pickpocket an organ  {}/{}",
            t.rent,
            t.heists,
            tuning().heists_before_boss
        ),
        Act3::Boss => format!(
            "BOSS  |  THE LANDLORD  {}/{}\nclick him to fight back",
            t.hp, BOSS_HP
        ),
        Act3::Won => "the landlord is defeated  |  the rent is free".to_string(),
    }
}

fn show_tenancy(
    time: Res<Time>,
    tenancy: Query<&Tenancy>,
    mut text: Query<&mut Text, With<TenancyText>>,
    mut body: Query<(&mut Transform, &mut RigClip), With<LandlordBody>>,
) {
    let Ok(t) = tenancy.single() else { return };
    let want = describe(t);
    for mut text in &mut text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
    let (scale, bob) = match t.act {
        Act3::Boss => (2.5, (time.elapsed_secs() * 6.0).sin().abs() * 0.4),
        Act3::Won => (0.6, 0.0),
        _ => (1.0, 0.0),
    };
    let want = Transform {
        translation: landlord_at() + Vec3::Y * bob,
        rotation: if t.act == Act3::Won {
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
        } else {
            Quat::IDENTITY
        },
        scale: Vec3::splat(scale),
    };
    let clip = RigClip::looping(clip_for(t.act));
    for (mut tf, mut playing) in &mut body {
        // Only re-propagate his parts when he actually moves (#151).
        tf.set_if_neq(want);
        playing.set_if_neq(clip.clone());
    }
}

/// The unified click landed on the landlord: pay, steal or hit.
fn click_landlord(
    click: On<Clicked>,
    mut commands: Commands,
    me: Option<Res<crate::player::LocalPlayer>>,
    body: Query<(), With<LandlordBody>>,
    tenancy: Query<&Tenancy>,
    purses: Query<(&crate::player::Player, &OrganPurse)>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    if !body.contains(click.entity) {
        return;
    }
    let broke = tenancy.iter().any(|t| t.act == Act3::Legal)
        && me.is_some_and(|me| {
            purses
                .iter()
                .any(|(p, purse)| p.id == me.0 && purse.0.is_empty())
        });
    if broke {
        refuse(toast, "you can't afford rent: no organs left to pay with");
        return;
    }
    commands.client_trigger(BotherLandlord);
}

/// New round: back to right after the inciting incident.
fn reset_tenancy(_reset: On<crate::rules::campaign::RoundReset>, mut tenancy: Query<&mut Tenancy>) {
    for mut t in &mut tenancy {
        *t = Tenancy::default();
    }
}

/// The camera this close to the landlord counts as near him (for the music).
const NEAR_LANDLORD: f32 = 12.0;

/// For the music: the boss fight and its health, near the landlord while
/// renting (and how often he's raised it), the tenancy won, and a victory
/// as it's won.
fn music_signals(
    tenancy: Query<&Tenancy>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut act_before: Local<Option<Act3>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let t = tenancy.iter().next();
    let act = t.map(|t| t.act);
    if act == Some(Act3::Won) && act_before.is_some_and(|a| a != Act3::Won) {
        signals.cue("victory");
    }
    *act_before = act;
    let near = t.is_some()
        && crate::music::camera_at(&camera)
            .is_some_and(|at| at.xz().distance(landlord_at().xz()) < NEAR_LANDLORD);
    signals.flag("landlord_boss", act == Some(Act3::Boss));
    signals.set("boss_hp", t.map_or(0.0, |t| t.hp as f32));
    signals.flag("near_landlord", near);
    signals.set("raises", t.map_or(0.0, |t| t.raises as f32));
    signals.flag("won", act == Some(Act3::Won));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::organ_currency::starting_purse;

    #[test]
    fn he_shakes_his_fists_only_as_the_boss() {
        assert_eq!(clip_for(Act3::Legal), "idle");
        assert_eq!(clip_for(Act3::Crime), "idle");
        assert_eq!(clip_for(Act3::Boss), "boss");
        assert_eq!(clip_for(Act3::Won), "idle");
    }

    #[test]
    fn the_whole_story() {
        let mut t = Tenancy::default();
        assert!(
            t.rent > tuning().old_rent,
            "the inciting incident raises the rent"
        );
        let mut purse = starting_purse();
        assert!(!t.bother(&mut purse), "paying rent is legal");
        assert!(purse.0.is_empty());
        assert_eq!(t.paid, 21);
        for _ in 0..tuning().raises_before_crime {
            t.tick(tuning().raise_secs + 1.0);
        }
        assert_eq!(t.act, Act3::Crime);
        assert!(t.bother(&mut purse), "pickpocketing harms him");
        assert_eq!(purse.0.len(), 1);
        for _ in 1..tuning().heists_before_boss {
            t.bother(&mut purse);
        }
        assert_eq!(t.act, Act3::Boss);
        for _ in 0..BOSS_HP {
            t.bother(&mut purse);
        }
        assert_eq!(t.act, Act3::Won);
        t.tick(tuning().raise_secs * 10.0);
        assert_eq!(t.act, Act3::Won);
    }

    #[test]
    fn paying_off_the_rent_moves_the_story_on() {
        // Six tenants pay with everything they've got (#135).
        let mut t = Tenancy::default();
        for _ in 0..6 {
            t.bother(&mut starting_purse());
        }
        assert_eq!(t.act, Act3::Crime, "they paid off three raises");
        assert!(t.next_raise >= tuning().raise_secs - 1.0);
        // One tenant can't keep up: they just pay towards it.
        let mut t = Tenancy::default();
        t.bother(&mut starting_purse());
        assert_eq!((t.act, t.raises), (Act3::Legal, 0));
    }

    #[test]
    fn the_stash_is_shared_by_everyone() {
        let mut stash: Vec<Organ> = (0..3).flat_map(|_| starting_purse().0).collect();
        let mut purses = vec![Vec::new(); 4];
        share_stash(&mut stash, purses.iter_mut());
        assert!(stash.is_empty());
        assert!(purses.iter().all(|p| p.len() >= 2));
        assert_eq!(purses.iter().map(Vec::len).sum::<usize>(), 9);
    }
}
