//! Screenshot mode (#167): grab one picture of the game and quit, so each new
//! feature (and each release) can be shown off in the friends' Discord channel.
//!
//! Not a game feature: nothing changes unless `FRIENDSLOP_SCREENSHOT` is set.
//!
//! ```sh
//! FRIENDSLOP_SCREENSHOT=shot.png cargo run -- host
//! # optional framing: eye x,y,z then look-at x,y,z (world metres)
//! FRIENDSLOP_SCREENSHOT_CAMERA="0,12,30,0,0,0" FRIENDSLOP_SCREENSHOT=shot.png cargo run -- host
//! # optional: seconds to wait after the loading curtain lifts (default 10, so
//! # the 9 s "CHAPTER 1" card is gone)
//! FRIENDSLOP_SCREENSHOT_WAIT=12
//! # optional: force a HUD state for visual checks against the Figma frames
//! # (play, busy, clickable, chapter, tab; see `Scene`)
//! FRIENDSLOP_SCREENSHOT_SHOTS=shots.txt  (lines `out.png x,y,z,lx,ly,lz`; edit and save it
//!   while the game runs to take more pictures without restarting)
//! FRIENDSLOP_SCREENSHOT_SCENE=busy
//! FRIENDSLOP_SCREENSHOT_WAREHOUSE=7   (open the warehouse with floor plan 7: its inside is at y -60)
//! # optional: window size in logical pixels (scale factor forced to 1)
//! FRIENDSLOP_SCREENSHOT_SIZE=1920x1080
//! ```
//!
//! On a machine without a GPU, run it under `xvfb-run` with Mesa's software
//! Vulkan driver (`mesa-vulkan-drivers`); see `scripts/screenshot.sh`.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use bevy::input::InputSystems;
use bevy::window::{PrimaryWindow, WindowResolution};
use bevy_replicon::prelude::ServerState;

use crate::AppState;
use crate::features::fat_cat::{FatCat, Fuse};
use crate::features::fps::Walker;
use crate::features::punishment::AttackMode;
use crate::loading::WorldReady;
use crate::player::{LocalPlayer, Player};
use crate::world::MainCamera;

/// Seconds after loading before the picture: the chapter card (9 s) has gone.
const DEFAULT_WAIT: f32 = 10.0;

/// A HUD state screenshot mode forces, to compare the UI with its design
/// frames (`FRIENDSLOP_SCREENSHOT_SCENE`). Only the host's own view is staged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scene {
    /// The normal HUD, nothing staged.
    #[default]
    Play,
    /// ATTACK MODE on you, three toasts, and your fat cat critical with 8 s left.
    Busy,
    /// Aiming at your cat, in reach.
    Clickable,
    /// Shot while the chapter intro is still up.
    Chapter,
    /// The Tab panel held open.
    Tab,
    /// The title screen (run without `host`).
    Title,
    /// The title screen with the settings notebook open (run without `host`).
    Settings,
    /// The loading curtain, shot before it lifts.
    Loading,
    /// The world with no UI at all (README pictures).
    Clean,
}

impl Scene {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "play" | "" => Scene::Play,
            "busy" => Scene::Busy,
            "clickable" => Scene::Clickable,
            "chapter" => Scene::Chapter,
            "tab" => Scene::Tab,
            "title" => Scene::Title,
            "settings" => Scene::Settings,
            "loading" => Scene::Loading,
            "clean" => Scene::Clean,
            _ => return None,
        })
    }

    /// Seconds after loading before the picture, when no wait is given.
    fn default_wait(self) -> f32 {
        match self {
            // After the chapter card (9 s).
            Scene::Play | Scene::Tab | Scene::Busy | Scene::Clickable | Scene::Clean => {
                DEFAULT_WAIT
            }
            // Before the chapter intro shrinks away.
            Scene::Chapter => 2.0,
            // Real seconds after the title screen opens.
            Scene::Title | Scene::Settings => 4.0,
            // Real seconds after entering the game, curtain still up.
            Scene::Loading => 0.6,
        }
    }

    /// Shot on the title screen instead of in game.
    pub fn on_title(self) -> bool {
        matches!(self, Scene::Title | Scene::Settings)
    }

    /// Doesn't wait for the loading curtain to lift.
    fn early(self) -> bool {
        matches!(self, Scene::Title | Scene::Settings | Scene::Loading)
    }
}

