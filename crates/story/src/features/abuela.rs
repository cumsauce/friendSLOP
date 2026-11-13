//! Abuela, who lives in heaven. From issue #105 by just meat and cheese: "for
//! the heaven aspect that's already implemented, could you add a character
//! similar to my grandma? ... she only spoke spanish so please have her make
//! old sounding spanish sounds".
//!
//! Heaven was only the campaign's goal, with no place to visit, so it gets a
//! small one: a soft cloudy garden with a golden glow on the highest floating
//! island (climb.rs), the closest the world gets to the sky. Abuela sits there
//! in her rocking chair, knitting: small and round, silver hair in a bun,
//! glasses, a shawl over a rose cardigan.
//!
//! When you climb up to her she greets you, and you can click her to talk.
//! The server decides what she says (`AbuelaSays`, to everyone) and counts the
//! visits on the replicated [`Abuela`]. Every peer then shows her words as a
//! subtitle with an English gloss, floats hearts above her, and plays her
//! voice: a soft, slightly breathy and wavering older woman's voice, made up
//! syllable by syllable from the Spanish phrase itself (open vowels, rolled
//! r's, soft s's, nasal ñ), quiet and heard at her chair. The one she spoke to
//! is told "you feel loved".

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::climb::{island, islands};
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::features::synth_audio::{RATE, master, play_synth_at};
use crate::hud::Toast;
use crate::people::{CharacterLook, Gait, HeadAnchor, RigRoot};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// The things she says, with what they mean.
pub static LINES: friendslop_data::Data<Vec<(String, String)>> =
    friendslop_data::Data::ron("data/text/abuela.ron");

/// Seat height of the rocking chair.
pub const SEAT: f32 = 0.45;

/// Tuning numbers: `assets/data/story/abuela.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How close (on the island) counts as visiting her.
    pub greet_radius: f32,
    /// She doesn't talk over herself.
    pub quiet_secs: f32,
    /// How long her words stay on screen.
    pub shown_secs: f32,
    /// Seconds per syllable: she speaks slowly.
    pub syl: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/story/abuela.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Where heaven is: the highest floating island.
pub fn heaven() -> crate::features::climb::Island {
    island(islands().len().saturating_sub(1))
}

/// Her chair, on the far side of heaven from where you climb up.
pub fn chair_at() -> Vec3 {
    let h = heaven();
    Vec3::new(h.at.x, h.top, h.at.y + h.radius * 0.35)
}

/// Is a body with feet at `feet` up in heaven, near her?
pub fn visiting(feet: Vec3, reach: f32) -> bool {
    within(feet, chair_at(), reach) && feet.y > heaven().top - 3.0
}

/// Abuela, on the server and replicated: how often she's been visited.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Abuela {
    pub visits: u32,
}

/// Client -> server: I'd like to talk to Abuela.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct VisitAbuela;

/// Server -> everyone: Abuela says `LINES[line]` to `player`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct AbuelaSays {
    pub player: u64,
    pub line: u8,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Abuela>()
        .add_client_event::<VisitAbuela>(Channel::Ordered)
        .add_server_event::<AbuelaSays>(Channel::Ordered)
        .init_resource::<Greeted>()
        .add_systems(OnEnter(ServerState::Running), spawn_abuela)
        .add_systems(
            Update,
            greet_arrivals.run_if(in_state(ServerState::Running)),
        )
        .add_observer(visit);
}

fn spawn_abuela(mut commands: Commands) {
    commands.spawn((Abuela::default(), Replicated));
}

/// Server only: who is with her right now, and when she last spoke.
#[derive(Resource, Default)]
struct Greeted {
    near: Vec<u64>,
    last: Option<f32>,
}

/// Server: Abuela speaks to `player`.
fn speak_to(
    commands: &mut Commands,
    abuela: &mut Abuela,
    greeted: &mut Greeted,
    now: f32,
    player: u64,
) {
    greeted.last = Some(now);
    let line = (abuela.visits as usize % LINES.get().len().max(1)) as u8;
    abuela.visits += 1;
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: AbuelaSays { player, line },
    });
}

