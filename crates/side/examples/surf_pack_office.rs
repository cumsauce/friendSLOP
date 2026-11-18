//! Bakes the surf office pack into modeler ops (see
//! `docs/design/surf-areas-brief.md`): its models, colliders, lights, box
//! geometry, neon dressing and course markers (the ridge points come from
//! `data/side/surf_office.ron`).
//!
//!   cargo run -p friendslop-side --example surf_pack_office
//!
//! Writes `crates/game/.modeler/gen/surf_office_ops.json`; apply it to the
//! modeler document `surf-office-map` and export `maps/surf_office.bsn`.

use friendslop_base::rules::surf;
use friendslop_side::features::surf as surf_feature;
use friendslop_side::features::surf_office;
use friendslop_side::features::surf_ops::{Ops, Scope, f, read};
use serde_json::json;

fn main() {
    surf::TUNING.set_bytes(&read("data/side/surf.ron"));
    surf_office::OFFICE.set_bytes(&read("data/side/surf_office.ron"));
    let d = surf_office::OFFICE.get();
    // The base tuning with this office's stage in (its pad, ridges and gap).
    let t = surf_office::with_course(&surf::tuning(), &d);
    let mut o = Ops::default();
    let s = surf_office::scene(&t, &d);
    o.layout(&s.layout, d.light_intensity, d.light_range, d.light_colour);
    for p in &s.parts {
        o.part(p.centre, p.yaw, p.size, &p.pbr);
    }
    for r in &s.rounds {
        o.block(r.centre, r.half * 2.0, 0.0, true);
    }
    let scope = Scope {
        stages: d.stage..d.stage + 1,
        finish: false,
        start_light: false,
    };
    surf_feature::DRESSING.set_bytes(&read("data/side/surf_dressing.ron"));
    o.dressing(&t, &surf_feature::DRESSING.get(), &scope);
    // How the stage's ramps look.
    let (r, g, b, glow, rough, metal) = d.ramp_look;
    o.ops.push(json!({
        "type": "marker", "name": "ramp_look_office", "kind": "ramp_look",
        "translation": [f(d.pad.0.0), f(d.pad.0.1), f(d.pad.0.2)],
        "components": {"RampLook": {
            "stage": d.stage, "r": r, "g": g, "b": b,
            "glow": glow, "rough": rough, "metal": metal,
        }},
    }));
    o.course(&t, &scope, &surf_office::course_blocks(&d));
    o.write("surf_office");
}
