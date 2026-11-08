//! One shared "commune person" body for players and villagers, so a hider and
//! a villager look alike at a glance: a round torso, a separate head with a
//! face, stubby arms that swing and little feet that step when moving, a seeded
//! accessory and clothes from one cohesive palette. Purely local visuals; the
//! collision capsule (fps.rs) is untouched and the body fits inside it
//! (feet at y 0, top of head ~1.7).

use std::collections::HashMap;

use bevy::prelude::*;

use crate::art;

/// Who this person looks like. `hue` is the clothing hue in degrees (players
/// use `id * 137.5`, villagers the same scheme); `seed` picks everything else.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub hue: f32,
    pub seed: u64,
}

impl Look {
    /// The per-id scheme shared by players and villagers.
    pub fn for_index(index: u64, seed: u64) -> Self {
        Self {
            hue: (index as f32 * 137.5) % 360.0,
            seed,
        }
    }
    fn r(&self, k: u32) -> f32 {
        art::hash(self.seed as i32, (self.seed >> 32) as i32, k)
    }
}

const SKINS: [Color; 5] = [
    Color::srgb(1.0, 0.86, 0.74),
    Color::srgb(0.96, 0.76, 0.6),
    Color::srgb(0.85, 0.62, 0.45),
    Color::srgb(0.66, 0.45, 0.32),
    Color::srgb(0.5, 0.34, 0.25),
];

/// Accessory colours: warm, soft, cohesive with the pastel clothes.
const CLOTH: [Color; 6] = [
    Color::srgb(0.95, 0.9, 0.78),
    Color::srgb(0.85, 0.45, 0.4),
    Color::srgb(0.45, 0.6, 0.8),
    Color::srgb(0.95, 0.78, 0.4),
    Color::srgb(0.55, 0.72, 0.5),
    Color::srgb(0.6, 0.5, 0.42),
];

const ACCESSORIES: usize = 8;

/// Shared meshes and materials for every person body.
#[derive(Resource)]
pub struct PeopleKit {
    torso: Handle<Mesh>,
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    hand: Handle<Mesh>,
    foot: Handle<Mesh>,
    ball: Handle<Mesh>,
    disc: Handle<Mesh>,
    cube: Handle<Mesh>,
    ring: Handle<Mesh>,
    skins: Vec<Handle<StandardMaterial>>,
    cloth: Vec<Handle<StandardMaterial>>,
    straw: Handle<StandardMaterial>,
    shoe: Handle<StandardMaterial>,
    petal: Handle<StandardMaterial>,
    clothes: HashMap<u32, Handle<StandardMaterial>>,
}

impl FromWorld for PeopleKit {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let torso = meshes.add(Capsule3d::new(0.3, 0.32));
        let head = meshes.add(Sphere::new(0.28));
        let arm = meshes.add(Capsule3d::new(0.07, 0.16));
        let hand = meshes.add(Sphere::new(0.075));
        let foot = meshes.add(Sphere::new(0.1));
        let ball = meshes.add(Sphere::new(1.0));
        let disc = meshes.add(Cylinder::new(1.0, 1.0));
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let ring = meshes.add(Torus::new(0.17, 0.27));
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        let skins = SKINS.iter().map(|c| art::soft(&mut mats, *c)).collect();
        let cloth = CLOTH.iter().map(|c| art::soft(&mut mats, *c)).collect();
        let straw = art::soft(&mut mats, Color::srgb(0.93, 0.82, 0.5));
        let shoe = art::soft(&mut mats, Color::srgb(0.32, 0.24, 0.2));
        let petal = art::soft(&mut mats, Color::srgb(1.0, 0.7, 0.85));
        Self {
            torso,
            head,
            arm,
            hand,
            foot,
            ball,
            disc,
            cube,
            ring,
            skins,
            cloth,
            straw,
            shoe,
            petal,
            clothes: HashMap::new(),
        }
    }
}

impl PeopleKit {
    fn clothes(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        hue: f32,
    ) -> Handle<StandardMaterial> {
        let key = (hue * 10.0) as u32;
        self.clothes
            .entry(key)
            .or_insert_with(|| art::soft(materials, Color::hsl(hue, 0.55, 0.62)))
            .clone()
    }
}

/// On the person's root: smoothed ground speed (from transform deltas) and the
/// stride phase that drives arms and feet.
#[derive(Component, Default)]
pub struct Gait {
    prev: Option<Vec3>,
    speed: f32,
    phase: f32,
}

#[derive(Clone, Copy)]
enum LimbKind {
    Arm,
    Foot,
}

/// An arm pivot or a foot that animates with its root's `Gait`.
#[derive(Component)]
struct Limb {
    root: Entity,
    side: f32,
    kind: LimbKind,
    base: Transform,
}

