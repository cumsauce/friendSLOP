//! `FRIENDSLOP_BOT=<route>`: a scripted autopilot for profiling runs (no AI).
//! It drives the host's own body along a route from `data/game/bot.ron`
//! (walk to points, start somewhere, ride a train, look about, click), through
//! the same [`VirtualInput`] a gamepad fills, so the server sees an ordinary
//! player. Each step's name goes to [`PerfMark`], so `FRIENDSLOP_PERF`'s hitch
//! lines say what the bot was doing; at the end it logs each step's frame
//! times and the worst hitches, then quits (`AppExit`), so a run is one
//! command: `FRIENDSLOP_BOT=tour FRIENDSLOP_PERF=1 cargo run -- host`.
//! Host only (it needs the server's walker to teleport).
//!
//! `FRIENDSLOP_BRP=1` (desktop) also serves the Bevy Remote Protocol on
//! localhost:15702, so a script can read the running game (entities,
//! components, [`PerfMark`]) and write it (`VirtualInput`) over HTTP.

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::Deserialize;

use crate::controls::VirtualInput;
use crate::features::fps::{FpsView, WalkInput, Walker, teleport};
use crate::player::PlayerOwner;

/// The routes: `data/game/bot.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Routes {
    /// Within this far (m) of a waypoint counts as there.
    pub reach: f32,
    /// No progress for this long (s) and the bot jumps; for `give_up` it
    /// skips the step and logs it.
    pub stuck_jump: f32,
    pub give_up: f32,
    /// A frame over this many ms is a hitch.
    pub hitch_ms: f32,
    /// How fast `Spin` turns (degrees a second).
    pub spin_degs: f32,
    /// Sprint on every walk (`Walk` too, not just `Run`).
    pub always_run: bool,
    /// Each route by name: its steps in order.
    pub routes: BTreeMap<String, Vec<Step>>,
}

/// One thing the bot does.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub enum Step {
    /// Teleport to a `FRIENDSLOP_START` name (a map spot or a feature's start).
    Start(String),
    /// Walk through these points (x, z) in turn.
    Walk(Vec<(f32, f32)>),
    /// Run through these points (x, z) in turn.
    Run(Vec<(f32, f32)>),
    /// Walk to a named start (a map spot or a feature's start).
    WalkTo(String),
    /// Board the train standing at `from` (a stop's start name, its
    /// platform) and ride until it sets you down near `to`.
    Ride { from: String, to: String },
    /// Stand still this many seconds.
    Wait(f32),
    /// Turn on the spot for this many seconds (draws every direction).
    Spin(f32),
    /// Face this way (degrees: yaw, pitch down).
    Look(f32, f32),
    /// Click the nearest clickable in reach.
    Click,
    /// Do these steps this many times.
    Repeat(u32, Vec<Step>),
}

pub static ROUTES: friendslop_data::Data<Routes> = friendslop_data::Data::ron("data/game/bot.ron");

/// What the bot is doing now, for the profiler's lines (empty: no bot).
#[derive(Resource, Reflect, Default, Debug, Clone)]
#[reflect(Resource)]
pub struct PerfMark(pub String);

/// The route, flattened (repeats unrolled), each step with its label.
pub fn flatten(steps: &[Step]) -> Vec<(String, Step)> {
    let mut out = Vec::new();
    for s in steps {
        match s {
            Step::Repeat(n, inner) => {
                for _ in 0..*n {
                    out.extend(flatten(inner));
                }
            }
            s => out.push((label(s), s.clone())),
        }
    }
    out
}

