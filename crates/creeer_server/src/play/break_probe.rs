//! *log_bridge*'s breaking tick, taken apart (G2.11 A6; run by hand): the scene played to
//! `FROM` (930), then tick by tick to `TO` (960) every body but the players: its nodes and
//! members (broken), its mass, how fast it moves and spins, and the energy it holds; and the
//! world's energy in all. With `--features creeer_physics/profile` each tick's physics phases.
//!
//! `cargo test -p creeer_server --release --features creeer_physics/profile break_probe --
//! --ignored --nocapture`

use super::Play;
use super::scenes::{scene, terrain};
use creeer_math::glam::{DMat3, DVec3};

fn env(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// A body's mass, its centre's speed, its fastest node's spin, and its kinetic, elastic and
/// gravity's energy.
fn body_numbers(s: &creeer_physics::structure::Structure) -> (f64, f64, f64, f64, f64, f64) {
    let mass = s.total_mass();
    let momentum: DVec3 = (0..s.node_count())
        .map(|i| s.velocity[i] * s.masses()[i])
        .sum();
    let spin = (s.angular_velocity.iter())
        .map(|w| w.length())
        .fold(0.0, f64::max);
    let (kinetic, potential) = s.node_energy();
    let elastic = s.elastic_energy();
    (
        mass,
        momentum.length() / mass,
        spin,
        kinetic,
        elastic,
        potential,
    )
}

#[test]
#[ignore = "a measurement, run by hand"]
fn break_probe() {
    let s = scene("log_bridge").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let (from, to) = (env("FROM", 930), env("TO", 960));
    let avatars: Vec<_> = (0..play.players())
        .map(|k| play.client(k).welcome.as_ref().expect("joined").avatar)
        .collect();
    for tick in 0..to {
        let acts = (s.script)(tick, &play);
        let cost = play.tick(&acts);
        // `CLEAR`: bodies found inside a 200 kg crate after this tick moved out of it (an
        // experiment: a ragdoll laid out clear of what fell on it).
        if std::env::var("CLEAR").is_ok() {
            let world = &mut play.server.world.world;
            let crates: Vec<usize> = (0..world.bodies.len())
                .filter(|&i| (world.bodies[i].structure.total_mass() - 200.0).abs() < 1e-9)
                .collect();
            for i in 0..world.bodies.len() {
                let deepest = (world.touching(i, 0.0).into_iter())
                    .filter(|t| t.other.is_some_and(|(j, _)| crates.contains(&j)))
                    .min_by(|a, b| a.gap.total_cmp(&b.gap));
                if let Some(t) = deepest.filter(|t| t.gap < -0.005) {
                    println!("  CLEAR body {i} out of a crate by {:.4} m", -t.gap);
                    world.bodies[i].structure.position[0] += t.normal * (0.001 - t.gap);
                    let b = &mut world.bodies[i];
                    b.solver.reset_history(&b.structure);
                }
            }
        }
        if tick < from {
            continue;
        }
        let w = &play.server.world;
        let phases: Vec<String> = (creeer_physics::profile::NAMES.iter())
            .filter(|(s, _)| cost.phases[*s as usize] > 0)
            .filter(|(s, _)| creeer_physics::profile::is_count(*s))
            .map(|(s, n)| format!("{n} {}", cost.phases[*s as usize]))
            .collect();
        println!(
            "tick {tick} (server {}): {:.1} ms, {} iterations; {}",
            w.tick,
            cost.ms,
            cost.work,
            phases.join(", ")
        );
        let mut sums = [0.0; 3];
        for (k, id) in w.ids().iter().enumerate() {
            if avatars.contains(id) {
                continue;
            }
            let b = &w.world.bodies[k];
            let st = &b.structure;
            let (m, v, spin, kin, ela, pot) = body_numbers(st);
            sums[0] += kin;
            sums[1] += ela;
            sums[2] += pot;
            // Each node's spin and its turning inertia about that spin.
            let fastest = (0..st.node_count())
                .max_by(|&a, &b| {
                    (st.angular_velocity[a].length()).total_cmp(&st.angular_velocity[b].length())
                })
                .unwrap_or(0);
            let wv = st.angular_velocity[fastest];
            let r = DMat3::from_quat(st.orientation[fastest]);
            let inertia = r * st.inertia_of(fastest) * r.transpose();
            let turning = if wv.length() > 0.0 {
                wv.dot(inertia * wv) / wv.length_squared()
            } else {
                0.0
            };
            println!(
                "  {:>3} {:2} nodes {:2} members ({} broken) {:7.2} kg  {:7.3} m/s  {:7.2} rad/s (I {:.2e})  kinetic {:9.3} elastic {:9.3} gravity {:10.3} J{}",
                id.0,
                st.node_count(),
                st.element_count(),
                st.elements_broken(),
                m,
                v,
                spin,
                turning,
                kin,
                ela,
                pot,
                if b.sleep.asleep { " asleep" } else { "" }
            );
        }
        println!(
            "  in all (players aside): kinetic {:.3} elastic {:.3} gravity {:.3} sum {:.3} J; world energy {:.3} J",
            sums[0],
            sums[1],
            sums[2],
            sums[0] + sums[1] + sums[2],
            w.world.energy()
        );
    }
}
