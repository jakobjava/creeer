//! Where the physics sandbox's scenes spend their ticks (`cargo test -p creeer_client
//! --features creeer_physics/profile profile_ -- --ignored --nocapture`): each slow tick's
//! phases (`creeer_physics::profile`), and the scene's total, p50, p99 and worst tick.

use crate::physics_sandbox::{PhysicsSandbox, PhysicsScene};
use creeer_core::Simulation;
use creeer_physics::profile::{self, NAMES, is_count};

/// Prints where each slow tick of a scene goes (`--features creeer_physics/profile`).
fn report(scene: PhysicsScene, ticks: usize, slow: f64) {
    let mut sb = PhysicsSandbox::new(scene, 1.0 / 60.0);
    let _ = profile::take();
    let mut all = Vec::with_capacity(ticks);
    for k in 0..ticks {
        sb.step(1.0 / 60.0);
        let p = profile::take();
        all.push(sb.step_ms());
        if sb.step_ms() > slow {
            let parts: Vec<String> = NAMES
                .iter()
                .zip(p)
                .filter(|(_, v)| *v > 0)
                .map(|((slot, name), v)| {
                    if is_count(*slot) {
                        format!("{name} {v}")
                    } else {
                        format!("{name} {:.1}", v as f64 / 1e6)
                    }
                })
                .collect();
            eprintln!(
                "{scene:?} tick {k}: {:.1} ms | {}",
                sb.step_ms(),
                parts.join(", ")
            );
        }
    }
    let total: f64 = all.iter().sum();
    all.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let at = |q: f64| all[((all.len() - 1) as f64 * q) as usize];
    eprintln!(
        "{scene:?}: total {total:.0} ms, p50 {:.2}, p99 {:.1}, worst {:.1} ms",
        at(0.5),
        at(0.99),
        at(1.0)
    );
}

#[test]
#[ignore = "profile"]
fn profile_bridge() {
    report(PhysicsScene::Bridge, 900, 3.0);
}

#[test]
#[ignore = "profile"]
fn profile_rocks() {
    report(PhysicsScene::Rocks, 900, 12.0);
}

#[test]
#[ignore = "profile"]
fn profile_stacks() {
    report(PhysicsScene::Stacks, 900, 3.0);
}

/// The whole run's counts (`--features creeer_physics/profile`).
#[test]
#[ignore = "profile"]
fn profile_rocks_counts() {
    let mut sb = PhysicsSandbox::new(PhysicsScene::Rocks, 1.0 / 60.0);
    let _ = profile::take();
    let mut sums = [0u64; profile::SLOTS];
    for _ in 0..900 {
        sb.step(1.0 / 60.0);
        for (s, v) in sums.iter_mut().zip(profile::take()) {
            *s += v;
        }
    }
    let parts: Vec<String> = NAMES
        .iter()
        .zip(sums)
        .filter(|((slot, _), v)| *v > 0 && is_count(*slot))
        .map(|((_, name), v)| format!("{name} {v}"))
        .collect();
    eprintln!("Rocks over 15 s: {}", parts.join(", "));
}

/// How often a world substeps for its turn while its largest group turns slowly enough.
#[test]
#[ignore = "profile"]
fn profile_rock_turns() {
    use creeer_physics::world::shape::Shape;
    let mut sb = PhysicsSandbox::new(PhysicsScene::Rocks, 1.0 / 60.0);
    let (mut fast, mut largest_slow, mut saved, mut total) = (0, 0, 0usize, 0usize);
    let mut ticks: Vec<(f64, usize, usize)> = Vec::new();
    for _ in 0..900 {
        // The turns at the step's start, per body (boxes and hulls).
        let world = sb.world();
        let needs = |b: usize| -> usize {
            let body = &world.bodies[b];
            let turning = body
                .shapes
                .iter()
                .any(|(_, s)| matches!(s, Shape::Box { .. } | Shape::Hull(_)));
            if !turning || body.sleep.asleep {
                return 1;
            }
            let turn = body.structure.angular_velocity[0].length() / 60.0;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = ((turn / 0.05).ceil() as usize).clamp(1, 16);
            n
        };
        let n = (0..world.bodies.len()).map(needs).max().unwrap_or(1);
        let groups: Vec<Vec<usize>> = world.groups().map(<[usize]>::to_vec).collect();
        if n > 1 {
            fast += 1;
            if let Some(big) = groups.iter().max_by_key(|g| g.len()) {
                let m = big.iter().map(|&b| needs(b)).max().unwrap_or(1);
                if m == 1 {
                    largest_slow += 1;
                }
                // Group-steps a world substep costs against what per-group substeps would.
                total += groups.iter().map(|g| g.len() * n).sum::<usize>();
                saved += groups
                    .iter()
                    .map(|g| g.len() * (n - g.iter().map(|&b| needs(b)).max().unwrap_or(1)))
                    .sum::<usize>();
            }
        }
        let big_needs = groups
            .iter()
            .max_by_key(|g| g.len())
            .map_or(1, |g| g.iter().map(|&b| needs(b)).max().unwrap_or(1));
        sb.step(1.0 / 60.0);
        ticks.push((sb.step_ms(), n, big_needs));
    }
    ticks.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (ms, n, big) in ticks.iter().take(12) {
        println!("slow tick {ms:.1} ms: world substeps {n}, largest group needs {big}");
    }
    println!(
        "ticks that substep: {fast}; of them the largest group slow: {largest_slow}; body-steps saved by per-group substeps: {saved} of {total}"
    );
}

#[test]
#[ignore = "profile"]
fn profile_crash() {
    report(PhysicsScene::Crash, 300, 3.0);
}
