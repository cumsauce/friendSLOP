//! The Jokah (#377): "The NPC spawns at Wendy's and paths towards the J
//! station. He laughs to himself. He has 3 unique laughs. When he reaches J
//! station he transforms into a Michael Jackson impersonator and paths back
//! to Wendy's. When he is in Michael Jackson mode he makes scatting noises and
//! has a random sliding animation."
//!
//! He starts outside Wendy's front door and walks the route in
//! `data/side/jokah.ron` to the J station (the Fulton St entrance, where the
//! J train is found), laughing one of his three laughs every few seconds
//! (speech-synth laughs at three pitches, no sound files). At the station he
//! pauses, changes clothes and cap ([`Mode::Michael`]) and walks the route back,
//! scatting and every so often sliding: a moonwalk backwards or a glide to
//! one side for a second or so. Back at Wendy's he turns into the Jokah again
//! and sets off once more (the issue doesn't say what happens there; looping
//! keeps him around).
//!
//! It's local dressing like the street people: every peer runs its own Jokah
//! from the data file, nothing is replicated. He's drawn by two rigged modeler
//! models (sources `.modeler/prefabs/np-jokah.json` and `np-jokah-mj.json`):
//! the Jokah in his purple tailcoat and the same man in Michael's black suit
//! and fedora, each with `idle`, `walk` and `slide` clips. A missing file only
//! makes him invisible; his walk and his sounds go on.

use bevy::prelude::*;
use serde::Deserialize;

use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player};

use super::street_people::{between, hash};

/// One of his laughs: what the speech synth says, at what pitch and speed.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Laugh {
    pub text: String,
    pub pitch: f32,
    pub speed: f32,
}

/// Tuning: `assets/data/side/jokah.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Where he stands out from Wendy's spot (x, z).
    pub wendys_offset: (f32, f32),
    /// Where he stops from where you come up out of Fulton St (x, z).
    pub station_offset: (f32, f32),
    /// Points between the two he walks through (world x, z), Wendy's first.
    pub route: Vec<(f32, f32)>,
    /// How close counts as reaching a point (m).
    pub arrive: f32,
    /// Walking speed as the Jokah and as Michael (m/s).
    pub walk_speed: f32,
    pub mj_speed: f32,
    /// Seconds he stands at each end.
    pub pause_secs: f32,
    /// Seconds between laughs, and between scats (min, max).
    pub laugh_every: (f32, f32),
    pub scat_every: (f32, f32),
    /// His three laughs.
    pub laughs: Vec<Laugh>,
    /// Michael's scats and voice (pitch Hz, speed).
    pub scats: Vec<String>,
    pub scat_voice: (f32, f32),
    /// Heard this close (m), this loud.
    pub hear_range: f32,
    pub volume: f32,
    /// Seconds between slides (min, max), a slide's length (s) and speed (m/s).
    pub slide_every: (f32, f32),
    pub slide_secs: f32,
    pub slide_speed: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/jokah.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Who he is right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Heading for the J station, laughing.
    Jokah,
    /// The Michael Jackson impersonator, heading back to Wendy's.
    Michael,
}

/// Where he is on his walk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Walk {
    pub mode: Mode,
    /// The index of the point he's walking to (0 is Wendy's, the last the station).
    pub target: usize,
    /// Seconds left standing still at an end.
    pub wait: f32,
}

/// Something that happened on a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrived {
    /// Reached the J station and became Michael.
    Station,
    /// Back at Wendy's and the Jokah again.
    Wendys,
}

impl Walk {
    /// Fresh at Wendy's, heading out.
    pub fn start(points: usize) -> Self {
        Walk {
            mode: Mode::Jokah,
            target: 1.min(points.saturating_sub(1)),
            wait: 0.0,
        }
    }
}

