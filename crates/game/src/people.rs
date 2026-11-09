//! Procedural, rigged characters for everyone (#100 by doomy): players, the
//! commune villagers and the menu's campfire crowd all use this one system, so
//! a hider and a villager are drawn from the same option pool.
//!
//! A [`CharacterLook`] is plain data (body type, proportions, palette colours,
//! face features, an accessory). Put it next to a [`Gait`] on any moving
//! entity and [`plugin`] builds a little bone hierarchy under it (root -> hips
//! -> spine -> head / arms, hips -> legs / tail) with shared primitive meshes
//! parented to the bones, and animates the bones procedurally: a walk cycle
//! from the measured ground speed, idle breathing, idle glances and a wagging
//! tail. Changing the look rebuilds the rig.
//!
//! On players the look is a replicated component: each peer sends the look it
//! made in the character creator with [`SetLook`], the server clamps it and
//! everyone sees it. Visuals never touch collision: the whole rig is scaled
//! down to fit the walker capsule (top <= [`MAX_TOP`], half-width <=
//! [`MAX_HALF_WIDTH`]).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::art;
use crate::player::{Player, PlayerOwner};

/// Highest a body (head plus hat) may reach, in metres.
pub const MAX_TOP: f32 = 1.8;
/// Widest a body may be from its centre line (the capsule is 0.35).
pub const MAX_HALF_WIDTH: f32 = 0.42;

pub const BODIES: [&str; 4] = [
    "person",
    "round critter",
    "tall noodle",
    "four-legged beast",
];
pub const EYES: [&str; 4] = ["googly", "beady", "sleepy", "cyclops"];
pub const EARS: [&str; 5] = ["none", "round", "pointy", "floppy", "bunny"];
pub const HORNS: [&str; 3] = ["none", "nubs", "spikes"];
pub const TAILS: [&str; 4] = ["none", "stub", "long", "fluffy"];
pub const SNOUTS: [&str; 4] = ["none", "button nose", "snout", "beak"];
pub const ACCESSORIES: [&str; 9] = [
    "beanie",
    "flat cap",
    "straw hat",
    "headscarf",
    "scarf",
    "apron",
    "backpack",
    "flower",
    "none",
];

/// Skin tones first, then fur colours.
const SKINS: [Color; 10] = [
    Color::srgb(1.0, 0.86, 0.74),
    Color::srgb(0.96, 0.76, 0.6),
    Color::srgb(0.85, 0.62, 0.45),
    Color::srgb(0.66, 0.45, 0.32),
    Color::srgb(0.5, 0.34, 0.25),
    Color::srgb(0.72, 0.72, 0.76),
    Color::srgb(0.98, 0.93, 0.82),
    Color::srgb(0.95, 0.6, 0.3),
    Color::srgb(0.55, 0.72, 0.45),
    Color::srgb(0.75, 0.65, 0.95),
];
const HUMAN_SKINS: usize = 5;
const CLOTHES: usize = 12;

/// Accessory colours: warm, soft, cohesive with the pastel clothes.
const ACCENTS: [Color; 6] = [
    Color::srgb(0.95, 0.9, 0.78),
    Color::srgb(0.85, 0.45, 0.4),
    Color::srgb(0.45, 0.6, 0.8),
    Color::srgb(0.95, 0.78, 0.4),
    Color::srgb(0.55, 0.72, 0.5),
    Color::srgb(0.6, 0.5, 0.42),
];

/// What a character looks like. Proportions are `0..=1` (0.5 is average),
/// everything else is an index into the option lists above.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CharacterLook {
    pub body: u8,
    pub height: f32,
    pub girth: f32,
    pub head: f32,
    pub limbs: f32,
    pub skin: u8,
    pub clothes: u8,
    pub accent: u8,
    pub eyes: u8,
    pub ears: u8,
    pub horns: u8,
    pub tail: u8,
    pub snout: u8,
    pub accessory: u8,
}

impl Default for CharacterLook {
    fn default() -> Self {
        Self {
            body: 0,
            height: 0.5,
            girth: 0.5,
            head: 0.5,
            limbs: 0.5,
            skin: 1,
            clothes: 0,
            accent: 1,
            eyes: 0,
            ears: 0,
            horns: 0,
            tail: 0,
            snout: 0,
            accessory: 8,
        }
    }
}

/// One editable property, for the character creator and the save file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Body,
    Height,
    Girth,
    Head,
    Limbs,
    Skin,
    Clothes,
    Accent,
    Eyes,
    Ears,
    Horns,
    Tail,
    Snout,
    Accessory,
}

impl Field {
    pub const ALL: [Field; 14] = [
        Field::Body,
        Field::Height,
        Field::Girth,
        Field::Head,
        Field::Limbs,
        Field::Skin,
        Field::Clothes,
        Field::Accent,
        Field::Eyes,
        Field::Ears,
        Field::Horns,
        Field::Tail,
        Field::Snout,
        Field::Accessory,
    ];

