//! Bakes the surf overgrown-ruins pack into modeler ops (see
//! `docs/design/surf-areas-brief.md`): its box geometry, colliders, lights,
//! placed trees and plants, speed-gap arrows, ramp looks and the course
//! markers (pads, ridge points, speed gaps, the pads' solid boxes). Stages
//! 10, 11 and 12 are here, and a stand-in landing pad for stage 13 (names
//! starting `standin_`, replaced by the foundry's own map).
//!
//!   cargo run -p friendslop-side --example surf_pack_overgrown
//!
//! Writes `crates/game/.modeler/gen/surf_overgrown_ops.json`; apply it to the
//! modeler document `surf-overgrown-map` (after loading the materials this
//! prints) and export `maps/surf_overgrown.bsn`.

use bevy::math::{Quat, Vec3};
use friendslop_base::rules::surf;
use friendslop_side::features::surf_ops::{Ops, Scope, f, read};
use friendslop_side::features::surf_overgrown::{self as og, Mat, Part};
use serde_json::json;

/// One box of geometry, turned by any rotation.
fn part(o: &mut Ops, p: &Part) {
    let name = o.name("g");
    let mut op = json!({
        "type": "primitive", "name": name, "kind": "box",
        "size": [f(p.size.x), f(p.size.y), f(p.size.z)],
        "translation": [f(p.centre.x), f(p.centre.y), f(p.centre.z)],
    });
    if p.rot.angle_between(Quat::IDENTITY) > 1e-4 {
        op["rotation"] = json!([f(p.rot.x), f(p.rot.y), f(p.rot.z), f(p.rot.w)]);
    }
    o.ops.push(op);
    match &p.mat {
        Mat::Pbr(m) => o
            .ops
            .push(json!({"type": "material", "name": name, "pbr": m})),
        Mat::Flat(c, rough) => o.ops.push(json!({
            "type": "material", "name": name, "base_color": c, "roughness": rough, "flat": true,
        })),
    }
}

fn main() {
    surf::TUNING.set_bytes(&read("data/side/surf.ron"));
    og::OVERGROWN.set_bytes(&read("data/side/surf_overgrown.ron"));
    let d = og::OVERGROWN.get();
    // The base tuning with this area's stages in (their pads, ridges, gaps).
    let t = og::with_course(&surf::tuning(), &d);
    let mut o = Ops::default();
    let l = og::layout(&t, &d);
    for p in &l.parts {
        part(&mut o, p);
    }
    for b in &l.blocks {
        o.block(b.centre, b.size, b.yaw, b.round);
    }
    for m in &l.models {
        o.model(&m.model, m.at, m.rot, m.scale);
    }
    for light in &l.lights {
        o.lamp(light.at, light.lumens, light.range, light.colour);
    }
    for (at, yaw) in &l.arrows {
        o.model(og::ARROW, *at, Quat::from_rotation_y(*yaw), Vec3::ONE);
    }
    // How each stage's ramps look.
    for k in &d.looks {
        let at = d
            .pads
            .iter()
            .find(|p| p.stage == k.stage as usize)
            .map_or(Vec3::ZERO, |p| Vec3::new(p.at.0, p.at.1, p.at.2));
        o.ops.push(json!({
            "type": "marker", "name": format!("ramp_look_{}", k.stage), "kind": "ramp_look",
            "translation": [f(at.x), f(at.y), f(at.z)],
            "components": {"RampLook": {
                "stage": k.stage, "r": k.colour.0, "g": k.colour.1, "b": k.colour.2,
                "glow": k.glow, "rough": k.rough, "metal": k.metal,
            }},
        }));
    }
    // The course: stages 10..13's pads, ridges, speed gaps and the pads' boxes
    // (the stand-in's are its own, below).
    let scope = Scope {
        stages: og::STAGES,
        finish: false,
        start_light: false,
    };
    let mut blocks = og::course_blocks(&d);
    let standin: Vec<_> = blocks.split_off(blocks.len() - 2);
    o.course(&t, &scope, &blocks);
    // The stand-in for the foundry's first pad (P13), replaced by its own map.
    let s = d.standin;
    o.ops.push(json!({
        "type": "marker", "name": "standin_pad_13", "kind": "pad",
        "translation": [f(s.at.0), f(s.at.1), f(s.at.2)],
        "size": [f(s.size.0), f(s.size.1), f(s.size.2)],
        "components": {"Pad": {"stage": s.stage, "finish": false}},
    }));
    for (n, (c, size)) in standin.iter().enumerate() {
        o.ops.push(json!({
            "type": "marker", "name": format!("standin_block_{n}"), "kind": "course_block",
            "translation": [f(c.0), f(c.1), f(c.2)],
            "size": [f(size.0), f(size.1), f(size.2)],
            "components": {"Block": {"round": false}},
        }));
    }
    o.write("surf_overgrown");
    println!(
        "{} parts, {} colliders, {} models, {} lights",
        l.parts.len(),
        l.blocks.len(),
        l.models.len(),
        l.lights.len()
    );
    println!("materials to load: {:?}", og::MATERIALS);
}