/// His whole path: Wendy's, the route, the station.
pub fn points(t: &Tuning, wendys: Vec2, station: Vec2) -> Vec<Vec2> {
    let mut p = vec![wendys + Vec2::new(t.wendys_offset.0, t.wendys_offset.1)];
    p.extend(t.route.iter().map(|&(x, z)| Vec2::new(x, z)));
    p.push(station + Vec2::new(t.station_offset.0, t.station_offset.1));
    p
}

/// Walk him `dt` seconds at `speed` along `pts` from `pos`: he goes point by
/// point to the station as the Jokah, becomes Michael there and comes back,
/// and becomes the Jokah again at Wendy's, standing `pause` seconds at each end.
pub fn step(
    w: &mut Walk,
    pos: Vec2,
    pts: &[Vec2],
    dt: f32,
    speed: f32,
    arrive: f32,
    pause: f32,
) -> (Vec2, Option<Arrived>) {
    if pts.is_empty() {
        return (pos, None);
    }
    if w.wait > 0.0 {
        w.wait -= dt;
        return (pos, None);
    }
    let last = pts.len() - 1;
    let target = w.target.min(last);
    let goal = pts[target];
    let d = goal - pos;
    let len = d.length();
    let stride = speed * dt;
    if len > arrive && len > stride {
        return (pos + d / len * stride, None);
    }
    match w.mode {
        Mode::Jokah if target >= last => {
            *w = Walk {
                mode: Mode::Michael,
                target: last.saturating_sub(1),
                wait: pause,
            };
            (goal, Some(Arrived::Station))
        }
        Mode::Jokah => {
            w.target = target + 1;
            (goal, None)
        }
        Mode::Michael if target == 0 => {
            *w = Walk {
                mode: Mode::Jokah,
                target: 1.min(last),
                wait: pause,
            };
            (goal, Some(Arrived::Wendys))
        }
        Mode::Michael => {
            w.target = target - 1;
            (goal, None)
        }
    }
}

/// Which of `n` laughs to laugh next (from hash `h`), never the same one twice running.
pub fn pick_laugh(h: u32, n: usize, last: Option<usize>) -> usize {
    if n == 0 {
        return 0;
    }
    let i = h as usize % n;
    if n > 1 && Some(i) == last {
        (i + 1) % n
    } else {
        i
    }
}

/// Which way a slide goes, given the way he's walking: backwards (a
/// moonwalk) or to either side.
pub fn slide_dir(h: u32, heading: Vec2) -> Vec2 {
    let back = -heading.normalize_or(Vec2::Y);
    match h % 3 {
        0 => back,
        1 => back.perp(),
        _ => -back.perp(),
    }
}

/// His model as the Jokah: purple tailcoat, orange hair, a wide grin.
pub const JOKAH_MODEL: &str = "models/creatures/jokah.gltf";
/// The same man as Michael: black suit, fedora, one white glove.
pub const MICHAEL_MODEL: &str = "models/creatures/jokah_michael.gltf";
/// The ground the models' `walk` clip covers at speed 1 (m/s): the clip is
/// played faster or slower so his feet keep up with how fast he's walking.
const WALK_CLIP_SPEED: f32 = 1.37;

/// The model he wears in a mode.
pub fn model(mode: Mode) -> &'static str {
    match mode {
        Mode::Jokah => JOKAH_MODEL,
        Mode::Michael => MICHAEL_MODEL,
    }
}

/// The clip he plays: `slide` while sliding, `idle` (laughing where he
/// stands) while waiting at an end, else `walk` paced to `speed` (m/s).
pub fn clip(waiting: bool, sliding: bool, speed: f32) -> crate::rigged::RigClip {
    use crate::rigged::RigClip;
    if sliding {
        RigClip::looping("slide")
    } else if waiting {
        RigClip::looping("idle")
    } else {
        RigClip::looping("walk").speed(speed / WALK_CLIP_SPEED)
    }
}

