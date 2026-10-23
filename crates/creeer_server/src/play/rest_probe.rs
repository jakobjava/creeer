//! G2.0's probe (run by hand): the lumberjack scene played, then left alone, and every awake
//! body's motion listed each second: what keeps felled trunks from resting.
//!
//! `cargo test -p creeer_server --release rest_probe -- --ignored --nocapture`

use super::Play;
use super::record::Act;
use super::scenes::{scene, terrain};
use creeer_math::glam::DVec3;

#[test]
#[ignore = "a measurement, run by hand"]
fn rest_probe() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let still = vec![Act::default(); s.players.len()];
    for tick in 0..1800 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
    }
    let start: Vec<DVec3> = (play.server.world.world.bodies.iter())
        .map(|b| b.structure.position[0])
        .collect();
    for second in 0..30 {
        let mut ms = 0.0f64;
        for _ in 0..60 {
            ms = ms.max(play.tick(&still).ms);
        }
        let w = &play.server.world.world;
        let mut line = format!("t+{second:2}s worst {ms:5.2} ms:");
        for (i, b) in w.bodies.iter().enumerate() {
            if b.sleep.asleep || b.held {
                continue;
            }
            let s = &b.structure;
            let v = s.velocity.iter().map(|v| v.length()).fold(0.0, f64::max);
            let w = s
                .angular_velocity
                .iter()
                .map(|v| v.length())
                .fold(0.0, f64::max);
            let moved = start.get(i).map_or(0.0, |p| (s.position[0] - *p).length());
            line += &format!(
                " [{i} m={:.0} v={:.1}mm/s w={:.1}mrad/s moved={:.0}mm]",
                s.total_mass(),
                v * 1e3,
                w * 1e3,
                moved * 1e3
            );
        }
        println!("{line}");
    }
}

/// Tick by tick, after the scene: each awake trunk's spin, its group, its Newton work and the
/// guard's fallbacks.
#[test]
#[ignore = "a measurement, run by hand"]
fn rest_probe_ticks() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let still = vec![Act::default(); s.players.len()];
    for tick in 0..1800 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
    }
    for _ in 0..600 {
        play.tick(&still);
    }
    for tick in 0..90 {
        let t = play.tick(&still);
        let w = &play.server.world.world;
        let mut line = format!("{tick:3} {:5.2}ms:", t.ms);
        for g in w.groups() {
            let awake: Vec<usize> = g.iter().copied().filter(|&b| !w.bodies[b].held).collect();
            if awake.is_empty() {
                continue;
            }
            line += " {";
            for b in awake {
                let body = &w.bodies[b];
                let s = &body.structure;
                let om = s.angular_velocity[0];
                line += &format!(
                    "{b}: w=({:+.0},{:+.0},{:+.0}) v={:.1} it={} fb={} sh={};",
                    om.x * 1e3,
                    om.y * 1e3,
                    om.z * 1e3,
                    s.velocity[0].length() * 1e3,
                    body.solver.last_iterations,
                    body.solver.fallbacks,
                    body.shapes.len(),
                );
            }
            line += "}";
        }
        line += &format!(" contacts={}", w.contacts.len());
        println!("{line}");
    }
}

/// The awake trunks' joints, and their node's path tick by tick.
#[test]
#[ignore = "a measurement, run by hand"]
fn rest_probe_paths() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let still = vec![Act::default(); s.players.len()];
    for tick in 0..1800 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
    }
    for _ in 0..600 {
        play.tick(&still);
    }
    let w = &play.server.world.world;
    for j in w.joints() {
        println!("joint {:?} - {:?} kind {:?}", j.a, j.b, j.kind);
    }
    let watch: Vec<usize> = (0..w.bodies.len())
        .filter(|&b| !w.bodies[b].sleep.asleep && !w.bodies[b].held)
        .collect();
    let origin: Vec<DVec3> = watch
        .iter()
        .map(|&b| w.bodies[b].structure.position[0])
        .collect();
    let turned: Vec<creeer_math::glam::DQuat> = watch
        .iter()
        .map(|&b| w.bodies[b].structure.orientation[0])
        .collect();
    for tick in 0..24 {
        play.tick(&still);
        let w = &play.server.world.world;
        let mut line = format!("{tick:3}:");
        for (k, &b) in watch.iter().enumerate() {
            let s = &w.bodies[b].structure;
            let d = (s.position[0] - origin[k]) * 1e6;
            let r = (s.orientation[0] * turned[k].inverse()).to_scaled_axis() * 1e6;
            line += &format!(
                " [{b} dx=({:+.0},{:+.0},{:+.0})um turn=({:+.0},{:+.0},{:+.0})urad]",
                d.x, d.y, d.z, r.x, r.y, r.z,
            );
        }
        println!("{line}");
    }
}

