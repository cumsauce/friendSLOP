//! Bakes the surf neon megacity into modeler ops (see
//! `docs/design/surf-areas-brief.md`): the course's pads, ridge points, speed
//! gaps and ramp look, the towers, the megatower, set pieces, lights and neon
//! dressing, and a stand-in landing pad for the overgrown ruins' first pad.
//!
//!   cargo run -p friendslop-side --example surf_pack_neon
//!
//! Writes `crates/game/.modeler/gen/surf_neon_ops.json`; apply it to the
//! modeler document `surf-neon-map` and export `maps/surf_neon.bsn`.

use bevy::math::{Quat, Vec3};
use friendslop_base::rules::surf;
use friendslop_side::features::surf as surf_feature;
use friendslop_side::features::surf_neon::{self, Item, Motion};
use friendslop_side::features::surf_ops::{Ops, f, read};
use serde_json::json;

fn marker(
    name: &str,
    kind: &str,
    c: Vec3,
    size: Vec3,
    comps: serde_json::Value,
) -> serde_json::Value {
    json!({
        "type": "marker", "name": name, "kind": kind,
        "translation": [f(c.x), f(c.y), f(c.z)],
        "size": [f(size.x), f(size.y), f(size.z)],
        "components": comps,
    })
}

fn main() {
    surf::TUNING.set_bytes(&read("data/side/surf.ron"));
    surf_neon::NEON.set_bytes(&read("data/side/surf_neon.ron"));
    surf_feature::DRESSING.set_bytes(&read("data/side/surf_dressing.ron"));
    let base = surf::TUNING.get();
    let d = surf_neon::NEON.get();
    let c = surf_neon::course(&d);
    let t = surf_neon::tuning_with(&base, &c);
    let layout = surf_neon::layout(&t, &d, &c);
    let dress = surf_feature::DRESSING.get();
    let mut o = Ops::default();

    // The city and the set pieces.
    for item in &layout.items {
        match item {
            Item::Part { at, yaw, size, mat } => o.part(*at, *yaw, *size, mat),
            Item::Block {
                at,
                yaw,
                size,
                round,
            } => o.block(*at, *size, *yaw, *round),
            Item::Lamp { at, rgb } => o.lamp(*at, d.lamps.lumens, d.lamps.range, *rgb),
            Item::Model {
                model,
                at,
                yaw,
                scale,
                motion,
                phase,
            } => {
                let extra = match motion {
                    Motion::Still => json!({}),
                    Motion::Bob => json!({"Bob": {"height": 1.5, "hz": 0.25, "phase": phase}}),
                    Motion::Spin => json!({"Spin": {"rpm": d.tower.crown_rpm}}),
                };
                o.model_with(
                    model,
                    *at,
                    Quat::from_rotation_y(*yaw),
                    Vec3::splat(*scale),
                    extra,
                );
            }
        }
    }

    // The speed-check arrows over each gap.
    for (at, yaw) in surf_neon::arrows(&t, &d, &c) {
        o.model_with(
            &d.lamps.arrow,
            at,
            Quat::from_rotation_y(yaw),
            Vec3::splat(d.lamps.arrow_scale),
            json!({"Bob": {"height": 0.8, "hz": 0.3, "phase": 0.0}}),
        );
    }

    // The pads, their pillars and dressing; the course's markers.
    for spec in &d.stages {
        let Some(st) = c.stage(spec.stage) else {
            continue;
        };
        let (centre, size) = st.pad;
        let i = spec.stage;
        o.ops.push(marker(
            &format!("pad_{i}"),
            "pad",
            centre,
            size,
            json!({"Pad": {"stage": i, "finish": false}}),
        ));
        // Stage 7's pad and pillar already stand (the mall's end pad).
        if i != 7 {
            o.part(centre, 0.0, size, "sf-hull");
            let (w, bottom) = d.pillar;
            let top = centre.y - size.y / 2.0;
            let h = top - bottom;
            let p = Vec3::new(centre.x, bottom + h / 2.0, centre.z);
            o.part(p, 0.0, Vec3::new(w, h, w), "sf-hull");
            o.ops.push(marker(
                &format!("course_block_{i}"),
                "course_block",
                p,
                Vec3::new(w, h, w),
                json!({"Block": {"round": false}}),
            ));
            for (sc, ss) in surf_feature::edge_strips((centre, size), dress.strip) {
                o.part(sc, 0.0, ss, &spec.neon);
            }
            let top = centre.y + size.y / 2.0;
            let corner = Vec3::new(
                centre.x - size.x / 2.0 + 0.6,
                top,
                centre.z + size.z / 2.0 - 0.6,
            );
            let hh = dress.marker_height;
            o.flat(
                corner + Vec3::Y * hh / 2.0,
                0.0,
                Vec3::new(0.2, hh, 0.2),
                [0.12, 0.12, 0.18],
                0.7,
            );
            for n in 0..=i {
                o.part(
                    corner + Vec3::Y * (hh + 0.35 + n as f32 * 0.55),
                    0.0,
                    Vec3::splat(0.4),
                    &spec.neon,
                );
            }
            o.lamp(
                centre + Vec3::Y * 4.0,
                d.lamps.pad_lumens,
                d.lamps.pad_range,
                spec.colour,
            );
        }
        for (r, b) in st.ramps.iter().enumerate() {
            for (k, p) in b.points.iter().enumerate() {
                o.ops.push(json!({
                    "type": "marker", "name": format!("ridge_s{i}_r{r}_p{k}"), "kind": "ridge",
                    "translation": [f(p.x), f(p.y), f(p.z)],
                    "components": {"Ridge": {"stage": i, "ramp": r, "order": k}},
                }));
            }
        }
        for &g in &spec.gaps {
            o.ops.push(json!({
                "type": "marker", "name": format!("gap_s{i}_r{g}"), "kind": "gap",
                "translation": [0.0, 0.0, 0.0],
                "components": {"SpeedGap": {"stage": i, "ramp": g}},
            }));
        }
        let (lr, lg, lb, glow, rough, metal) = spec.look;
        o.ops.push(json!({
            "type": "marker", "name": format!("look_s{i}"), "kind": "ramp_look",
            "translation": [0.0, 0.0, 0.0],
            "components": {"RampLook": {
                "stage": i, "r": lr, "g": lg, "b": lb,
                "glow": glow, "rough": rough, "metal": metal,
            }},
        }));
    }

    // A stand-in for the overgrown ruins' first pad (its map overrides it).
    let (p10, s10) = (Vec3::from(d.p10), Vec3::from(d.p10_size));
    o.ops.push(marker(
        "standin_pad_10",
        "pad",
        p10,
        s10,
        json!({"Pad": {"stage": 10, "finish": false}}),
    ));
    let (w, bottom) = d.pillar;
    let top = p10.y - s10.y / 2.0;
    let h = top - bottom;
    let p = Vec3::new(p10.x, bottom + h / 2.0, p10.z);
    o.ops.push(marker(
        "standin_course_block_10",
        "course_block",
        p,
        Vec3::new(w, h, w),
        json!({"Block": {"round": false}}),
    ));
    o.part(p10, 0.0, s10, "sf-hull");
    o.part(p, 0.0, Vec3::new(w, h, w), "sf-hull");
    for (sc, ss) in surf_feature::edge_strips((p10, s10), dress.strip) {
        o.part(sc, 0.0, ss, "sf-neon-lime");
    }

    o.write("surf_neon");
    println!(
        "{} items, {} blocks, {} lamps",
        layout.items.len(),
        layout.blocks().count(),
        layout.lamps()
    );
    for s in &c.stages {
        for (r, b) in s.ramps.iter().enumerate() {
            println!(
                "stage {} ramp {r}: {} points, {:.0} m, {:.1} -> {:.1}",
                s.stage,
                b.points.len(),
                b.length(),
                b.points[0].y,
                b.end().0.y
            );
        }
        println!("stage {} pad {:?}", s.stage, s.pad.0);
    }
}