    /// Name in the save file.
    pub fn key(self) -> &'static str {
        match self {
            Field::Body => "body",
            Field::Height => "height",
            Field::Girth => "girth",
            Field::Head => "head",
            Field::Limbs => "limbs",
            Field::Skin => "skin",
            Field::Clothes => "clothes",
            Field::Accent => "accent",
            Field::Eyes => "eyes",
            Field::Ears => "ears",
            Field::Horns => "horns",
            Field::Tail => "tail",
            Field::Snout => "snout",
            Field::Accessory => "accessory",
        }
    }

    /// Name in the creator.
    pub fn label(self) -> &'static str {
        match self {
            Field::Body => "Body",
            Field::Height => "Height",
            Field::Girth => "Girth",
            Field::Head => "Head size",
            Field::Limbs => "Limb length",
            Field::Skin => "Skin / fur",
            Field::Clothes => "Clothes",
            Field::Accent => "Accent",
            Field::Eyes => "Eyes",
            Field::Ears => "Ears",
            Field::Horns => "Horns",
            Field::Tail => "Tail",
            Field::Snout => "Snout",
            Field::Accessory => "Accessory",
        }
    }

    /// Option count for index fields, `None` for proportions.
    pub fn options(self) -> Option<u8> {
        Some(match self {
            Field::Body => BODIES.len(),
            Field::Skin => SKINS.len(),
            Field::Clothes => CLOTHES,
            Field::Accent => ACCENTS.len(),
            Field::Eyes => EYES.len(),
            Field::Ears => EARS.len(),
            Field::Horns => HORNS.len(),
            Field::Tail => TAILS.len(),
            Field::Snout => SNOUTS.len(),
            Field::Accessory => ACCESSORIES.len(),
            Field::Height | Field::Girth | Field::Head | Field::Limbs => return None,
        } as u8)
    }
}

impl CharacterLook {
    fn index_mut(&mut self, f: Field) -> Option<&mut u8> {
        Some(match f {
            Field::Body => &mut self.body,
            Field::Skin => &mut self.skin,
            Field::Clothes => &mut self.clothes,
            Field::Accent => &mut self.accent,
            Field::Eyes => &mut self.eyes,
            Field::Ears => &mut self.ears,
            Field::Horns => &mut self.horns,
            Field::Tail => &mut self.tail,
            Field::Snout => &mut self.snout,
            Field::Accessory => &mut self.accessory,
            _ => return None,
        })
    }

    fn scalar_mut(&mut self, f: Field) -> Option<&mut f32> {
        Some(match f {
            Field::Height => &mut self.height,
            Field::Girth => &mut self.girth,
            Field::Head => &mut self.head,
            Field::Limbs => &mut self.limbs,
            _ => return None,
        })
    }

    /// The same look with every value forced into range (what the server keeps).
    pub fn clamped(mut self) -> Self {
        for f in Field::ALL {
            if let Some(n) = f.options()
                && let Some(v) = self.index_mut(f)
            {
                *v = (*v).min(n - 1);
            }
            if let Some(v) = self.scalar_mut(f) {
                *v = if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.5
                };
            }
        }
        self
    }

    /// Step a field: indices wrap round, proportions move in tenths.
    pub fn nudge(&mut self, f: Field, steps: i32) {
        if let Some(n) = f.options()
            && let Some(v) = self.index_mut(f)
        {
            *v = (*v as i32 + steps).rem_euclid(n as i32) as u8;
        }
        if let Some(v) = self.scalar_mut(f) {
            *v = ((*v + steps as f32 * 0.1).clamp(0.0, 1.0) * 10.0).round() / 10.0;
        }
    }

    /// Human-readable value of a field.
    pub fn describe(&mut self, f: Field) -> String {
        let name = |list: &[&str], i: u8| list.get(i as usize).copied().unwrap_or("?").to_string();
        match f {
            Field::Body => name(&BODIES, self.body),
            Field::Eyes => name(&EYES, self.eyes),
            Field::Ears => name(&EARS, self.ears),
            Field::Horns => name(&HORNS, self.horns),
            Field::Tail => name(&TAILS, self.tail),
            Field::Snout => name(&SNOUTS, self.snout),
            Field::Accessory => name(&ACCESSORIES, self.accessory),
            _ => {
                if let Some(v) = self.scalar_mut(f) {
                    format!("{}%", (*v * 100.0).round() as i32)
                } else {
                    let n = f.options().unwrap_or(1);
                    let v = self.index_mut(f).map_or(0, |v| *v);
                    format!("{}/{n}", v + 1)
                }
            }
        }
    }

    /// A random look from the shared pool (villagers, Randomize, defaults):
    /// mostly people, sometimes a critter, noodle or beast.
    pub fn random(seed: u64) -> Self {
        let r = |k: u32| art::hash(seed as i32, (seed >> 32) as i32, k);
        let pick = |k: u32, n: usize| ((r(k) * n as f32) as usize).min(n - 1) as u8;
        let body = match r(0) {
            x if x < 0.55 => 0,
            x if x < 0.7 => 1,
            x if x < 0.85 => 2,
            _ => 3,
        };
        let person = body == 0 || body == 2;
        let prop = |k: u32| ((0.15 + r(k) * 0.7) * 10.0).round() / 10.0;
        Self {
            body,
            height: prop(1),
            girth: prop(2),
            head: prop(3),
            limbs: prop(4),
            skin: pick(5, if person { HUMAN_SKINS } else { SKINS.len() }),
            clothes: pick(6, CLOTHES),
            accent: pick(7, ACCENTS.len()),
            eyes: if r(8) < 0.6 { 0 } else { pick(9, EYES.len()) },
            ears: if person && r(10) < 0.8 {
                0
            } else {
                pick(11, EARS.len())
            },
            horns: if person || r(12) < 0.5 {
                0
            } else {
                pick(13, HORNS.len())
            },
            tail: if person { 0 } else { pick(14, TAILS.len()) },
            snout: if person {
                if r(15) < 0.2 { 1 } else { 0 }
            } else {
                pick(16, SNOUTS.len())
            },
            accessory: pick(17, ACCESSORIES.len()),
        }
    }

    /// The default for player `id` before they send their own: random, with
    /// clothes spread over the palette so friends tell each other apart.
    pub fn for_player(id: u64) -> Self {
        Self {
            clothes: ((id * 5) % CLOTHES as u64) as u8,
            ..Self::random(id.wrapping_mul(7) + 1)
        }
    }

    /// `key=value` pairs, one per line (the save file).
    pub fn to_text(mut self) -> String {
        let mut out = String::new();
        for f in Field::ALL {
            let v = if let Some(v) = self.scalar_mut(f) {
                format!("{v}")
            } else {
                format!("{}", self.index_mut(f).map_or(0, |v| *v))
            };
            out.push_str(&format!("{}={v}\n", f.key()));
        }
        out
    }

    /// Parse [`Self::to_text`]; unknown or broken lines are skipped, missing
    /// ones keep the default, and the result is clamped.
    pub fn from_text(text: &str) -> Self {
        let mut look = Self::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let Some(f) = Field::ALL.into_iter().find(|f| f.key() == k.trim()) else {
                continue;
            };
            let v = v.trim();
            if let Some(slot) = look.scalar_mut(f)
                && let Ok(x) = v.parse::<f32>()
            {
                *slot = x;
            }
            if let Some(slot) = look.index_mut(f)
                && let Ok(x) = v.parse::<u8>()
            {
                *slot = x;
            }
        }
        look.clamped()
    }
}

