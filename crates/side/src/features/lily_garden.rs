//! The Lily Garden (#384, catherine): "I want Yuri!!! give me Yuri plot lines
//! and characters and story lines and imagery and sign posts and locations".
//!
//! A little walled-in-by-flowers garden by the pond (`models/props/lily_*`)
//! where two women, Marigold and Juniper, stand at opposite ends pining for
//! each other and far too shy to say so. Click whoever is writing to take
//! her letter, carry it across, click the other to deliver it: everyone sees
//! how it landed, and the reply starts. After the last letter they meet on
//! the bench in the middle, hand in hand, with hearts over them. Signposts
//! around the commune point the way. Kept sweet, never explicit: the
//! issue's title is read as "lesbian romance". One replicated
//! [`LilyStory`] holds the progress; the women, beds, bench and signs are
//! local visuals placed from it. Writing and layout:
//! `assets/data/side/lily_garden.ron`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::player::{Player, PlayerOwner};

/// One of the two women.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Woman {
    pub name: String,
    /// Seeds her look (`CharacterLook::random`).
    pub look: u64,
    /// Her clothes colour index.
    pub clothes: u8,
    /// Where she pines alone (garden-local x, z).
    pub at: (f32, f32),
}

/// One letter: the line its carrier sees, and the line everyone sees on delivery.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Letter {
    pub take: String,
    pub give: String,
}

/// Tuning, layout and writing: `assets/data/side/lily_garden.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The map spot the garden sits by.
    pub near: String,
    /// Offset from that spot (x, z).
    pub offset: (f32, f32),
    /// Ground kept clear around it.
    pub radius: f32,
    pub women: Vec<Woman>,
    /// The bench (garden-local x, z).
    pub bench: (f32, f32),
    /// Flowerbeds (garden-local x, z).
    pub beds: Vec<(f32, f32)>,
    /// Signposts: (spot, offset x, offset z).
    pub signs: Vec<(String, f32, f32)>,
    pub sign_line: String,
    pub letters: Vec<Letter>,
    pub waiting_line: String,
    pub carrying_line: String,
    pub together_lines: Vec<String>,
    pub heart_count: usize,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/lily_garden.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub const SIGN_MODEL: &str = "models/props/lily_sign.gltf";
pub const BENCH_MODEL: &str = "models/props/lily_bench.gltf";
pub const BED_MODEL: &str = "models/props/lily_bed.gltf";

/// The love story's progress. Replicated; the server owns the one copy.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct LilyStory {
    /// Letters delivered so far.
    pub delivered: u32,
    /// Who carries the current letter (player id), if anyone.
    pub carrier: Option<u64>,
}

impl LilyStory {
    /// Every letter delivered: they're together.
    pub fn together(&self, letters: usize) -> bool {
        self.delivered as usize >= letters
    }
}

/// Client → server: I clicked woman `0` or `1`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TalkToLily {
    pub woman: u8,
}

/// Server → everyone: a letter arrived.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct LilyNews {
    pub text: String,
}

/// What clicking a woman does.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Talk {
    /// You take letter `n` from her.
    Take(usize),
    /// You deliver letter `n` to her.
    Deliver(usize),
    /// You already carry her letter.
    Carrying,
    /// It's the other one's turn to write (or someone else carries it).
    Waiting,
    /// They're together: a happy line.
    Together,
}

/// Pure: clicking woman `woman` as player `me`.
pub fn talk(story: LilyStory, letters: usize, woman: u8, me: u64) -> Talk {
    if story.together(letters) {
        return Talk::Together;
    }
    let n = story.delivered as usize;
    let writer = (n % 2) as u8;
    match story.carrier {
        Some(c) if c == me && woman != writer => Talk::Deliver(n),
        Some(c) if c == me => Talk::Carrying,
        // Someone else has it: if they wander off, a fresh copy is fine.
        _ if woman == writer => Talk::Take(n),
        _ => Talk::Waiting,
    }
}