/// A short name for a step, for the logs.
pub fn label(s: &Step) -> String {
    match s {
        Step::Start(n) => format!("start {n}"),
        Step::Walk(p) | Step::Run(p) => {
            let to = p.last().copied().unwrap_or_default();
            let verb = if matches!(s, Step::Run(_)) {
                "run"
            } else {
                "walk"
            };
            format!("{verb} to {:.0},{:.0}", to.0, to.1)
        }
        Step::WalkTo(n) => format!("walk to {n}"),
        Step::Ride { from, to } => format!("ride {from} -> {to}"),
        Step::Wait(s) => format!("wait {s:.0}s"),
        Step::Spin(s) => format!("spin {s:.0}s"),
        Step::Look(y, p) => format!("look {y:.0},{p:.0}"),
        Step::Click => "click".into(),
        Step::Repeat(n, _) => format!("repeat {n}"),
    }
}

/// Which way to face and how hard to push to go from `pos` to `target`:
/// (yaw, stick). The stick is straight ahead; it eases off close in so the
/// bot doesn't overshoot. Yaw 0 faces -z, the way `fps.rs` turns bodies.
pub fn steer(pos: Vec3, target: Vec2) -> (f32, Vec2) {
    let d = target - pos.xz();
    let dist = d.length();
    if dist < 1e-3 {
        return (0.0, Vec2::ZERO);
    }
    let yaw = (-d.x).atan2(-d.y);
    (yaw, Vec2::Y * (dist / 1.5).clamp(0.3, 1.0))
}

/// Per-step frame times, for the end-of-run summary.
#[derive(Default)]
struct Stats {
    frames: Vec<f32>,
    hitches: Vec<(f32, Vec3, String)>,
    skipped: bool,
}

/// The run: where in the route, and what each step has seen.
#[derive(Resource, Default)]
struct Bot {
    route: String,
    steps: Vec<(String, Step)>,
    at: usize,
    /// Seconds into the step.
    secs: f32,
    /// Waypoint index within a walk.
    point: usize,
    /// Closest we got to the current target, and when that was.
    best: (f32, f32),
    /// Where the body last made real progress (moved half a metre).
    anchor: Vec2,
    /// A ride: seen outside a car yet (the trains need that before boarding).
    cleared: bool,
    /// A ride: on board yet.
    aboard: bool,
    /// Starts looked up so far (a lookup asks every feature, and some work
    /// their places out from scratch: never every frame), and when a missing
    /// one was last tried.
    found: std::collections::HashMap<String, Vec3>,
    missing: std::collections::HashMap<String, f32>,
    stats: Vec<Stats>,
    /// Seconds since the game started, before the route begins.
    warmup: f32,
    /// The bot's landmass agent (`nav.rs`): which way the area's navmesh
    /// says to go this frame, and where the walk wants it to go.
    agent: Option<Entity>,
    nav_dir: Option<Vec2>,
    /// The agent says it's as close as the mesh lets it get.
    nav_reached: bool,
    /// The agent can't put the target on the mesh (inside something solid).
    nav_unreachable: bool,
    nav_target: Option<Vec3>,
    /// Real seconds since the route began.
    clock: f32,
    done: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<PerfMark>().register_type::<PerfMark>();
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    if std::env::var("FRIENDSLOP_BRP").is_ok_and(|v| v == "1") {
        app.add_plugins((
            bevy_remote::RemotePlugin::default(),
            bevy_remote::http::RemoteHttpPlugin::default(),
        ));
        info!("bot: Bevy Remote Protocol on http://127.0.0.1:15702");
    }
    let Ok(route) = std::env::var("FRIENDSLOP_BOT") else {
        return;
    };
    app.insert_resource(Bot { route, ..default() })
        .add_systems(
            PreUpdate,
            drive
                .after(crate::controls::silence_world_keys)
                .run_if(in_state(crate::AppState::InGame))
                .run_if(in_state(ServerState::Running)),
        )
        .add_systems(Last, record.run_if(in_state(crate::AppState::InGame)));
}

