//! An affair with a beautiful intelligence officer. From issue #160 by doomy:
//! "allow me to cheat on my wife with a beautiful chinese intelligence
//! officer" (no details: surprise me).
//!
//! - **Your wife** walks back and forth between the bonfire and the east side
//!   of the commune ([`wife_at`]). It's a commune: she's married to all of you.
//! - **Officer Mei**, an intelligence officer in a dress uniform and peaked
//!   cap, waits at the east edge ([`officer_at()`]). Click her to flirt. If your
//!   wife is within [`WIFE_SIGHT`] of you when you do, she catches you.
//! - [`FLIRTS_FOR_AFFAIR`] flirts and you're having an affair: pink hearts
//!   float up from you and from Mei (everyone sees). Every flirt after that she
//!   "debriefs" you and you blurt out one of the commune's secrets.
//! - While the affair is on, if your wife ever gets within [`PERFUME_RANGE`]
//!   of you she smells the perfume. Caught either way: the affair is over,
//!   everyone is told, and you lose morality.
//! - Every flirt is a small moral act ([`Act::Cheat`]).
//!
//! The server owns the wife's walk ([`Wife`], replicated) and each player's
//! [`Affair`], and re-checks reach.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, in_reach};
use crate::features::morality::{Act, MoralAct};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Where Officer Mei waits (x, z): the east edge of the commune.
pub use friendslop_world::places::officer_at;
/// Tuning numbers: `assets/data/side/affair.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The wife catches a flirt within this distance of you.
    pub wife_sight: f32,
    /// During an affair she smells the perfume this close.
    pub perfume_range: f32,
    /// Flirts until it's an affair.
    pub flirts_for_affair: u32,
    /// The wife's walking speed, m/s.
    pub wife_speed: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/affair.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// Replicated wife moves per second.
const WIFE_HZ: f32 = 5.0;

/// The wife's beat (x, z): bonfire, the middle, and the east side, then back.
const PATH: [Vec2; 4] = [
    Vec2::new(-8.5, 0.5),
    Vec2::new(0.0, 2.0),
    Vec2::new(8.0, 3.0),
    Vec2::new(12.0, -1.0),
];

/// Server -> everyone: where the wife is.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Wife {
    pub pos: Vec2,
    pub yaw: f32,
}

/// Each player's standing with Officer Mei.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Affair {
    pub flirts: u32,
}

impl Affair {
    pub fn on(self) -> bool {
        self.flirts >= tuning().flirts_for_affair
    }
}

/// Client -> server: I flirt with Officer Mei.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Flirt;

/// Server -> client(s): what happened.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AffairNews {
    pub text: String,
}

/// Length of the wife's beat, one way.
fn path_len() -> f32 {
    PATH.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Where the wife is `t` seconds into her walk, and which way she faces.
/// She walks the path there and back, forever.
pub fn wife_at(t: f32) -> (Vec2, f32) {
    let len = path_len();
    let mut d = (t * tuning().wife_speed).rem_euclid(2.0 * len);
    let back = d > len;
    if back {
        d = 2.0 * len - d;
    }
    for w in PATH.windows(2) {
        let seg = w[0].distance(w[1]);
        if d <= seg || seg <= 0.0 {
            let k = if seg > 0.0 { d / seg } else { 0.0 };
            let pos = w[0].lerp(w[1], k);
            let dir = if back { w[0] - w[1] } else { w[1] - w[0] };
            // Bodies face -Z: yaw so that -Z points along `dir`.
            return (pos, f32::atan2(-dir.x, -dir.y));
        }
        d -= seg;
    }
    (PATH[PATH.len() - 1], 0.0)
}

/// What one flirt does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Your wife saw. The affair (or the start of one) is over.
    Caught,
    /// Still courting: line `n`.
    Courting(u32),
    /// It's an affair now.
    Begins,
    /// She debriefs you: secret `n`.
    Secret(u32),
}

/// Flirt once, with your wife `wife_dist` metres away. Pure, so it's tested.
pub fn flirt(affair: &mut Affair, wife_dist: f32) -> Outcome {
    if wife_dist <= tuning().wife_sight {
        affair.flirts = 0;
        return Outcome::Caught;
    }
    affair.flirts += 1;
    match affair.flirts {
        n if n < tuning().flirts_for_affair => Outcome::Courting(n - 1),
        n if n == tuning().flirts_for_affair => Outcome::Begins,
        n => Outcome::Secret(n - tuning().flirts_for_affair - 1),
    }
}