/// The Jokah himself (local): where he is on his walk. He walks between
/// the commune and New York, so he isn't in an area himself; his [`Body`]
/// is, in whichever one he's in, drawn while that one shows.
#[derive(Resource)]
struct Jokah {
    walk: Walk,
    pos: Vec2,
    heading: Vec2,
    /// Seconds until he next laughs or scats, and how many times he has.
    next_sound: f32,
    said: u32,
    last_laugh: Option<usize>,
    /// Seconds until the next slide, seconds left in this one and its way.
    next_slide: f32,
    sliding: f32,
    slide: Vec2,
    slides: u32,
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnExit(AppState::InGame), |mut commands: Commands| {
        commands.remove_resource::<Jokah>()
    })
    .add_systems(
        Update,
        (
            start.run_if(not(resource_exists::<Jokah>)),
            (walk, body, sound).run_if(resource_exists::<Jokah>),
        )
            .chain()
            .run_if(in_state(AppState::InGame)),
    );
}

fn path() -> Vec<Vec2> {
    let fulton = super::fulton::street_at(&super::fulton::tuning());
    points(
        &tuning(),
        friendslop_world::places::wendys_at(),
        Vec2::new(fulton.x, fulton.z),
    )
}

/// The ground under a point: the city floor in New York, the terrain elsewhere.
fn ground(p: Vec2) -> Vec3 {
    let y = if crate::rules::nyc::in_city(p.x, p.y) {
        crate::rules::nyc::CITY_Y
    } else {
        crate::terrain::height_at(p.x, p.y)
    };
    Vec3::new(p.x, y, p.y)
}

fn start(mut commands: Commands) {
    let t = tuning();
    let pts = path();
    let Some(&at) = pts.first() else { return };
    commands.insert_resource(Jokah {
        walk: Walk::start(pts.len()),
        pos: at,
        heading: Vec2::Y,
        next_sound: 2.0,
        said: 0,
        last_laugh: None,
        next_slide: between(t.slide_every, hash(377, 1)),
        sliding: 0.0,
        slide: Vec2::ZERO,
        slides: 0,
    });
}

/// His body, in the area he's in (and the mode whose model it wears).
#[derive(Component)]
struct Body {
    area: String,
    mode: Mode,
}

/// Local: draw him where he is, while the area he's in shows. Changing mode
/// (or area) drops the body and the next frame makes one with the new model.
fn body(
    mut commands: Commands,
    j: Res<Jokah>,
    loaded: Res<friendslop_base::areas::Loaded>,
    mut bodies: Query<(Entity, &Body, &mut Transform, &mut crate::rigged::RigClip)>,
) {
    let Some(area) = friendslop_base::areas::nearest_area(ground(j.pos)) else {
        return;
    };
    let t = tuning();
    let at = Transform::from_translation(ground(j.pos))
        .with_rotation(Quat::from_rotation_y(j.heading.x.atan2(j.heading.y)));
    let speed = match j.walk.mode {
        Mode::Jokah => t.walk_speed,
        Mode::Michael => t.mj_speed,
    };
    let playing = clip(j.walk.wait > 0.0, j.sliding > 0.0, speed);
    let mut have = false;
    for (e, b, mut tf, mut c) in &mut bodies {
        if b.area != area || b.mode != j.walk.mode {
            commands.entity(e).despawn();
            continue;
        }
        have = true;
        *tf = at;
        c.set_if_neq(playing.clone());
    }
    if !have && loaded.is(&area, friendslop_base::areas::Layer::Show) {
        commands.spawn((
            Body {
                area: area.clone(),
                mode: j.walk.mode,
            },
            at,
            Visibility::default(),
            crate::rigged::Rigged {
                path: model(j.walk.mode),
            },
            playing,
            crate::area::show_in(&area),
        ));
    }
}