/// Server: someone clicked her.
fn visit(
    ev: On<FromClient<VisitAbuela>>,
    mut commands: Commands,
    time: Res<Time>,
    mut greeted: ResMut<Greeted>,
    mut abuela: Query<&mut Abuela>,
    walkers: Query<(&Player, &PlayerOwner, &Walker)>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((player, _, walker)) = walkers.iter().find(|(_, o, _)| o.0 == owner) else {
        return;
    };
    if !visiting(walker.pos, SERVER_REACH) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let now = time.elapsed_secs();
    if greeted.last.is_some_and(|l| now - l < tuning().quiet_secs) {
        return; // still talking
    }
    let Ok(mut abuela) = abuela.single_mut() else {
        return;
    };
    speak_to(&mut commands, &mut abuela, &mut greeted, now, player.id);
}

/// Server: she greets whoever comes up to her.
fn greet_arrivals(
    mut commands: Commands,
    time: Res<Time>,
    mut greeted: ResMut<Greeted>,
    mut abuela: Query<&mut Abuela>,
    walkers: Query<(&Player, &Walker)>,
) {
    let Ok(mut abuela) = abuela.single_mut() else {
        return;
    };
    let now = time.elapsed_secs();
    let near: Vec<u64> = walkers
        .iter()
        .filter(|(_, w)| visiting(w.pos, tuning().greet_radius))
        .map(|(p, _)| p.id)
        .collect();
    let quiet = greeted.last.is_none_or(|l| now - l >= tuning().quiet_secs);
    let arrived = near.iter().copied().find(|id| !greeted.near.contains(id));
    if let (true, Some(id)) = (quiet, arrived) {
        greeted.near.push(id);
        speak_to(&mut commands, &mut abuela, &mut greeted, now, id);
    }
    // Forget whoever left, so she says hello again when they come back.
    greeted.near.retain(|id| near.contains(id));
}

pub fn plugin(app: &mut App) {
    app.add_observer(hear_abuela)
        .add_observer(click_abuela)
        .add_systems(OnEnter(AppState::InGame), build_heaven)
        .add_systems(
            Update,
            (dress_abuela, glasses_and_bun, rock, show_words).run_if(in_state(AppState::InGame)),
        );
}

/// Her look, made with the same character system as everyone else.
pub fn look() -> CharacterLook {
    CharacterLook {
        body: 0,
        height: 0.0, // small
        girth: 0.95, // and round
        head: 0.55,
        limbs: 0.15,
        skin: 2,
        clothes: 11, // rose cardigan
        accent: 0,   // cream shawl
        eyes: 2,     // soft, sleepy-kind eyes
        ears: 0,
        horns: 0,
        tail: 0,
        snout: 1, // button nose
        accessory: 4,
    }
}

#[derive(Component)]
struct Dressed;

fn dress_abuela(
    mut commands: Commands,
    new: Query<Entity, (With<Abuela>, Without<Dressed>)>,
    assets: Option<Res<AssetServer>>,
) {
    let Some(assets) = assets else {
        return;
    };
    // Modeler-made chair, yarn and needles (sources in `.modeler/`).
    let model = |file: &str| {
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
        )
    };
    for entity in &new {
        commands
            .entity(entity)
            .insert((
                Dressed,
                Transform::from_translation(chair_at()),
                Visibility::default(),
                Clickable::at_height(1.0)
                    .radius(70.0)
                    .labelled("talk to Abuela"),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                // She sits in her chair: the rig is a child, so the chair
                // and Abuela rock together (see `rock`).
                p.spawn((
                    Chair,
                    Transform::default(),
                    Visibility::default(),
                    Gait::default(),
                    look(),
                ))
                .with_children(|c| {
                    c.spawn((model("rocking_chair.gltf"), Transform::default()));
                    // Knitting in her lap: a ball of yarn and two needles.
                    c.spawn((
                        model("yarn_ball.gltf"),
                        Transform::from_xyz(0.05, SEAT + 0.32, -0.35),
                    ));
                    for side in [-1.0f32, 1.0] {
                        c.spawn((
                            model("knitting_needle.gltf"),
                            Transform::from_xyz(side * 0.08, SEAT + 0.42, -0.38)
                                .with_rotation(Quat::from_rotation_z(side * 0.7)),
                            Knitting(side),
                        ));
                    }
                });
            });
    }
}

