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
