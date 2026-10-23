//! P3 (plan.md, performance track): the struck ticks of the bridge and the crash timed on a
//! busy machine. The scene is run to just before its impact, kept, and its struck ticks are
//! stepped again from that copy many times: each tick's best time is what it costs the
//! solver, without the other processes' share of the cores (`cargo test --release -p
//! creeer_client struck_ -- --ignored --nocapture`). The work (Newton iterations, impact
//! substeps) and the state's bits at the end are printed too: the same on every run.

use crate::physics_sandbox::{PhysicsSandbox, PhysicsScene};
use std::time::Instant;

/// Steps `scene` to tick `from`, then times ticks `from..to` from there `runs` times.
fn struck(scene: PhysicsScene, from: usize, to: usize, runs: usize) {
    let mut sb = PhysicsSandbox::new(scene, 1.0 / 60.0);
    for _ in 0..from {
        creeer_core::Simulation::step(&mut sb, 1.0 / 60.0);
    }
    let kept = sb.world().clone();
    let mut best = vec![f64::INFINITY; to - from];
    let mut hash = 0u64;
    let mut work = 0u64;
    for _ in 0..runs {
        let mut world = kept.clone();
        let done: u64 = world.bodies.iter().map(|b| b.solver.iterations_total).sum();
        for b in &mut best {
            let start = Instant::now();
            world.step();
            *b = b.min(start.elapsed().as_secs_f64() * 1e3);
        }
        work = world
            .bodies
            .iter()
            .map(|b| b.solver.iterations_total)
            .sum::<u64>()
            - done;
        hash = world
            .bodies
            .iter()
            .flat_map(|b| b.structure.position.iter())
            .flat_map(|p| p.to_array())
            .fold(0xcbf2_9ce4_8422_2325, |h: u64, v| {
                (h ^ v.to_bits()).wrapping_mul(0x0100_0000_01b3)
            });
    }
    let worst = best.iter().copied().fold(0.0, f64::max);
    let total: f64 = best.iter().sum();
    println!(
        "{scene:?} ticks {from}..{to}, best of {runs}: worst {worst:.2} ms, total {total:.1} ms, {work} iterations, state {hash:016x}"
    );
    let line: Vec<String> = best.iter().map(|b| format!("{b:.1}")).collect();
    println!("  per tick: {}", line.join(" "));
}

#[test]
#[ignore = "timing"]
fn struck_bridge() {
    struck(PhysicsScene::Bridge, 18, 40, 7);
}

#[test]
#[ignore = "timing"]
fn struck_crash() {
    struck(PhysicsScene::Crash, 0, 60, 7);
}
