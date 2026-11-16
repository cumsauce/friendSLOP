//! Miami and JFK set dressing (detail pass): beachfront and festival props on
//! Miami's island (palms, umbrellas, towels, coolers, neon, stage truss) and a
//! gate feel on JFK's platform (seat rows, a departure board, luggage).
//! Visual only: no colliders, no gameplay. Placeholder primitives in flat
//! colours, laid out in `data/side/miami_dressing.ron`; they load and unload
//! with New York's visuals like the rest of Miami.

use bevy::prelude::*;
use serde::Deserialize;

use crate::features::{miami, trains};

/// What a prop is (each a few primitives).
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Palm,
    Umbrella,
    Towel,
    Cooler,
    Neon,
    Truss,
    Seats,
    Board,
    Suitcase,
}

/// One prop: its kind, where (x, z) and its turn (degrees), and a hue (0..360)
/// for the things that come in colours.
#[derive(Deserialize, Debug, Clone)]
pub struct Prop {
    pub kind: Kind,
    pub at: (f32, f32),
    #[serde(default)]
    pub yaw: f32,
    #[serde(default)]
    pub hue: f32,
}

/// Layout: `assets/data/side/miami_dressing.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Layout {
    /// Miami's props, from the island ground's middle (x, z).
    pub miami: Vec<Prop>,
    /// JFK's props, in the gate's frame (x along the track, +z the platform side).
    pub jfk: Vec<Prop>,
    /// How bright the neon glows (emissive multiplier).
    pub neon_glow: f32,
}

pub static LAYOUT: friendslop_data::Data<Layout> =
    friendslop_data::Data::ron("data/side/miami_dressing.ron");

pub fn plugin(app: &mut App) {
    crate::area::on_show(app, build);
}

/// A part of a prop: its shape, colour (hue/sat/light, glow) and offset.
struct Part {
    shape: Shape,
    hsl: (f32, f32, f32),
    glow: bool,
    at: Vec3,
    turn: Quat,
}

enum Shape {
    Box(Vec3),
    Cyl(f32, f32),
    Ball(f32),
}

fn part(shape: Shape, hsl: (f32, f32, f32), at: Vec3) -> Part {
    Part {
        shape,
        hsl,
        glow: false,
        at,
        turn: Quat::IDENTITY,
    }
}

