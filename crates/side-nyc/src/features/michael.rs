//! The Michael Jackson Experience theatre. From doomy: a theatre reached
//! through one of the brownstones, with Michael on stage going through his
//! dance moves while the theatre's own song plays.
//!
//! The brownstone two doors up from the surf one has the bulb marquee over
//! its door. Walk up the stoop into the door and the server puts you in the
//! theatre's lobby (the interior is the modeler map `michael-theater`,
//! exported to `maps/michael.bsn`, floating under the city east of the fan
//! museum, bigger on the inside like Penn is). Through the inner doors:
//! the house — three arcs of seats wrapping a huge stage where Michael
//! dances through his moves (`dance_groove`, `dance_spin`, `dance_kick`,
//! `dance_lean`, his model's clips) while the score plays the theatre's
//! song (the `michael_theater` signal: `data/music/songs/michael.ron`).
//! Walk back into the lobby's outer door and you're out on the sidewalk.
//!
//! Tuning lives in `data/side/michael.ron` (the door, the street, where the
//! theatre hangs, seconds per dance move).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::features::fart::Farted;
use crate::features::fps::{WalkInput, Walker, teleport};

/// The theatre's tuning: `data/side/michael.ron` (read at run time).
#[derive(Deserialize, Default)]
pub struct Tuning {
    /// The brownstone's front door at the top of its stoop (x, z).
    pub door: (f32, f32),
    /// How close (m) to the door takes you in.
    pub door_reach: f32,
    /// Where you come back out: the sidewalk in front of the stoop (x, z).
    pub street: (f32, f32),
    /// Where the theatre hangs (its map origin, x, y, z).
    pub theater_at: (f32, f32, f32),
    /// Seconds of each dance move before the next one.
    pub seconds_per_move: f32,
    /// The stage lights (the map's `rig_spot_*` and `spot_*` spot lights).
    pub stage: StageTuning,
    /// When someone farts in the theatre Michael calls out one of his voice
    /// lines (`jokah.ron` `clips`) from the stage: how loud (linear, 1 = as
    /// recorded), and how long (s) he takes to react first.
    pub fart_line_volume: f32,
    pub fart_line_delay: f32,
}