/// Build a person under `p`. `root` is the entity that moves (gets a `Gait`).
pub fn spawn_person(
    p: &mut ChildSpawnerCommands,
    root: Entity,
    kit: &mut PeopleKit,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    look: Look,
) {
    let clothes = kit.clothes(materials, look.hue);
    let skin = kit.skins[(look.r(1) * SKINS.len() as f32) as usize % SKINS.len()].clone();
    let accent = kit.cloth[(look.r(2) * CLOTH.len() as f32) as usize % CLOTH.len()].clone();
    let phase = look.r(3) * 6.0;

    // Torso: a soft bean, breathing.
    p.spawn((
        Mesh3d(kit.torso.clone()),
        MeshMaterial3d(clothes.clone()),
        Transform::from_xyz(0.0, 0.6, 0.0).with_scale(Vec3::new(1.05, 1.0, 0.92)),
        art::Breathe::new(Vec3::new(1.05, 1.0, 0.92), 0.025, phase),
    ));
    // Head with face (eyes look along -Z like the rest of the game).
    p.spawn((
        Transform::from_xyz(0.0, 1.38, 0.0),
        Visibility::default(),
        art::Wiggle::sway(Quat::IDENTITY, Vec3::Z, 0.04, 1.1, phase),
    ))
    .with_children(|h| {
        h.spawn((Mesh3d(kit.head.clone()), MeshMaterial3d(skin.clone())));
        art::eyes(
            h,
            meshes,
            materials,
            Vec3::new(0.0, 0.03, -0.24),
            0.19,
            0.06,
        );
        art::blush(
            h,
            meshes,
            materials,
            Vec3::new(0.0, -0.06, -0.24),
            0.3,
            0.045,
        );
        accessory_on_head(h, kit, &look, accent.clone());
    });
    // Arms: pivot at the shoulder, sleeve plus a hand.
    for side in [-1.0f32, 1.0] {
        let base = Transform::from_xyz(side * 0.33, 0.92, 0.0)
            .with_rotation(Quat::from_rotation_z(side * 0.25));
        p.spawn((
            base,
            Visibility::default(),
            Limb {
                root,
                side,
                kind: LimbKind::Arm,
                base,
            },
        ))
        .with_children(|a| {
            a.spawn((
                Mesh3d(kit.arm.clone()),
                MeshMaterial3d(clothes.clone()),
                Transform::from_xyz(0.0, -0.13, 0.0),
            ));
            a.spawn((
                Mesh3d(kit.hand.clone()),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.0, -0.3, 0.0),
            ));
        });
        // Feet.
        let base =
            Transform::from_xyz(side * 0.13, 0.06, -0.03).with_scale(Vec3::new(1.0, 0.65, 1.45));
        p.spawn((
            Mesh3d(kit.foot.clone()),
            MeshMaterial3d(kit.shoe.clone()),
            base,
            Limb {
                root,
                side,
                kind: LimbKind::Foot,
                base,
            },
        ));
    }
    accessory_on_body(p, kit, &look, accent);
}

fn accessory(look: &Look) -> usize {
    (look.r(4) * ACCESSORIES as f32) as usize % ACCESSORIES
}

/// Hats, scarves on the head and flowers (head-local, head centre at origin).
fn accessory_on_head(
    h: &mut ChildSpawnerCommands,
    kit: &PeopleKit,
    look: &Look,
    accent: Handle<StandardMaterial>,
) {
    let part = |h: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>, mat, tf: Transform| {
        h.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), tf));
    };
    match accessory(look) {
        // Beanie with a pompom.
        0 => {
            part(
                h,
                &kit.ball,
                accent.clone(),
                Transform::from_xyz(0.0, 0.1, 0.02).with_scale(Vec3::new(0.29, 0.22, 0.29)),
            );
            part(
                h,
                &kit.ball,
                kit.cloth[0].clone(),
                Transform::from_xyz(0.0, 0.33, 0.02).with_scale(Vec3::splat(0.06)),
            );
        }
        // Flat cap with a brim out front.
        1 => {
            part(
                h,
                &kit.disc,
                accent.clone(),
                Transform::from_xyz(0.0, 0.2, 0.0).with_scale(Vec3::new(0.27, 0.09, 0.27)),
            );
            part(
                h,
                &kit.disc,
                accent,
                Transform::from_xyz(0.0, 0.17, -0.2).with_scale(Vec3::new(0.18, 0.03, 0.14)),
            );
        }
        // Straw hat with a band.
        2 => {
            part(
                h,
                &kit.disc,
                kit.straw.clone(),
                Transform::from_xyz(0.0, 0.2, 0.0).with_scale(Vec3::new(0.48, 0.025, 0.48)),
            );
            part(
                h,
                &kit.disc,
                kit.straw.clone(),
                Transform::from_xyz(0.0, 0.26, 0.0).with_scale(Vec3::new(0.21, 0.1, 0.21)),
            );
            part(
                h,
                &kit.disc,
                accent,
                Transform::from_xyz(0.0, 0.24, 0.0).with_scale(Vec3::new(0.205, 0.04, 0.205)),
            );
        }
        // Headscarf over the back of the head, knot behind.
        3 => {
            part(
                h,
                &kit.ball,
                accent.clone(),
                Transform::from_xyz(0.0, 0.06, 0.04).with_scale(Vec3::new(0.3, 0.27, 0.29)),
            );
            part(
                h,
                &kit.ball,
                accent,
                Transform::from_xyz(0.0, -0.05, 0.3).with_scale(Vec3::splat(0.07)),
            );
        }
        _ => {}
    }
    // A flower tucked by the ear for some.
    if accessory(look) == 7 || look.r(5) < 0.2 {
        for k in 0..5 {
            let a = k as f32 * std::f32::consts::TAU / 5.0;
            part(
                h,
                &kit.ball,
                kit.petal.clone(),
                Transform::from_xyz(0.26, 0.12 + a.sin() * 0.04, a.cos() * 0.04)
                    .with_scale(Vec3::splat(0.03)),
            );
        }
        part(
            h,
            &kit.ball,
            kit.cloth[3].clone(),
            Transform::from_xyz(0.28, 0.12, 0.0).with_scale(Vec3::splat(0.025)),
        );
    }
}

