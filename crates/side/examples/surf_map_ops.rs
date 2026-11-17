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

use std::path::PathBuf;

use bevy::math::{Quat, Vec3};
use friendslop_base::rules::surf::{self, Block, Piece, Tuning, joints};
use friendslop_side::features::surf as surf_feature;
use friendslop_side::features::{
    surf_mall, surf_office, surf_pillars, surf_props, surf_runs, surf_slalom, surf_tower,
    surf_train,
};
use serde_json::{Value, json};

fn assets(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../game/assets")
        .join(path)
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(assets(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The modeler prefab a model file was exported from: a few by name, the
/// surf set pieces by their file name (`surf_water_tower.gltf` is
/// `sf-water-tower`, `surf_train_beam.gltf` is `surf-train-beam`), the
/// meadow's by theirs.
fn prefab_of(model: &str) -> Option<String> {
    let stem = model
        .trim_start_matches("features/")
        .trim_start_matches("nature/")
        .trim_end_matches(".gltf");
    let table = [
        ("surf_catwalk_segment", "sf-cable-catwalk-segment"),
        ("surf_slalom_low_bridge_segment", "sf-slalom-low-bridge"),
        ("surf_slalom_tower_section", "sf-slalom-tower"),
        ("surf_truss_bridge_segment", "sf-truss-bridge-segment"),
        ("tree_1", "tr-tree-1"),
        ("tree_2", "tr-tree-2"),
        ("tree_3", "tr-tree-3"),
        ("bush_1", "bush-1"),
        ("bush_2", "bush-2"),
        ("flower_1", "flower-1"),
        ("flower_2", "flower-2"),
        ("flower_3", "flower-3"),
    ];
    if let Some((_, p)) = table.iter().find(|(m, _)| *m == stem) {
        return Some((*p).to_string());
    }
    let dashed = stem.replace('_', "-");
    let dir = assets("../.modeler/prefabs");
    for cand in [dashed.clone(), dashed.replacen("surf-", "sf-", 1)] {
        if dir.join(format!("{cand}.json")).exists() {
            return Some(cand);
        }
    }
    None
}

#[derive(Default)]
struct Ops {
    ops: Vec<Value>,
    n: usize,
}

fn f(v: f32) -> f64 {
    (f64::from(v) * 1000.0).round() / 1000.0
}

impl Ops {
    fn name(&mut self, stem: &str) -> String {
        self.n += 1;
        format!("{stem}{}", self.n)
    }

    /// A placed model. Unscaled, yaw-only ones are a `GltfSource` marker on the
    /// existing model file; the rest are prefab instances.
    fn model(&mut self, model: &str, at: Vec3, rot: Quat, scale: Vec3) {
        self.model_with(model, at, rot, scale, json!({}));
    }

    /// [`Ops::model`] with extra components (a `Bob`, a `Body`).
    fn model_with(&mut self, model: &str, at: Vec3, rot: Quat, scale: Vec3, extra: Value) {
        let (yaw, pitch, roll) = rot.to_euler(bevy::math::EulerRot::YXZ);
        let plain = (scale - Vec3::ONE).abs().max_element() < 1e-4
            && pitch.abs() < 1e-4
            && roll.abs() < 1e-4;
        let name = self.name("m");
        if plain && !model.starts_with("nature/") {
            let mut comps =
                json!({"GltfSource": {"path": format!("models/{model}"), "scene_index": 0}});
            if let (Some(c), Some(e)) = (comps.as_object_mut(), extra.as_object()) {
                c.extend(e.clone());
            }
            self.ops.push(json!({
                "type": "marker", "name": name, "kind": "model",
                "translation": [f(at.x), f(at.y), f(at.z)],
                "yaw": f(yaw.to_degrees()),
                "components": comps,
            }));
        } else {
            let Some(prefab) = prefab_of(model) else {
                panic!("no prefab for {model}");
            };
            self.ops.push(json!({
                "type": "prefab", "name": name, "prefab": prefab,
                "translation": [f(at.x), f(at.y), f(at.z)],
                "rotation": [f(rot.x), f(rot.y), f(rot.z), f(rot.w)],
                "scale": [f(scale.x), f(scale.y), f(scale.z)],
                "components": extra,
            }));
        }
    }

    /// A static collider: the box (or upright cylinder) `size` at `at`, turned `yaw`.
    fn block(&mut self, at: Vec3, size: Vec3, yaw: f32, round: bool) {
        let name = self.name("b");
        self.ops.push(json!({
            "type": "marker", "name": name, "kind": "block",
            "translation": [f(at.x), f(at.y), f(at.z)],
            "size": [f(size.x), f(size.y), f(size.z)],
            "yaw": f(yaw.to_degrees()),
            "components": {"Block": {"round": round}},
        }));
    }

    fn lamp(&mut self, at: Vec3, lumens: f32, range: f32, rgb: (f32, f32, f32)) {
        let name = self.name("l");
        self.ops.push(json!({
            "type": "marker", "name": name, "kind": "lamp",
            "translation": [f(at.x), f(at.y), f(at.z)],
            "components": {"Lamp": {"lumens": lumens, "range": range, "r": rgb.0, "g": rgb.1, "b": rgb.2}},
        }));
    }

    fn layout(&mut self, l: &surf_office::Layout, lumens: f32, range: f32, rgb: (f32, f32, f32)) {
        for p in &l.pieces {
            self.model(&p.model, p.at, p.rot, p.scale);
        }
        for s in l.solids.iter().chain(&l.catwalks) {
            self.block(s.centre, s.half * 2.0, s.yaw, false);
        }
        for at in &l.lights {
            self.lamp(*at, lumens, range, rgb);
        }
    }

    /// A box part of the map's own geometry, with a modeler material.
    fn part(&mut self, centre: Vec3, yaw: f32, size: Vec3, pbr: &str) {
        let name = self.name("g");
        self.ops.push(json!({
            "type": "primitive", "name": name, "kind": "box",
            "size": [f(size.x), f(size.y), f(size.z)],
            "translation": [f(centre.x), f(centre.y), f(centre.z)],
            "rotation": {"y": f(yaw.to_degrees())},
        }));
        self.ops
            .push(json!({"type": "material", "name": name, "pbr": pbr}));
    }

    /// A box with a flat colour (pallets, plywood).
    fn flat(&mut self, centre: Vec3, yaw: f32, size: Vec3, rgb: [f32; 3], rough: f32) {
        let name = self.name("g");
        self.ops.push(json!({
            "type": "primitive", "name": name, "kind": "box",
            "size": [f(size.x), f(size.y), f(size.z)],
            "translation": [f(centre.x), f(centre.y), f(centre.z)],
            "rotation": {"y": f(yaw.to_degrees())},
        }));
        self.ops.push(json!({
            "type": "material", "name": name, "base_color": rgb, "roughness": rough, "flat": true,
        }));
    }

    /// The city: each block's collider, and its looks as boxes of modeler
    /// materials (the same parts `surf.rs` used to spawn at run time).
    fn city(&mut self, t: &Tuning) {
        const FACADES: [&str; 4] = ["sf-fa-glass", "sf-fa-office", "sf-fa-brick", "sf-fa-neon"];
        let mut wedges: Vec<Block> = Vec::new();
        for b in surf::city(t) {
            if b.piece == Piece::Kicker {
                let name = self.name("w");
                self.ops.push(json!({
                    "type": "marker", "name": name, "kind": "wedge",
                    "translation": [f(b.centre.x), f(b.centre.y), f(b.centre.z)],
                    "size": [f(b.size.x), f(b.size.y), f(b.size.z)],
                    "yaw": f(b.yaw.to_degrees()),
                    "components": {"Wedge": {}},
                }));
                wedges.push(b);
                continue;
            }
            self.block(b.centre, b.size, b.yaw, false);
            let top = b.size.y / 2.0;
            let at = |off: Vec3| b.centre + b.rotation() * off;
            match b.piece {
                Piece::Tower { style } => {
                    self.part(
                        at(Vec3::ZERO),
                        b.yaw,
                        b.size,
                        FACADES[style as usize % FACADES.len()],
                    );
                    self.part(
                        at(Vec3::Y * (top + 0.15)),
                        b.yaw,
                        b.size.with_y(0.3) - Vec3::new(0.4, 0.0, 0.4),
                        "sf-fa-roof",
                    );
                    if style % 2 == 0 {
                        self.part(
                            at(Vec3::Y * (top - 2.5)),
                            b.yaw,
                            b.size.with_y(0.5) + Vec3::new(0.2, 0.0, 0.2),
                            "sf-neon-pink",
                        );
                    }
                }
                Piece::Site => {
                    self.part(
                        at(Vec3::ZERO),
                        b.yaw,
                        b.size - Vec3::new(0.6, 0.0, 0.6),
                        "sf-fa-deck",
                    );
                    self.part(
                        at(Vec3::Y * (top - 0.3)),
                        b.yaw,
                        b.size.with_y(0.6),
                        "sf-fa-deck",
                    );
                    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        let off = Vec3::new(
                            x * (b.size.x / 2.0 - 0.6),
                            top - 5.0,
                            z * (b.size.z / 2.0 - 0.6),
                        );
                        self.part(at(off), b.yaw, Vec3::new(0.8, 9.4, 0.8), "sf-fa-deck");
                    }
                    self.part(
                        at(Vec3::Y * (top - 6.0)),
                        b.yaw,
                        b.size.with_y(5.0) + Vec3::new(0.1, 0.0, 0.1),
                        "sf-safety-net",
                    );
                }
                Piece::Steel => self.part(b.centre, b.yaw, b.size, "sf-crane-yellow"),
                Piece::Clutter => self.flat(b.centre, b.yaw, b.size, [0.5, 0.36, 0.22], 0.9),
                Piece::Kicker => {}
            }
        }
        // The kickers' looks: one flat-shaded mesh.
        let (mut verts, mut nor, mut idx) = (
            Vec::<[f32; 3]>::new(),
            Vec::<[f32; 3]>::new(),
            Vec::<u32>::new(),
        );
        for b in &wedges {
            let c = b.wedge().map(|p| b.centre + b.rotation() * p);
            let faces: [&[usize]; 5] = [
                &[0, 2, 3, 1],
                &[0, 1, 5, 4],
                &[2, 4, 5, 3],
                &[0, 4, 2],
                &[1, 3, 5],
            ];
            let mid = c.iter().copied().sum::<Vec3>() / 6.0;
            for fc in faces {
                let mut v: Vec<Vec3> = fc.iter().map(|&i| c[i]).collect();
                let mut n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
                if n.dot(v[0] - mid) < 0.0 {
                    v.reverse();
                    n = -n;
                }
                let base = verts.len() as u32;
                for p in &v {
                    verts.push(p.to_array());
                    nor.push(n.to_array());
                }
                for i in 1..v.len() as u32 - 1 {
                    idx.extend([base, base + i, base + i + 1]);
                }
            }
        }
        if !verts.is_empty() {
            self.ops.push(json!({"type": "tri_mesh", "name": "kickers", "verts": verts, "normals": nor, "indices": idx}));
            self.ops.push(json!({
                "type": "material", "name": "kickers", "base_color": [0.8, 0.62, 0.38],
                "roughness": 0.85, "flat": true,
            }));
        }
    }

    /// The neon dressing: pad strips, stage markers, lane lights, the finish
    /// arch, and the lamps over the pads.
    fn dressing(&mut self, t: &Tuning, d: &surf_feature::Dressing) {
        // Each stage's neon material, nearest the stage's colour.
        let neon = |i: usize| match i {
            0 => "sf-neon-pink",
            1 => "sf-neon-cyan",
            2 => "sf-neon-lime",
            3 => "sf-neon-orange",
            4 => "sf-neon-violet",
            5 => "sf-lamp-white",
            _ => "sf-neon-coral",
        };
        for (i, stage) in t.stages.iter().enumerate() {
            let col = surf_feature::stage_colour(d, i);
            let m = neon(i);
            let p = surf::pad(stage.pad);
            for (c, s) in surf_feature::edge_strips(p, d.strip) {
                self.part(c, 0.0, s, m);
            }
            let (c, s) = p;
            let top = c.y + s.y / 2.0;
            let corner = Vec3::new(c.x - s.x / 2.0 + 0.6, top, c.z + s.z / 2.0 - 0.6);
            let h = d.marker_height;
            self.flat(
                corner + Vec3::Y * h / 2.0,
                0.0,
                Vec3::new(0.2, h, 0.2),
                [0.12, 0.12, 0.18],
                0.7,
            );
            for n in 0..=i {
                self.part(
                    corner + Vec3::Y * (h + 0.35 + n as f32 * 0.55),
                    0.0,
                    Vec3::splat(0.4),
                    m,
                );
            }
            self.lamp(
                c + Vec3::Y * 4.0,
                d.pad_light,
                d.pad_light_range,
                (col.x, col.y, col.z),
            );
            for ramp in &stage.ramps {
                for at in surf_feature::lane_lights(t, d, ramp) {
                    self.part(at, 0.0, Vec3::splat(d.lane_size), m);
                }
            }
        }
        // The finish: gold strips and an arch.
        let f = surf::pad(t.finish);
        for (c, s) in surf_feature::edge_strips(f, d.strip * 1.5) {
            self.part(c, 0.0, s, "sf-neon-gold");
        }
        let (c, s) = f;
        let top = c.y + s.y / 2.0;
        let (h, w) = (d.arch_height, d.arch_width);
        for side in [-1.0, 1.0] {
            self.part(
                Vec3::new(c.x + side * w / 2.0, top + h / 2.0, c.z),
                0.0,
                Vec3::new(0.4, h, 0.4),
                "sf-neon-gold",
            );
        }
        self.part(
            Vec3::new(c.x, top + h, c.z),
            0.0,
            Vec3::new(w + 0.4, 0.5, 0.4),
            "sf-neon-gold",
        );
        // Pink lights over the start and the finish.
        for at in [surf_feature::start_pad(t).0, surf_feature::finish_pad(t).0] {
            self.lamp(at + Vec3::Y * 6.0, 400_000.0, 30.0, (1.0, 0.6, 0.95));
        }
    }

    /// The course itself: every pad, the finish, each ridge point and the
    /// pads' solid boxes, as markers (`friendslop_base::rules::surf::course`
    /// reads them back).
    fn course(&mut self, t: &Tuning) {
        let boxed =
            |name: String, kind: &str, c: (f32, f32, f32), size: (f32, f32, f32), comps: Value| {
                json!({
                    "type": "marker", "name": name, "kind": kind,
                    "translation": [f(c.0), f(c.1), f(c.2)],
                    "size": [f(size.0), f(size.1), f(size.2)],
                    "components": comps,
                })
            };
        for (i, st) in t.stages.iter().enumerate() {
            self.ops.push(boxed(
                format!("pad_{i}"),
                "pad",
                st.pad.0,
                st.pad.1,
                json!({"Pad": {"stage": i, "finish": false}}),
            ));
            for (r, ramp) in st.ramps.iter().enumerate() {
                for (k, p) in ramp.iter().enumerate() {
                    self.ops.push(json!({
                        "type": "marker", "name": format!("ridge_s{i}_r{r}_p{k}"), "kind": "ridge",
                        "translation": [f(p.0), f(p.1), f(p.2)],
                        "components": {"Ridge": {"stage": i, "ramp": r, "order": k}},
                    }));
                }
            }
        }
        self.ops.push(boxed(
            "pad_finish".into(),
            "pad",
            t.finish.0,
            t.finish.1,
            json!({"Pad": {"stage": 0, "finish": true}}),
        ));
        for (n, b) in t.blocks.iter().enumerate() {
            self.ops.push(boxed(
                format!("course_block_{n}"),
                "course_block",
                b.0,
                b.1,
                json!({"Block": {"round": false}}),
            ));
        }
    }

    /// A stage's ramps as one mesh part, a closed prism per span.
    fn ramps(&mut self, t: &Tuning, stages: &[usize]) {
        let (mut verts, mut nor, mut idx) = (
            Vec::<[f32; 3]>::new(),
            Vec::<[f32; 3]>::new(),
            Vec::<u32>::new(),
        );
        for ramp in stages
            .iter()
            .filter_map(|&i| t.stages.get(i))
            .flat_map(|s| &s.ramps)
        {
            let js = joints(ramp, t.ramp_samples, t.ramp_half_width, t.ramp_height);
            for w in js.windows(2) {
                let (a, b) = (w[0], w[1]);
                let up = |q: [Vec3; 4], want_up: bool| {
                    let mut q = q;
                    let mut n = (q[1] - q[0]).cross(q[2] - q[0]).normalize_or_zero();
                    if (n.y < 0.0) == want_up {
                        q.reverse();
                        n = -n;
                    }
                    (q, n)
                };
                for (q, want_up) in [
                    ([a[0], a[1], b[1], b[0]], true),
                    ([a[0], b[0], b[2], a[2]], true),
                    ([a[1], a[2], b[2], b[1]], false),
                ] {
                    let (q, n) = up(q, want_up);
                    let base = verts.len() as u32;
                    for v in q {
                        verts.push(v.to_array());
                        nor.push(n.to_array());
                    }
                    idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
        }
        if verts.is_empty() {
            return;
        }
        let name = "ramps_office_mall".to_string();
        self.ops.push(json!({
            "type": "tri_mesh", "name": name,
            "verts": verts, "normals": nor, "indices": idx,
        }));
        self.ops.push(json!({
            "type": "material", "name": name, "base_color": [0.16, 0.19, 0.32],
            "roughness": 0.35, "metalness": 0.5, "flat": true,
        }));
    }
}

fn main() {
    surf::TUNING.set_bytes(&read("data/side/surf.ron"));
    let t = surf::tuning();
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

    // The office tower and the mall.
    surf_office::OFFICE.set_bytes(&read("data/side/surf_office.ron"));
    let d = surf_office::OFFICE.get();
    let office_stage = d.stage;
    let l = surf_office::layout(&t, &d);
    o.layout(&l, d.light_intensity, d.light_range, d.light_colour);
    surf_mall::MALL.set_bytes(&read("data/side/surf_mall.ron"));
    let d = surf_mall::MALL.get();
    let l = surf_mall::layout(&t, &d);
    o.layout(&l, d.light_intensity, d.light_range, d.light_colour);

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
        let d = surf_office::OFFICE.get();
        let l = surf_office::layout(&t, &d);
        surf_props::claim(l.claim.0, l.claim.1);
        let d = surf_mall::MALL.get();
        let l = surf_mall::layout(&t, &d);
        surf_props::claim(l.claim.0, l.claim.1);
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
    o.dressing(&t, &surf_feature::DRESSING.get());
    o.course(&t);

    // The ramps of the stages the hall model doesn't have.
    o.ramps(&t, &[office_stage, d.stage]);

    let out = assets("../.modeler/gen/surf_map_ops.json");
    let text = serde_json::to_string(&o.ops).unwrap_or_default();
    if let Err(e) = std::fs::write(&out, text) {
        panic!("{}: {e}", out.display());
    }
    println!("{} ops -> {}", o.ops.len(), out.display());
}
