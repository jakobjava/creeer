//! G2.0's sticking-contact probe (run by hand): *lumberjack*'s felled parts after the last
//! blow, written once as a world (players taken out), then stepped by any build: when each
//! group falls asleep, and what a tick costs while they lie.
//!
//! `STATE=<file> cargo test -p creeer_server --release rest_stick -- --ignored --nocapture`

use super::Play;
use super::scenes::{scene, terrain};
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::world::World;
use creeer_physics::world::contact::Side;
use creeer_sim::TerrainGround;
use std::sync::Arc;

fn state_file() -> String {
    std::env::var("STATE").unwrap_or_else(|_| "lumberjack-after.bin".into())
}

/// The scene played to its end, its held bodies (the players) taken out, written to `STATE`.
#[test]
#[ignore = "a measurement, run by hand"]
fn rest_stick_write() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    for tick in 0..s.ticks.0 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
    }
    let mut world = play.server.world.world.clone();
    world.retain_bodies(|_, b| !b.held);
    std::fs::write(state_file(), world.to_bytes()).expect("written");
    println!("{} bodies written", world.bodies.len());
}

/// The written state stepped 20 s: each second the awake bodies, the worst tick, and when all
/// slept.
#[test]
#[ignore = "a measurement, run by hand"]
fn rest_stick_step() {
    let bytes = std::fs::read(state_file()).expect("the state (rest_stick_write)");
    let mut world = World::from_bytes(&bytes).expect("a world");
    world.ground = Some(Arc::new(TerrainGround(terrain())));
    if let Ok(only) = std::env::var("ONLY") {
        let keep: Vec<usize> = only.split(',').filter_map(|t| t.parse().ok()).collect();
        world.retain_bodies(|i, _| keep.contains(&i));
    }
    if let Some(t) = std::env::var("TOL")
        .ok()
        .and_then(|t| t.parse::<f64>().ok())
    {
        for b in &mut world.bodies {
            b.solver.tolerance = t;
        }
    }
    let watch: usize = std::env::var("WATCH")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(1);
    let mut all_asleep = None;
    let origin: Vec<DVec3> = world
        .bodies
        .iter()
        .map(|b| b.structure.position[0])
        .collect();
    let mut total = 0.0;
    for second in 0..20 {
        let mut worst = 0.0f64;
        for tick in 0..60 {
            let t = std::time::Instant::now();
            world.step();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            worst = worst.max(ms);
            total += ms;
            if std::env::var("TRACE").is_ok_and(|t| t.parse() == Ok(second)) {
                let mut line = format!("{tick:2}:");
                for (i, b) in world.bodies.iter().enumerate() {
                    let (s, w) = (&b.structure, b.structure.angular_velocity[0] * 1e3);
                    let v = s.velocity[0] * 1e3;
                    let r = DMat3::from_quat(s.orientation[0]);
                    let om = s.angular_velocity[0];
                    let ke = 0.5 * s.total_mass() * s.velocity[0].length_squared()
                        + 0.5 * om.dot(r * s.inertia_of(0) * r.transpose() * om);
                    line += &format!(
                        " [{i} w=({:+.2},{:+.2},{:+.2}) v=({:+.2},{:+.2},{:+.2}) it={} fb={} up={:+.1}um ke={:.4}mJ]",
                        w.x,
                        w.y,
                        w.z,
                        v.x,
                        v.y,
                        v.z,
                        b.solver.last_iterations,
                        b.solver.fallbacks,
                        (s.position[0].length() - origin[i].length()) * 1e6,
                        ke * 1e3
                    );
                }
                let pairs: Vec<String> = (world.contacts.iter())
                    .filter(|c| {
                        c.normal_force() > 20.0
                            && [c.a, c.b]
                                .iter()
                                .any(|s| matches!(s, Side::Node { member, .. } if *member == watch))
                    })
                    .map(|c| {
                        let kind =
                            if matches!(c.law, creeer_physics::world::contact::Law::Patch { .. }) {
                                "P"
                            } else {
                                "H"
                            };
                        let at = |s: Side| match s {
                            Side::Node {
                                member,
                                node,
                                local,
                            } => {
                                let b = &world.bodies[member].structure;
                                b.position[node] + b.orientation[node] * local
                            }
                            Side::Fixed(p) => p,
                        };
                        let travel = at(c.a) - at(c.anchor);
                        let u = travel - c.normal * c.normal.dot(travel);
                        // The torques on body 1 about its node along the rocking axis.
                        let axis = DVec3::new(0.46, 0.31, 0.88).normalize();
                        let e = 1e-4;
                        let r = u.length();
                        let f1 = if r >= e {
                            1.0 / r
                        } else {
                            2.0 / e - r / (e * e)
                        };
                        let fric = -u * (c.friction * c.normal_force() * f1);
                        let normal = c.normal * c.normal_force();
                        let mut tn = 0.0;
                        let mut tf = 0.0;
                        let centre = world.bodies[watch].structure.position[0];
                        for (s, sign) in [(c.a, 1.0), (c.b, -1.0)] {
                            if let Side::Node { member, .. } = s
                                && member == watch
                            {
                                let arm = at(s) - centre;
                                tn += sign * axis.dot(arm.cross(normal));
                                tf += sign * axis.dot(arm.cross(fric));
                            }
                        }
                        format!(
                            "{}-{}{kind}:{:.0}N/u={:.0}um/tn={:+.2}/tf={:+.2}",
                            side(c.a),
                            side(c.b),
                            c.normal_force(),
                            u.length() * 1e6,
                            tn,
                            tf
                        )
                    })
                    .collect();
                println!("{line} {}", pairs.join(" "));
            }
            if world.bodies.iter().all(|b| b.sleep.asleep) {
                all_asleep.get_or_insert(second * 60 + tick);
            } else {
                all_asleep = None;
            }
        }
        let mut line = format!("t+{second:2}s worst {worst:5.2} ms:");
        for (i, b) in world.bodies.iter().enumerate() {
            if b.sleep.asleep {
                continue;
            }
            let s = &b.structure;
            line += &format!(
                " [{i} {:.0}kg {:.2}mm/s {:.2}mrad/s]",
                s.total_mass(),
                s.velocity[0].length() * 1e3,
                s.angular_velocity[0].length() * 1e3
            );
        }
        println!("{line}");
    }
    println!(
        "all asleep from tick {all_asleep:?}; {:.2} ms a tick on average",
        total / 1200.0
    );
}

