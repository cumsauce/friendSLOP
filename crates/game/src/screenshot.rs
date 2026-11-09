//! Screenshot mode (#167): grab one picture of the game and quit, so each new
//! feature (and each release) can be shown off in the friends' Discord channel.
//!
//! Not a game feature: nothing changes unless `FRIENDSLOP_SCREENSHOT` is set.
//!
//! ```sh
//! FRIENDSLOP_SCREENSHOT=shot.png cargo run -- host
//! # optional framing: eye x,y,z then look-at x,y,z (world metres)
//! FRIENDSLOP_SCREENSHOT_CAMERA="0,12,30,0,0,0" FRIENDSLOP_SCREENSHOT=shot.png cargo run -- host
//! # optional: seconds to wait after the loading curtain lifts (default 3)
//! FRIENDSLOP_SCREENSHOT_WAIT=6
//! ```
//!
//! On a machine without a GPU, run it under `xvfb-run` with Mesa's software
//! Vulkan driver (`mesa-vulkan-drivers`); see `scripts/screenshot.sh`.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use crate::AppState;
use crate::loading::WorldReady;
use crate::world::MainCamera;

/// What screenshot mode was asked to do. Present only when it's on.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ScreenshotJob {
    pub path: String,
    /// Eye position and look-at point, or `None` for the normal camera.
    pub camera: Option<(Vec3, Vec3)>,
    /// Real seconds to wait after the world is ready.
    pub wait: f32,
}

impl ScreenshotJob {
    /// Read the job from environment-style values (`None` = mode off).
    pub fn parse(path: Option<&str>, camera: Option<&str>, wait: Option<&str>) -> Option<Self> {
        let path = path.map(str::trim).filter(|p| !p.is_empty())?;
        Some(Self {
            path: path.to_owned(),
            camera: camera.and_then(parse_camera),
            wait: wait
                .and_then(|w| w.trim().parse::<f32>().ok())
                .filter(|w| w.is_finite() && *w >= 0.0)
                .unwrap_or(3.0),
        })
    }

    fn from_env() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok();
        Self::parse(
            var("FRIENDSLOP_SCREENSHOT").as_deref(),
            var("FRIENDSLOP_SCREENSHOT_CAMERA").as_deref(),
            var("FRIENDSLOP_SCREENSHOT_WAIT").as_deref(),
        )
    }
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

/// Screenshot mode. Does nothing unless `FRIENDSLOP_SCREENSHOT` is set.
pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        let Some(job) = ScreenshotJob::from_env() else {
            return;
        };
        info!("screenshot mode: will save {}", job.path);
        app.insert_resource(job)
            .init_resource::<Progress>()
            .add_systems(Update, take.run_if(in_state(AppState::InGame)))
            .add_systems(
                PostUpdate,
                frame_camera
                    .run_if(in_state(AppState::InGame))
                    .before(TransformSystems::Propagate),
            );
    }
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
            exit.write(AppExit::Success);
        }
        return;
    }
    if !ready.ready || progress.requested {
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
        assert_eq!(job.wait, 3.0);
        assert_eq!(parse_camera("1,1,1,1,1,1"), None);
        assert_eq!(parse_camera("x,1,1,0,0,0"), None);
    }
}
