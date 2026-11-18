//! Tools for baking a surf area into modeler ops (see `docs/design/surf-areas-brief.md`):
//! the placed models, `Block` colliders, `Lamp`s, geometry parts and course
//! markers a generator example writes to `.modeler/gen/<area>_ops.json`. Only
//! the example binaries use it; the game never does.

use std::path::PathBuf;

use crate::features::surf as surf_feature;
use crate::features::{
    surf_mall, surf_office, surf_pillars, surf_props, surf_runs, surf_slalom, surf_tower,
    surf_train,
};
use crate::rules::surf::{self, Block, Piece, Tuning, joints};
use bevy::math::{Quat, Vec3};
use serde_json::{Value, json};

pub fn assets(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../game/assets")
        .join(path)
}

pub fn read(path: &str) -> Vec<u8> {
    std::fs::read(assets(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The modeler prefab a model file was exported from: a few by name, the
/// surf set pieces by their file name (`surf_water_tower.gltf` is
/// `sf-water-tower`, `surf_train_beam.gltf` is `surf-train-beam`), the
/// meadow's by theirs.
pub fn prefab_of(model: &str) -> Option<String> {
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

/// Which part of the course an area's ops cover.
#[derive(Clone, Debug)]
pub struct Scope {
    /// The stages (0-based, start pad to the next stage's pad) it owns.
    pub stages: std::ops::Range<usize>,
    /// It owns the finish pad (and the course's end).
    pub finish: bool,
    /// It owns the start pad's light.
    pub start_light: bool,
}

#[derive(Default)]
pub struct Ops {
    pub ops: Vec<Value>,
    pub n: usize,
}

pub fn f(v: f32) -> f64 {
    (f64::from(v) * 1000.0).round() / 1000.0
}

impl Ops {
    /// Write the ops to `crates/game/.modeler/gen/<name>_ops.json`.
    pub fn write(&self, name: &str) {
        let out = assets(&format!("../.modeler/gen/{name}_ops.json"));
        let text = serde_json::to_string(&self.ops).unwrap_or_default();
        if let Err(e) = std::fs::write(&out, text) {
            panic!("{}: {e}", out.display());
        }
        println!("{} ops -> {}", self.ops.len(), out.display());
    }

    pub fn name(&mut self, stem: &str) -> String {
        self.n += 1;
        format!("{stem}{}", self.n)
    }

    /// A placed model. Unscaled, yaw-only ones are a `GltfSource` marker on the
    /// existing model file; the rest are prefab instances.
    pub fn model(&mut self, model: &str, at: Vec3, rot: Quat, scale: Vec3) {
        self.model_with(model, at, rot, scale, json!({}));
    }

    /// [`Ops::model`] with extra components (a `Bob`, a `Body`).
    pub fn model_with(&mut self, model: &str, at: Vec3, rot: Quat, scale: Vec3, extra: Value) {
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
    pub fn block(&mut self, at: Vec3, size: Vec3, yaw: f32, round: bool) {
        let name = self.name("b");
        self.ops.push(json!({
            "type": "marker", "name": name, "kind": "block",
            "translation": [f(at.x), f(at.y), f(at.z)],
            "size": [f(size.x), f(size.y), f(size.z)],
            "yaw": f(yaw.to_degrees()),
            "components": {"Block": {"round": round}},
        }));
    }

    pub fn lamp(&mut self, at: Vec3, lumens: f32, range: f32, rgb: (f32, f32, f32)) {
        let name = self.name("l");
        self.ops.push(json!({
            "type": "marker", "name": name, "kind": "lamp",
            "translation": [f(at.x), f(at.y), f(at.z)],
            "components": {"Lamp": {"lumens": lumens, "range": range, "r": rgb.0, "g": rgb.1, "b": rgb.2}},
        }));
    }

    pub fn layout(
        &mut self,
        l: &surf_office::Layout,
        lumens: f32,
        range: f32,
        rgb: (f32, f32, f32),
    ) {
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
    pub fn part(&mut self, centre: Vec3, yaw: f32, size: Vec3, pbr: &str) {
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
    pub fn flat(&mut self, centre: Vec3, yaw: f32, size: Vec3, rgb: [f32; 3], rough: f32) {
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
    pub fn city(&mut self, t: &Tuning) {
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
    pub fn dressing(&mut self, t: &Tuning, d: &surf_feature::Dressing, scope: &Scope) {
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
        for (i, stage) in t
            .stages
            .iter()
            .enumerate()
            .filter(|(i, _)| scope.stages.contains(i))
        {
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
        if scope.finish {
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
            self.lamp(
                surf_feature::finish_pad(t).0 + Vec3::Y * 6.0,
                400_000.0,
                30.0,
                (1.0, 0.6, 0.95),
            );
        }
        if scope.start_light {
            self.lamp(
                surf_feature::start_pad(t).0 + Vec3::Y * 6.0,
                400_000.0,
                30.0,
                (1.0, 0.6, 0.95),
            );
        }
    }

    /// The course itself: every pad, the finish, each ridge point and the
    /// pads' solid boxes, as markers (`friendslop_base::rules::surf::course`
    /// reads them back).
    pub fn course(&mut self, t: &Tuning, scope: &Scope, blocks: &[surf::Pad]) {
        let boxed =
            |name: String, kind: &str, c: (f32, f32, f32), size: (f32, f32, f32), comps: Value| {
                json!({
                    "type": "marker", "name": name, "kind": kind,
                    "translation": [f(c.0), f(c.1), f(c.2)],
                    "size": [f(size.0), f(size.1), f(size.2)],
                    "components": comps,
                })
            };
        for (i, st) in t
            .stages
            .iter()
            .enumerate()
            .filter(|(i, _)| scope.stages.contains(i))
        {
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
        for &(stage, ramp) in t.speed_gaps.iter().filter(|g| scope.stages.contains(&g.0)) {
            self.ops.push(json!({
                "type": "marker", "name": format!("gap_s{stage}_r{ramp}"), "kind": "gap",
                "translation": [0.0, 0.0, 0.0],
                "components": {"SpeedGap": {"stage": stage, "ramp": ramp}},
            }));
        }
        if scope.finish {
            self.ops.push(boxed(
                "pad_finish".into(),
                "pad",
                t.finish.0,
                t.finish.1,
                json!({"Pad": {"stage": 0, "finish": true}}),
            ));
        }
        for (n, b) in blocks.iter().enumerate() {
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
    pub fn ramps(&mut self, t: &Tuning, stages: &[usize]) {
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
        self.ops
            .push(json!({"type": "material", "name": name, "pbr": "sf-ramp"}));
    }
}