/// What a step wants this frame.
#[derive(Default)]
struct Out {
    stick: Vec2,
    yaw: Option<f32>,
    pitch: Option<f32>,
    turn: f32,
    jump: bool,
    run: bool,
    teleport: Option<Vec3>,
    click: bool,
    next: bool,
    skip: bool,
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut bot: ResMut<Bot>,
    time: Res<Time<Real>>,
    points: Res<crate::features::start::StartPoints>,
    mut input: ResMut<VirtualInput>,
    mut view: ResMut<FpsView>,
    mut mark: ResMut<PerfMark>,
    ready: Option<Res<crate::loading::WorldReady>>,
    mut bodies: Query<(
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
    runs: Query<&crate::features::trains::Run>,
    navs: Res<friendslop_base::nav::AreaNavs>,
    states: Query<&bevy_landmass::AgentState>,
    mut agents: Query<(
        &mut Transform,
        &mut bevy_landmass::AgentTarget3d,
        &mut bevy_landmass::ArchipelagoRef3d,
        Option<&bevy_landmass::AgentDesiredVelocity3d>,
    )>,
    clickables: Query<(Entity, &GlobalTransform), With<crate::features::interact::Clickable>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    if bot.done {
        return;
    }
    let dt = time.delta_secs();
    if bot.steps.is_empty() {
        let routes = ROUTES.get();
        let Some(steps) = routes.routes.get(&bot.route) else {
            error!("bot: no route {:?} in data/game/bot.ron", bot.route);
            bot.done = true;
            return;
        };
        bot.steps = flatten(steps);
        bot.stats = (0..bot.steps.len()).map(|_| Stats::default()).collect();
        bot.best = (f32::MAX, 0.0);
        info!("bot: route {} ({} steps)", bot.route, bot.steps.len());
    }
    // Let the first load settle before timing anything.
    if bot.warmup < 3.0 || !ready.as_deref().is_some_and(|r| r.ready) {
        bot.warmup += dt;
        return;
    }
    let Some((_, mut walker, mut walk, mut kcc)) = bodies.iter_mut().find(|b| b.0.0.is_none())
    else {
        return;
    };
    if bot.at >= bot.steps.len() {
        summary(&bot);
        bot.done = true;
        *input = VirtualInput::default();
        mark.0.clear();
        exit.write(AppExit::Success);
        return;
    }
    let t = ROUTES.get();
    let (name, step) = bot.steps[bot.at].clone();
    if mark.0 != name {
        mark.0 = name.clone();
        info!("bot: {name}");
    }
    bot.secs += dt;
    bot.clock += dt;
    let pos = walker.pos;
    // The navmesh's way this frame (from last frame's agent), if any.
    let archipelago = crate::areas::area_at(pos).and_then(|a| navs.archipelago(&a));
    bot.nav_dir = bot
        .agent
        .and_then(|e| agents.get(e).ok())
        .filter(|a| Some(a.2.entity) == archipelago)
        .and_then(|a| a.3)
        .map(|d| d.velocity().xz())
        .filter(|v| v.length() > 0.1)
        .map(|v| v.normalize());
    bot.nav_reached = bot
        .agent
        .and_then(|e| states.get(e).ok())
        .is_some_and(|s| matches!(s, bevy_landmass::AgentState::ReachedTarget));
    bot.nav_unreachable = bot.agent.and_then(|e| states.get(e).ok()).is_some_and(|s| {
        matches!(
            s,
            bevy_landmass::AgentState::TargetNotOnNavMesh | bevy_landmass::AgentState::NoPath
        )
    });
    bot.nav_target = None;
    if (bot.clock * 1.0).fract() < dt {
        let state = bot.agent.and_then(|e| states.get(e).ok());
        debug!(
            "bot nav: at {pos:.1} mesh {} dir {:?} agent {state:?}",
            archipelago.is_some(),
            bot.nav_dir
        );
    }
    let out = step_out(&mut bot, &step, pos, &points, &runs, &t);
    steer_agent(&mut bot, &mut commands, &mut agents, archipelago, pos);
    if let Some(at) = out.teleport {
        let yaw = walker.yaw;
        teleport(
            &mut walker,
            walk.as_deref_mut(),
            kcc.as_deref_mut(),
            at,
            yaw,
        );
    }
    if out.click {
        let near = clickables
            .iter()
            .map(|(e, g)| (e, g.translation().distance(pos + Vec3::Y * 1.2)))
            .filter(|(_, d)| *d < crate::features::interact::REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((e, _)) = near {
            commands.trigger(crate::features::interact::Clicked { entity: e });
        } else {
            warn!("bot: nothing to click at {pos:.1}");
        }
    }
    if let Some(y) = out.yaw {
        view.yaw = y;
    }
    if let Some(p) = out.pitch {
        view.pitch = p;
    }
    view.yaw += out.turn * dt;
    *input = VirtualInput {
        stick: out.stick,
        look: Vec2::ZERO,
        jump: out.jump,
        run: out.run,
    };
    if out.next || out.skip {
        if out.skip {
            let state = bot.agent.and_then(|e| states.get(e).ok());
            warn!(
                "bot: gave up on {name} at {pos:.1} (mesh {}, agent {state:?})",
                archipelago.is_some()
            );
            let i = bot.at;
            bot.stats[i].skipped = true;
        }
        bot.at += 1;
        bot.secs = 0.0;
        bot.point = 0;
        bot.best = (f32::MAX, 0.0);
        bot.aboard = false;
        bot.cleared = false;
    }
}

/// One frame of `step`: what to press and whether it's done.
fn step_out(
    bot: &mut Bot,
    step: &Step,
    pos: Vec3,
    points: &crate::features::start::StartPoints,
    runs: &Query<&crate::features::trains::Run>,
    t: &Routes,
) -> Out {
    match step {
        Step::Start(n) => match lookup(bot, points, n) {
            // `aboard` marks the teleport done; then half a second to land.
            Some(at) if !bot.aboard => {
                bot.aboard = true;
                bot.best = (0.0, bot.secs);
                Out {
                    teleport: Some(at + Vec3::Y * 0.05),
                    ..default()
                }
            }
            Some(_) => Out {
                next: bot.secs - bot.best.1 > 0.5,
                ..default()
            },
            // A place built only when its area loads: wait for it.
            None => Out {
                skip: bot.secs > t.give_up,
                ..default()
            },
        },
        Step::Walk(p) | Step::Run(p) => {
            let run = matches!(step, Step::Run(_));
            let Some(&target) = p.get(bot.point) else {
                return Out {
                    next: true,
                    ..default()
                };
            };
            let target = Vec2::from(target);
            let o = go(bot, pos, target, run, t);
            if o.next && bot.point + 1 < p.len() {
                bot.point += 1;
                bot.best = (f32::MAX, bot.secs);
                return Out { next: false, ..o };
            }
            o
        }
        Step::WalkTo(n) => match lookup(bot, points, n) {
            Some(at) => go(bot, pos, at.xz(), false, t),
            None => Out {
                skip: bot.secs > t.give_up,
                ..default()
            },
        },
        Step::Ride { from, to } => {
            let Some(dest) = lookup(bot, points, to) else {
                return Out {
                    skip: bot.secs > t.give_up,
                    ..default()
                };
            };
            if pos.distance(dest) < 25.0 {
                return Out {
                    next: true,
                    ..default()
                };
            }
            let trains = crate::features::trains::tuning();
            let stop = trains.lines.iter().enumerate().find_map(|(li, l)| {
                l.stops.iter().enumerate().find_map(|(si, s)| {
                    (crate::features::trains::start_name(s) == *from).then_some((li, si, s))
                })
            });
            let Some((li, si, s)) = stop else {
                return Out {
                    skip: true,
                    ..default()
                };
            };
            let skip = bot.secs > t.give_up * 30.0;
            // The trains only take someone they've seen outside a car first
            // (a start can put you down inside one): step clear, then board.
            let in_a_car = crate::features::trains::car_at(&trains, pos).is_some();
            if !in_a_car {
                bot.cleared = true;
            }
            // Inside a car after that (the standing train or the ride car) is
            // aboard: stand still and let it carry us; no steering at doors.
            if bot.cleared && in_a_car {
                bot.aboard = true;
            }
            if bot.aboard {
                // Stand still on board until it sets us down.
                return Out { skip, ..default() };
            }
            let waiting = runs.iter().any(|r| {
                r.line as usize == li
                    && r.stop as usize == si
                    && r.phase == crate::features::trains::Phase::Waiting
            });
            if !waiting {
                // Wait on the platform for the train.
                return Out { skip, ..default() };
            }
            // To the door first, then in through it.
            let (front, inside) = crate::features::trains::door_way(s);
            if !bot.cleared {
                // Out through the door and a couple of metres on.
                let out = front + (front - inside).normalize_or_zero() * 2.0;
                // By the navmesh: a gate's walls stand between a start and
                // its door.
                let o = go(bot, pos, out.xz(), true, t);
                return Out {
                    yaw: o.yaw,
                    stick: o.stick,
                    run: true,
                    jump: o.jump,
                    skip,
                    ..default()
                };
            }
            let in_front = bot.point > 0 || pos.xz().distance(front.xz()) < 0.6;
            if in_front {
                bot.point = 1;
            }
            let to = if in_front { inside } else { front };
            if !in_front {
                let o = go(bot, pos, front.xz(), true, t);
                return Out {
                    yaw: o.yaw,
                    stick: o.stick,
                    run: true,
                    jump: o.jump,
                    skip,
                    ..default()
                };
            }
            let (yaw, stick) = steer(pos, to.xz());

            Out {
                yaw: Some(yaw),
                stick,
                run: t.always_run,
                skip,
                ..default()
            }
        }
        Step::Wait(s) => Out {
            next: bot.secs >= *s,
            ..default()
        },
        Step::Spin(s) => Out {
            turn: t.spin_degs.to_radians(),
            next: bot.secs >= *s,
            ..default()
        },
        Step::Look(y, p) => Out {
            yaw: Some(y.to_radians()),
            pitch: Some(p.to_radians()),
            next: bot.secs > 0.5,
            ..default()
        },
        Step::Click => Out {
            click: bot.secs <= 0.02,
            next: bot.secs > 1.0,
            ..default()
        },
        Step::Repeat(..) => Out {
            next: true,
            ..default()
        },
    }
}

/// Where the start `name` is, looked up once (a missing one is tried again
/// at most once a second: its place may only be built when its area loads).
fn lookup(bot: &mut Bot, points: &crate::features::start::StartPoints, name: &str) -> Option<Vec3> {
    if let Some(at) = bot.found.get(name) {
        return Some(*at);
    }
    let now = bot.clock;
    if bot.missing.get(name).is_some_and(|t| now - t < 1.0) {
        return None;
    }
    let Some(at) = crate::features::start::find(points, name) else {
        bot.missing.insert(name.to_owned(), now);
        return None;
    };
    bot.found.insert(name.to_owned(), at);
    Some(at)
}

/// Walk (or run) towards `target`: done in reach, a hop when stuck, skipped
/// when hopelessly stuck.
fn go(bot: &mut Bot, pos: Vec3, target: Vec2, run: bool, t: &Routes) -> Out {
    let dist = pos.xz().distance(target);
    if dist < t.reach {
        return Out {
            next: true,
            ..default()
        };
    }
    // Progress is moving, not closing in: a path round a wall leads away first.
    if pos.xz().distance(bot.anchor) > 0.5 || bot.best.0 == f32::MAX {
        bot.anchor = pos.xz();
        bot.best = (dist, bot.secs);
    }
    let stuck = bot.secs - bot.best.1;
    // A place that's solid in the middle (the warehouse, the windmill): done
    // when the mesh says this is as close as it gets, or stuck close by.
    let close = stuck > t.stuck_jump && (dist < 20.0 || bot.nav_unreachable);
    if bot.nav_reached && bot.secs > 0.5 || close {
        return Out {
            next: true,
            ..default()
        };
    }
    bot.nav_target = Some(Vec3::new(target.x, pos.y, target.y));
    let (mut yaw, mut stick) = steer(pos, target);
    // Follow the navmesh round walls when it knows a way; straight otherwise.
    if let Some(d) = bot.nav_dir {
        yaw = (-d.x).atan2(-d.y);
    }
    // Stuck on the mesh: against something it leaves out (a tree, a rock).
    // Sidestep, one way then the other, rather than climb it.
    if bot.nav_dir.is_some() && stuck > t.stuck_jump {
        let side = if (stuck / 1.5) as i32 % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        stick = Vec2::new(side, 0.5).normalize();
    }
    Out {
        yaw: Some(yaw),
        stick,
        run: run || t.always_run,
        // Hop only without a path: on the mesh, hopping just climbs walls.
        jump: bot.nav_dir.is_none() && stuck > t.stuck_jump && (stuck * 2.0).fract() < 0.5,
        // Stuck too long, or wandering far too long.
        skip: stuck > t.give_up || bot.secs > t.give_up * 15.0,
        ..default()
    }
}

/// Keep the bot's landmass agent at its feet, aimed at the walk's target, on
/// the area's archipelago; drop it where there's no mesh or nowhere to go.
fn steer_agent(
    bot: &mut Bot,
    commands: &mut Commands,
    agents: &mut Query<(
        &mut Transform,
        &mut bevy_landmass::AgentTarget3d,
        &mut bevy_landmass::ArchipelagoRef3d,
        Option<&bevy_landmass::AgentDesiredVelocity3d>,
    )>,
    archipelago: Option<Entity>,
    pos: Vec3,
) {
    let (Some(arch), Some(target)) = (archipelago, bot.nav_target) else {
        if let Some(e) = bot.agent.take() {
            commands.entity(e).try_despawn();
        }
        return;
    };
    if let Some(e) = bot.agent
        && let Ok((mut tf, mut aim, mut on, _)) = agents.get_mut(e)
    {
        tf.translation = pos;
        *aim = bevy_landmass::AgentTarget3d::Point(target);
        if on.entity != arch {
            *on = bevy_landmass::ArchipelagoRef3d::new(arch);
        }
        return;
    }
    let radius = friendslop_base::nav::TUNING.get().agent_radius;
    bot.agent = Some(
        commands
            .spawn((
                Transform::from_translation(pos),
                bevy_landmass::Agent3d::default(),
                bevy_landmass::AgentSettings {
                    radius,
                    desired_speed: crate::kcc::RUN_SPEED,
                    max_speed: crate::kcc::RUN_SPEED,
                },
                bevy_landmass::ArchipelagoRef3d::new(arch),
                bevy_landmass::AgentTarget3d::Point(target),
                crate::areas::Global,
            ))
            .id(),
    );
}

/// Every frame: the frame time and any hitch, filed under the step.
fn record(
    mut bot: Option<ResMut<Bot>>,
    time: Res<Time<Real>>,
    cam: Query<&GlobalTransform, With<Camera3d>>,
) {
    let Some(bot) = bot.as_deref_mut() else {
        return;
    };
    if bot.done || bot.steps.is_empty() || bot.warmup < 3.0 {
        return;
    }
    let i = bot.at.min(bot.steps.len().saturating_sub(1));
    let ms = time.delta_secs() * 1000.0;
    let hitch_ms = ROUTES.get().hitch_ms;
    let at = cam
        .iter()
        .next()
        .map(|g| g.translation())
        .unwrap_or_default();
    let Some(s) = bot.stats.get_mut(i) else {
        return;
    };
    s.frames.push(ms);
    if ms > hitch_ms {
        let area = crate::areas::area_at(at).unwrap_or_else(|| "no area".into());
        s.hitches.push((ms, at, area));
    }
}

/// Median, 95th percentile and worst of some frame times (ms).
pub fn spread(frames: &[f32]) -> (f32, f32, f32) {
    let mut v = frames.to_vec();
    v.sort_by(f32::total_cmp);
    let at = |q: f32| {
        v.get(((v.len() as f32 - 1.0) * q) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    (at(0.5), at(0.95), v.last().copied().unwrap_or(0.0))
}

fn summary(bot: &Bot) {
    let all: Vec<f32> = bot
        .stats
        .iter()
        .flat_map(|s| s.frames.iter().copied())
        .collect();
    let (p50, p95, worst) = spread(&all);
    let secs: f32 = all.iter().sum::<f32>() / 1000.0;
    let hitches = bot.stats.iter().map(|s| s.hitches.len()).sum::<usize>();
    info!(
        "bot summary: route {} in {secs:.0} s | median {p50:.1} p95 {p95:.1} worst {worst:.0} ms | hitches {hitches}",
        bot.route
    );
    for ((name, _), s) in bot.steps.iter().zip(&bot.stats) {
        let (p50, p95, worst) = spread(&s.frames);
        info!(
            "bot step: {name:<32} median {p50:5.1} p95 {p95:5.1} worst {worst:4.0} ms | hitches {:3}{}",
            s.hitches.len(),
            if s.skipped { " | GAVE UP" } else { "" }
        );
    }
    let mut worst: Vec<(&str, &(f32, Vec3, String))> = bot
        .steps
        .iter()
        .zip(&bot.stats)
        .flat_map(|((n, _), s)| s.hitches.iter().map(move |h| (n.as_str(), h)))
        .collect();
    worst.sort_by(|a, b| b.1.0.total_cmp(&a.1.0));
    for (name, (ms, at, area)) in worst.iter().take(5) {
        info!("bot worst: {ms:.0} ms at {at:.0} in {area} during {name}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steering_faces_the_target() {
        // Yaw 0 faces -z: straight ahead is down the -z axis.
        let (yaw, stick) = steer(Vec3::ZERO, Vec2::new(0.0, -10.0));
        assert!(yaw.abs() < 1e-4 && stick == Vec2::Y);
        // A point to the right (+x) is a quarter turn clockwise.
        let (yaw, _) = steer(Vec3::ZERO, Vec2::new(10.0, 0.0));
        let fwd = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
        assert!(fwd.distance(Vec3::X) < 1e-4, "{fwd}");
        // Close in it eases off.
        let (_, near) = steer(Vec3::ZERO, Vec2::new(0.0, -0.6));
        assert!(near.y < 1.0 && near.y >= 0.3);
    }

    #[test]
    fn repeats_unroll_with_labels() {
        let s = flatten(&[
            Step::Start("lake".into()),
            Step::Repeat(2, vec![Step::Wait(1.0), Step::Spin(2.0)]),
        ]);
        let names: Vec<&str> = s.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            ["start lake", "wait 1s", "spin 2s", "wait 1s", "spin 2s"]
        );
    }

    #[test]
    fn spread_reads_median_p95_and_worst() {
        let v: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        let (p50, p95, worst) = spread(&v);
        assert_eq!((p50, p95, worst), (50.0, 95.0, 100.0));
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let path = format!("{}/assets/{}", env!("CARGO_MANIFEST_DIR"), ROUTES.path());
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(ROUTES.set_bytes(&bytes), "{path} doesn't parse");
        let r = ROUTES.get();
        assert!(r.reach > 0.0 && r.stuck_jump > 0.0 && r.give_up > r.stuck_jump);
        assert!(r.hitch_ms > 0.0 && r.spin_degs > 0.0);
        for name in ["tour", "borders"] {
            let steps = r.routes.get(name).map(|s| flatten(s)).unwrap_or_default();
            assert!(!steps.is_empty(), "route {name}");
        }
    }
}
