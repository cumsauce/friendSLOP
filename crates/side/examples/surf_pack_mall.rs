//! Bakes the surf mall pack into modeler ops (see
//! `docs/design/surf-areas-brief.md`): its geometry, colliders, lights, models,
//! ramp look, neon dressing and course markers.
//!
//!   cargo run -p friendslop-side --example surf_pack_mall
//!
//! Writes `crates/game/.modeler/gen/surf_mall_ops.json`; apply it to the
//! modeler document `surf-mall-map` and export `maps/surf_mall.bsn`.

use friendslop_base::rules::surf;
use friendslop_side::features::surf as surf_feature;
use friendslop_side::features::surf_mall;
use friendslop_side::features::surf_ops::{Ops, Scope, f, read};
use serde_json::json;

fn main() {
    // The tuning is just surf.ron here: this pack builds its own stage.
    surf::TUNING.set_bytes(&read("data/side/surf.ron"));
    surf_mall::MALL.set_bytes(&read("data/side/surf_mall.ron"));
    let d = surf_mall::MALL.get();
    let mut t = (*surf::TUNING.get()).clone();
    t.stages = vec![surf::Stage::default(); d.stage];
    t.stages.push(surf_mall::stage(&d));
    t.finish = d.pads.end;
    t.speed_gaps = surf_mall::speed_gaps(&d);

    let mut o = Ops::default();
    let l = surf_mall::layout(&t, &d);
    for m in &l.models {
        o.model(&m.model, m.at, m.rot, m.scale);
    }
    for p in &l.parts {
        o.part(p.centre, p.yaw, p.size, &p.material);
    }
    for c in &l.cyls {
        let name = o.name("c");
        o.ops.push(json!({
            "type": "primitive", "name": name, "kind": "cylinder",
            "size": {"radius": f(c.radius), "height": f(c.height)},
            "translation": [f(c.centre.x), f(c.centre.y), f(c.centre.z)],
        }));
        o.ops
            .push(json!({"type": "material", "name": name, "pbr": c.material}));
    }
    for s in l.solids.iter().chain(&l.catwalks) {
        o.block(s.centre, s.half * 2.0, s.yaw, false);
    }
    for lamp in &l.lamps {
        o.lamp(lamp.at, lamp.lumens, lamp.range, lamp.colour);
    }
    // The ramps' look (they are drawn from their ridge points at run time).
    let k = d.look;
    o.ops.push(json!({
        "type": "marker", "name": "ramp_look", "kind": "ramp_look",
        "translation": [0.0, 0.0, 0.0],
        "components": {"RampLook": {
            "stage": d.stage, "r": k.0, "g": k.1, "b": k.2,
            "glow": k.3, "rough": k.4, "metal": k.5,
        }},
    }));
    // The neon dressing, the course markers, the stand-in finish at P7.
    let scope = Scope {
        stages: d.stage..d.stage + 1,
        finish: true,
        start_light: false,
    };
    surf_feature::DRESSING.set_bytes(&read("data/side/surf_dressing.ron"));
    o.dressing(&t, &surf_feature::DRESSING.get(), &scope);
    o.course(&t, &scope, &surf_mall::blocks(&d));
    o.write("surf_mall");
}