const COURTING: [&str; 2] = [
    "Officer Mei laughs at your joke. \"you're funny. where does the commune keep its organs?\"",
    "Officer Mei straightens your collar. \"tell me about the landlord's vault. just for fun\"",
];

const SECRETS: [&str; 6] = [
    "she debriefs you. you tell her where Sewer Paul hides his crow eggs",
    "she debriefs you. you tell her the Stillness comes every minute. she writes it down",
    "she debriefs you. you tell her Abuela's recipe. she photographs it with a tiny camera",
    "she debriefs you. you tell her the duel arena wobbles a bit on the left",
    "she debriefs you. you tell her the goat knows too much. she already knew",
    "she debriefs you. you tell her the pond has opinions. she asks which ones",
];

fn line(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Caught => "your wife saw you flirting with Officer Mei! it's over",
        Outcome::Courting(n) => COURTING[n as usize % COURTING.len()],
        Outcome::Begins => {
            "Officer Mei: \"meet me here again, darling. don't tell your wife\" (you're having an affair)"
        }
        Outcome::Secret(n) => SECRETS[n as usize % SECRETS.len()],
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Wife>()
        .replicate::<Affair>()
        .add_client_event::<Flirt>(Channel::Ordered)
        .add_server_event::<AffairNews>(Channel::Ordered)
        .add_observer(give_affair)
        .add_observer(on_flirt)
        .add_systems(OnEnter(ServerState::Running), spawn_wife)
        .add_systems(
            Update,
            (walk_wife, smell_perfume)
                .chain()
                .run_if(in_state(ServerState::Running)),
        );
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

fn tell(commands: &mut Commands, targets: SendTargets, text: impl Into<String>) {
    commands.server_trigger(ToClients {
        targets,
        message: AffairNews { text: text.into() },
    });
}

fn give_affair(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Affair::default());
    }
}

fn spawn_wife(mut commands: Commands) {
    let (pos, yaw) = wife_at(0.0);
    commands.spawn((Wife { pos, yaw }, Replicated));
}

/// Server: the wife walks her beat (and stands still in the Stillness).
fn walk_wife(
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    mut wives: Query<&mut Wife>,
    mut clock: Local<f32>,
    mut since: Local<f32>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    *clock += time.delta_secs();
    *since += time.delta_secs();
    if *since < 1.0 / WIFE_HZ {
        return;
    }
    *since = 0.0;
    let (pos, yaw) = wife_at(*clock);
    for mut wife in &mut wives {
        *wife = Wife { pos, yaw };
    }
}

fn wife_distance(wives: &Query<&Wife>, at: Vec3) -> f32 {
    wives
        .iter()
        .map(|w| w.pos.distance(at.xz()))
        .fold(f32::INFINITY, f32::min)
}

fn caught_news(id: u64) -> String {
    format!("player {id}'s wife caught them with Officer Mei!")
}

fn on_flirt(
    ev: On<FromClient<Flirt>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    wives: Query<&Wife>,
    mut players: Query<(Entity, &Player, &PlayerOwner, &mut Affair)>,
) {
    let target = Vec3::new(officer_at().x, 0.0, officer_at().y);
    if !in_reach(&mut commands, ev.client_id, &walkers, target) {
        return;
    }
    let owner = owner_of(ev.client_id);
    let Some(me) = walkers
        .iter()
        .find(|(o, _)| o.0 == owner)
        .map(|(_, w)| w.pos)
    else {
        return;
    };
    let Some((entity, player, _, mut affair)) =
        players.iter_mut().find(|(_, _, o, _)| o.0 == owner)
    else {
        return;
    };
    let outcome = flirt(&mut affair, wife_distance(&wives, me));
    tell(
        &mut commands,
        SendTargets::Single(ev.client_id),
        line(outcome),
    );
    if outcome == Outcome::Caught {
        tell(&mut commands, SendTargets::All, caught_news(player.id));
    }
    commands.trigger(MoralAct {
        player: entity,
        act: Act::Cheat,
    });
}