/// Client -> server: "this is what I look like". The host sends it too.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SetLook(pub CharacterLook);

/// Registered on every peer (append-only, see features/mod.rs).
pub fn protocol(app: &mut App) {
    app.replicate::<CharacterLook>()
        .add_client_event::<SetLook>(Channel::Ordered)
        .add_observer(default_look)
        .add_observer(set_look);
}

fn default_look(
    add: On<Add<Player>>,
    server: Res<State<ServerState>>,
    players: Query<&Player, Without<CharacterLook>>,
    mut commands: Commands,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    let Ok(p) = players.get(add.entity) else {
        return;
    };
    commands
        .entity(add.entity)
        .insert(CharacterLook::for_player(p.id));
}

fn set_look(ev: On<FromClient<SetLook>>, mut players: Query<(&PlayerOwner, &mut CharacterLook)>) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let look = ev.message.0.clamped();
    for (o, mut l) in &mut players {
        if o.0 == owner {
            l.set_if_neq(look);
        }
    }
}

/// Shared meshes (unit primitives, scaled per part) and one material per
/// palette colour, so a crowd costs no extra assets.
#[derive(Resource)]
pub struct CharacterKit {
    ball: Handle<Mesh>,
    capsule: Handle<Mesh>,
    disc: Handle<Mesh>,
    cube: Handle<Mesh>,
    cone: Handle<Mesh>,
    ring: Handle<Mesh>,
    skins: Vec<Handle<StandardMaterial>>,
    clothes: Vec<Handle<StandardMaterial>>,
    accents: Vec<Handle<StandardMaterial>>,
    white: Handle<StandardMaterial>,
    pupil: Handle<StandardMaterial>,
    nose: Handle<StandardMaterial>,
    blush: Handle<StandardMaterial>,
    horn: Handle<StandardMaterial>,
    shoe: Handle<StandardMaterial>,
    straw: Handle<StandardMaterial>,
    petal: Handle<StandardMaterial>,
}

impl FromWorld for CharacterKit {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let ball = meshes.add(Sphere::new(1.0));
        let capsule = meshes.add(Capsule3d::new(0.5, 1.0));
        let disc = meshes.add(Cylinder::new(1.0, 1.0));
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let cone = meshes.add(Cone::new(0.5, 1.0));
        let ring = meshes.add(Torus::new(0.17, 0.27));
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        let skins = SKINS.iter().map(|c| art::soft(&mut mats, *c)).collect();
        let clothes = (0..CLOTHES)
            .map(|i| art::soft(&mut mats, Color::hsl(i as f32 * 30.0, 0.55, 0.62)))
            .collect();
        let accents = ACCENTS.iter().map(|c| art::soft(&mut mats, *c)).collect();
        Self {
            ball,
            capsule,
            disc,
            cube,
            cone,
            ring,
            skins,
            clothes,
            accents,
            white: art::glossy(&mut mats, Color::srgb(0.98, 0.98, 0.96)),
            pupil: art::glossy(&mut mats, Color::srgb(0.04, 0.04, 0.06)),
            nose: art::glossy(&mut mats, Color::srgb(0.35, 0.2, 0.22)),
            blush: art::soft(&mut mats, Color::srgb(1.0, 0.55, 0.6)),
            horn: art::soft(&mut mats, Color::srgb(0.95, 0.9, 0.78)),
            shoe: art::soft(&mut mats, Color::srgb(0.32, 0.24, 0.2)),
            straw: art::soft(&mut mats, Color::srgb(0.93, 0.82, 0.5)),
            petal: art::soft(&mut mats, Color::srgb(1.0, 0.7, 0.85)),
        }
    }
}

/// On the person's root: smoothed ground speed (from transform deltas) and the
/// stride phase that drives the bones.
#[derive(Component, Default)]
pub struct Gait {
    prev: Option<Vec3>,
    speed: f32,
    phase: f32,
}

/// The rig built for a root (despawned and rebuilt when the look changes).
#[derive(Component)]
pub struct CharacterRig(pub Entity);

/// The rig's top entity, a direct child of the root. Its transform is free for
/// the owner to use (villagers squash it when they sit).
#[derive(Component)]
pub struct RigRoot;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BoneKind {
    Hips,
    Spine,
    Head,
    Arm,
    Leg,
    Tail,
}

