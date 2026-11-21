//! The brownstone's stoop climbs to its door: the tallest step meets the
//! landing under the door, and both handrails are highest there too.

use bevy::prelude::*;

fn prims() -> Vec<serde_json::Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.modeler/prefabs/nyc-kit-brownstone.json"
    );
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    doc["ops"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o.get("primitive").cloned())
        .collect()
}

fn vec3(v: &serde_json::Value) -> Vec3 {
    let f = |i: usize| v[i].as_f64().unwrap_or(0.0) as f32;
    Vec3::new(f(0), f(1), f(2))
}

#[test]
fn brownstone_stoop_rises_to_the_door() {
    let prims = prims();
    let get = |name: &str| prims.iter().find(|p| p["name"] == name).cloned();
    // CI checks out without LFS, so the prefab is only a pointer there.
    if prims.is_empty() {
        return;
    }
    let (Some(door), Some(landing)) = (get("door"), get("landing")) else {
        panic!("brownstone prefab has no door or landing");
    };
    let door_z = vec3(&door["translation"]).z;
    let landing = vec3(&landing["translation"]);
    let mut steps: Vec<(f32, f32)> = (0..)
        .map_while(|i| get(&format!("step{i}")))
        .map(|s| (vec3(&s["translation"]).z, vec3(&s["size"]).y))
        .collect();
    assert!(steps.len() > 2, "no steps");
    // Order from the door outwards: each step lower than the one before.
    steps.sort_by(|a, b| (a.0 - door_z).abs().total_cmp(&(b.0 - door_z).abs()));
    for w in steps.windows(2) {
        assert!(
            w[0].1 > w[1].1,
            "steps don't descend from the door: {steps:?}"
        );
    }
    assert!((landing.z - door_z).abs() < (steps[0].0 - door_z).abs());

    for side in ["rail_l", "rail_r"] {
        let Some(rail) = get(side) else {
            panic!("no {side}")
        };
        let r = &rail["rotation"];
        let q = Quat::from_xyzw(
            r[0].as_f64().unwrap_or(0.0) as f32,
            r[1].as_f64().unwrap_or(0.0) as f32,
            r[2].as_f64().unwrap_or(0.0) as f32,
            r[3].as_f64().unwrap_or(1.0) as f32,
        );
        // The rail runs along its local z: its end nearer the door is higher.
        let along = q * Vec3::Z;
        let toward_door = (door_z - steps[0].0).signum();
        assert!(
            along.y * along.z.signum() * toward_door > 0.0,
            "{side} slopes away from the door"
        );
    }
}