/// Server: mid-affair, the wife smells the perfume if she gets close.
fn smell_perfume(
    mut commands: Commands,
    wives: Query<&Wife>,
    mut players: Query<(Entity, &Player, &Walker, &mut Affair)>,
) {
    for (entity, player, walker, mut affair) in &mut players {
        if !affair.on() || wife_distance(&wives, walker.pos) > tuning().perfume_range {
            continue;
        }
        affair.flirts = 0;
        tell(
            &mut commands,
            SendTargets::All,
            format!(
                "{} (she smelled Officer Mei's perfume on them)",
                caught_news(player.id)
            ),
        );
        commands.trigger(MoralAct {
            player: entity,
            act: Act::Cheat,
        });
    }
}

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_observer(show_news)
        .add_systems(OnEnter(AppState::InGame), (spawn_officer, spawn_help))
        .add_systems(
            Update,
            (dress_wife, move_wife, hearts, rise).run_if(in_state(AppState::InGame)),
        );
}

/// Officer Mei's body (local, the same on every peer).
#[derive(Component)]
struct Officer;

/// The wife's body is dressed.
#[derive(Component)]
struct Dressed;

fn officer_look() -> crate::people::CharacterLook {
    crate::people::CharacterLook {
        body: 0,
        height: 0.6,
        girth: 0.4,
        head: 0.5,
        limbs: 0.6,
        skin: 1,
        clothes: 4,
        accent: 4,
        eyes: 1,
        accessory: 1, // a flat cap, standing in for a peaked one
        ..default()
    }
}

fn wife_look() -> crate::people::CharacterLook {
    crate::people::CharacterLook {
        body: 0,
        height: 0.45,
        girth: 0.55,
        skin: 2,
        clothes: 9,
        accent: 1,
        eyes: 2,
        accessory: 5, // an apron
        ..default()
    }
}

fn spawn_officer(mut commands: Commands) {
    let y = crate::terrain::height_at(officer_at().x, officer_at().y);
    // Facing back towards the commune.
    let face = -officer_at();
    commands.spawn((
        Officer,
        crate::people::Gait::default(),
        Transform::from_xyz(officer_at().x, y, officer_at().y)
            .with_rotation(Quat::from_rotation_y(f32::atan2(-face.x, -face.y))),
        Visibility::default(),
        Clickable::at_height(1.0)
            .radius(60.0)
            .labelled("flirt with Officer Mei"),
        officer_look(),
        DespawnOnExit(AppState::InGame),
    ));
}