/// The toasts the busy scene shows (the design frame's three).
pub const BUSY_TOASTS: [&str; 3] = [
    "Moss farted, then pointed at YOU",
    "You pocket a crow egg",
    "ACTION FAILED: everyone is bees",
];
/// How long before the busy picture the attack starts.
const ATTACK_LEAD: f32 = 1.5;
/// The busy scene's cat: nearly maxed out, fuse at 8 s.
pub const BUSY_CAT: (f32, f32) = (24.8, 8.0);

/// What screenshot mode was asked to do. Present only when it's on.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ScreenshotJob {
    pub path: String,
    /// Eye position and look-at point, or `None` for the normal camera.
    pub camera: Option<(Vec3, Vec3)>,
    /// Real seconds to wait after the world is ready.
    pub wait: f32,
    pub scene: Scene,
    /// Window size in logical pixels, or `None` for the default.
    pub size: Option<UVec2>,
    /// Live mode: keep running and save the picture again every this many
    /// seconds (for watching UI work against a design), instead of quitting.
    pub every: Option<f32>,
    /// Shot-list mode (`FRIENDSLOP_SCREENSHOT_SHOTS`, desktop only): a file of
    /// `out.png x,y,z,lx,ly,lz` lines the running game watches. Saving it
    /// moves the camera to each line in turn and saves that picture, so you
    /// can frame shots without restarting. The game keeps running.
    pub shots: Option<String>,
}

impl ScreenshotJob {
    /// Read the job from environment-style values (`None` = mode off).
    pub fn parse(path: Option<&str>, camera: Option<&str>, wait: Option<&str>) -> Option<Self> {
        Self::parse_all(path, camera, wait, None, None)
    }

    /// [`Self::parse`] plus the scene and window size. Unknown scenes and
    /// malformed sizes fall back to the defaults.
    pub fn parse_all(
        path: Option<&str>,
        camera: Option<&str>,
        wait: Option<&str>,
        scene: Option<&str>,
        size: Option<&str>,
    ) -> Option<Self> {
        let path = path.map(str::trim).filter(|p| !p.is_empty())?;
        let scene = scene.and_then(Scene::parse).unwrap_or_default();
        Some(Self {
            path: path.to_owned(),
            camera: camera.and_then(parse_camera),
            wait: wait
                .and_then(|w| w.trim().parse::<f32>().ok())
                .filter(|w| w.is_finite() && *w >= 0.0)
                .unwrap_or(scene.default_wait()),
            scene,
            size: size.and_then(parse_size),
            every: None,
            shots: None,
        })
    }

    fn from_env() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok();
        let mut job = Self::parse_all(
            var("FRIENDSLOP_SCREENSHOT").as_deref(),
            var("FRIENDSLOP_SCREENSHOT_CAMERA").as_deref(),
            var("FRIENDSLOP_SCREENSHOT_WAIT").as_deref(),
            var("FRIENDSLOP_SCREENSHOT_SCENE").as_deref(),
            var("FRIENDSLOP_SCREENSHOT_SIZE").as_deref(),
        )?;
        job.every = var("FRIENDSLOP_SCREENSHOT_EVERY")
            .as_deref()
            .and_then(parse_every);
        job.shots = var("FRIENDSLOP_SCREENSHOT_SHOTS").filter(|f| !f.trim().is_empty());
        Some(job)
    }
}

/// One line of a shot list: `out.png x,y,z,lx,ly,lz`. Blank lines and
/// `#` comments are skipped, and so is anything malformed.
pub fn parse_shot(line: &str) -> Option<(String, (Vec3, Vec3))> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (path, camera) = line.split_once(char::is_whitespace)?;
    Some((path.to_owned(), parse_camera(camera)?))
}

/// Shot-list mode: what's waiting, and the file as last read.
#[derive(Resource, Default)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // shot lists are desktop only
struct ShotQueue {
    seen: String,
    pending: std::collections::VecDeque<(String, (Vec3, Vec3))>,
    /// Real time of the last look at the file.
    checked: f32,
}

/// Seconds a new camera spot settles (models and trees stream in) before
/// its picture.
#[cfg(not(target_arch = "wasm32"))]
const SHOT_SETTLE: f32 = 2.5;