/// Pure: the garden's middle (x, z).
pub fn garden_at(t: &Tuning) -> Vec2 {
    friendslop_world::places::spot(&t.near).unwrap_or_default() + Vec2::new(t.offset.0, t.offset.1)
}

/// Pure: where woman `i` stands (garden-local x, z) and which way she faces.
pub fn woman_spot(t: &Tuning, i: usize, together: bool) -> (Vec2, f32) {
    let bench = Vec2::new(t.bench.0, t.bench.1);
    if together {
        // Side by side on the bench, turned a little towards each other.
        let side = if i == 0 { -0.35 } else { 0.35 };
        return (
            bench + Vec2::new(side, 0.15),
            if i == 0 { -0.4 } else { 0.4 },
        );
    }
    let Some(w) = t.women.get(i) else {
        return (bench, 0.0);
    };
    let at = Vec2::new(w.at.0, w.at.1);
    // Facing away from the other one, sighing.
    (at, if at.x < 0.0 { -1.9 } else { 1.9 })
}

/// Pure: the yaw that points a sign's +X arrow from `from` at `to`.
pub fn arrow_yaw(from: Vec2, to: Vec2) -> f32 {
    let d = to - from;
    // +X rotated by yaw about Y is (cos, -sin).
    (-d.y).atan2(d.x)
}

pub fn protocol(app: &mut App) {
    app.replicate::<LilyStory>()
        .add_client_event::<TalkToLily>(Channel::Ordered)
        .add_server_event::<LilyNews>(Channel::Ordered)
        .add_observer(on_talk)
        .add_systems(OnEnter(ServerState::Running), spawn_story);
    let t = tuning();
    friendslop_world::footprints::claim(garden_at(&t), t.radius);
}

fn spawn_story(mut commands: Commands) {
    commands.spawn((LilyStory::default(), Replicated));
}

fn on_talk(
    ev: On<FromClient<TalkToLily>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    players: Query<(&Player, &PlayerOwner)>,
    mut stories: Query<&mut LilyStory>,
) {
    let t = tuning();
    let Ok(mut story) = stories.single_mut() else {
        return;
    };
    let together = story.together(t.letters.len());
    let (local, _) = woman_spot(&t, ev.woman as usize, together);
    let g = garden_at(&t) + local;
    let at = Vec3::new(g.x, crate::terrain::height_at(g.x, g.y) + 1.0, g.y);
    if !in_reach(&mut commands, ev.client_id, &walkers, at) {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((me, _)) = players.iter().find(|p| p.1.0 == owner) else {
        return;
    };
    match talk(*story, t.letters.len(), ev.woman, me.id) {
        Talk::Take(n) => {
            story.carrier = Some(me.id);
            if let Some(l) = t.letters.get(n) {
                deny(&mut commands, ev.client_id, l.take.clone());
            }
        }
        Talk::Deliver(n) => {
            story.delivered += 1;
            story.carrier = None;
            if let Some(l) = t.letters.get(n) {
                commands.server_trigger(ToClients {
                    targets: SendTargets::All,
                    message: LilyNews {
                        text: l.give.clone(),
                    },
                });
            }
        }
        Talk::Carrying => deny(&mut commands, ev.client_id, t.carrying_line.clone()),
        Talk::Waiting => deny(&mut commands, ev.client_id, t.waiting_line.clone()),
        Talk::Together => {
            let lines = &t.together_lines;
            if !lines.is_empty() {
                let i = (me.id as usize + story.delivered as usize * 7 + ev.woman as usize)
                    % lines.len();
                // Rotate so clicking again says something new.
                story.delivered += 1;
                deny(&mut commands, ev.client_id, lines[i].clone());
            }
        }
    }
}

// ------------------------------------------------------------------ every peer

/// A woman's local body: index into `women`.
#[derive(Component)]
struct LilyWoman(u8);

/// A sign (clicking reads it).
#[derive(Component)]
struct LilySign;

/// Hearts over the couple.
#[derive(Component)]
struct LilyHeart(usize);

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_garden)
        .add_observer(click)
        .add_observer(news)
        .add_systems(
            Update,
            (place_women, float_hearts).run_if(in_state(AppState::InGame)),
        );
}