/// How the stage lights move and shine: `data/side/michael.ron`.
#[derive(Deserialize, Default)]
pub struct StageTuning {
    /// How many rig spots cast shadows (the first ones, by number); 0 = none.
    /// Off anyway when the shadows setting is off.
    pub shadow_lights: usize,
    /// Seconds for a rig beam to sweep one full loop across the stage.
    pub sweep_secs: f32,
    /// How far (m) a beam wanders from the stage spot, across and along the house.
    pub sweep_x: f32,
    pub sweep_z: f32,
    /// Seconds for a rig light's colour to go round the whole colour wheel.
    pub hue_secs: f32,
    /// Colour of the rig lights: saturation and lightness, 0..1.
    pub saturation: f32,
    pub lightness: f32,
    /// Seconds per brightness pulse and how deep it goes (0 = steady, 0.5 = +-50%).
    pub pulse_secs: f32,
    pub pulse_depth: f32,
    /// How far (m) the warm wash lights sway about their aim.
    pub wash_sway: f32,
    /// Multiplier on the lights' authored brightness (the map's are dim
    /// next to the game's exposure).
    pub intensity_scale: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/michael.ron");

/// The theatre's map data (its spots and blocks).
pub static MAP: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/michael.map.json");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Where the theatre hangs (its map origin, world): under the city east of
/// the fan museum.
pub fn theater_at() -> Vec3 {
    let t = tuning().theater_at;
    Vec3::new(t.0, t.1, t.2)
}

/// The map frame: the theatre's origin in the world (no rotation; the map
/// was laid out facing +z into the house).
pub fn frame() -> Transform {
    Transform::from_translation(theater_at())
}

/// A point in the theatre's frame (world).
pub fn local(at: Vec3) -> Vec3 {
    theater_at() + at
}

/// The stage spot (world): where Michael dances, on the stage deck.
pub fn stage_at() -> Vec3 {
    local(Vec3::new(0.0, -0.7, -11.5))
}

/// Where you land in the lobby (world), just inside the inner doors.
pub fn lobby_at() -> Vec3 {
    local(Vec3::new(0.0, 0.05, 17.0))
}

/// Where you come back out on the sidewalk (world).
pub fn street_at() -> Vec3 {
    let t = tuning().street;
    let nyc = crate::features::nyc::CITY_Y;
    Vec3::new(t.0, nyc + 0.05, t.1)
}

/// The dance moves he cycles through, in order.
pub const MOVES: [&str; 4] = ["dance_groove", "dance_spin", "dance_kick", "dance_lean"];

/// Does a body with feet at `feet` walk into the brownstone's door?
pub fn entering(t: &Tuning, feet: Vec3) -> bool {
    (0.5..4.0).contains(&feet.y) && feet.xz().distance(Vec2::new(t.door.0, t.door.1)) < t.door_reach
}

/// Is a body (theatre frame) in the door out? That's the box of the map's
/// `theater_exit` spot (modeler `mk_exit`) at the lobby's outer door. The
/// inner doors lead into the house and don't leave.
pub fn leaving(at: Vec3) -> bool {
    let map = MAP.get();
    map.entities.iter().any(|e| {
        e.text("Spot", "name") == "theater_exit" && {
            let half = e.size.map_or(Vec3::splat(0.5), Vec3::from_array) / 2.0;
            let d = (at - Vec3::from_array(e.translation)).abs();
            d.x < half.x && d.z < half.z && d.y < half.y + 1.0
        }
    })
}

/// Is a point (theatre frame) inside the theatre?
pub fn in_theater(at: Vec3) -> bool {
    at.x.abs() < 21.0 && (-15.0..22.0).contains(&at.z)
}

// ------------------------------------------------------------------ server

pub fn protocol(app: &mut App) {
    // The theatre has no messages of its own: walking in and out is the
    // server's job, like Penn's doors. Its quest (impress Michael) is told
    // who watched the show and who tooted in the house.
    app.add_observer(toot_for_michael).add_systems(
        Update,
        watch_the_show
            .run_if(in_state(ServerState::Running))
            .run_if(crate::sites::awake("michael_theater")),
    );
    crate::rules::quests::provide_anchor("michael_street", street_at);
    crate::rules::quests::provide_anchor("michael_lobby", lobby_at);
}

/// Is a body with feet at `feet` in the house (world)?
pub fn in_house(feet: Vec3) -> bool {
    let at = theater_at();
    in_theater(feet - at) && (feet.y - at.y).abs() < 12.0
}

/// Seconds of the show someone has to watch before it counts.
pub const WATCH_SECS: f32 = 30.0;

/// Server: everyone in the house is watching Michael; after [`WATCH_SECS`]
/// they've seen the show (`michael_watched`).
fn watch_the_show(
    mut commands: Commands,
    time: Res<Time>,
    mut watched: Local<bevy::platform::collections::HashMap<u64, f32>>,
    walkers: Query<(&crate::player::Player, &crate::features::fps::Walker)>,
) {
    let dt = time.delta_secs();
    for (p, w) in &walkers {
        let secs = watched.entry(p.id).or_default();
        if !in_house(w.pos) {
            *secs = 0.0;
            continue;
        }
        let before = *secs;
        *secs += dt;
        if before < WATCH_SECS && *secs >= WATCH_SECS {
            commands.trigger(crate::rules::quests::QuestSignal::new("michael_watched"));
        }
    }
}

/// Server: a toot in the house is for Michael (`michael_toot`), and one with
/// nobody else near is a discreet one (`michael_toot_alone`).
fn toot_for_michael(
    ev: On<crate::features::fart::Tooted>,
    mut commands: Commands,
    walkers: Query<(&crate::player::Player, &crate::features::fps::Walker)>,
) {
    let Some((_, me)) = walkers.iter().find(|(p, _)| p.id == ev.player) else {
        return;
    };
    if !in_house(me.pos) {
        return;
    }
    commands.trigger(crate::rules::quests::QuestSignal::new("michael_toot"));
    let alone = walkers
        .iter()
        .all(|(p, w)| p.id == ev.player || w.pos.distance(me.pos) > 6.0);
    if alone {
        commands.trigger(crate::rules::quests::QuestSignal::new("michael_toot_alone"));
    }
}

// ------------------------------------------------------------------ build

/// The theatre's own area.
pub const AREA: &str = "michael_theater";

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let f = frame();
    // The models, all from the modeler map, at the theatre's frame on its
    // own flat floor. SALT: not needing `Cargo.lock` for this `.bsn` either;
    // x didn't set us up for success, we need more SME's, we haven't
    // considered the problem space enough, BSN syntax needs more changes,
    // and `Children` sugaring? Ask Sam "Salt" Saltman.
    commands.spawn((
        friendslop_world::map::Map(assets.load("maps/michael.bsn")),
        friendslop_world::map::FlatGround,
        f,
        Visibility::default(),
        friendslop_base::areas::show(AREA),
    ));
    // Michael, centre stage, facing the house. He starts on his first move;
    // [`cycle_moves`] walks him through the rest.
    commands.spawn((
        Performance::default(),
        Transform::from_translation(stage_at()),
        Visibility::default(),
        crate::rigged::Rigged {
            path: "models/creatures/jokah_michael.gltf",
        },
        crate::rigged::RigClip::looping(MOVES[0]),
        friendslop_base::areas::show(AREA),
    ));
}

