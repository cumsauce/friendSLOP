//! The nightmare bathroom (#161, doomy: "in the warehouse a biome can be
//! nightmare bathroom. like the bathrooms you have in nightmares where the
//! doors are broken or too high and theres dirty poopy toilets and some
//! stalls are connected to other stalls and the ceilings really high and
//! theres a still ambience with tile floors and fluorescent lighting and a
//! confusing layout and a fart monster that patrols").
//!
//! - Most expeditions (about two in three), a rectangle of the warehouse's
//!   floor plan, 2-3 cells each way and never by the loading door, is a
//!   bathroom instead of storage. Which cells, and whether there is one at
//!   all, comes from the floor-plan seed, so every peer agrees ([`zone`]).
//! - The aisles there are still the wave-function-collapse plan; only what's
//!   in them changes (see `visuals/bathroom.rs` for the look and sound).
//! - **The fart monster** lives in there and patrols it, cell to cell,
//!   never leaving the bathroom. It lets one rip every few seconds (so you
//!   can hear where it is). If it spots you close by, it waddles after you,
//!   slower than you walk. If it gets right up to you, it gasses you: you're
//!   thrown out of the warehouse and drop your soul piece, like being caught
//!   by the other monster.

use super::*;

/// Chance an expedition's warehouse has a bathroom in it.
pub const CHANCE: f32 = 0.65;
/// The bathroom's ceiling height (the rest of the warehouse is [`INSIDE_WALL`]).
pub const CEILING: f32 = 11.0;
/// The fart monster's patrol and waddle speeds (walking is 5 m/s).
pub const PATROL_SPEED: f32 = 1.4;
pub const WADDLE_SPEED: f32 = 2.6;
/// It notices you this close (down a clear aisle).
pub const NOTICE: f32 = 6.0;
/// It gasses you this close.
pub const GAS_REACH: f32 = 1.5;
/// Seconds it stands there, satisfied, after gassing someone.
const SMUG: f32 = 4.0;

/// A rectangle of floor-plan cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub x0: usize,
    pub z0: usize,
    pub w: usize,
    pub h: usize,
}

impl Zone {
    pub fn contains(&self, (x, z): (usize, usize)) -> bool {
        (self.x0..self.x0 + self.w).contains(&x) && (self.z0..self.z0 + self.h).contains(&z)
    }

    /// Is this floor-plane offset (metres from the grid centre) in here?
    pub fn holds(&self, p: Vec2) -> bool {
        cell_of(p).is_some_and(|c| self.contains(c))
    }

    pub fn cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (self.z0..self.z0 + self.h)
            .flat_map(move |z| (self.x0..self.x0 + self.w).map(move |x| (x, z)))
    }
}

/// Where the bathroom is for floor plan `seed`, if this warehouse has one.
pub fn zone(seed: u32) -> Option<Zone> {
    let mut rng = Rng::new(seed ^ 0xba7b_0161);
    if rng.roll() >= CHANCE {
        return None;
    }
    let w = 2 + rng.below(2);
    let h = 2 + rng.below(2);
    let x0 = rng.below(W - w + 1);
    // Rows 0 and 1 stay storage, so you always walk in through the warehouse.
    let z0 = 2 + rng.below(H - h - 1);
    Some(Zone { x0, z0, w, h })
}

/// The ceiling over a cell.
pub fn ceiling(zone: Option<Zone>, cell: (usize, usize)) -> f32 {
    if zone.is_some_and(|z| z.contains(cell)) {
        CEILING
    } else {
        INSIDE_WALL
    }
}

/// The fart monster. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct FartMonster {
    pub pos: Vec3,
    pub yaw: f32,
    /// After someone (it waddles faster and leans in).
    pub waddling: bool,
    /// Farts so far: every peer plays one each time this goes up.
    pub puffs: u32,
}

/// Server -> everyone: the fart monster gassed `player` at `at`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Gassed {
    pub player: u64,
    pub at: Vec3,
}

/// Server only: the fart monster's (small) mind.
#[derive(Component, Debug)]
pub(super) struct FartBrain {
    plan: Plan,
    zone: Zone,
    goal: (usize, usize),
    rng: Rng,
    /// Seconds to the next fart.
    next_puff: f32,
    /// Seconds left standing smugly after gassing someone.
    smug: f32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<FartMonster>()
        .add_server_event::<Gassed>(Channel::Ordered);
}