/// Shot-list mode: reread the file when it changes and start the next shot
/// once the last one is saved.
#[cfg(not(target_arch = "wasm32"))]
fn run_shot_list(
    time: Res<Time<Real>>,
    mut job: ResMut<ScreenshotJob>,
    mut queue: ResMut<ShotQueue>,
    mut progress: ResMut<Progress>,
) {
    let Some(file) = job.shots.clone() else {
        return;
    };
    let now = time.elapsed_secs();
    if now - queue.checked > 0.5 {
        queue.checked = now;
        if let Ok(text) = std::fs::read_to_string(&file)
            && text != queue.seen
        {
            queue.pending = text.lines().filter_map(parse_shot).collect();
            queue.seen = text;
        }
    }
    // Idle (the last picture is saved, or the first one never asked for)?
    let idle = progress.requested && progress.saved_frames.is_none();
    if !idle {
        return;
    }
    let Some((path, camera)) = queue.pending.pop_front() else {
        return;
    };
    info!("shot list: {path}");
    job.path = path;
    job.camera = Some(camera);
    progress.requested = false;
    progress.ready_at = Some(now - job.wait + SHOT_SETTLE);
}

#[cfg(target_arch = "wasm32")]
fn run_shot_list() {}

/// `"1920x1080"` into a size. Anything malformed or tiny is ignored.
/// Seconds between live re-saves (at least half a second).
fn parse_every(s: &str) -> Option<f32> {
    s.trim()
        .parse::<f32>()
        .ok()
        .filter(|e| e.is_finite() && *e > 0.0)
        .map(|e| e.max(0.5))
}

fn parse_size(s: &str) -> Option<UVec2> {
    let (w, h) = s.trim().split_once(['x', 'X'])?;
    let size = UVec2::new(w.trim().parse().ok()?, h.trim().parse().ok()?);
    (size.x >= 320 && size.y >= 240).then_some(size)
}

/// `"x,y,z,lx,ly,lz"` into (eye, look-at). Anything malformed is ignored.
fn parse_camera(s: &str) -> Option<(Vec3, Vec3)> {
    let n: Vec<f32> = s
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    let [x, y, z, lx, ly, lz] = n.as_slice() else {
        return None;
    };
    let (eye, at) = (Vec3::new(*x, *y, *z), Vec3::new(*lx, *ly, *lz));
    (eye.is_finite() && at.is_finite() && eye.distance(at) > 1e-3).then_some((eye, at))
}

#[derive(Resource, Default)]
struct Progress {
    /// Real time the world became ready.
    ready_at: Option<f32>,
    requested: bool,
    /// Frames since the picture was written (exit shortly after).
    saved_frames: Option<u32>,
}

/// The clean scene: every UI root hidden, so only the world is drawn.
fn hide_ui(mut roots: Query<&mut Visibility, (With<Node>, Without<ChildOf>)>) {
    for mut v in &mut roots {
        *v = Visibility::Hidden;
    }
}

/// Screenshot mode. Does nothing unless `FRIENDSLOP_SCREENSHOT` is set.
pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        let Some(job) = ScreenshotJob::from_env() else {
            return;
        };
        info!(
            "screenshot mode: will save {} (scene {:?})",
            job.path, job.scene
        );
        let scene = job.scene;
        app.insert_resource(job)
            .init_resource::<Progress>()
            .init_resource::<ShotQueue>()
            .add_systems(Startup, size_window)
            .add_systems(Update, run_shot_list.before(take))
            .add_systems(
                Update,
                take.run_if(in_state(if scene.on_title() {
                    AppState::Menu
                } else {
                    AppState::InGame
                })),
            )
            .add_systems(
                PostUpdate,
                frame_camera
                    .run_if(in_state(AppState::InGame))
                    .before(TransformSystems::Propagate),
            );
        // `FRIENDSLOP_SCREENSHOT_WAREHOUSE=<seed>`: open the warehouse with
        // that floor plan, so a shot list can look round its inside (y -60).
        if let Some(seed) = std::env::var("FRIENDSLOP_SCREENSHOT_WAREHOUSE")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
        {
            app.add_systems(
                Update,
                (move |mut commands: Commands,
                       mut wh: Query<&mut crate::features::warehouse::Warehouse>| {
                    if let Ok(mut wh) = wh.single_mut()
                        && wh.seed.is_none()
                    {
                        crate::features::warehouse::start_expedition(&mut commands, &mut wh, seed);
                    }
                })
                .run_if(in_state(AppState::InGame))
                .run_if(in_state(ServerState::Running)),
            );
        }
        match scene {
            Scene::Play | Scene::Chapter | Scene::Title | Scene::Loading => {}
            Scene::Clean => {
                app.add_systems(PostUpdate, hide_ui);
            }
            Scene::Settings => {
                app.add_systems(Update, open_settings.run_if(in_state(AppState::Menu)));
            }
            Scene::Busy => {
                app.add_systems(
                    Update,
                    (stage_attack, stage_critical_cat)
                        .run_if(in_state(AppState::InGame))
                        .run_if(in_state(ServerState::Running)),
                )
                .add_systems(Update, stage_toasts.run_if(in_state(AppState::InGame)));
            }
            Scene::Clickable => {
                app.add_systems(
                    PostUpdate,
                    aim_at_cat
                        .run_if(in_state(AppState::InGame))
                        .after(frame_camera)
                        .before(TransformSystems::Propagate),
                );
            }
            Scene::Tab => {
                app.add_systems(
                    PreUpdate,
                    hold_tab
                        .after(InputSystems)
                        .run_if(in_state(AppState::InGame)),
                );
            }
        }
    }
}