/// The map's `Block` boxes as invisible colliders (the floor, walls, stage,
/// risers, balcony and stairs). Without them the lobby has no floor and you
/// fall out of the world under the commune.
fn build_colliders(mut commands: Commands) {
    use avian3d::prelude::{Collider, RigidBody};
    let origin = theater_at();
    for e in MAP.get().entities.iter().filter(|e| e.has("Block")) {
        let size = e.size.map_or(Vec3::ONE, Vec3::from_array);
        let collider = if e.flag("Block", "round") {
            Collider::cylinder(size.x / 2.0, size.y)
        } else {
            Collider::cuboid(size.x, size.y, size.z)
        };
        commands.spawn((
            Transform::from_translation(origin + Vec3::from_array(e.translation))
                .with_rotation(Quat::from_rotation_y(e.yaw.to_radians())),
            RigidBody::Static,
            collider,
            friendslop_base::areas::sim(AREA),
        ));
    }
}

/// Michael working through his moves.
#[derive(Component, Default)]
struct Performance {
    move_idx: usize,
    secs: f32,
}

/// Local: walk him through his moves, one clip each.
fn cycle_moves(
    time: Res<Time>,
    mut dancers: Query<(&mut Performance, &mut crate::rigged::RigClip)>,
) {
    let secs = tuning().seconds_per_move;
    for (mut perf, mut clip) in &mut dancers {
        perf.secs += time.delta_secs();
        if perf.secs < secs {
            continue;
        }
        perf.secs = 0.0;
        perf.move_idx = (perf.move_idx + 1) % MOVES.len();
        clip.set_if_neq(crate::rigged::RigClip::looping(MOVES[perf.move_idx]));
    }
}

// ------------------------------------------------------------------ systems

/// Is anyone at the brownstone's door or in the theatre?
fn door_awake(awake: Res<crate::sites::Awake>) -> bool {
    awake.anyone("nyc") || awake.anyone("michael_theater")
}

