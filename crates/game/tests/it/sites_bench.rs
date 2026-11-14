//! How much the sleeping sites save (#412): the host's frame with everyone at
//! the commune, against everyone in New York. Not a check, a measurement:
//! `cargo test -p friendslop --release --test it sites_bench -- --ignored --nocapture`.

use std::time::Instant;

use bevy::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::features::fps::Walker;
use friendslop::player::Player;

use super::harness::Sim;

/// Put every body at `at` (x, z) and hold it there.
fn gather(sim: &mut Sim, at: Vec2) {
    let world = sim.server.world_mut();
    for (_, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
        w.pos = Vec3::new(at.x, w.pos.y, at.y);
    }
}

/// The host's mean and 95th-percentile frame (ms) over `frames`, with
/// everyone at `at`, and how many entities it holds by then.
fn host_frame(at: Vec2, frames: usize) -> (f64, f64, usize) {
    let mut sim = Sim::new(3);
    // Settle: the sites wake or sleep, the world spawns.
    for _ in 0..120 {
        gather(&mut sim, at);
        sim.step();
    }
    let mut ms = Vec::with_capacity(frames);
    for _ in 0..frames {
        gather(&mut sim, at);
        let start = Instant::now();
        sim.server.update();
        ms.push(start.elapsed().as_secs_f64() * 1000.0);
        for client in &mut sim.clients {
            sim.server.exchange_with_client(client);
            client.update();
            sim.server.exchange_with_client(client);
        }
    }
    ms.sort_by(f64::total_cmp);
    let mean = ms.iter().sum::<f64>() / ms.len() as f64;
    let p95 = ms[(ms.len() - 1) * 95 / 100];
    let world = sim.server.world_mut();
    let entities = world.query::<()>().iter(world).count();
    (mean, p95, entities)
}

#[test]
#[ignore = "a measurement, run by hand"]
fn host_frame_with_everyone_at_the_commune() {
    let commune = friendslop::sites::at("bonfire");
    let nyc = Vec2::new(0.0, 223.0);
    for (name, at) in [("commune", commune), ("new york", nyc)] {
        let (mean, p95, entities) = host_frame(at, 1200);
        println!(
            "sites bench: everyone at {name}: host frame mean {mean:.3} ms p95 {p95:.3} ms, {entities} entities"
        );
    }
}