/// Scarf, apron or backpack (body-local).
fn accessory_on_body(
    p: &mut ChildSpawnerCommands,
    kit: &PeopleKit,
    look: &Look,
    accent: Handle<StandardMaterial>,
) {
    let part = |p: &mut ChildSpawnerCommands, mesh: &Handle<Mesh>, mat, tf: Transform| {
        p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat), tf));
    };
    match accessory(look) {
        // Scarf round the neck with a hanging tail.
        4 => {
            part(
                p,
                &kit.ring,
                accent.clone(),
                Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(1.0, 1.3, 0.95)),
            );
            part(
                p,
                &kit.cube,
                accent,
                Transform::from_xyz(0.12, 0.92, -0.27)
                    .with_scale(Vec3::new(0.09, 0.26, 0.04))
                    .with_rotation(Quat::from_rotation_z(0.15)),
            );
        }
        // Apron on the front.
        5 => {
            part(
                p,
                &kit.cube,
                accent,
                Transform::from_xyz(0.0, 0.55, -0.29)
                    .with_scale(Vec3::new(0.38, 0.5, 0.03))
                    .with_rotation(Quat::from_rotation_x(0.08)),
            );
        }
        // Backpack.
        6 => {
            part(
                p,
                &kit.cube,
                accent.clone(),
                Transform::from_xyz(0.0, 0.72, 0.33).with_scale(Vec3::new(0.36, 0.42, 0.2)),
            );
            part(
                p,
                &kit.cube,
                kit.cloth[5].clone(),
                Transform::from_xyz(0.0, 0.62, 0.44).with_scale(Vec3::new(0.26, 0.16, 0.06)),
            );
        }
        _ => {}
    }
}

/// Registers gait + limb animation. Purely local visuals.
pub fn plugin(app: &mut App) {
    app.init_resource::<PeopleKit>()
        .add_systems(Update, (track_gait, swing_limbs).chain());
}

fn track_gait(time: Res<Time>, mut q: Query<(&GlobalTransform, &mut Gait)>) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (gt, mut g) in &mut q {
        let now = gt.translation();
        let raw = g
            .prev
            .map_or(0.0, |p| (now - p).with_y(0.0).length() / dt)
            .min(12.0);
        g.prev = Some(now);
        let k = 1.0 - (-10.0 * dt).exp();
        g.speed += (raw - g.speed) * k;
        let speed = g.speed;
        g.phase = (g.phase + speed.min(6.0) * 2.2 * dt).rem_euclid(std::f32::consts::TAU);
    }
}

fn swing_limbs(gaits: Query<&Gait>, mut limbs: Query<(&Limb, &mut Transform)>) {
    for (limb, mut tf) in &mut limbs {
        let Ok(g) = gaits.get(limb.root) else {
            continue;
        };
        let amt = (g.speed / 4.0).clamp(0.0, 1.0);
        let s = (g.phase
            + if limb.side > 0.0 {
                0.0
            } else {
                std::f32::consts::PI
            })
        .sin();
        let c = (g.phase
            + if limb.side > 0.0 {
                0.0
            } else {
                std::f32::consts::PI
            })
        .cos();
        match limb.kind {
            LimbKind::Arm => {
                // Arms swing opposite to the foot on the same side.
                tf.rotation = limb.base.rotation * Quat::from_rotation_x(s * 0.8 * amt);
            }
            LimbKind::Foot => {
                tf.translation = limb.base.translation
                    + Vec3::new(0.0, c.max(0.0) * 0.07 * amt, -s * 0.13 * amt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_spread_over_accessories() {
        let mut seen = [false; ACCESSORIES];
        for s in 0..200 {
            seen[accessory(&Look::for_index(s, s * 7 + 1))] = true;
        }
        assert!(seen.iter().all(|b| *b));
    }
}