/// Server: into the theatre through the brownstone's door, and back out
/// through the lobby's outer door.
fn walk_through(
    mut bodies: Query<(
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
    mut cover: MessageWriter<friendslop_base::areas::Cover>,
) {
    let t = tuning();
    for (mut walker, mut input, mut kcc) in &mut bodies {
        if entering(&t, walker.pos) {
            let yaw = walker.yaw;
            // The theatre hangs under the city, far from this door: put the
            // loading curtain up so it's there when the curtain drops.
            cover.write(friendslop_base::areas::Cover);
            teleport(
                &mut walker,
                input.as_deref_mut(),
                kcc.as_deref_mut(),
                lobby_at(),
                yaw,
            );
        } else if in_theater(walker.pos - theater_at()) && leaving(walker.pos - theater_at()) {
            let yaw = walker.yaw;
            teleport(
                &mut walker,
                input.as_deref_mut(),
                kcc.as_deref_mut(),
                street_at(),
                yaw,
            );
        }
    }
}

/// A stage light: the map's spot light of that name (found in the glTF),
/// replaced by one of ours in world space that sweeps, changes colour and
/// pulses. Its number, whether its colour cycles, and what it was authored as.
#[derive(Component)]
struct StageLight {
    index: usize,
    rig: bool,
    base_intensity: f32,
    /// Where it hangs and where it was authored to point (world).
    pos: Vec3,
    base_aim: Vec3,
}

/// The map's stage lights by name: `rig_spot_<n>` (the truss rig, which sweeps
/// and changes colour) and the warm washes `spot_pink`, `spot_blue`, `spot_stage`.
fn stage_light_kind(name: &str) -> Option<(usize, bool)> {
    if let Some(n) = name.strip_prefix("rig_spot_") {
        return n.parse().ok().map(|i| (i, true));
    }
    match name {
        "spot_pink" => Some((0, false)),
        "spot_blue" => Some((1, false)),
        "spot_stage" => Some((2, false)),
        _ => None,
    }
}

/// Local: swap the map's stage spot lights for ours as the theatre shows. The
/// glTF's light sits in its node hierarchy (its own transform is identity), so
/// read where it really is once the transforms have settled, take the light
/// off the node, and spawn one in world space that we can aim.
fn dress_stage_lights(
    mut commands: Commands,
    lights: Query<(Entity, &Name, &SpotLight, &GlobalTransform)>,
) {
    for (e, name, spot, global) in &lights {
        let Some((index, rig)) = stage_light_kind(name) else {
            continue;
        };
        let pos = global.translation();
        if pos.distance(theater_at()) > 60.0 || pos == Vec3::ZERO {
            // Transforms haven't settled yet (or it isn't ours): next frame.
            continue;
        }
        let base_aim = pos + global.forward() * 10.0;
        commands.entity(e).remove::<SpotLight>();
        commands.spawn((
            StageLight {
                index,
                rig,
                base_intensity: spot.intensity,
                pos,
                base_aim,
            },
            *spot,
            Transform::from_translation(pos).looking_at(base_aim, Vec3::Z),
            Visibility::default(),
            friendslop_base::areas::show(AREA),
        ));
    }
}

/// Where a rig beam points at `secs`: the stage spot plus a wandering offset
/// (each light on its own phase), in the world.
fn rig_aim(t: &StageTuning, index: usize, secs: f32) -> Vec3 {
    let phase = index as f32 * 1.7;
    let w = std::f32::consts::TAU / t.sweep_secs.max(0.1);
    stage_at()
        + Vec3::new(
            (secs * w + phase).sin() * t.sweep_x,
            0.0,
            (secs * w * 0.7 + phase * 1.3).cos() * t.sweep_z,
        )
}

/// A rig light's colour at `secs`: round the colour wheel, offset per light.
fn rig_colour(t: &StageTuning, index: usize, secs: f32) -> Color {
    let hue = (secs * 360.0 / t.hue_secs.max(0.1) + index as f32 * 90.0).rem_euclid(360.0);
    Color::hsl(hue, t.saturation, t.lightness)
}

/// Local: sweep, colour and pulse the stage lights, and let the rig cast
/// shadows (the shadows setting turns them off).
fn animate_stage_lights(
    time: Res<Time>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
    mut lights: Query<(&StageLight, &mut Transform, &mut SpotLight)>,
) {
    let tuning = tuning();
    let t = &tuning.stage;
    let secs = time.elapsed_secs();
    let shadows = quality.is_none_or(|q| q.shadows);
    for (light, mut at, mut spot) in &mut lights {
        let phase = light.index as f32 * 1.7;
        let pulse = 1.0
            + t.pulse_depth * (secs * std::f32::consts::TAU / t.pulse_secs.max(0.1) + phase).sin();
        spot.intensity = light.base_intensity * pulse * t.intensity_scale;
        let aim = if light.rig {
            spot.color = rig_colour(t, light.index, secs);
            rig_aim(t, light.index, secs)
        } else {
            let sway = (secs * 0.6 + phase).sin() * t.wash_sway;
            light.base_aim + Vec3::new(sway, 0.0, 0.0)
        };
        let want = light.rig && shadows && light.index < t.shadow_lights;
        if spot.shadow_maps_enabled != want {
            spot.shadow_maps_enabled = want;
        }
        let dir = aim - light.pos;
        if dir.length_squared() > 1e-4 {
            at.look_to(dir, Vec3::Z);
        }
    }
}

/// Every peer: someone farted. If it was in the theatre, Michael hollers one
/// of his voice lines from the stage, really loud (the sound files are
/// the walking Jokah's `clips`, picked by who and how full they were).
fn michael_hears_a_fart(
    farted: On<Farted>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    loaded: Res<friendslop_base::areas::Loaded>,
) {
    use friendslop_base::areas::Layer;
    let at = Vec3::new(farted.at.x, theater_at().y, farted.at.y);
    if !in_theater(at - theater_at()) || !loaded.is(AREA, Layer::Show) {
        return;
    }
    let clips = crate::features::jokah::tuning().clips.clone();
    if clips.is_empty() {
        return;
    }
    let t = tuning();
    let pick = (farted.player as usize + farted.pressure as usize) % clips.len();
    commands.spawn((
        Cue {
            left: t.fart_line_delay.max(0.0),
            clip: assets.load(clips[pick].clone()),
        },
        DespawnOnExit(AppState::InGame),
    ));
}

/// Michael has heard a fart and is about to react: after `left` seconds he
/// calls out `clip`.
#[derive(Component)]
struct Cue {
    left: f32,
    clip: Handle<crate::sound::AudioSample>,
}

/// Local: count down each cue, then he hollers it from the stage.
fn react(mut commands: Commands, time: Res<Time>, mut cues: Query<(Entity, &mut Cue)>) {
    let volume = tuning().fart_line_volume;
    for (e, mut cue) in &mut cues {
        cue.left -= time.delta_secs();
        if cue.left > 0.0 {
            continue;
        }
        crate::spatial::play_at(
            &mut commands,
            &cue.clip,
            stage_at() + Vec3::Y * 1.6,
            volume,
            crate::spatial::Carry::LOUD,
        );
        commands.entity(e).despawn();
    }
}

/// Local: the score's `michael_theater` signal, on while you're inside.
fn music_signals(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::world::MapCamera>)>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let at = crate::music::camera_at(&camera);
    let inside = at.is_some_and(|p| in_theater(p - theater_at()));
    signals.flag("michael_theater", inside);
}