/// A bone the animator drives, relative to its rest pose `base`.
#[derive(Component)]
struct Bone {
    root: Entity,
    kind: BoneKind,
    side: f32,
    /// Extra stride phase (a beast's front legs step diagonally).
    offset: f32,
    idle: f32,
    base: Transform,
}

/// Registers rig building and the procedural animator. Purely local visuals.
pub fn plugin(app: &mut App) {
    app.init_resource::<CharacterKit>()
        .add_systems(Update, (build_rigs, track_gait, animate_bones).chain());
}

#[allow(clippy::type_complexity)]
fn build_rigs(
    mut commands: Commands,
    kit: Res<CharacterKit>,
    changed: Query<
        (Entity, &CharacterLook, Option<&CharacterRig>),
        (With<Gait>, Or<(Changed<CharacterLook>, Added<Gait>)>),
    >,
) {
    for (root, look, old) in &changed {
        if let Some(old) = old {
            commands.entity(old.0).try_despawn();
        }
        let rig = commands
            .spawn((
                RigRoot,
                Transform::default(),
                Visibility::default(),
                ChildOf(root),
            ))
            .id();
        let look = look.clamped();
        commands
            .entity(rig)
            .with_children(|p| build(p, root, &kit, &look));
        commands.entity(root).insert(CharacterRig(rig));
    }
}

struct Ctx<'a> {
    kit: &'a CharacterKit,
    look: &'a CharacterLook,
    root: Entity,
    skin: Handle<StandardMaterial>,
    clothes: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    idle: f32,
}

impl Ctx<'_> {
    fn part(
        &self,
        p: &mut ChildSpawnerCommands,
        mesh: &Handle<Mesh>,
        mat: &Handle<StandardMaterial>,
        tf: Transform,
    ) {
        p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), tf));
    }

    fn bone<'a>(
        &self,
        p: &'a mut ChildSpawnerCommands,
        kind: BoneKind,
        side: f32,
        offset: f32,
        base: Transform,
    ) -> EntityCommands<'a> {
        p.spawn((
            base,
            Visibility::default(),
            Bone {
                root: self.root,
                kind,
                side,
                offset,
                idle: self.idle,
                base,
            },
        ))
    }
}

/// Body dimensions for one look, before the fit-the-capsule scale.
struct Shape {
    leg: f32,
    rx: f32,
    ry: f32,
    rz: f32,
    head: f32,
    arm: f32,
    limb_r: f32,
}

fn shape(look: &CharacterLook) -> Shape {
    let s = 0.85 + 0.3 * look.height;
    let (g, h, l) = (look.girth, look.head, look.limbs);
    match look.body {
        // Round critter: a ball on tiny feet with nubby arms.
        1 => {
            let rx = 0.3 + 0.14 * g;
            Shape {
                leg: (0.1 + 0.1 * l) * s,
                rx,
                ry: rx * 0.95 * s,
                rz: rx * 0.9,
                head: 0.18 + 0.1 * h,
                arm: 0.12 + 0.1 * l,
                limb_r: 0.06,
            }
        }
        // Tall noodle: long, thin, long-necked.
        2 => Shape {
            leg: (0.45 + 0.2 * l) * s,
            rx: 0.13 + 0.08 * g,
            ry: 0.4 * s,
            rz: (0.13 + 0.08 * g) * 0.85,
            head: 0.15 + 0.08 * h,
            arm: (0.4 + 0.2 * l) * s,
            limb_r: 0.045,
        },
        // Four-legged beast: a long horizontal body.
        3 => Shape {
            leg: (0.25 + 0.2 * l) * s,
            rx: 0.2 + 0.1 * g,
            ry: 0.2 + 0.06 * g,
            rz: 0.4 * s,
            head: 0.17 + 0.1 * h,
            arm: 0.0,
            limb_r: 0.07,
        },
        // Person (the old commune body).
        _ => {
            let rx = 0.24 + 0.12 * g;
            Shape {
                leg: (0.28 + 0.18 * l) * s,
                rx,
                ry: 0.33 * s,
                rz: rx * 0.85,
                head: 0.2 + 0.12 * h,
                arm: (0.24 + 0.16 * l) * s,
                limb_r: 0.07,
            }
        }
    }
}

/// Where the bones sit (rig-local, feet at y 0, facing -Z) and the uniform
/// scale that fits the body inside the walker capsule.
struct Layout {
    hip_y: f32,
    torso_c: f32,
    neck: f32,
    head_at: Vec3,
    #[cfg_attr(not(test), allow(dead_code))]
    top: f32,
    #[cfg_attr(not(test), allow(dead_code))]
    half_w: f32,
    fit: f32,
}

fn layout(look: &CharacterLook, sh: &Shape) -> Layout {
    let beast = look.body == 3;
    let hip_y = if beast { sh.leg + sh.ry * 0.5 } else { sh.leg };
    let torso_c = if beast { 0.0 } else { sh.ry * 0.9 };
    let neck = if look.body == 2 { 0.18 } else { 0.0 };
    let head_at = if beast {
        Vec3::new(0.0, sh.ry * 0.8, -sh.rz - sh.head * 0.35)
    } else if look.body == 1 {
        Vec3::new(0.0, torso_c + sh.ry * 0.75, -sh.rz * 0.15)
    } else {
        Vec3::new(0.0, torso_c + sh.ry + neck + sh.head * 0.8, 0.0)
    };
    let hat: f32 = if look.accessory <= 3 { 0.35 } else { 0.0 };
    let horn = if look.horns > 0 { 0.25 } else { 0.0 };
    let ear = if look.ears == 4 { 0.6 } else { 0.0 };
    let top = hip_y + head_at.y + sh.head * (1.0 + hat.max(horn).max(ear));
    let half_w = if beast { sh.rx } else { sh.rx + sh.limb_r };
    let fit = (MAX_TOP / top).min(MAX_HALF_WIDTH / half_w).min(1.0);
    Layout {
        hip_y,
        torso_c,
        neck,
        head_at,
        top,
        half_w,
        fit,
    }
}