/// The parts of a prop, in its own frame (y up from the ground).
fn parts(kind: Kind, hue: f32) -> Vec<Part> {
    use Shape::*;
    match kind {
        Kind::Palm => {
            let mut p = vec![part(Cyl(0.18, 6.0), (30.0, 0.35, 0.35), Vec3::Y * 3.0)];
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                let dir = Vec3::new(a.cos(), 0.0, a.sin());
                p.push(Part {
                    turn: Quat::from_rotation_y(-a) * Quat::from_rotation_z(-0.35),
                    ..part(
                        Box(Vec3::new(2.4, 0.06, 0.6)),
                        (110.0, 0.55, 0.3),
                        Vec3::Y * 5.8 + dir * 1.1,
                    )
                });
            }
            p.push(part(Ball(0.35), (100.0, 0.4, 0.22), Vec3::Y * 5.9));
            p
        }
        Kind::Umbrella => vec![
            part(Cyl(0.04, 2.3), (0.0, 0.0, 0.9), Vec3::Y * 1.15),
            part(Cyl(1.3, 0.12), (hue, 0.8, 0.55), Vec3::Y * 2.3),
            part(Cyl(0.9, 0.1), (hue, 0.2, 0.92), Vec3::Y * 2.4),
        ],
        Kind::Towel => vec![
            part(
                Box(Vec3::new(0.9, 0.02, 1.8)),
                (hue, 0.75, 0.55),
                Vec3::Y * 0.01,
            ),
            part(
                Box(Vec3::new(0.9, 0.025, 0.3)),
                (hue, 0.2, 0.92),
                Vec3::new(0.0, 0.012, 0.5),
            ),
        ],
        Kind::Cooler => vec![
            part(
                Box(Vec3::new(0.7, 0.42, 0.42)),
                (hue, 0.7, 0.5),
                Vec3::Y * 0.21,
            ),
            part(
                Box(Vec3::new(0.74, 0.08, 0.46)),
                (0.0, 0.0, 0.95),
                Vec3::Y * 0.46,
            ),
        ],
        Kind::Neon => vec![
            part(
                Box(Vec3::new(0.08, 2.6, 0.08)),
                (0.0, 0.0, 0.2),
                Vec3::Y * 1.3,
            ),
            Part {
                glow: true,
                ..part(
                    Box(Vec3::new(2.4, 0.12, 0.12)),
                    (hue, 1.0, 0.55),
                    Vec3::Y * 2.7,
                )
            },
            Part {
                glow: true,
                ..part(
                    Box(Vec3::new(1.6, 0.1, 0.1)),
                    (hue + 60.0, 1.0, 0.55),
                    Vec3::Y * 2.35,
                )
            },
        ],
        Kind::Truss => {
            let grey = (0.0, 0.0, 0.6);
            let mut p = Vec::new();
            for x in [-6.0, 6.0] {
                p.push(part(
                    Box(Vec3::new(0.3, 7.0, 0.3)),
                    grey,
                    Vec3::new(x, 3.5, 0.0),
                ));
            }
            p.push(part(Box(Vec3::new(12.3, 0.3, 0.3)), grey, Vec3::Y * 7.0));
            for i in 0..5 {
                p.push(Part {
                    glow: true,
                    ..part(
                        Cyl(0.15, 0.3),
                        (hue + i as f32 * 50.0, 1.0, 0.6),
                        Vec3::new(-4.8 + i as f32 * 2.4, 6.7, 0.0),
                    )
                });
            }
            p
        }
        Kind::Seats => {
            let (frame, seat) = ((0.0, 0.0, 0.25), (hue, 0.35, 0.35));
            let mut p = vec![part(
                Box(Vec3::new(3.0, 0.06, 0.1)),
                frame,
                Vec3::new(0.0, 0.25, 0.0),
            )];
            for i in 0..5 {
                let x = -1.2 + i as f32 * 0.6;
                p.push(part(
                    Box(Vec3::new(0.5, 0.06, 0.48)),
                    seat,
                    Vec3::new(x, 0.45, 0.0),
                ));
                p.push(part(
                    Box(Vec3::new(0.5, 0.5, 0.06)),
                    seat,
                    Vec3::new(x, 0.72, 0.24),
                ));
            }
            for x in [-1.4, 1.4] {
                p.push(part(
                    Box(Vec3::new(0.06, 0.45, 0.4)),
                    frame,
                    Vec3::new(x, 0.22, 0.0),
                ));
            }
            p
        }
        Kind::Board => {
            let mut p = vec![
                part(
                    Box(Vec3::new(0.12, 3.0, 0.12)),
                    (0.0, 0.0, 0.3),
                    Vec3::Y * 1.5,
                ),
                part(
                    Box(Vec3::new(2.6, 1.3, 0.12)),
                    (0.0, 0.0, 0.08),
                    Vec3::Y * 3.4,
                ),
            ];
            for r in 0..5 {
                p.push(Part {
                    glow: true,
                    ..part(
                        Box(Vec3::new(2.2, 0.1, 0.02)),
                        (if r == 0 { 45.0 } else { 120.0 }, 1.0, 0.5),
                        Vec3::new(0.0, 3.9 - r as f32 * 0.22, -0.07),
                    )
                });
            }
            p
        }
        Kind::Suitcase => vec![
            part(
                Box(Vec3::new(0.42, 0.6, 0.24)),
                (hue, 0.55, 0.4),
                Vec3::Y * 0.32,
            ),
            part(
                Box(Vec3::new(0.2, 0.3, 0.03)),
                (0.0, 0.0, 0.15),
                Vec3::new(0.0, 0.77, 0.0),
            ),
        ],
    }
}

fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let l = LAYOUT.get();
    let mut spawn = |frame: Transform, p: &Prop| {
        let at = frame * Vec3::new(p.at.0, 0.0, p.at.1);
        let turn = frame.rotation * Quat::from_rotation_y(p.yaw.to_radians());
        let root = commands
            .spawn((
                Transform::from_translation(at).with_rotation(turn),
                Visibility::default(),
                crate::area::show(),
            ))
            .id();
        for pt in parts(p.kind, p.hue) {
            let mesh = match pt.shape {
                Shape::Box(s) => meshes.add(Cuboid::new(s.x, s.y, s.z)),
                Shape::Cyl(r, h) => meshes.add(Cylinder::new(r, h)),
                Shape::Ball(r) => meshes.add(Sphere::new(r)),
            };
            let (h, s, li) = pt.hsl;
            let c = Color::hsl(h.rem_euclid(360.0), s, li);
            let mat = materials.add(if pt.glow {
                StandardMaterial {
                    base_color: c,
                    emissive: c.to_linear() * l.neon_glow,
                    unlit: true,
                    ..default()
                }
            } else {
                StandardMaterial {
                    base_color: c,
                    perceptual_roughness: 0.8,
                    ..default()
                }
            });
            commands.spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(pt.at).with_rotation(pt.turn),
                ChildOf(root),
            ));
        }
    };
    let island = Transform::from_translation(miami::ground());
    for p in &l.miami {
        spawn(island, p);
    }
    let t = trains::tuning();
    let gate = t
        .lines
        .iter()
        .flat_map(|line| line.stops.iter())
        .find(|s| s.name == "JFK");
    if let Some(s) = gate {
        let mut f = trains::stop_frame(s);
        f.translation.y += trains::PLATFORM_TOP;
        for p in &l.jfk {
            spawn(f, p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            LAYOUT.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(LAYOUT.set_bytes(&bytes), "{path} doesn't parse");
        let l = LAYOUT.get();
        assert!(!l.miami.is_empty() && !l.jfk.is_empty() && l.neon_glow > 0.0);
    }
}