/// Walk him along, change him at the ends and slide him as Michael.
fn walk(time: Res<Time>, mut j: ResMut<Jokah>) {
    let t = tuning();
    let dt = time.delta_secs();
    let pts = path();
    let (arrive, pause) = (t.arrive, t.pause_secs);
    {
        let j = &mut *j;
        if j.sliding > 0.0 {
            j.sliding -= dt;
            j.pos += j.slide * t.slide_speed * dt;
        } else {
            let speed = match j.walk.mode {
                Mode::Jokah => t.walk_speed,
                Mode::Michael => t.mj_speed,
            };
            let was = j.pos;
            let (pos, arrived) = step(&mut j.walk, j.pos, &pts, dt, speed, arrive, pause);
            j.pos = pos;
            let d = pos - was;
            if d.length_squared() > 1e-8 {
                j.heading = d.normalize();
            }
            if arrived.is_some() {
                j.next_sound = 1.0;
            }
            if j.walk.mode == Mode::Michael && j.walk.wait <= 0.0 {
                j.next_slide -= dt;
                if j.next_slide <= 0.0 {
                    j.slides += 1;
                    j.slide = slide_dir(hash(377, j.slides), j.heading);
                    j.sliding = t.slide_secs;
                    j.next_slide = between(t.slide_every, hash(378, j.slides));
                }
            }
        }
    }
}