/// Spawn the bones and parts for `look` under the rig root.
fn build(p: &mut ChildSpawnerCommands, root: Entity, kit: &CharacterKit, look: &CharacterLook) {
    let idle = art::hash(
        look.skin as i32 * 31 + look.clothes as i32,
        root.index_u32() as i32,
        9,
    ) * TAU;
    let ctx = Ctx {
        kit,
        look,
        root,
        skin: kit.skins[look.skin as usize % kit.skins.len()].clone(),
        clothes: kit.clothes[look.clothes as usize % kit.clothes.len()].clone(),
        accent: kit.accents[look.accent as usize % kit.accents.len()].clone(),
        idle,
    };
    let sh = shape(look);
    let beast = look.body == 3;
    let furry = look.body == 1 || beast;

    let Layout {
        hip_y,
        torso_c,
        neck,
        head_at,
        fit,
        ..
    } = layout(look, &sh);

    p.spawn((
        Transform::from_scale(Vec3::splat(fit)),
        Visibility::default(),
    ))
    .with_children(|f| {
        ctx.bone(
            f,
            BoneKind::Hips,
            0.0,
            0.0,
            Transform::from_xyz(0.0, hip_y, 0.0),
        )
        .with_children(|hips| {
            ctx.bone(hips, BoneKind::Spine, 0.0, 0.0, Transform::IDENTITY)
                .with_children(|sp| {
                    spine(sp, &ctx, &sh, torso_c, neck, head_at, furry);
                });
            legs(hips, &ctx, &sh);
            if look.tail > 0 {
                let at = if beast {
                    Vec3::new(0.0, sh.ry * 0.3, sh.rz * 0.95)
                } else {
                    Vec3::new(0.0, torso_c * 0.4, sh.rz * 0.9)
                };
                ctx.bone(
                    hips,
                    BoneKind::Tail,
                    0.0,
                    0.0,
                    Transform::from_translation(at).with_rotation(Quat::from_rotation_x(-0.5)),
                )
                .with_children(|t| tail(t, &ctx));
            }
        });
    });
}

fn spine(
    sp: &mut ChildSpawnerCommands,
    ctx: &Ctx,
    sh: &Shape,
    torso_c: f32,
    neck: f32,
    head_at: Vec3,
    furry: bool,
) {
    let kit = ctx.kit;
    let look = ctx.look;
    let beast = look.body == 3;
    let body_mat = if furry { &ctx.skin } else { &ctx.clothes };
    let torso = Transform::from_xyz(0.0, torso_c, 0.0).with_scale(Vec3::new(sh.rx, sh.ry, sh.rz));
    ctx.part(sp, &kit.ball, body_mat, torso);
    if look.body == 1 {
        // Belly patch in the clothes colour.
        ctx.part(
            sp,
            &kit.ball,
            &ctx.clothes,
            Transform::from_xyz(0.0, torso_c - sh.ry * 0.15, -sh.rz * 0.45).with_scale(Vec3::new(
                sh.rx * 0.65,
                sh.ry * 0.6,
                sh.rz * 0.6,
            )),
        );
    }
    if beast {
        // A blanket over the back.
        ctx.part(
            sp,
            &kit.cube,
            &ctx.clothes,
            Transform::from_xyz(0.0, sh.ry * 0.85, 0.05).with_scale(Vec3::new(
                sh.rx * 2.05,
                0.06,
                sh.rz * 0.9,
            )),
        );
        // Front legs hang from the shoulders.
        for side in [-1.0f32, 1.0] {
            leg(
                sp,
                ctx,
                sh,
                Vec3::new(side * sh.rx * 0.6, -sh.ry * 0.5, -sh.rz * 0.6),
                side,
                PI,
            );
        }
    }
    if neck > 0.0 {
        ctx.part(
            sp,
            &kit.capsule,
            &ctx.skin,
            Transform::from_xyz(0.0, torso_c + sh.ry + neck * 0.4, 0.0)
                .with_scale(Vec3::new(0.12, neck, 0.12)),
        );
    }
    // Accessories worn on the body ride in a frame matching the old torso,
    // turned forward on a beast so the scarf goes round its neck.
    let mut frame = Transform::from_xyz(0.0, torso_c, 0.0).with_scale(Vec3::new(
        sh.rx / 0.315,
        sh.ry / 0.46,
        sh.rz / 0.276,
    ));
    if beast {
        frame.rotation = Quat::from_rotation_x(-FRAC_PI_2);
        frame.scale = Vec3::new(sh.rx / 0.315, sh.rz / 0.46, sh.ry / 0.276);
    }
    sp.spawn((frame, Visibility::default()))
        .with_children(|a| accessory_on_body(a, ctx));

    ctx.bone(
        sp,
        BoneKind::Head,
        0.0,
        0.0,
        Transform::from_translation(head_at),
    )
    .with_children(|h| head(h, ctx, sh.head));

    if !beast {
        for side in [-1.0f32, 1.0] {
            let base = Transform::from_xyz(
                side * sh.rx * 0.95,
                torso_c + sh.ry * if look.body == 1 { 0.1 } else { 0.55 },
                0.0,
            )
            .with_rotation(Quat::from_rotation_z(side * 0.25));
            ctx.bone(sp, BoneKind::Arm, side, 0.0, base)
                .with_children(|a| {
                    let sleeve = if furry { &ctx.skin } else { &ctx.clothes };
                    ctx.part(
                        a,
                        &kit.capsule,
                        sleeve,
                        Transform::from_xyz(0.0, -sh.arm * 0.45, 0.0).with_scale(Vec3::new(
                            sh.limb_r * 2.0,
                            sh.arm * 0.5,
                            sh.limb_r * 2.0,
                        )),
                    );
                    ctx.part(
                        a,
                        &kit.ball,
                        &ctx.skin,
                        Transform::from_xyz(0.0, -sh.arm, 0.0)
                            .with_scale(Vec3::splat(sh.limb_r * 1.1)),
                    );
                });
        }
    }
}