fn ground(p: Vec2) -> Vec3 {
    Vec3::new(p.x, crate::terrain::height_at(p.x, p.y), p.y)
}

fn spawn_garden(
    mut commands: Commands,
    assets: Option<Res<AssetServer>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = tuning();
    let mid = garden_at(&t);
    // Again here, in case the data file arrived after the protocol ran.
    friendslop_world::footprints::claim(mid, t.radius);
    let scene = |p: &'static str| {
        assets
            .as_ref()
            .map(|a| WorldAssetRoot(a.load(GltfAssetLabel::Scene(0).from_asset(p))))
    };
    let prop = |commands: &mut Commands, at: Vec2, yaw: f32, path: &'static str, collider| {
        let mut e = commands.spawn((
            Transform::from_translation(ground(at)).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
        if let Some(s) = scene(path) {
            e.insert(s);
        }
        if let Some((x, y, z)) = collider {
            e.with_child((
                Transform::from_xyz(0.0, y / 2.0, 0.0),
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(x, y, z),
            ));
        }
        e.id()
    };
    // The bench faces the pond side (-Z), the couple's view.
    prop(
        &mut commands,
        mid + Vec2::new(t.bench.0, t.bench.1),
        std::f32::consts::PI,
        BENCH_MODEL,
        Some((1.5, 0.45, 0.5)),
    );
    for b in &t.beds {
        prop(
            &mut commands,
            mid + Vec2::new(b.0, b.1),
            b.0 + b.1,
            BED_MODEL,
            Some((2.2, 0.3, 2.2)),
        );
    }
    for (spot, dx, dz) in &t.signs {
        let Some(s) = friendslop_world::places::spot(spot) else {
            continue;
        };
        let at = s + Vec2::new(*dx, *dz);
        let e = prop(
            &mut commands,
            at,
            arrow_yaw(at, mid),
            SIGN_MODEL,
            Some((0.15, 2.0, 0.15)),
        );
        commands.entity(e).insert((
            LilySign,
            Clickable::at_height(1.6).labelled("read the sign"),
        ));
    }
    for (i, w) in t.women.iter().enumerate().take(2) {
        let mut look = crate::people::CharacterLook::random(w.look);
        look.body = 0;
        look.clothes = w.clothes;
        let (local, yaw) = woman_spot(&t, i, false);
        commands.spawn((
            LilyWoman(i as u8),
            crate::people::Gait::default(),
            Transform::from_translation(ground(mid + local))
                .with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            look,
            Clickable::at_height(1.0).labelled("talk"),
            DespawnOnExit(AppState::InGame),
        ));
    }
    let pink = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.4, 0.62),
        emissive: LinearRgba::rgb(0.6, 0.15, 0.3),
        ..default()
    });
    for i in 0..t.heart_count {
        commands
            .spawn((
                LilyHeart(i),
                Visibility::Hidden,
                Transform::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_child((
                crate::features::romance::HeartShape,
                MeshMaterial3d(pink.clone()),
            ));
    }
}

/// Walk the women to the bench once the story's done (and back on a new session).
fn place_women(
    time: Res<Time>,
    stories: Query<&LilyStory>,
    mut women: Query<(&LilyWoman, &mut Transform)>,
) {
    let t = tuning();
    let together = stories
        .iter()
        .next()
        .is_some_and(|s| s.together(t.letters.len()));
    let mid = garden_at(&t);
    let k = (time.delta_secs() * 1.5).min(1.0);
    for (w, mut tf) in &mut women {
        let (local, yaw) = woman_spot(&t, w.0 as usize, together);
        let target = ground(mid + local);
        // A bench seat is a little higher than the ground.
        let target = target + Vec3::Y * if together { 0.05 } else { 0.0 };
        tf.translation = tf.translation.lerp(target, k);
        let face = if together {
            // Face the same way as the bench (towards -Z... after its flip, +Z).
            Quat::from_rotation_y(std::f32::consts::PI + yaw)
        } else {
            Quat::from_rotation_y(yaw)
        };
        tf.rotation = tf.rotation.slerp(face, k);
    }
}

fn float_hearts(
    time: Res<Time>,
    stories: Query<&LilyStory>,
    mut hearts: Query<(&LilyHeart, &mut Transform, &mut Visibility)>,
) {
    let t = tuning();
    let together = stories
        .iter()
        .next()
        .is_some_and(|s| s.together(t.letters.len()));
    let mid = garden_at(&t) + Vec2::new(t.bench.0, t.bench.1);
    let base = ground(mid);
    let now = time.elapsed_secs();
    for (h, mut tf, mut vis) in &mut hearts {
        *vis = if together {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let a = h.0 as f32 * 1.7 + now * 0.6;
        let rise = (now * 0.4 + h.0 as f32 * 0.37).fract();
        tf.translation = base + Vec3::new(a.cos() * 0.6, 1.8 + rise * 1.5, a.sin() * 0.6);
        tf.rotation = Quat::from_rotation_y(now);
        tf.scale = Vec3::splat(0.6 + (1.0 - rise) * 0.6);
    }
}

fn click(
    c: On<Clicked>,
    mut commands: Commands,
    women: Query<&LilyWoman>,
    signs: Query<(), With<LilySign>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    if let Ok(w) = women.get(c.entity) {
        commands.client_trigger(TalkToLily { woman: w.0 });
    } else if signs.contains(c.entity)
        && let Some(mut toast) = toast
    {
        toast.show(tuning().sign_line.clone());
    }
}

fn news(ev: On<LilyNews>, toast: Option<ResMut<crate::hud::Toast>>) {
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> std::sync::Arc<Tuning> {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        TUNING.get()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = load();
        assert!(!t.near.is_empty() && t.radius > 0.0);
        assert_eq!(t.women.len(), 2);
        assert!(t.women.iter().all(|w| !w.name.is_empty()));
        assert!(!t.beds.is_empty() && !t.signs.is_empty());
        assert!(!t.letters.is_empty());
        assert!(
            t.letters
                .iter()
                .all(|l| !l.take.is_empty() && !l.give.is_empty())
        );
        assert!(!t.sign_line.is_empty() && !t.waiting_line.is_empty());
        assert!(!t.carrying_line.is_empty() && !t.together_lines.is_empty());
        assert!(t.heart_count > 0);
    }

    #[test]
    fn letters_go_back_and_forth_until_theyre_together() {
        let mut s = LilyStory::default();
        let letters = 4;
        for n in 0..letters {
            let writer = (n % 2) as u8;
            let other = 1 - writer;
            assert_eq!(talk(s, letters, other, 7), Talk::Waiting);
            assert_eq!(talk(s, letters, writer, 7), Talk::Take(n));
            s.carrier = Some(7);
            assert_eq!(talk(s, letters, writer, 7), Talk::Carrying);
            // Somebody else can grab a fresh copy, but can't deliver yours.
            assert_eq!(talk(s, letters, other, 9), Talk::Waiting);
            assert_eq!(talk(s, letters, other, 7), Talk::Deliver(n));
            s.delivered += 1;
            s.carrier = None;
        }
        assert!(s.together(letters));
        assert_eq!(talk(s, letters, 0, 1), Talk::Together);
    }

    #[test]
    fn signs_point_at_the_garden() {
        for to in [Vec2::X, Vec2::Y, -Vec2::X, Vec2::new(-3.0, -4.0)] {
            let yaw = arrow_yaw(Vec2::ZERO, to);
            let dir = Quat::from_rotation_y(yaw) * Vec3::X;
            assert!(dir.xz().normalize().distance(to.normalize()) < 1e-4);
        }
    }
}