fn dress_wife(mut commands: Commands, new: Query<(Entity, &Wife), Without<Dressed>>) {
    for (entity, w) in &new {
        let y = crate::terrain::height_at(w.pos.x, w.pos.y);
        commands.entity(entity).insert((
            Dressed,
            crate::people::Gait::default(),
            Transform::from_xyz(w.pos.x, y, w.pos.y).with_rotation(Quat::from_rotation_y(w.yaw)),
            Visibility::default(),
            Clickable::at_height(1.0)
                .radius(60.0)
                .labelled("talk to your wife"),
            wife_look(),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Glide to the replicated spot.
fn move_wife(time: Res<Time>, mut wives: Query<(&Wife, &mut Transform), With<Dressed>>) {
    let t = 1.0 - (-6.0 * time.delta_secs()).exp();
    for (w, mut tf) in &mut wives {
        let target = Vec3::new(
            w.pos.x,
            crate::terrain::height_at(w.pos.x, w.pos.y),
            w.pos.y,
        );
        tf.translation = tf.translation.lerp(target, t);
        tf.rotation = tf.rotation.slerp(Quat::from_rotation_y(w.yaw), t);
    }
}

fn click(
    ev: On<Clicked>,
    mut commands: Commands,
    officer: Query<(), With<Officer>>,
    wives: Query<(), With<Wife>>,
    me: Option<Res<LocalPlayer>>,
    affairs: Query<(&Player, &Affair)>,
    toast: Option<ResMut<Toast>>,
) {
    if officer.contains(ev.entity) {
        commands.client_trigger(Flirt);
        return;
    }
    if !wives.contains(ev.entity) {
        return;
    }
    let cheating = me
        .as_deref()
        .is_some_and(|m| affairs.iter().any(|(p, a)| p.id == m.0 && a.on()));
    let text = if cheating {
        "your wife: \"you smell like expensive perfume. and cigarettes. and lies\""
    } else {
        "your wife: \"hi honey. who's that woman in the uniform?\""
    };
    if let Some(mut toast) = toast {
        toast.show(text);
    }
}

fn show_news(ev: On<AffairNews>, toast: Option<ResMut<Toast>>) {
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

fn spawn_help(mut commands: Commands) {
    commands.spawn((
        Text::new(
            "Officer Mei (east edge): flirt for an affair. don't let your wife (she's everyone's) see",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

/// A rising, fading pink heart-bubble.
#[derive(Component)]
struct Heart {
    age: f32,
}

/// Lovers (and Mei, while anyone is in love with her) give off hearts.
fn hearts(
    mut commands: Commands,
    time: Res<Time>,
    lovers: Query<(&Affair, &GlobalTransform)>,
    officer: Query<&GlobalTransform, With<Officer>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut since: Local<f32>,
    mut kit: Local<Option<(Handle<Mesh>, Handle<StandardMaterial>)>>,
) {
    *since += time.delta_secs();
    if *since < 0.5 {
        return;
    }
    *since = 0.0;
    let mut spots: Vec<Vec3> = lovers
        .iter()
        .filter(|(a, _)| a.on())
        .map(|(_, tf)| tf.translation())
        .collect();
    if spots.is_empty() {
        return;
    }
    spots.extend(officer.iter().map(|tf| tf.translation()));
    let (mesh, mat) = kit
        .get_or_insert_with(|| {
            (
                meshes.add(Sphere::new(0.09)),
                materials.add(StandardMaterial {
                    base_color: Color::srgb(1.0, 0.4, 0.65),
                    emissive: LinearRgba::rgb(0.8, 0.15, 0.4),
                    unlit: true,
                    ..default()
                }),
            )
        })
        .clone();
    for at in spots {
        commands.spawn((
            Heart { age: 0.0 },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at + Vec3::Y * 1.9),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

fn rise(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Heart, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (e, mut h, mut tf) in &mut q {
        h.age += dt;
        if h.age > 2.0 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation += Vec3::new((h.age * 4.0).sin() * 0.15, 0.6, 0.0) * dt;
        tf.scale = Vec3::splat((1.0 - h.age / 2.0).max(0.05));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_flirts_make_an_affair_then_secrets_spill() {
        let mut a = Affair::default();
        assert_eq!(flirt(&mut a, 50.0), Outcome::Courting(0));
        assert_eq!(flirt(&mut a, 50.0), Outcome::Courting(1));
        assert!(!a.on());
        assert_eq!(flirt(&mut a, 50.0), Outcome::Begins);
        assert!(a.on());
        assert_eq!(flirt(&mut a, 50.0), Outcome::Secret(0));
    }

    #[test]
    fn the_wife_catches_flirts_she_can_see() {
        let mut a = Affair { flirts: 5 };
        assert_eq!(flirt(&mut a, tuning().wife_sight - 1.0), Outcome::Caught);
        assert!(!a.on());
        assert_eq!(a.flirts, 0);
    }

    #[test]
    fn the_wife_walks_there_and_back() {
        let len = path_len();
        let (start, _) = wife_at(0.0);
        let (far, _) = wife_at(len / tuning().wife_speed);
        let (home, _) = wife_at(2.0 * len / tuning().wife_speed);
        assert!(start.distance(PATH[0]) < 0.01);
        assert!(far.distance(PATH[PATH.len() - 1]) < 0.01);
        assert!(home.distance(PATH[0]) < 0.01);
        // Her far end brings her within sight of Mei, so flirting is timing.
        assert!(far.distance(officer_at()) < tuning().wife_sight);
        assert!(start.distance(officer_at()) > tuning().wife_sight);
    }

    #[test]
    fn mei_stays_out_of_the_duel_arena() {
        assert_eq!(
            super::super::villagers::outside_arena(officer_at()),
            officer_at()
        );
    }
}