/// Server: if this plan has a bathroom, put the fart monster in it, on a
/// cell you can get to from the door.
pub(super) fn spawn(commands: &mut Commands, seed: u32, plan: &Plan) {
    let Some(zone) = zone(seed) else { return };
    let dist = plan.distances(ENTRANCE);
    let mut rng = Rng::new(seed ^ 0xfa27);
    let open: Vec<(usize, usize)> = zone
        .cells()
        .filter(|&(x, z)| dist[z][x] != u8::MAX)
        .collect();
    if open.is_empty() {
        return;
    }
    let home = open[rng.below(open.len())];
    commands.spawn((
        FartMonster {
            pos: cell_world(home.0, home.1),
            yaw: 0.0,
            waddling: false,
            puffs: 0,
        },
        FartBrain {
            plan: plan.clone(),
            zone,
            goal: home,
            rng,
            next_puff: 2.0,
            smug: 0.0,
        },
        Replicated,
    ));
}

/// Walking distance in cells from `from`, without leaving the bathroom.
fn zone_distances(plan: &Plan, zone: Zone, from: (usize, usize)) -> [[u8; W]; H] {
    let mut d = [[u8::MAX; W]; H];
    if !zone.contains(from) {
        return d;
    }
    d[from.1][from.0] = 0;
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some((x, z)) = queue.pop_front() {
        let here = d[z][x];
        for (nx, nz) in plan.exits(x, z) {
            if zone.contains((nx, nz)) && d[nz][nx] == u8::MAX {
                d[nz][nx] = here + 1;
                queue.push_back((nx, nz));
            }
        }
    }
    d
}

/// What the fart monster does this tick.
#[derive(Debug, PartialEq)]
pub(super) struct Shuffle {
    pub pos: Vec2,
    pub waddling: bool,
    pub puff: bool,
    pub gassed: Option<u64>,
}

/// The fart monster's brain, pure: patrol the bathroom, waddle at anyone
/// close in it, gas whoever it reaches, fart now and then regardless.
pub(super) fn shuffle(brain: &mut FartBrain, here: Vec2, prey: &[(u64, Vec2)], dt: f32) -> Shuffle {
    let mut puff = false;
    brain.next_puff -= dt;
    if brain.next_puff <= 0.0 {
        puff = true;
        brain.next_puff = 3.0 + brain.rng.roll() * 5.0;
    }
    if brain.smug > 0.0 {
        brain.smug -= dt;
        return Shuffle {
            pos: here,
            waddling: false,
            puff,
            gassed: None,
        };
    }
    let near = |p: &Vec2| p.distance(here);
    if let Some(&(id, _)) = prey
        .iter()
        .filter(|(_, p)| near(p) < GAS_REACH)
        .min_by(|a, b| near(&a.1).total_cmp(&near(&b.1)))
    {
        brain.smug = SMUG;
        brain.next_puff = 3.0 + brain.rng.roll() * 5.0;
        return Shuffle {
            pos: here,
            waddling: false,
            puff: true,
            gassed: Some(id),
        };
    }
    // Someone close, in the bathroom, in plain sight: after them.
    let target = prey
        .iter()
        .filter(|(_, p)| near(p) < NOTICE && brain.zone.holds(*p) && brain.plan.sees(here, *p))
        .min_by(|a, b| near(&a.1).total_cmp(&near(&b.1)));
    if let Some(&(_, p)) = target {
        return Shuffle {
            pos: move_to(here, p, WADDLE_SPEED * dt),
            waddling: true,
            puff,
            gassed: None,
        };
    }
    // Patrol: on to the goal cell, then pick another one.
    let goal = cell_centre(brain.goal.0, brain.goal.1);
    let pos = if here.distance(goal) < 0.2 {
        let d = zone_distances(&brain.plan, brain.zone, brain.goal);
        let options: Vec<(usize, usize)> = brain
            .zone
            .cells()
            .filter(|&(x, z)| d[z][x] != u8::MAX && (x, z) != brain.goal)
            .collect();
        if !options.is_empty() {
            brain.goal = options[brain.rng.below(options.len())];
        }
        here
    } else {
        let towards = match cell_of(here) {
            Some(c) if c != brain.goal && brain.zone.contains(c) => {
                let d = zone_distances(&brain.plan, brain.zone, brain.goal);
                brain
                    .plan
                    .exits(c.0, c.1)
                    .filter(|&(x, z)| d[z][x] < d[c.1][c.0])
                    .min_by_key(|&(x, z)| d[z][x])
                    .map_or(goal, |(x, z)| cell_centre(x, z))
            }
            _ => goal,
        };
        move_to(here, towards, PATROL_SPEED * dt)
    };
    Shuffle {
        pos,
        waddling: false,
        puff,
        gassed: None,
    }
}