/// The rocking chair (and Abuela in it).
#[derive(Component)]
struct Chair;

/// A knitting needle, clicking away on this side.
#[derive(Component)]
struct Knitting(f32);

/// Head radius her hair model was made for (`abuela_hair.gltf` is in
/// head-local space: origin at the head's centre, facing -Z).
const HAIR_FOR_HEAD: f32 = 0.266;

/// Silver hair with a bun and round glasses, on her head so they turn with it.
fn glasses_and_bun(
    mut commands: Commands,
    new: Query<(Entity, &HeadAnchor), Added<HeadAnchor>>,
    chairs: Query<(), With<Chair>>,
    assets: Option<Res<AssetServer>>,
) {
    let Some(assets) = assets else {
        return;
    };
    for (anchor, head) in &new {
        if chairs.get(head.root).is_err() {
            continue;
        }
        // Modeler-made (source `.modeler/prefabs/np-abuela-hair.json`).
        let hair =
            assets.load(GltfAssetLabel::Scene(0).from_asset("models/features/abuela_hair.gltf"));
        commands.entity(anchor).with_children(|h| {
            h.spawn((
                WorldAssetRoot(hair),
                Transform::from_scale(Vec3::splat(head.radius / HAIR_FOR_HEAD)),
            ));
        });
    }
}

/// Slow, gentle rocking, and the needles go click, click.
fn rock(
    time: Res<Time>,
    mut chairs: Query<(&mut Transform, &Children), With<Chair>>,
    mut bodies: Query<&mut Transform, (With<RigRoot>, Without<Chair>, Without<Knitting>)>,
    mut needles: Query<(&Knitting, &mut Transform), (Without<Chair>, Without<RigRoot>)>,
) {
    let t = time.elapsed_secs();
    for (mut tf, children) in &mut chairs {
        tf.rotation = Quat::from_rotation_x((t * 0.9).sin() * 0.07);
        for child in children.iter() {
            if let Ok(mut body) = bodies.get_mut(child) {
                // Sitting: down onto the cushion, a little squashed.
                body.translation = Vec3::new(0.0, -0.25, 0.05);
                body.scale = Vec3::new(1.0, 0.8, 1.0);
            }
        }
    }
    for (k, mut tf) in &mut needles {
        tf.rotation = Quat::from_rotation_z(k.0 * (0.7 + (t * 3.0 + k.0).sin() * 0.15));
    }
}