/// Each awake trunk alone on the ground (the others taken away): does it rest by itself?
#[test]
#[ignore = "a measurement, run by hand"]
fn rest_probe_alone() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let still = vec![Act::default(); s.players.len()];
    for tick in 0..1800 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
    }
    for _ in 0..600 {
        play.tick(&still);
    }
    let all = &play.server.world.world;
    let watch: Vec<usize> = (0..all.bodies.len())
        .filter(|&b| !all.bodies[b].sleep.asleep && !all.bodies[b].held)
        .collect();
    for b in watch {
        let mut w = creeer_physics::world::World::new(all.dt());
        w.ground = all.ground.clone();
        w.statics = all.statics.clone();
        w.static_friction = all.static_friction;
        w.static_surface = all.static_surface;
        w.add(all.bodies[b].clone());
        let mut line = format!("body {b} m={:.0}:", w.bodies[0].structure.total_mass());
        for second in 0..20 {
            let q0 = w.bodies[0].structure.orientation[0];
            let p0 = w.bodies[0].structure.position[0];
            let (mut most, mut moved) = (0.0f64, 0.0f64);
            for _ in 0..60 {
                w.step();
                let q = w.bodies[0].structure.orientation[0];
                most = most.max((q * q0.inverse()).to_scaled_axis().length());
                moved = moved.max((w.bodies[0].structure.position[0] - p0).length());
            }
            let asleep = w.bodies[0].sleep.asleep;
            let z = if asleep { "Z" } else { "" };
            line += &format!(" {second}:{:.0}ur/{:.0}um{z}", most * 1e6, moved * 1e6);
        }
        println!("{line}");
    }
}

/// Trunk `TRUNK` (env, default 5) alone, tick by tick: its kinetic energy, contacts and the
/// guard's fallbacks.
#[test]
#[ignore = "a measurement, run by hand"]
fn rest_probe_trace() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let still = vec![Act::default(); s.players.len()];
    for tick in 0..1800 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
    }
    for _ in 0..600 {
        play.tick(&still);
    }
    let all = &play.server.world.world;
    let b: usize = std::env::var("TRUNK")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(5);
    let mut w = creeer_physics::world::World::new(all.dt());
    w.ground = all.ground.clone();
    w.static_friction = all.static_friction;
    w.static_surface = all.static_surface;
    w.add(all.bodies[b].clone());
    if let Some(mu) = std::env::var("MU").ok().and_then(|m| m.parse::<f64>().ok()) {
        w.bodies[0].friction = mu;
        w.static_friction = mu;
    }
    println!(
        "slope: {:.1} deg, friction {} / {}",
        0.0, w.bodies[0].friction, w.static_friction
    );
    for _ in 0..120 {
        w.step();
    }
    let p0 = w.bodies[0].structure.position[0];
    for tick in 0..90 {
        w.step();
        let body = &w.bodies[0];
        let s = &body.structure;
        let i = s.inertia_of(0);
        let r = creeer_math::glam::DMat3::from_quat(s.orientation[0]);
        let om = s.angular_velocity[0];
        let ke = 0.5 * s.total_mass() * s.velocity[0].length_squared()
            + 0.5 * om.dot(r * i * r.transpose() * om);
        let d = (s.position[0] - p0) * 1e6;
        let normals: Vec<String> = (w.contacts.iter())
            .map(|c| format!("{:.2}", c.normal.dot((s.position[0]).normalize())))
            .collect();
        println!(
            "{tick:3} ke={:8.3}mJ dx=({:+6.0},{:+6.0},{:+6.0})um it={:2} fb={} n={} {}",
            ke * 1e3,
            d.x,
            d.y,
            d.z,
            body.solver.last_iterations,
            body.solver.fallbacks,
            w.contacts.len(),
            normals.join(",")
        );
    }
}