/// Give the window the asked-for logical size (at scale factor 1, so the
/// picture's pixels are the UI's logical pixels).
fn size_window(job: Res<ScreenshotJob>, mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let (Some(size), Ok(mut window)) = (job.size, window.single_mut()) else {
        return;
    };
    window.resolution = WindowResolution::new(size.x, size.y).with_scale_factor_override(1.0);
}

/// Busy scene: the villagers come for the host just before the picture
/// (any earlier and they've already caught them, which ends the attack).
fn stage_attack(
    mut commands: Commands,
    time: Res<Time<Real>>,
    job: Res<ScreenshotJob>,
    ready: Res<WorldReady>,
    attacks: Query<(), With<AttackMode>>,
    mut ready_at: Local<Option<f32>>,
) {
    if !ready.ready || !attacks.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    let since = now - *ready_at.get_or_insert(now);
    // In live mode, keep one going all the time.
    if job.every.is_some() || since >= job.wait - ATTACK_LEAD {
        crate::features::punishment::start_attack(&mut commands, 0);
    }
}

/// Busy scene: the host's cat held at critical mass with its fuse at 8 s.
fn stage_critical_cat(mut commands: Commands, mut cats: Query<(Entity, &mut FatCat)>) {
    let (size, fuse) = BUSY_CAT;
    for (e, mut cat) in &mut cats {
        if cat.owner != 0 {
            continue;
        }
        if cat.size != size {
            cat.size = size;
        }
        commands.entity(e).insert(Fuse(fuse));
    }
}

/// Busy scene: keep the three toasts coming.
fn stage_toasts(ready: Res<WorldReady>, mut toast: ResMut<crate::hud::Toast>) {
    if ready.ready && toast.secs_left < 1.0 {
        toast.show(BUSY_TOASTS.join("\n"));
    }
}

/// Clickable scene: look at your own cat from just out of its face, with
/// the aim point on it. Reach is measured from your body, which it heels to.
fn aim_at_cat(
    me: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &Walker)>,
    cats: Query<(&FatCat, &GlobalTransform)>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
) {
    let Some(me) = me else { return };
    let Some((_, walker)) = players.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let Some((_, cat)) = cats.iter().find(|(c, _)| c.owner == me.0) else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    let at = cat.translation() + Vec3::Y * 0.5;
    let toward_me = (walker.pos - at).with_y(0.0).normalize_or(Vec3::Z);
    let eye = at + toward_me * 1.8 + Vec3::Y * 0.6;
    *cam = Transform::from_translation(eye).looking_at(at, Vec3::Y);
}

/// Settings scene: open the notebook once.
fn open_settings(
    mut done: Local<bool>,
    mut panel: Query<&mut Node, With<crate::ui::settings_panel::SettingsPanel>>,
) {
    if !*done && !panel.is_empty() {
        *done = true;
        crate::ui::settings_panel::toggle(&mut panel);
    }
}

/// Tab scene: hold Tab down.
fn hold_tab(mut keys: ResMut<ButtonInput<KeyCode>>) {
    keys.press(KeyCode::Tab);
}

