//! Bakes the surf foundry pack into modeler ops (see
//! `docs/design/surf-areas-brief.md`): its geometry, colliders, lights,
//! placed models, ramp looks and course markers.
//!
//!   cargo run -p friendslop-side --example surf_pack_foundry
//!
//! Writes `crates/game/.modeler/gen/surf_foundry_ops.json`; apply it to the
//! modeler document `surf-foundry-map` and export `maps/surf_foundry.bsn`.

use bevy::math::{Quat, Vec3};
use friendslop_base::rules::surf;
use friendslop_side::features::surf as surf_feature;
use friendslop_side::features::surf_foundry::{self as fd, Motion, Shape};
use friendslop_side::features::surf_ops::{Ops, Scope, f, read};
use serde_json::json;

fn main() {
    surf::TUNING.set_bytes(&read("data/side/surf.ron"));
    fd::FOUNDRY.set_bytes(&read("data/side/surf_foundry.ron"));
    surf_feature::DRESSING.set_bytes(&read("data/side/surf_dressing.ron"));
    let d = fd::FOUNDRY.get();
    // The course alone: the stages before the foundry are left empty.
    let t = fd::course(&surf::TUNING.get(), &d);
    let scene = fd::scene(&t, &d);
    let mut o = Ops::default();

    for item in &scene.items {
        let name = o.name("g");
        let at = [f(item.at.x), f(item.at.y), f(item.at.z)];
        let rot = [item.rot.x, item.rot.y, item.rot.z, item.rot.w].map(f);
        match &item.shape {
            Shape::Box(s) => o.ops.push(json!({
                "type": "primitive", "name": name, "kind": "box",
                "size": [f(s.x), f(s.y), f(s.z)], "translation": at, "rotation": rot,
            })),
            Shape::Cyl(r, h) => o.ops.push(json!({
                "type": "primitive", "name": name, "kind": "cylinder",
                "size": {"radius": f(*r), "height": f(*h)}, "translation": at, "rotation": rot,
            })),
            Shape::Cone(r, h) => o.ops.push(json!({
                "type": "primitive", "name": name, "kind": "cone",
                "size": {"radius": f(*r), "height": f(*h)}, "translation": at, "rotation": rot,
            })),
            Shape::Revolve(profile) => {
                let p: Vec<[f64; 2]> = profile.iter().map(|&(r, y)| [f(r), f(y)]).collect();
                o.ops.push(json!({
                    "type": "revolve", "name": name, "profile": p, "segments": 40,
                    "translation": at, "rotation": rot,
                }));
            }
            Shape::Text(text, size) => o.ops.push(json!({
                "type": "text", "name": name, "text": text, "size": f(*size),
                "depth": f(*size / 5.0), "font": "bold",
                "translation": at, "rotation": rot,
            })),
        }
        o.ops
            .push(json!({"type": "material", "name": name, "pbr": item.mat}));
    }
    for b in &scene.blocks {
        o.block(b.centre, b.size, 0.0, b.round);
    }
    for l in &scene.lamps {
        o.lamp(l.at, l.lumens, l.range, l.rgb);
    }
    for p in &scene.placed {
        let extra = match p.motion {
            Motion::Still => json!({}),
            Motion::Bob { height, hz, phase } => {
                json!({"Bob": {"height": height, "hz": hz, "phase": phase}})
            }
            Motion::Spin { rpm } => json!({"Spin": {"rpm": rpm}}),
        };
        o.model_with(p.model, p.at, Quat::from_rotation_y(p.yaw), p.scale, extra);
    }
    // How each stage's ramps look.
    for look in &d.looks {
        o.ops.push(json!({
            "type": "marker", "name": format!("ramp_look_{}", look.stage), "kind": "ramp_look",
            "translation": [0.0, 0.0, 0.0],
            "components": {"RampLook": {
                "stage": look.stage,
                "r": look.colour.0, "g": look.colour.1, "b": look.colour.2,
                "glow": look.glow, "rough": look.rough, "metal": look.metal,
            }},
        }));
    }
    // The course: the stages' pads and ridges, the speed checks, the finish.
    let first = fd::first_stage(&d);
    let scope = Scope {
        stages: first..first + d.pads.len(),
        finish: true,
        start_light: false,
    };
    o.course(&t, &scope, &t.blocks);
    let _ = Vec3::ZERO;
    o.write("surf_foundry");
}