/// Every peer: heaven, a soft cloudy garden with a golden glow.
fn build_heaven(
    mut commands: Commands,
    assets: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(assets), Some(mut meshes), Some(mut materials)) = (assets, meshes, materials) else {
        return;
    };
    let h = heaven();
    let gold = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.85, 0.45),
        emissive: LinearRgba::rgb(1.6, 1.2, 0.4),
        ..default()
    });
    // Modeler-made cloud puff and flowers (sources `.modeler/prefabs/np-heaven-*`).
    let model = |file: &str| {
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
        )
    };
    let flowers = [
        "heaven_flower_pink.gltf",
        "heaven_flower_butter.gltf",
        "heaven_flower_lilac.gltf",
    ];
    // Clouds hugging the rim.
    for i in 0..14 {
        let a = i as f32 / 14.0 * std::f32::consts::TAU;
        let r = h.radius + 0.3 + crate::art::hash(i, 3, 105) * 0.6;
        let size = 0.7 + crate::art::hash(i, 7, 105) * 0.6;
        commands.spawn((
            model("heaven_cloud.gltf"),
            Transform::from_xyz(h.at.x + a.cos() * r, h.top - 0.4, h.at.y + a.sin() * r)
                .with_rotation(Quat::from_rotation_y(-a))
                .with_scale(Vec3::new(size * 1.4, size * 0.6, size)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // Little flowers in the grass.
    for i in 0..30 {
        let a = crate::art::hash(i, 1, 205) * std::f32::consts::TAU;
        let r = crate::art::hash(i, 2, 205).sqrt() * (h.radius - 0.6);
        let at = Vec3::new(h.at.x + a.cos() * r, h.top + 0.06, h.at.y + a.sin() * r);
        if at.xz().distance(chair_at().xz()) < 0.9 {
            continue;
        }
        commands.spawn((
            model(flowers[i as usize % flowers.len()]),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(a * 3.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // A golden halo arch behind her chair: a glow, so it stays in code.
    let behind = chair_at() + Vec3::new(0.0, 0.0, 1.0);
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(0.06, 1.5))),
        MeshMaterial3d(gold),
        Transform::from_translation(behind + Vec3::Y * 1.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.85, 0.6),
            intensity: 400_000.0,
            range: 12.0,
            ..default()
        },
        Transform::from_translation(chair_at() + Vec3::new(0.0, 3.0, -1.0)),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Local: clicking her asks the server if she'll talk.
fn click_abuela(ev: On<Clicked>, mut commands: Commands, abuela: Query<(), With<Abuela>>) {
    if abuela.get(ev.entity).is_ok() {
        commands.client_trigger(VisitAbuela);
    }
}

/// Her words on screen, until this time.
#[derive(Default)]
struct Words {
    text: String,
    until: f32,
}

#[derive(Component)]
struct WordsText;

/// Every peer: she speaks. Voice at her chair, hearts above her, the words on
/// screen for whoever is up in heaven, and a warm toast for the one she's
/// talking to.
#[allow(clippy::too_many_arguments)]
fn hear_abuela(
    says: On<AbuelaSays>,
    mut commands: Commands,
    time: Res<Time>,
    state: Option<Res<State<AppState>>>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    audio: Option<Res<Assets<AudioSource>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    toast: Option<ResMut<Toast>>,
    mut words: Query<&mut WordsBox>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let lines = LINES.get();
    let Some((spanish, english)) = lines.get(says.line as usize).cloned() else {
        return;
    };
    if audio.is_some() {
        let seed = says.line as u32 * 31 + says.player as u32;
        let carry = crate::spatial::Carry::NEARBY;
        let spanish = spanish.clone();
        play_synth_at(&mut commands, chair_at() + Vec3::Y, carry, 0.6, move || {
            wav_bytes(&voice(&spanish, seed, RATE), RATE)
        });
    }
    if let Some(mut materials) = materials {
        crate::features::romance::spawn_hearts(
            &mut commands,
            &mut materials,
            chair_at() + Vec3::Y * 1.6,
        );
    }
    let me = me.map(|m| m.0);
    let mine = me == Some(says.player);
    let up_here = me.is_some_and(|m| {
        walkers
            .iter()
            .any(|(p, w)| p.id == m && visiting(w.pos, 12.0))
    });
    if mine && let Some(mut toast) = toast {
        toast.show("Abuela holds your hands. You feel loved.");
        toast.secs_left = tuning().shown_secs;
    }
    if mine || up_here {
        for mut w in &mut words {
            w.0 = Words {
                text: format!("Abuela: \"{spanish}\"\n({english})"),
                until: time.elapsed_secs() + tuning().shown_secs,
            };
        }
    }
}

/// Where the words live (one per game, spawned by `show_words`).
#[derive(Component, Default)]
struct WordsBox(Words);

fn show_words(
    mut commands: Commands,
    time: Res<Time>,
    mut boxes: Query<(&WordsBox, &mut Text, &mut Visibility), With<WordsText>>,
) {
    if boxes.is_empty() {
        commands.spawn((
            Text::new(""),
            WordsText,
            WordsBox::default(),
            TextFont::from_font_size(20.0),
            TextColor(Color::srgb(1.0, 0.9, 0.75)),
            TextLayout::justify(Justify::Center),
            crate::hud::slot(crate::hud::HudSlot::Subtitle),
            Visibility::Hidden,
            DespawnOnExit(AppState::InGame),
        ));
        return;
    }
    let now = time.elapsed_secs();
    for (w, mut text, mut vis) in &mut boxes {
        let on = now < w.0.until;
        if on && text.0 != w.0.text {
            text.0.clone_from(&w.0.text);
        }
        vis.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Vowel formants (Hz) of a soft older woman's voice: a, e, i, o, u.
fn formants(v: char) -> [(f32, f32); 2] {
    match v {
        'a' => [(850.0, 160.0), (1250.0, 200.0)],
        'e' => [(520.0, 130.0), (1900.0, 230.0)],
        'i' => [(330.0, 110.0), (2400.0, 260.0)],
        'o' => [(520.0, 130.0), (920.0, 170.0)],
        _ => [(360.0, 110.0), (820.0, 160.0)],
    }
}

/// How a syllable starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Onset {
    None,
    /// m, n, ñ, l, y: a soft hum into the vowel.
    Hum,
    /// s, z, c (soft), j, f: a breath of noise.
    Hiss,
    /// r: a rolled trill.
    Trill,
    /// everything else: a short soft tap.
    Tap,
}

/// The phrase as syllables: (onset, vowel, end of word?).
pub fn syllables(phrase: &str) -> Vec<(Onset, char, bool)> {
    let mut out = Vec::new();
    let mut onset = Onset::None;
    let lower: Vec<char> = phrase
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            c => c,
        })
        .collect();
    for (i, &c) in lower.iter().enumerate() {
        let next = lower.get(i + 1).copied().unwrap_or(' ');
        match c {
            'a' | 'e' | 'i' | 'o' | 'u' => {
                // "ie", "ue", "io": the first vowel glides into the next.
                if matches!(c, 'i' | 'u') && matches!(next, 'a' | 'e' | 'o') {
                    continue;
                }
                let end = !next.is_alphabetic();
                out.push((onset, c, end));
                onset = Onset::None;
            }
            'm' | 'n' | 'ñ' | 'l' | 'y' => onset = Onset::Hum,
            's' | 'z' | 'c' | 'j' | 'f' | 'g'
                if !matches!(c, 'c' | 'g') || matches!(next, 'e' | 'i') =>
            {
                onset = Onset::Hiss
            }
            'r' => onset = Onset::Trill,
            'h' => {}
            c if c.is_alphabetic() => onset = Onset::Tap,
            _ => {}
        }
    }
    out
}

/// Abuela's voice saying `phrase`: a warm, breathy older woman's voice with a
/// gentle tremble, built from the phrase's own Spanish syllables. Pitch drifts
/// down over a statement and lifts at the end of a question.
pub fn voice(phrase: &str, seed: u32, rate: u32) -> Vec<f32> {
    use std::f32::consts::TAU;
    let syl = syllables(phrase);
    let question = phrase.contains('?');
    let mut rng = seed.wrapping_mul(2_654_435_761).wrapping_add(0x5eed);
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng as f32 / u32::MAX as f32
    };
    let mut out: Vec<f32> = Vec::new();
    let mut phase = 0.0f32;
    let total = syl.len().max(1) as f32;
    let dt = 1.0 / rate as f32;
    for (k, &(onset, vowel, end)) in syl.iter().enumerate() {
        let u = k as f32 / total;
        // Intonation: a gentle fall, or a lift at the end of a question.
        let contour = if question && u > 0.7 {
            1.0 + (u - 0.7) * 0.6
        } else {
            1.08 - 0.15 * u
        };
        let f0 = 205.0 * contour * (0.97 + next() * 0.06);
        // The last syllable of a word lingers a little.
        let len = tuning().syl * if end { 1.35 } else { 1.0 } * (0.9 + next() * 0.2);
        let n = (len * rate as f32) as usize;
        let fm = formants(vowel);
        let onset_len = match onset {
            Onset::None => 0.0,
            Onset::Tap => 0.025,
            Onset::Hum => 0.06,
            Onset::Hiss => 0.07,
            Onset::Trill => 0.08,
        };
        let start = out.len();
        for i in 0..n {
            let t = i as f32 * dt;
            let abs_t = (start + i) as f32 * dt;
            // An older voice trembles a little, slowly, and wavers in pitch.
            let tremble =
                1.0 + 0.035 * (abs_t * 5.3 * TAU).sin() + 0.01 * (abs_t * 1.7 * TAU).sin();
            let f = f0 * tremble;
            phase = (phase + f * dt).fract();
            let in_onset = t < onset_len;
            // Voiced part: harmonics shaped by the vowel (or a nasal hum).
            let shape: [(f32, f32); 2] = if in_onset && onset == Onset::Hum {
                [(280.0, 90.0), (1100.0, 300.0)]
            } else {
                fm
            };
            let mut v = 0.0;
            for h in 1..=14 {
                let fh = f * h as f32;
                let amp: f32 = shape
                    .iter()
                    .map(|(c, w)| (-((fh - c) / w).powi(2)).exp())
                    .sum::<f32>()
                    + 0.25 / h as f32;
                v += amp * (phase * h as f32 * TAU).sin();
            }
            let noise = next() * 2.0 - 1.0;
            let mut voiced = 1.0;
            let mut breath = 0.12; // a soft breathiness throughout
            if in_onset {
                match onset {
                    Onset::Hiss => {
                        voiced = 0.15;
                        breath = 0.5;
                    }
                    Onset::Trill => voiced = 0.35 + 0.65 * (t * 26.0 * TAU).sin().abs(),
                    Onset::Tap => voiced = 0.3,
                    Onset::Hum => voiced = 0.6,
                    Onset::None => {}
                }
            }
            let p = i as f32 / n as f32;
            let env = (p * 8.0).min(1.0) * ((1.0 - p) * 4.0).min(1.0);
            out.push((v * voiced * 0.12 + noise * breath * 0.15) * env);
        }
        if end {
            // A small breath between words.
            out.extend(std::iter::repeat_n(0.0, (0.07 * rate as f32) as usize));
        }
    }
    if out.is_empty() {
        out.push(0.0);
    }
    for s in &mut out {
        *s = s.tanh();
    }
    // Warm and quiet: soften the top end, keep the level low.
    master(&mut out, rate, 3800.0, 0.35);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_syllables() {
        let s = syllables("¡Ay, mi niño!");
        let vowels: String = s.iter().map(|x| x.1).collect();
        assert_eq!(vowels, "aiio");
        assert_eq!(s[1].0, Onset::Hum);
        let r = syllables("Abrígate");
        assert!(r.iter().any(|x| x.0 == Onset::Trill));
        assert!(syllables("Cuídate").len() >= 2);
        assert!(syllables("Siempre")[0].0 == Onset::Hiss);
    }

    #[test]
    fn her_voice_is_soft_and_sane() {
        for (i, (es, _)) in LINES.get().iter().enumerate() {
            let v = voice(es, i as u32, RATE);
            assert!(v.len() > RATE as usize / 4, "{es}");
            assert!(v.iter().all(|x| x.is_finite()));
            let peak = v.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.1 && peak <= 0.36, "{es}: {peak}");
        }
    }

    #[test]
    fn she_is_in_heaven() {
        let h = heaven();
        assert!(h.top >= islands().iter().map(|i| i.top).fold(0.0, f32::max));
        assert!(visiting(chair_at(), 1.0));
        assert!(!visiting(chair_at().with_y(0.0), 1.0), "not from below");
        assert!(look().clamped() == look());
    }
}