fn legs(hips: &mut ChildSpawnerCommands, ctx: &Ctx, sh: &Shape) {
    for side in [-1.0f32, 1.0] {
        let at = if ctx.look.body == 3 {
            Vec3::new(side * sh.rx * 0.6, -sh.ry * 0.5, sh.rz * 0.6)
        } else {
            Vec3::new(side * sh.rx * 0.45, 0.0, 0.0)
        };
        leg(hips, ctx, sh, at, side, 0.0);
    }
}

fn leg(p: &mut ChildSpawnerCommands, ctx: &Ctx, sh: &Shape, at: Vec3, side: f32, offset: f32) {
    let kit = ctx.kit;
    let furry = ctx.look.body == 1 || ctx.look.body == 3;
    let len = sh.leg;
    ctx.bone(
        p,
        BoneKind::Leg,
        side,
        offset,
        Transform::from_translation(at),
    )
    .with_children(|l| {
        let mat = if furry { &ctx.skin } else { &ctx.clothes };
        if len > 0.12 {
            ctx.part(
                l,
                &kit.capsule,
                mat,
                Transform::from_xyz(0.0, -len * 0.5, 0.0).with_scale(Vec3::new(
                    sh.limb_r * 2.2,
                    len * 0.5,
                    sh.limb_r * 2.2,
                )),
            );
        }
        let foot = if furry { &ctx.skin } else { &kit.shoe };
        ctx.part(
            l,
            &kit.ball,
            foot,
            Transform::from_xyz(0.0, -len + 0.06, -0.03).with_scale(Vec3::new(0.1, 0.065, 0.145)),
        );
    });
}

fn tail(t: &mut ChildSpawnerCommands, ctx: &Ctx) {
    let kit = ctx.kit;
    match ctx.look.tail {
        1 => ctx.part(
            t,
            &kit.ball,
            &ctx.skin,
            Transform::from_scale(Vec3::splat(0.09)),
        ),
        2 => ctx.part(
            t,
            &kit.capsule,
            &ctx.skin,
            Transform::from_xyz(0.0, 0.0, 0.25)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                .with_scale(Vec3::new(0.09, 0.25, 0.09)),
        ),
        _ => ctx.part(
            t,
            &kit.ball,
            &ctx.skin,
            Transform::from_xyz(0.0, 0.0, 0.12).with_scale(Vec3::new(0.16, 0.16, 0.2)),
        ),
    }
}

