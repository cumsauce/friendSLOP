//! Bad people get chased by a helicopter that shoots at them. From
//! just meat and cheese, answering #134 ("What should your morality actually
//! change?").
//!
//! The server watches every player's [`Morality`]. While someone is
//! [`HUNTED_AT`] or worse ("dubious" and below), a replicated [`Helicopter`]
//! flies in from the edge of the map, hovers above them, follows them faster
//! than anyone can run, and shoots bursts of bullets at them with some spread.
//! Win your morality back (harming the landlord always helps, #43) and it
//! flies away. A new campaign resets morality, so it leaves then too.
//!
//! Every peer draws the helicopter (modeler-made body and spinning rotors),
//! hears its generated rotor thump and gunfire, and sees tracers and dirt
//! puffs where the bullets land. A hit only tells you that you've been shot:
//! what getting shot actually does is still an open question.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player};
use crate::rules::morality::Morality;

/// Morality at or below which the helicopter comes for you ("dubious").
pub const HUNTED_AT: i32 = -6;
/// Height it hovers at above its target's feet.
pub const HOVER_HEIGHT: f32 = 14.0;
/// How far off to one side it hovers (so it can see you past its own nose).
pub const HOVER_RADIUS: f32 = 9.0;
/// Top speed (running is 5 m/s or so: you can't outrun it).
pub const FLY_SPEED: f32 = 11.0;
/// Shots in a burst.
pub const BURST: u32 = 5;
/// Seconds between shots in a burst.
pub const SHOT_GAP: f32 = 0.18;
/// Seconds of quiet between bursts.
pub const BURST_GAP: f32 = 2.4;
/// How far a bullet can land from you.
pub const SPREAD: f32 = 2.6;
/// A bullet this close to your feet (on the ground) hits you.
pub const HIT_RADIUS: f32 = 0.7;
/// It only opens fire this close (horizontally) to where it wants to be.
const FIRING_RANGE: f32 = 25.0;
/// Where it comes in from (and flies back to).
const ARRIVE_FROM: Vec3 = Vec3::new(-150.0, 40.0, -150.0);

/// Server -> everyone: a helicopter after player `target`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Helicopter {
    pub target: u64,
    pub pos: Vec3,
    pub yaw: f32,
    /// Bullets fired so far (each new one is a shot to show).
    pub shots: u32,
    /// Where the latest bullet landed.
    pub aim: Vec3,
    /// Did the latest bullet hit?
    pub hit: bool,
}

/// Server only: the gunner's timing and dice.
#[derive(Component, Debug)]
struct Gunner {
    /// Seconds until the next shot.
    wait: f32,
    /// Shots left in this burst.
    left: u32,
    dice: u32,
    /// Flying home (its target is good again): despawns at the edge.
    leaving: bool,
}