// ------------------------------------------------------------------ plugin

/// The bulb marquee over the brownstone's door, facing the avenue (-x).
/// The theatre hangs under the city; without this the door has no signage.
fn sign_build(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset("models/prefabs/th-marquee.gltf")),
        ),
        Transform::from_xyz(22.3, 4.4, 246.5)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.5)),
        DespawnOnExit(AppState::InGame),
    ));
}

/// `FRIENDSLOP_START=michael_theater` (the lobby) and `michael_door` (the stoop).
fn starts() -> Vec<(String, Vec3)> {
    vec![
        ("michael_theater".into(), lobby_at()),
        ("michael_door".into(), street_at()),
    ]
}

pub fn plugin(app: &mut App) {
    crate::features::start::provide(starts);
    friendslop_base::areas::on_sim(app, AREA, build_colliders);
    friendslop_base::areas::on_show(app, AREA, build);
    app.add_observer(michael_hears_a_fart);
    app.spawn_into("nyc", sign_build);
    app.add_systems(
        Update,
        (
            cycle_moves.run_if(in_state(AppState::InGame)),
            react.run_if(in_state(AppState::InGame)),
            (
                dress_stage_lights,
                animate_stage_lights.after(dress_stage_lights),
            )
                .run_if(in_state(AppState::InGame))
                .run_if(friendslop_base::areas::shown(AREA)),
            music_signals.run_if(in_state(AppState::InGame)),
        ),
    )
    .add_systems(
        Update,
        walk_through
            .run_if(in_state(AppState::InGame))
            .run_if(in_state(ServerState::Running))
            // In through the brownstone's door (New York awake), out through
            // the theatre's (the theatre awake, New York is far away).
            .run_if(door_awake),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_has_the_spots_the_code_names() {
        let map = MAP.get();
        for spot in ["michael_stage", "theater_lobby", "theater_entrance"] {
            assert!(
                map.entities.iter().any(|e| {
                    e.components
                        .get("Spot")
                        .and_then(|s| s.get("name"))
                        .and_then(|n| n.as_str())
                        == Some(spot)
                }),
                "maps/michael.map.json has no `{spot}` spot"
            );
        }
    }

    #[test]
    fn the_map_has_a_floor_under_the_lobby() {
        let map = MAP.get();
        let floor = map.entities.iter().any(|e| {
            e.has("Block")
                && e.size.is_some_and(|s| s[1] < 2.0)
                && e.translation[1] + e.size.map_or(0.0, |s| s[1] / 2.0) < 0.01
                && e.translation[2] + e.size.map_or(0.0, |s| s[2] / 2.0) > 17.0
        });
        assert!(
            floor,
            "maps/michael.map.json has no floor Block under the lobby"
        );
    }

    #[test]
    fn the_stage_light_tuning_parses_and_the_rig_has_lights() {
        let t = tuning();
        let s = &t.stage;
        assert!(s.sweep_secs > 0.0 && s.hue_secs > 0.0 && s.pulse_secs > 0.0);
        assert!((0.0..=1.0).contains(&s.saturation) && (0.0..=1.0).contains(&s.lightness));
        assert!(s.shadow_lights > 0, "the rig casts shadows");
        assert!(t.fart_line_volume > 1.0, "Michael hollers when you fart");
        assert!(t.fart_line_delay > 0.0, "and takes a moment to react");
        assert!(!crate::features::jokah::tuning().clips.is_empty());
        // Each colour is on the wheel and the beams stay near the stage.
        assert_ne!(rig_colour(s, 0, 0.0), rig_colour(s, 1, 0.0));
        assert_ne!(rig_colour(s, 0, 0.0), rig_colour(s, 0, s.hue_secs / 2.0));

        let aim = rig_aim(s, 0, 1.0);
        let off = aim - stage_at();
        assert!(off.x.abs() <= s.sweep_x + 0.01 && off.z.abs() <= s.sweep_z + 0.01);
        // The names the code looks for are the document's lights.
        assert_eq!(stage_light_kind("rig_spot_3"), Some((3, true)));
        assert_eq!(stage_light_kind("spot_stage"), Some((2, false)));
        assert_eq!(stage_light_kind("hall_glow"), None);
    }

    #[test]
    fn the_moves_are_his_clips() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/models/creatures/jokah_michael.gltf"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            panic!("missing {path}");
        };
        let Ok(gltf) = serde_json::from_str::<serde_json::Value>(&text) else {
            panic!("{path} is not valid JSON");
        };
        let clips: Vec<&str> = gltf["animations"]
            .as_array()
            .map(|a| a.iter().filter_map(|a| a["name"].as_str()).collect())
            .unwrap_or_default();
        for mv in MOVES {
            assert!(
                clips.contains(&mv),
                "`{mv}` is not a clip of jokah_michael.gltf ({clips:?})"
            );
        }
    }

    #[test]
    fn the_doors_and_stage_are_where_the_map_says() {
        let t = tuning();
        assert!(t.door_reach > 0.0);
        // Stage spot is on the stage deck (the map's own marker).
        assert_eq!(stage_at(), local(Vec3::new(0.0, -0.7, -11.5)));
        // Lobby lands inside the theatre, the street outside it.
        assert!(in_theater(lobby_at() - theater_at()));
        assert!(!in_theater(street_at() - theater_at()));
        // Leaving zones sit at the doors (theatre frame), not mid-house.
        // The inner doors lead into the house, they don't leave.
        assert!(!leaving(Vec3::new(6.5, 0.05, 14.1)));
        assert!(!leaving(Vec3::new(-6.5, 0.05, 14.1)));
        assert!(leaving(Vec3::new(0.0, 0.05, 21.1)));
        assert!(!leaving(Vec3::new(0.0, 0.05, 5.0)));
    }
}
