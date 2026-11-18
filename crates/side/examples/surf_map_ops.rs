//! Bakes the surf map's generated sections into modeler ops (the same idea
//! as `scripts/fan_museum_map.py`): placed models, `Block` colliders and
//! `Lamp`s for the pillar run, the fin / orb / gate runs, the office tower and
//! the mall, plus the ramp meshes of the stages the hall model doesn't have.
//!
//!   cargo run -p friendslop-side --example surf_map_ops
//!
//! Writes `crates/game/.modeler/gen/surf_map_ops.json`. Apply it to the
//! modeler document `surf-map` (`apply_ops {ops_file: ".modeler/gen/surf_map_ops.json"}`)
//! and export `maps/surf.bsn`. After that the map is the truth: move things in
//! the modeler; re-run this only to regenerate the layout from scratch.

use bevy::math::{Quat, Vec3};
use friendslop_base::rules::surf;
use friendslop_side::features::surf as surf_feature;
use friendslop_side::features::surf_ops::{Ops, Scope, read};
use friendslop_side::features::{
    surf_pillars, surf_props, surf_runs, surf_slalom, surf_tower, surf_train,
};
use serde_json::json;

fn main() {
    surf::TUNING.set_bytes(&read("data/side/surf.ron"));
    // The base map: stages 0-4 (the office and the mall are their own packs).
    let full = surf::tuning();
    let mut t = (*full).clone();
    t.stages.truncate(5);
    t.finish = Default::default();
    let t = std::sync::Arc::new(t);
    let mut o = Ops::default();

    // The pillar run.
    surf_pillars::PILLARS.set_bytes(&read("data/side/surf_pillars.ron"));
    let d = surf_pillars::PILLARS.get();
    for p in surf_pillars::layout(&t, &d) {
        o.model(
            &format!("features/{}", d.model),
            p.centre,
            Quat::IDENTITY,
            Vec3::new(
                d.radius / surf_pillars::MODEL_RADIUS,
                p.height / surf_pillars::MODEL_HEIGHT,
                d.radius / surf_pillars::MODEL_RADIUS,
            ),
        );
        o.block(
            p.centre,
            Vec3::new(d.radius * 2.0, p.height, d.radius * 2.0),
            0.0,
            true,
        );
    }

    // The fin, orb and gate runs.
    surf_runs::RUNS.set_bytes(&read("data/side/surf_runs.ron"));
    let d = surf_runs::RUNS.get();
    let l = surf_runs::layout(&t, &d);
    for p in &l.pieces {
        o.model(
            &format!("features/{}", p.model),
            p.at,
            Quat::from_rotation_y(p.yaw),
            Vec3::new(1.0, 1.0, 1.0),
        );
    }
    for s in &l.solids {
        // A sphere becomes an upright cylinder (the map has no sphere block).
        if s.sphere {
            o.block(
                s.centre,
                Vec3::new(s.half.x * 2.0, s.half.x * 2.0, s.half.x * 2.0),
                0.0,
                true,
            );
        } else {
            o.block(s.centre, s.half * 2.0, s.yaw, false);
        }
    }

    // The older sections.
    let sd = {
        surf_slalom::SLALOM.set_bytes(&read("data/side/surf_slalom.ron"));
        surf_slalom::SLALOM.get()
    };
    let sl = surf_slalom::layout(&t, &sd);
    surf_props::claim(sl.claim.0, sl.claim.1);
    for p in &sl.pieces {
        o.model(
            &format!("features/{}", p.model),
            p.at,
            Quat::from_rotation_y(p.yaw),
            Vec3::new(p.stretch, 1.0, 1.0),
        );
    }
    for b in &sl.solids {
        o.block(b.centre, b.size, 0.0, false);
    }

    surf_tower::TOWER.set_bytes(&read("data/side/surf_tower.ron"));
    let td = surf_tower::TOWER.get();
    let (c, h) = surf_tower::bounds(&td);
    surf_props::claim(c, h);
    let at = surf_tower::origin(&td);
    for name in ["surf_tower_shell.gltf", "surf_tower_interior.gltf"] {
        o.model(&format!("features/{name}"), at, Quat::IDENTITY, Vec3::ONE);
    }
    let (parts, lights) = surf_tower::layout(&t, &td);
    for p in parts.iter().filter(|p| p.kind.solid()) {
        o.block(p.centre, p.size, 0.0, false);
    }
    for l in lights {
        o.lamp(l, td.light_intensity, td.light_range, td.light_colour);
    }

    surf_train::TRAIN.set_bytes(&read("data/side/surf_train.ron"));
    let tr = surf_train::TRAIN.get();
    let line = surf_train::rail(&t, &tr);
    for (c, h) in surf_train::claims(&line, &tr) {
        surf_props::claim(c, h);
    }
    for (top, rot, len) in surf_train::beams(&line) {
        let body = json!({"Body": {
            "sx": tr.beam.0, "sy": tr.beam.1, "sz": len,
            "ox": 0.0, "oy": -tr.beam.1 / 2.0, "oz": 0.0,
        }});
        o.model_with(
            "features/surf_train_beam.gltf",
            top,
            rot,
            Vec3::new(1.0, 1.0, len / tr.segment.max(0.1)),
            body,
        );
    }
    for (top, height) in surf_train::pylons(&line, &tr) {
        let foot = top - Vec3::Y * height;
        o.model(
            "features/surf_train_pylon.gltf",
            foot,
            Quat::IDENTITY,
            Vec3::new(1.0, height / 10.0, 1.0),
        );
        o.model(
            "features/surf_train_head.gltf",
            top,
            Quat::IDENTITY,
            Vec3::ONE,
        );
        o.block(
            (top + foot) / 2.0,
            Vec3::new(tr.pylon_width, height, tr.pylon_width),
            0.0,
            false,
        );
    }

    // Everything that keeps the generated props out, then the props.
    surf_props::PROPS.set_bytes(&read("data/side/surf_props.ron"));
    let pd = surf_props::PROPS.get();
    {
        let d = surf_pillars::PILLARS.get();
        if let Some((c, h)) = surf_pillars::claim_box(&surf_pillars::layout(&t, &d), &d) {
            surf_props::claim(c, h);
        }
        let d = surf_runs::RUNS.get();
        for r in &d.runs {
            if let Some((c, h)) =
                surf_runs::claim_box(&surf_runs::run_layout(&t, r), d.claim_margin)
            {
                surf_props::claim(c, h);
            }
        }
    }
    for (i, piece) in surf_props::layout(&t, &pd).into_iter().enumerate() {
        let rot = Quat::from_rotation_y(piece.yaw);
        if !piece.model.is_empty() {
            let extra = match piece.motion {
                surf_props::Motion::Bob => json!({"Bob": {
                    "height": pd.bob.0, "hz": pd.bob.1, "phase": i as f32 * 1.7,
                }}),
                surf_props::Motion::Spin => {
                    json!({"Spin": {"rpm": pd.spin / std::f32::consts::TAU * 60.0}})
                }
                surf_props::Motion::Still => json!({}),
            };
            o.model_with(
                &format!("features/{}", piece.model),
                piece.at,
                rot,
                Vec3::new(piece.stretch, 1.0, 1.0),
                extra,
            );
        }
        if piece.motion == surf_props::Motion::Still {
            for (c, size) in &piece.solids {
                o.block(piece.at + rot * *c, *size, piece.yaw, false);
            }
        }
    }

    // The hall model (the earlier stages' ramps, pads and signs), the city and
    // the neon dressing.
    o.model(
        "features/surf-hall.gltf",
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::ONE,
    );
    o.city(&t);
    surf_feature::DRESSING.set_bytes(&read("data/side/surf_dressing.ron"));
    let scope = Scope {
        stages: 0..5,
        finish: false,
        start_light: true,
    };
    o.dressing(&t, &surf_feature::DRESSING.get(), &scope);
    let blocks: Vec<_> = full
        .blocks
        .iter()
        .copied()
        .filter(|b| b.0.0 < 1000.0)
        .collect();
    o.course(&t, &scope, &blocks);

    o.write("surf_map");
}