/// The scene played and left alone with its players (held): body `WATCH` (default 6) tick by
/// tick from `FROM` ticks after the script (default 300), with its group's contacts.
#[test]
#[ignore = "a measurement, run by hand"]
fn rest_stick_scene() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    for tick in 0..s.ticks.0 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
    }
    let var = |n: &str, d: usize| {
        std::env::var(n)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let (watch, from) = (var("WATCH", 6), var("FROM", 300));
    let still = vec![super::record::Act::default(); s.players.len()];
    let every = var("EVERY", 1);
    for tick in 0..from + 40 * every {
        play.tick(&still);
        if tick < from || (tick - from) % every != 0 {
            continue;
        }
        let w = &play.server.world.world;
        let b = &w.bodies[watch];
        let om = b.structure.angular_velocity[0] * 1e3;
        let mut line = format!(
            "{tick}: w=({:+.1},{:+.1},{:+.1}) it={} fb={} asleep={}",
            om.x, om.y, om.z, b.solver.last_iterations, b.solver.fallbacks, b.sleep.asleep
        );
        if let Ok(other) = std::env::var("OTHER").map(|o| o.parse::<usize>().unwrap_or(2)) {
            for t in w
                .touching(watch, 0.01)
                .iter()
                .filter(|t| t.other.is_some_and(|o| o.0 == other))
            {
                let p = t.point - b.structure.position[0];
                line += &format!(
                    " touch n=({:+.4},{:+.4},{:+.4}) gap={:+.1}um at=({:+.3},{:+.3},{:+.3})",
                    t.normal.x,
                    t.normal.y,
                    t.normal.z,
                    t.gap * 1e6,
                    p.x,
                    p.y,
                    p.z
                );
            }
        }
        for (group, contacts) in w.group_contacts().filter(|(g, _)| g.contains(&watch)) {
            line += &format!(
                " group {group:?} held {:?}:",
                group.iter().map(|&i| w.bodies[i].held).collect::<Vec<_>>()
            );
            for c in contacts {
                let name = |s: Side| match s {
                    Side::Node { member, .. } => group[member].to_string(),
                    Side::Fixed(_) => "F".into(),
                };
                let kind = if matches!(c.law, creeer_physics::world::contact::Law::Patch { .. }) {
                    "P"
                } else {
                    "H"
                };
                line += &format!(" {}-{}{kind}:{:.0}", name(c.a), name(c.b), c.normal_force());
            }
        }
        println!("{line}");
    }
}

/// A contact's side as its member in the group, or `F` for fixed geometry and the ground.
fn side(s: creeer_physics::world::contact::Side) -> String {
    match s {
        creeer_physics::world::contact::Side::Node { member, .. } => member.to_string(),
        creeer_physics::world::contact::Side::Fixed(_) => "F".into(),
    }
}