impl Gunner {
    fn roll(&mut self) -> f32 {
        self.dice = self
            .dice
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.dice >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Helicopter>().add_systems(
        Update,
        (summon, fly).chain().run_if(in_state(ServerState::Running)),
    );
}

/// Is a player with this morality wicked enough to be hunted?
pub fn hunted(morality: Morality) -> bool {
    hunted_below(morality, HUNTED_AT)
}

/// Like [`hunted`], with the line Marmalade's wheel drew (#173).
pub fn hunted_below(morality: Morality, at: i32) -> bool {
    morality.0 <= at
}

/// Server: send a helicopter after every wicked player, and send it home
/// once they're good again (or gone).
fn summon(
    mut commands: Commands,
    players: Query<(&Player, &Morality)>,
    mut helis: Query<(&Helicopter, &mut Gunner)>,
    odds: Query<&crate::rules::house_odds::HouseOdds>,
) {
    let line = crate::rules::house_odds::odds(&odds).hunted_at();
    let hunted = |m: Morality| hunted_below(m, line);
    for (heli, mut gunner) in &mut helis {
        let still_wicked = players
            .iter()
            .any(|(p, m)| p.id == heli.target && hunted(*m));
        gunner.leaving = !still_wicked;
    }
    for (player, morality) in &players {
        if !hunted(*morality) {
            continue;
        }
        if let Some((_, mut gunner)) = helis.iter_mut().find(|(h, _)| h.target == player.id) {
            gunner.leaving = false;
            continue;
        }
        commands.spawn((
            Helicopter {
                target: player.id,
                pos: ARRIVE_FROM,
                yaw: 0.0,
                shots: 0,
                aim: Vec3::ZERO,
                hit: false,
            },
            Gunner {
                wait: BURST_GAP,
                left: 0,
                dice: 0x4e11_c0b7 ^ (player.id as u32).wrapping_mul(2_654_435_761),
                leaving: false,
            },
            Replicated,
        ));
    }
}

/// Where a helicopter after someone standing at `feet` wants to hover.
pub fn hover_spot(feet: Vec3, from: Vec3) -> Vec3 {
    let side = (from - feet).xz();
    let side = if side.length() < 1e-3 {
        Vec2::X
    } else {
        side.normalize()
    };
    let ground = crate::terrain::height_at(feet.x, feet.z).max(feet.y);
    Vec3::new(
        feet.x + side.x * HOVER_RADIUS,
        ground + HOVER_HEIGHT,
        feet.z + side.y * HOVER_RADIUS,
    )
}

/// Move from `pos` towards `goal` at no more than [`FLY_SPEED`].
pub fn fly_step(pos: Vec3, goal: Vec3, dt: f32) -> Vec3 {
    let to = goal - pos;
    let d = to.length();
    if d < 1e-4 {
        return goal;
    }
    pos + to / d * (FLY_SPEED * dt).min(d)
}

/// Where a bullet aimed at `feet` lands, given two rolls in `0.0..1.0`.
pub fn bullet_lands(feet: Vec3, a: f32, b: f32) -> Vec3 {
    let angle = a * std::f32::consts::TAU;
    // sqrt: spread evenly over the disc, not bunched in the middle.
    let r = b.sqrt() * SPREAD;
    let x = feet.x + angle.cos() * r;
    let z = feet.z + angle.sin() * r;
    Vec3::new(x, crate::terrain::height_at(x, z).max(feet.y), z)
}

/// Does a bullet landing at `at` hit someone standing at `feet`?
pub fn is_hit(at: Vec3, feet: Vec3) -> bool {
    at.xz().distance(feet.xz()) <= HIT_RADIUS
}

/// Server: follow, aim and shoot (paused during the stillness, like the NPCs).
fn fly(
    time: Res<Time>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    walkers: Query<(&Player, &Walker)>,
    mut helis: Query<(Entity, &mut Helicopter, &mut Gunner)>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let dt = time.delta_secs();
    for (entity, mut heli, mut gunner) in &mut helis {
        let feet = walkers
            .iter()
            .find(|(p, _)| p.id == heli.target)
            .map(|(_, w)| w.pos)
            // Under the meadow in the warehouse (#130), it loses you for a while.
            .filter(|feet| !super::warehouse::inside(*feet) && !super::underground::below(*feet));
        let goal = match (gunner.leaving, feet) {
            (false, Some(feet)) => hover_spot(feet, heli.pos),
            _ => ARRIVE_FROM,
        };
        let pos = fly_step(heli.pos, goal, dt);
        if gunner.leaving && pos.distance(ARRIVE_FROM) < 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        // Nose towards the target (or home), like a gunship.
        let look = feet.filter(|_| !gunner.leaving).unwrap_or(goal);
        let to = (look - pos).xz();
        let yaw = if to.length() > 0.1 {
            f32::atan2(-to.x, -to.y)
        } else {
            heli.yaw
        };
        if heli.pos != pos || heli.yaw != yaw {
            heli.pos = pos;
            heli.yaw = yaw;
        }
        let Some(feet) = feet.filter(|_| !gunner.leaving) else {
            continue;
        };
        if pos.xz().distance(goal.xz()) > FIRING_RANGE {
            continue;
        }
        gunner.wait -= dt;
        if gunner.wait > 0.0 {
            continue;
        }
        if gunner.left == 0 {
            gunner.left = BURST;
        }
        let (a, b) = (gunner.roll(), gunner.roll());
        let aim = bullet_lands(feet, a, b);
        gunner.left -= 1;
        gunner.wait = if gunner.left == 0 {
            BURST_GAP
        } else {
            SHOT_GAP
        };
        heli.shots = heli.shots.wrapping_add(1);
        heli.aim = aim;
        heli.hit = is_hit(aim, feet);
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<HeliSounds>()
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            (
                dress,
                animate,
                show_shots,
                fade_effects,
                show_warning.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Generated sounds, made once.
#[derive(Resource, Default)]
struct HeliSounds {
    rotor: Option<Handle<AudioSource>>,
    shot: Option<Handle<AudioSource>>,
}

impl HeliSounds {
    fn ensure(&mut self, audio: &mut Assets<AudioSource>) {
        if self.rotor.is_none() {
            self.rotor = Some(audio.add(AudioSource {
                bytes: wav_bytes(&rotor_loop(SOUND_RATE), SOUND_RATE).into(),
            }));
        }
        if self.shot.is_none() {
            self.shot = Some(audio.add(AudioSource {
                bytes: wav_bytes(&gunshot(SOUND_RATE), SOUND_RATE).into(),
            }));
        }
    }
}

/// A spinning rotor: `rate` is radians per second around its local Y (or X
/// for the tail rotor, which is turned on its side).
#[derive(Component)]
struct Rotor(f32);

/// A short-lived effect (tracer or dirt puff) that shrinks away.
#[derive(Component)]
struct Fleeting {
    left: f32,
    total: f32,
    scale: Vec3,
}

/// Local: how many shots of this helicopter we've already shown.
#[derive(Component)]
struct Shown(u32);

/// The body of a helicopter: dark green fuselage, glass, tail, skids, rotors.
fn dress(
    mut commands: Commands,
    new: Query<(Entity, &Helicopter), Added<Helicopter>>,
    assets: Res<AssetServer>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    mut sounds: ResMut<HeliSounds>,
) {
    if new.is_empty() {
        return;
    }
    let mut audio = audio;
    if let Some(audio) = audio.as_deref_mut() {
        sounds.ensure(audio);
    }
    for (entity, heli) in &new {
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(heli.pos),
                Visibility::default(),
                Shown(heli.shots),
            ))
            .with_children(|c| {
                // Modeler-made body, rotor and tail rotor (prefabs `sp-heli-*`).
                // Nose points along -Z, like everything else's forward.
                c.spawn((model(&assets, "heli_body.gltf"), Transform::default()));
                c.spawn((
                    Rotor(22.0),
                    model(&assets, "heli_rotor.gltf"),
                    Transform::from_xyz(0.0, 1.45, 0.0),
                ));
                c.spawn((
                    Rotor(-40.0),
                    model(&assets, "heli_tail_rotor.gltf"),
                    Transform::from_xyz(0.25, 0.9, 5.8),
                ));
            });
        if let Some(rotor) = &sounds.rotor {
            let sound = crate::spatial::loop_at(
                &mut commands,
                rotor,
                Vec3::ZERO,
                0.8,
                crate::spatial::Carry::LOUD,
            );
            commands.entity(sound).insert(ChildOf(entity));
        }
    }
}

/// A modeler-made feature model in `assets/models/features`.
fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

/// Glide to the replicated position, lean into the flight, spin the rotors.
fn animate(
    time: Res<Time>,
    mut helis: Query<(&Helicopter, &mut Transform), Without<Rotor>>,
    mut rotors: Query<(&Rotor, &mut Transform), Without<Helicopter>>,
) {
    let dt = time.delta_secs();
    let blend = 1.0 - (-dt * 8.0).exp();
    for (heli, mut tf) in &mut helis {
        let before = tf.translation;
        tf.translation = tf.translation.lerp(heli.pos, blend);
        let speed = if dt > 0.0 {
            (tf.translation - before).length() / dt
        } else {
            0.0
        };
        let bob = (time.elapsed_secs() * 1.7).sin() * 0.15;
        tf.translation.y += bob * dt;
        let lean = (speed / FLY_SPEED).min(1.0) * 0.25;
        let want = Quat::from_rotation_y(heli.yaw) * Quat::from_rotation_x(-lean);
        tf.rotation = tf.rotation.slerp(want, blend);
    }
    for (rotor, mut tf) in &mut rotors {
        let axis = if rotor.0 < 0.0 { Vec3::X } else { Vec3::Y };
        tf.rotate_local_axis(Dir3::new(axis).unwrap_or(Dir3::Y), rotor.0.abs() * dt);
    }
}

/// A new bullet: a tracer from the gun to where it landed, a puff of dirt
/// there, and a bang.
fn show_shots(
    mut commands: Commands,
    mut helis: Query<(&Helicopter, &Transform, &mut Shown)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sounds: Res<HeliSounds>,
    mut kit: Local<
        Option<(
            Handle<Mesh>,
            Handle<Mesh>,
            Handle<StandardMaterial>,
            Handle<StandardMaterial>,
        )>,
    >,
) {
    for (heli, tf, mut shown) in &mut helis {
        if shown.0 == heli.shots {
            continue;
        }
        shown.0 = heli.shots;
        let (tracer, puff, hot, dirt) = kit
            .get_or_insert_with(|| {
                (
                    meshes.add(Cuboid::new(0.06, 0.06, 1.0)),
                    meshes.add(Sphere::new(0.35)),
                    materials.add(StandardMaterial {
                        base_color: Color::srgb(1.0, 0.85, 0.3),
                        emissive: LinearRgba::rgb(6.0, 4.0, 1.0),
                        unlit: true,
                        ..default()
                    }),
                    materials.add(Color::srgb(0.45, 0.36, 0.25)),
                )
            })
            .clone();
        let muzzle = tf.transform_point(Vec3::new(0.0, -0.9, -2.4));
        let to = heli.aim - muzzle;
        let len = to.length().max(0.1);
        let scale = Vec3::new(1.0, 1.0, len);
        commands.spawn((
            Mesh3d(tracer),
            MeshMaterial3d(hot),
            Transform::from_translation(muzzle + to * 0.5)
                .looking_to(to / len, Vec3::Y)
                .with_scale(scale),
            Fleeting {
                left: 0.08,
                total: 0.08,
                scale,
            },
            DespawnOnExit(AppState::InGame),
        ));
        commands.spawn((
            Mesh3d(puff),
            MeshMaterial3d(dirt),
            Transform::from_translation(heli.aim + Vec3::Y * 0.2),
            Fleeting {
                left: 0.5,
                total: 0.5,
                scale: Vec3::ONE,
            },
            DespawnOnExit(AppState::InGame),
        ));
        if let Some(shot) = &sounds.shot {
            crate::spatial::play_at(
                &mut commands,
                shot,
                muzzle,
                0.7,
                crate::spatial::Carry::LOUD,
            );
        }
    }
}

fn fade_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut fx: Query<(Entity, &mut Fleeting, &mut Transform)>,
) {
    for (entity, mut f, mut tf) in &mut fx {
        f.left -= time.delta_secs();
        if f.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        tf.scale = f.scale * (f.left / f.total).max(0.05);
    }
}

#[derive(Component)]
struct WarningText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::ROSE_RED),
        WarningText,
        crate::hud::slot(crate::hud::HudSlot::Toasts),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(format!(
            "helicopter: sink to morality {HUNTED_AT} or lower and a helicopter hunts you \
             and shoots at you until you're better."
        )),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Tell the hunted when they've been shot (who's hunted shows in the HUD's