/// Head-local, centre at the origin, face towards -Z.
fn head(h: &mut ChildSpawnerCommands, ctx: &Ctx, r: f32) {
    let kit = ctx.kit;
    let look = ctx.look;
    ctx.part(
        h,
        &kit.ball,
        &ctx.skin,
        Transform::from_scale(Vec3::splat(r)),
    );

    // Eyes.
    let eye = |h: &mut ChildSpawnerCommands, at: Vec3, er: f32, style: u8| {
        h.spawn((
            Transform::from_translation(at),
            Visibility::default(),
            art::Blink {
                phase: ctx.idle + at.x,
            },
        ))
        .with_children(|e| {
            if style == 1 {
                ctx.part(
                    e,
                    &kit.ball,
                    &kit.pupil,
                    Transform::from_scale(Vec3::splat(er * 0.55)),
                );
                return;
            }
            ctx.part(
                e,
                &kit.ball,
                &kit.white,
                Transform::from_scale(Vec3::splat(er)),
            );
            ctx.part(
                e,
                &kit.ball,
                &kit.pupil,
                Transform::from_xyz(0.0, -er * 0.05, -er * 0.5).with_scale(Vec3::splat(er * 0.62)),
            );
            ctx.part(
                e,
                &kit.ball,
                &kit.white,
                Transform::from_xyz(er * 0.28, er * 0.3, -er * 0.95)
                    .with_scale(Vec3::splat(er * 0.22)),
            );
            if style == 2 {
                // A heavy lid over the top half.
                ctx.part(
                    e,
                    &kit.ball,
                    &ctx.skin,
                    Transform::from_xyz(0.0, er * 0.5, -er * 0.1).with_scale(Vec3::new(
                        er * 1.12,
                        er * 0.65,
                        er * 1.12,
                    )),
                );
            }
        });
    };
    let face_z = -r * 0.84;
    if look.eyes == 3 {
        eye(h, Vec3::new(0.0, r * 0.15, face_z), r * 0.34, 0);
    } else {
        for side in [-1.0f32, 1.0] {
            eye(
                h,
                Vec3::new(side * r * 0.34, r * 0.1, face_z),
                r * 0.22,
                look.eyes,
            );
        }
    }
    for side in [-1.0f32, 1.0] {
        ctx.part(
            h,
            &kit.ball,
            &kit.blush,
            Transform::from_xyz(side * r * 0.55, -r * 0.22, -r * 0.78)
                .with_scale(Vec3::splat(r * 0.15)),
        );
    }

    // Snout.
    match look.snout {
        1 => ctx.part(
            h,
            &kit.ball,
            &kit.nose,
            Transform::from_xyz(0.0, -r * 0.08, -r * 0.98).with_scale(Vec3::splat(r * 0.11)),
        ),
        2 => {
            ctx.part(
                h,
                &kit.ball,
                &ctx.skin,
                Transform::from_xyz(0.0, -r * 0.28, -r * 0.82).with_scale(Vec3::new(
                    r * 0.38,
                    r * 0.28,
                    r * 0.38,
                )),
            );
            ctx.part(
                h,
                &kit.ball,
                &kit.nose,
                Transform::from_xyz(0.0, -r * 0.18, -r * 1.18).with_scale(Vec3::new(
                    r * 0.14,
                    r * 0.1,
                    r * 0.1,
                )),
            );
        }
        3 => ctx.part(
            h,
            &kit.cone,
            &kit.straw,
            Transform::from_xyz(0.0, -r * 0.15, -r * 1.1)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                .with_scale(Vec3::new(r * 0.35, r * 0.5, r * 0.25)),
        ),
        _ => {}
    }

    // Ears and horns.
    for side in [-1.0f32, 1.0] {
        let tf = match look.ears {
            1 => Some((
                &kit.ball,
                Transform::from_xyz(side * r * 0.78, r * 0.6, 0.0).with_scale(Vec3::new(
                    r * 0.28,
                    r * 0.28,
                    r * 0.12,
                )),
            )),
            2 => Some((
                &kit.cone,
                Transform::from_xyz(side * r * 0.55, r * 0.95, 0.0)
                    .with_rotation(Quat::from_rotation_z(-side * 0.45))
                    .with_scale(Vec3::new(r * 0.4, r * 0.6, r * 0.2)),
            )),
            3 => Some((
                &kit.ball,
                Transform::from_xyz(side * r * 0.95, r * 0.05, 0.05)
                    .with_rotation(Quat::from_rotation_z(side * 0.3))
                    .with_scale(Vec3::new(r * 0.16, r * 0.5, r * 0.3)),
            )),
            4 => Some((
                &kit.capsule,
                Transform::from_xyz(side * r * 0.35, r * 1.3, 0.0)
                    .with_rotation(Quat::from_rotation_z(-side * 0.15))
                    .with_scale(Vec3::new(r * 0.3, r * 0.55, r * 0.16)),
            )),
            _ => None,
        };
        if let Some((mesh, tf)) = tf {
            ctx.part(h, mesh, &ctx.skin, tf);
        }
        let (tall, lean) = match look.horns {
            1 => (0.25, 0.2),
            2 => (0.6, 0.5),
            _ => continue,
        };
        ctx.part(
            h,
            &kit.cone,
            &kit.horn,
            Transform::from_xyz(side * r * 0.42, r * (0.85 + tall * 0.3), -r * 0.1)
                .with_rotation(Quat::from_rotation_z(-side * lean))
                .with_scale(Vec3::new(r * 0.22, r * tall, r * 0.22)),
        );
    }

    // Hats ride in a frame sized like the old 0.28 head.
    h.spawn((
        Transform::from_scale(Vec3::splat(r / 0.28)),
        Visibility::default(),
    ))
    .with_children(|a| accessory_on_head(a, ctx));
}

/// Hats, scarves on the head and flowers (old 0.28 head, centre at origin).
fn accessory_on_head(h: &mut ChildSpawnerCommands, ctx: &Ctx) {
    let kit = ctx.kit;
    let accent = &ctx.accent;
    let part = |h: &mut ChildSpawnerCommands,
                mesh: &Handle<Mesh>,
                mat: &Handle<StandardMaterial>,
                tf| { ctx.part(h, mesh, mat, tf) };
    match ctx.look.accessory {
        // Beanie with a pompom.
        0 => {
            part(
                h,
                &kit.ball,
                accent,
                Transform::from_xyz(0.0, 0.1, 0.02).with_scale(Vec3::new(0.29, 0.22, 0.29)),
            );
            part(
                h,
                &kit.ball,
                &kit.accents[0],
                Transform::from_xyz(0.0, 0.33, 0.02).with_scale(Vec3::splat(0.06)),
            );
        }
        // Flat cap with a brim out front.
        1 => {
            part(
                h,
                &kit.disc,
                accent,
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
                &kit.straw,
                Transform::from_xyz(0.0, 0.2, 0.0).with_scale(Vec3::new(0.48, 0.025, 0.48)),
            );
            part(
                h,
                &kit.disc,
                &kit.straw,
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
                accent,
                Transform::from_xyz(0.0, 0.06, 0.04).with_scale(Vec3::new(0.3, 0.27, 0.29)),
            );
            part(
                h,
                &kit.ball,
                accent,
                Transform::from_xyz(0.0, -0.05, 0.3).with_scale(Vec3::splat(0.07)),
            );
        }
        // A flower tucked by the ear.
        7 => {
            for k in 0..5 {
                let a = k as f32 * TAU / 5.0;
                part(
                    h,
                    &kit.ball,
                    &kit.petal,
                    Transform::from_xyz(0.26, 0.12 + a.sin() * 0.04, a.cos() * 0.04)
                        .with_scale(Vec3::splat(0.03)),
                );
            }
            part(
                h,
                &kit.ball,
                &kit.accents[3],
                Transform::from_xyz(0.28, 0.12, 0.0).with_scale(Vec3::splat(0.025)),
            );
        }
        _ => {}
    }
}