/// Hold the camera where the job asked, over whatever the game set.
fn frame_camera(job: Res<ScreenshotJob>, mut camera: Query<&mut Transform, With<MainCamera>>) {
    let Some((eye, at)) = job.camera else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    *cam = Transform::from_translation(eye).looking_at(at, Vec3::Y);
}

fn take(
    mut commands: Commands,
    time: Res<Time<Real>>,
    job: Res<ScreenshotJob>,
    ready: Res<WorldReady>,
    mut progress: ResMut<Progress>,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(frames) = progress.saved_frames.as_mut() {
        *frames += 1;
        if *frames > 2 {
            if job.shots.is_some() {
                // Shot list: go idle until the list hands over the next one.
                progress.saved_frames = None;
                return;
            }
            match job.every {
                // Live mode: wait `every` seconds, then take the next one.
                Some(every) => {
                    progress.saved_frames = None;
                    progress.requested = false;
                    progress.ready_at = Some(time.elapsed_secs() - job.wait + every);
                }
                None => {
                    exit.write(AppExit::Success);
                }
            }
        }
        return;
    }
    if !(ready.ready || job.scene.early()) || progress.requested {
        return;
    }
    let now = time.elapsed_secs();
    let ready_at = *progress.ready_at.get_or_insert(now);
    if now - ready_at < job.wait {
        return;
    }
    progress.requested = true;
    let mut save = save_to_disk(job.path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut progress: ResMut<Progress>| {
            save(captured);
            progress.saved_frames = Some(0);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shot_lines() {
        let Some((p, (eye, at))) = parse_shot("  a.png 1,2,3, 0,0,0") else {
            panic!("shot")
        };
        assert_eq!(
            (p.as_str(), eye, at),
            ("a.png", Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO)
        );
        assert_eq!(parse_shot("# note"), None);
        assert_eq!(parse_shot("b.png 1,2"), None);
    }

    #[test]
    fn off_without_a_path() {
        assert_eq!(ScreenshotJob::parse(None, Some("1,2,3,0,0,0"), None), None);
        assert_eq!(ScreenshotJob::parse(Some("  "), None, None), None);
    }

    #[test]
    fn reads_path_camera_and_wait() {
        let job = ScreenshotJob::parse(Some("a.png"), Some("0, 10, 20, 0,0,0"), Some("5"));
        let Some(job) = job else { panic!("job") };
        assert_eq!(job.path, "a.png");
        assert_eq!(job.camera, Some((Vec3::new(0.0, 10.0, 20.0), Vec3::ZERO)));
        assert_eq!(job.wait, 5.0);
    }

    #[test]
    fn bad_camera_and_wait_fall_back() {
        let Some(job) = ScreenshotJob::parse(Some("a.png"), Some("1,2,3"), Some("-1")) else {
            panic!("job")
        };
        assert_eq!(job.camera, None);
        assert_eq!(job.wait, DEFAULT_WAIT);
        assert_eq!(parse_camera("1,1,1,1,1,1"), None);
        assert_eq!(parse_camera("x,1,1,0,0,0"), None);
    }

    #[test]
    fn live_mode_waits_at_least_half_a_second() {
        assert_eq!(parse_every("2"), Some(2.0));
        assert_eq!(parse_every("0.1"), Some(0.5));
        assert_eq!(parse_every("0"), None);
        assert_eq!(parse_every("x"), None);
    }

    #[test]
    fn reads_scene_and_size() {
        let job =
            ScreenshotJob::parse_all(Some("a.png"), None, None, Some("Busy"), Some("1920x1080"));
        let Some(job) = job else { panic!("job") };
        assert_eq!(job.scene, Scene::Busy);
        assert_eq!(job.wait, Scene::Busy.default_wait());
        assert_eq!(job.size, Some(UVec2::new(1920, 1080)));
        let Some(job) =
            ScreenshotJob::parse_all(Some("a.png"), None, Some("3"), Some("nope"), Some("1x1"))
        else {
            panic!("job")
        };
        assert_eq!((job.scene, job.wait, job.size), (Scene::Play, 3.0, None));
        for s in [
            "play",
            "busy",
            "clickable",
            "chapter",
            "tab",
            "title",
            "settings",
            "loading",
            "clean",
        ] {
            assert!(Scene::parse(s).is_some(), "{s}");
        }
    }
}