/// danger lane, `ui::danger`).
fn show_warning(
    me: Option<Res<LocalPlayer>>,
    helis: Query<&Helicopter>,
    mut text: Query<&mut Text, With<WarningText>>,
) {
    let me = me.map(|m| m.0);
    let mut lines = Vec::new();
    for heli in &helis {
        if Some(heli.target) == me && heli.hit {
            lines.push("You've been shot! (It doesn't do anything. Yet.)".to_string());
        }
    }
    let want = lines.join("\n");
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

const SOUND_RATE: u32 = 22_050;

/// White-ish noise from a little xorshift.
fn noise(s: &mut u32) -> f32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    *s as f32 / u32::MAX as f32 * 2.0 - 1.0
}

/// One second of rotor: low "whup-whup" thumps (about 6 a second, a whole
/// number per loop so it loops cleanly) over a turbine whine.
pub fn rotor_loop(rate: u32) -> Vec<f32> {
    let r = rate as f32;
    let n = rate as usize;
    let mut out = vec![0.0f32; n];
    let mut s = 0x1234_5679u32;
    let mut lp = 0.0f32;
    let a = 1.0 - (-std::f32::consts::TAU * 260.0 / r).exp();
    for (i, x) in out.iter_mut().enumerate() {
        let t = i as f32 / r;
        let beat = (t * 6.0).fract();
        let thump = (-beat / 0.05).exp();
        lp += a * (noise(&mut s) - lp);
        let body = (std::f32::consts::TAU * 55.0 * t).sin() * 0.5 + lp * 2.5;
        let whine = (std::f32::consts::TAU * 880.0 * t).sin() * 0.04;
        *x = (body * (0.15 + 0.85 * thump) + whine).clamp(-1.0, 1.0) * 0.8;
    }
    out
}