/// Server: the fart monster patrols, farts and gasses.
pub(super) fn patrol(
    mut commands: Commands,
    time: Res<Time>,
    still: Query<&crate::features::stillness::Stillness>,
    mut monsters: Query<(&mut FartMonster, &mut FartBrain)>,
    mut pieces: Query<&mut SoulPiece>,
    mut bodies: Bodies,
) {
    if crate::features::stillness::is_frozen(&still) {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let prey: Vec<(u64, Vec2)> = bodies
        .iter()
        .filter(|b| inside(b.2.pos))
        .map(|b| (b.0.id, local(b.2.pos)))
        .collect();
    for (mut monster, mut brain) in &mut monsters {
        let here = local(monster.pos);
        let step = shuffle(&mut brain, here, &prey, dt);
        if monster.waddling != step.waddling {
            monster.waddling = step.waddling;
        }
        if step.puff {
            monster.puffs = monster.puffs.wrapping_add(1);
        }
        if step.pos != here {
            monster.pos = Vec3::new(step.pos.x, INSIDE_Y, step.pos.y);
            let d = step.pos - here;
            monster.yaw = (-d.x).atan2(-d.y);
        }
        let Some(victim) = step.gassed else { continue };
        let at = monster.pos;
        throw_out(victim, at, &mut pieces, &mut bodies);
        commands.server_trigger(ToClients {
            targets: SendTargets::All,
            message: Gassed { player: victim, at },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brain(zone: Zone) -> FartBrain {
        FartBrain {
            plan: Plan::fallback(),
            zone,
            goal: (zone.x0, zone.z0),
            rng: Rng::new(3),
            next_puff: 100.0,
            smug: 0.0,
        }
    }

    const ZONE: Zone = Zone {
        x0: 2,
        z0: 3,
        w: 3,
        h: 3,
    };

    #[test]
    fn zones_are_seeded_away_from_the_door_and_not_always_there() {
        let mut some = 0;
        for seed in 0..200 {
            let z = zone(seed);
            assert_eq!(z, zone(seed));
            let Some(z) = z else { continue };
            some += 1;
            assert!(z.x0 + z.w <= W && z.z0 + z.h <= H, "{z:?}");
            assert!(z.z0 >= 2 && !z.contains(ENTRANCE), "{z:?}");
            assert!((2..=3).contains(&z.w) && (2..=3).contains(&z.h));
            assert_eq!(z.cells().count(), z.w * z.h);
        }
        assert!((80..180).contains(&some), "{some} of 200 have a bathroom");
        assert_eq!(ceiling(Some(ZONE), (3, 4)), CEILING);
        assert_eq!(ceiling(Some(ZONE), (0, 0)), INSIDE_WALL);
    }

    #[test]
    fn it_patrols_without_leaving_the_bathroom() {
        let mut b = brain(ZONE);
        let mut pos = cell_centre(3, 4);
        let mut visited = std::collections::HashSet::new();
        for _ in 0..4000 {
            pos = shuffle(&mut b, pos, &[], 0.1).pos;
            assert!(ZONE.holds(pos), "wandered out to {pos}");
            if let Some(c) = cell_of(pos) {
                visited.insert(c);
            }
        }
        assert!(visited.len() >= 4, "it should get around: {visited:?}");
    }

    #[test]
    fn it_farts_now_and_then() {
        let mut b = brain(ZONE);
        b.next_puff = 0.5;
        let here = cell_centre(3, 4);
        let puffs = (0..300)
            .filter(|_| shuffle(&mut b, here, &[], 0.1).puff)
            .count();
        assert!((2..=12).contains(&puffs), "{puffs} farts in 30 s");
    }

    #[test]
    fn it_waddles_at_you_and_gasses_you() {
        let mut b = brain(ZONE);
        let here = cell_centre(3, 4);
        let you = here + Vec2::X * 4.0;
        let t = shuffle(&mut b, here, &[(5, you)], 0.1);
        assert!(t.waddling);
        assert!(t.pos.distance(you) < here.distance(you));
        let mut pos = here;
        let mut gassed = None;
        for _ in 0..40 {
            let t = shuffle(&mut b, pos, &[(5, you)], 0.1);
            pos = t.pos;
            if t.gassed.is_some() {
                gassed = t.gassed;
                assert!(t.puff, "it gasses you with a fart");
                break;
            }
        }
        assert_eq!(gassed, Some(5));
        // Then it stands there, smug, and leaves you be.
        let t = shuffle(&mut b, pos, &[(5, you)], 0.1);
        assert_eq!((t.gassed, t.pos), (None, pos));
    }

    #[test]
    fn it_ignores_you_outside_the_bathroom() {
        let mut b = brain(ZONE);
        let here = cell_centre(2, 3);
        let you = cell_centre(1, 3);
        let t = shuffle(&mut b, here, &[(5, you)], 0.1);
        assert!(!t.waddling);
    }
}