/// Local: laugh as the Jokah, scat as Michael, while the local player's near.
fn sound(
    mut commands: Commands,
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    mut j: ResMut<Jokah>,
) {
    let Some(me) = me else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let t = tuning();
    let at = ground(j.pos);
    {
        j.next_sound -= time.delta_secs();
        if j.next_sound > 0.0 {
            return;
        }
        j.said += 1;
        let h = hash(377, j.said + 100);
        let (text, voice) = match j.walk.mode {
            Mode::Jokah => {
                j.next_sound = between(t.laugh_every, h);
                if t.laughs.is_empty() {
                    return;
                }
                let i = pick_laugh(h, t.laughs.len(), j.last_laugh);
                j.last_laugh = Some(i);
                let l = &t.laughs[i];
                (l.text.clone(), (l.pitch, l.speed))
            }
            Mode::Michael => {
                j.next_sound = between(t.scat_every, h);
                if t.scats.is_empty() {
                    return;
                }
                (t.scats[h as usize % t.scats.len()].clone(), t.scat_voice)
            }
        };
        if at.distance(w.pos) > t.hear_range {
            return;
        }
        crate::spatial::play_wav_at(
            &mut commands,
            at + Vec3::Y * 1.6,
            t.volume,
            crate::spatial::Carry::CALL,
            move || {
                let (s, rate) = super::trains::announcer_says(&text, voice);
                crate::features::bear::wav_bytes(&s, rate)
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert_eq!(t.laughs.len(), 3, "he has 3 unique laughs");
        for (i, a) in t.laughs.iter().enumerate() {
            assert!(a.pitch > 0.0 && a.speed > 0.0 && !a.text.is_empty());
            for b in &t.laughs[i + 1..] {
                assert!(a.text != b.text || a.pitch != b.pitch, "laughs repeat");
            }
        }
        assert!(!t.scats.is_empty() && t.scat_voice.0 > 0.0 && t.scat_voice.1 > 0.0);
        assert!(t.arrive > 0.0 && t.walk_speed > 0.0 && t.mj_speed > 0.0);
        assert!(t.pause_secs >= 0.0 && t.hear_range > 0.0 && t.volume > 0.0);
        assert!(t.laugh_every.0 > 0.0 && t.laugh_every.1 >= t.laugh_every.0);
        assert!(t.scat_every.0 > 0.0 && t.scat_every.1 >= t.scat_every.0);
        assert!(t.slide_every.0 > 0.0 && t.slide_every.1 >= t.slide_every.0);
        assert!(t.slide_secs > 0.0 && t.slide_speed > 0.0);
    }

    #[test]
    fn each_mode_has_its_own_model_and_the_files_are_there() {
        assert_ne!(model(Mode::Jokah), model(Mode::Michael));
        for mode in [Mode::Jokah, Mode::Michael] {
            let path = format!(
                "{}/../game/assets/{}",
                env!("CARGO_MANIFEST_DIR"),
                model(mode)
            );
            assert!(std::path::Path::new(&path).exists(), "no {path}");
        }
    }

    #[test]
    fn he_slides_idles_or_walks_in_step_with_his_speed() {
        assert_eq!(clip(false, true, 1.6).name, "slide");
        assert_eq!(clip(true, true, 1.6).name, "slide");
        assert_eq!(clip(true, false, 1.3).name, "idle");
        let slow = clip(false, false, 1.3);
        let fast = clip(false, false, 1.6);
        assert_eq!(slow.name, "walk");
        assert!(fast.speed > slow.speed && slow.speed > 0.5 && fast.speed < 1.5);
    }

    fn run(w: &mut Walk, mut pos: Vec2, pts: &[Vec2], secs: f32) -> (Vec2, Vec<Arrived>) {
        let mut seen = Vec::new();
        let dt = 0.1;
        for _ in 0..(secs / dt).round() as usize {
            let (p, a) = step(w, pos, pts, dt, 2.0, 0.3, 1.0);
            pos = p;
            seen.extend(a);
        }
        (pos, seen)
    }

    #[test]
    fn walks_to_the_station_and_becomes_michael() {
        let pts = [Vec2::ZERO, Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0)];
        let mut w = Walk::start(pts.len());
        assert_eq!(w.mode, Mode::Jokah);
        // Halfway along the first leg he's still the Jokah, heading out.
        let (pos, seen) = run(&mut w, pts[0], &pts, 2.5);
        assert!(seen.is_empty() && w.mode == Mode::Jokah);
        assert!(pos.y > 4.0 && pos.y < 6.0);
        // 20 m at 2 m/s: by 10.5 s he's at the station and changed.
        let (pos, seen) = run(&mut w, pos, &pts, 8.0);
        assert_eq!(seen, vec![Arrived::Station]);
        assert_eq!(w.mode, Mode::Michael);
        assert!(pos.distance(pts[2]) < 0.01);
        assert_eq!(w.target, 1, "and turns back along the route");
    }

    #[test]
    fn michael_walks_back_to_wendys_and_turns_jokah_again() {
        let pts = [Vec2::ZERO, Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0)];
        let mut w = Walk::start(pts.len());
        let (pos, seen) = run(&mut w, pts[0], &pts, 10.3);
        assert_eq!(seen, vec![Arrived::Station]);
        // He pauses at the station before leaving.
        let (still, _) = run(&mut w, pos, &pts, 0.3);
        assert_eq!(still, pos);
        let (pos, seen) = run(&mut w, still, &pts, 10.6);
        assert_eq!(seen, vec![Arrived::Wendys]);
        assert_eq!(w.mode, Mode::Jokah);
        assert!(pos.distance(pts[0]) < 0.01);
        assert_eq!(w.target, 1);
    }

    #[test]
    fn a_route_of_two_points_works() {
        let pts = [Vec2::ZERO, Vec2::new(3.0, 0.0)];
        let mut w = Walk::start(pts.len());
        let (_, seen) = run(&mut w, pts[0], &pts, 6.0);
        assert_eq!(seen, vec![Arrived::Station, Arrived::Wendys]);
    }

    #[test]
    fn laughs_use_all_three_and_never_repeat_back_to_back() {
        let mut last = None;
        let mut used = [false; 3];
        for s in 0..60 {
            let i = pick_laugh(hash(377, s), 3, last);
            assert!(i < 3);
            assert_ne!(Some(i), last);
            used[i] = true;
            last = Some(i);
        }
        assert!(used.iter().all(|&u| u));
        assert_eq!(pick_laugh(7, 1, Some(0)), 0);
    }

    #[test]
    fn slides_go_back_or_sideways() {
        let heading = Vec2::new(0.0, 1.0);
        for h in 0..3 {
            let d = slide_dir(h, heading);
            assert!((d.length() - 1.0).abs() < 1e-4);
            assert!(d.dot(heading) <= 1e-4, "never forwards");
        }
        assert_eq!(slide_dir(0, heading), Vec2::new(0.0, -1.0));
    }
}