/// One gunshot: a sharp crack and a short, low boom.
pub fn gunshot(rate: u32) -> Vec<f32> {
    let r = rate as f32;
    let n = (0.25 * r) as usize;
    let mut out = vec![0.0f32; n];
    let mut s = 0x0bad_5eedu32;
    let mut lp = 0.0f32;
    let a = 1.0 - (-std::f32::consts::TAU * 900.0 / r).exp();
    for (i, x) in out.iter_mut().enumerate() {
        let t = i as f32 / r;
        let crack = noise(&mut s) * (-t / 0.012).exp();
        lp += a * (crack - lp);
        let boom = (std::f32::consts::TAU * (90.0 - 120.0 * t) * t).sin() * (-t / 0.06).exp();
        *x = ((crack * 0.6 + lp * 1.5 + boom * 0.8) * 1.3).tanh() * 0.85;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_wicked_are_hunted() {
        assert!(!hunted(Morality(0)));
        assert!(!hunted(Morality(HUNTED_AT + 1)));
        assert!(hunted(Morality(HUNTED_AT)));
        assert!(hunted(Morality(-40)));
    }

    #[test]
    fn it_flies_fast_but_not_through_its_goal() {
        let p = fly_step(Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0), 0.5);
        assert!((p.x - FLY_SPEED * 0.5).abs() < 1e-4);
        let goal = Vec3::new(1.0, 2.0, 0.0);
        assert_eq!(fly_step(Vec3::ZERO, goal, 1.0), goal);
    }

    #[test]
    fn it_hovers_above_and_beside_you() {
        let feet = Vec3::new(3.0, 0.0, 4.0);
        let spot = hover_spot(feet, Vec3::new(100.0, 30.0, 4.0));
        assert!(spot.y >= HOVER_HEIGHT - 0.01);
        assert!((spot.xz().distance(feet.xz()) - HOVER_RADIUS).abs() < 1e-3);
        assert!(spot.x > feet.x, "stays on the side it came from");
    }

    #[test]
    fn bullets_land_near_you_and_some_hit() {
        let feet = Vec3::new(10.0, 0.0, -5.0);
        let mut hits = 0;
        for i in 0..20 {
            for j in 0..20 {
                let at = bullet_lands(feet, i as f32 / 20.0, j as f32 / 20.0);
                assert!(at.xz().distance(feet.xz()) <= SPREAD + 1e-3);
                hits += is_hit(at, feet) as u32;
            }
        }
        assert!(hits > 0 && hits < 400, "{hits}");
    }

    #[test]
    fn sounds_are_sane() {
        for s in [rotor_loop(SOUND_RATE), gunshot(SOUND_RATE)] {
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            assert!(s.iter().any(|x| x.abs() > 0.2));
        }
    }
}