/// Scarf, apron or backpack (old torso frame: centre at origin, 0.46 tall).
fn accessory_on_body(p: &mut ChildSpawnerCommands, ctx: &Ctx) {
    let kit = ctx.kit;
    let accent = &ctx.accent;
    match ctx.look.accessory {
        4 => {
            ctx.part(
                p,
                &kit.ring,
                accent,
                Transform::from_xyz(0.0, 0.5, 0.0).with_scale(Vec3::new(1.0, 1.3, 0.95)),
            );
            ctx.part(
                p,
                &kit.cube,
                accent,
                Transform::from_xyz(0.12, 0.32, -0.27)
                    .with_scale(Vec3::new(0.09, 0.26, 0.04))
                    .with_rotation(Quat::from_rotation_z(0.15)),
            );
        }
        5 => ctx.part(
            p,
            &kit.cube,
            accent,
            Transform::from_xyz(0.0, -0.05, -0.29)
                .with_scale(Vec3::new(0.38, 0.5, 0.03))
                .with_rotation(Quat::from_rotation_x(0.08)),
        ),
        6 => {
            ctx.part(
                p,
                &kit.cube,
                accent,
                Transform::from_xyz(0.0, 0.12, 0.33).with_scale(Vec3::new(0.36, 0.42, 0.2)),
            );
            ctx.part(
                p,
                &kit.cube,
                &kit.accents[5],
                Transform::from_xyz(0.0, 0.02, 0.44).with_scale(Vec3::new(0.26, 0.16, 0.06)),
            );
        }
        _ => {}
    }
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
        g.phase = (g.phase + speed.min(6.0) * 2.2 * dt).rem_euclid(TAU);
    }
}

/// The procedural animator: walk cycle from speed, breathing, idle glances.
fn animate_bones(time: Res<Time>, gaits: Query<&Gait>, mut bones: Query<(&Bone, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (bone, mut tf) in &mut bones {
        let Ok(g) = gaits.get(bone.root) else {
            continue;
        };
        let walk = (g.speed / 4.0).clamp(0.0, 1.0);
        let still = 1.0 - walk;
        let t = now + bone.idle;
        let stride = g.phase + if bone.side < 0.0 { PI } else { 0.0 } + bone.offset;
        let s = stride.sin();
        let base = bone.base;
        match bone.kind {
            BoneKind::Hips => {
                tf.translation = base.translation + Vec3::Y * g.phase.sin().abs() * 0.04 * walk;
            }
            BoneKind::Spine => {
                let br = (t * 2.2).sin();
                tf.scale = Vec3::new(1.0 + 0.012 * br, 1.0 + 0.025 * br, 1.0 + 0.012 * br);
                tf.rotation = base.rotation * Quat::from_rotation_x(-0.08 * walk);
            }
            BoneKind::Head => {
                let yaw = still * ((t * 0.37).sin() * 0.45 + (t * 0.13).sin() * 0.25);
                let pitch = still * (t * 0.29).sin() * 0.1 + walk * (g.phase * 2.0).sin() * 0.05;
                tf.rotation = base.rotation * Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
            }
            BoneKind::Arm => {
                tf.rotation = base.rotation
                    * Quat::from_rotation_x(s * 0.8 * walk + still * (t * 1.3).sin() * 0.05);
            }
            BoneKind::Leg => {
                tf.rotation = base.rotation * Quat::from_rotation_x(-s * 0.6 * walk);
            }
            BoneKind::Tail => {
                tf.rotation =
                    base.rotation * Quat::from_rotation_y((t * (3.0 + g.speed)).sin() * 0.5);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_round_trip_through_text() {
        for s in 0..50 {
            let look = CharacterLook::random(s * 31 + 7);
            assert_eq!(CharacterLook::from_text(&look.to_text()), look);
        }
    }

    #[test]
    fn broken_text_is_clamped_and_tolerated() {
        let l = CharacterLook::from_text("height=9\neyes=200\ngarbage\nears=x\nhead=NaN\n");
        assert_eq!(l.height, 1.0);
        assert_eq!(l.eyes as usize, EYES.len() - 1);
        assert_eq!(l.ears, 0);
        assert_eq!(l.head, 0.5);
        assert_eq!(l.clamped(), l);
    }

    #[test]
    fn nudge_wraps_and_steps() {
        let mut l = CharacterLook::default();
        l.nudge(Field::Body, -1);
        assert_eq!(l.body as usize, BODIES.len() - 1);
        l.nudge(Field::Height, 7);
        assert_eq!(l.height, 1.0);
        assert_eq!(l.describe(Field::Height), "100%");
    }

    #[test]
    fn the_pool_covers_every_option() {
        let looks: Vec<_> = (0..400).map(CharacterLook::random).collect();
        for f in Field::ALL {
            let Some(n) = f.options() else { continue };
            for i in 0..n {
                assert!(
                    looks.iter().any(|l| {
                        let mut l = *l;
                        l.index_mut(f).is_some_and(|v| *v == i)
                    }),
                    "{f:?} option {i} never rolled"
                );
            }
        }
    }

    #[test]
    fn every_shape_fits_the_capsule() {
        for s in 0..200 {
            let l = CharacterLook::random(s);
            let lay = layout(&l, &shape(&l));
            assert!(lay.top * lay.fit <= MAX_TOP + 1e-4, "{l:?}");
            assert!(lay.half_w * lay.fit <= MAX_HALF_WIDTH + 1e-4, "{l:?}");
        }
    }
}
