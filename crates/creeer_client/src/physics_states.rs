//! (P1, ADR-0024.) The sandbox's piles stepped on the same states by any build, run by hand:
//! `PHYSICS_STATES=<file> cargo test --release -p creeer_client states_written -- --ignored
//! --nocapture` once (every tick of a scene's run, as saved), then `states_stepped` by each
//! build to compare. Whole runs of a pile are chaotic: any change sends them elsewhere, and
//! two builds' runs compare different piles (ADR-0017's method). `SCENE=rocks|stacks` (rocks
//! if unset); `ONLY=<state>` steps that one alone (with `--features creeer_physics/profile`,
//! its phases).

use crate::physics_sandbox::{PhysicsSandbox, PhysicsScene};
use creeer_core::Simulation;
use creeer_physics::world::World;

const DT: f64 = 1.0 / 60.0;

fn scene() -> (PhysicsScene, usize) {
    match std::env::var("SCENE").as_deref() {
        Ok("stacks") => (PhysicsScene::Stacks, 900),
        _ => (PhysicsScene::Rocks, 900),
    }
}

/// Every tick's state of the scene's run, written to `PHYSICS_STATES`.
#[test]
#[ignore = "a bench, run by hand"]
fn states_written() {
    let path = std::env::var("PHYSICS_STATES").expect("PHYSICS_STATES=<file>");
    let (scene, ticks) = scene();
    let mut sb = PhysicsSandbox::new(scene, DT);
    let mut w = creeer_math::codec::Writer::new();
    w.varint(ticks as u64);
    for _ in 0..ticks {
        let bytes = sb.world().to_bytes();
        w.varint(bytes.len() as u64);
        w.raw(&bytes);
        sb.step(DT);
    }
    std::fs::write(path, w.finish()).unwrap();
    eprintln!("{ticks} states written");
}

/// Each state of `PHYSICS_STATES` stepped once: the time in all, the worst step and p99, the
/// Newton iterations (each step's most of any body) in all, and the slowest eight (ms, state,
/// iterations).
#[test]
#[ignore = "a bench, run by hand"]
fn states_stepped() {
    let path = std::env::var("PHYSICS_STATES").expect("PHYSICS_STATES=<file>");
    let bytes = std::fs::read(path).unwrap();
    let mut r = creeer_math::codec::Reader::new(&bytes);
    let n = usize::try_from(r.varint().unwrap()).unwrap();
    let states: Vec<Vec<u8>> = (0..n)
        .map(|_| r.bytes_with_length().unwrap().to_vec())
        .collect();
    let only: Option<usize> = std::env::var("ONLY").ok().map(|s| s.parse().unwrap());
    // The pool awake before the first timed step (its first wake-up is not the physics').
    let mut warm = World::from_bytes(&states[n / 2]).unwrap();
    warm.step();
    let _ = creeer_physics::profile::take();
    let (mut total, mut its, mut work) = (0.0, 0u64, (0u64, 0usize));
    let mut steps: Vec<(f64, usize, u32)> = Vec::new();
    for (k, state) in states.iter().enumerate() {
        if only.is_some_and(|o| o != k) {
            continue;
        }
        let mut world = World::from_bytes(state).unwrap();
        let before: u64 = world.bodies.iter().map(|b| b.solver.iterations_total).sum();
        let start = std::time::Instant::now();
        world.step();
        let after: u64 = world.bodies.iter().map(|b| b.solver.iterations_total).sum();
        work = work.max((after - before, k));
        let ms = start.elapsed().as_secs_f64() * 1e3;
        total += ms;
        let most = (world.bodies.iter())
            .map(|b| b.solver.last_iterations)
            .max()
            .unwrap_or(0);
        its += u64::from(most);
        steps.push((ms, k, most));
    }
    // `DUMP=<file>`: each state's (state, iterations, ms), for comparing builds state by state.
    if let Ok(dump) = std::env::var("DUMP") {
        let lines: Vec<String> = (steps.iter())
            .map(|(ms, k, its)| format!("{k} {its} {ms:.3}"))
            .collect();
        std::fs::write(dump, lines.join("\n")).unwrap();
    }
    steps.sort_by(|a, b| b.0.total_cmp(&a.0));
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let p99 = steps[((steps.len() - 1) as f64 * 0.01) as usize].0;
    eprintln!("slowest {:?}", &steps[..steps.len().min(8)]);
    eprintln!(
        "{} states stepped: {total:.0} ms in all, p99 {p99:.2} ms, worst {:.2} ms, {its} iterations",
        steps.len(),
        steps[0].0
    );
    eprintln!(
        "most work in a step (iterations summed over bodies): {} (state {})",
        work.0, work.1
    );
    let spent = creeer_physics::profile::take();
    let phases: Vec<String> = (creeer_physics::profile::NAMES.iter())
        .filter(|(s, _)| spent[*s as usize] > 0)
        .map(|(s, n)| format!("{n} {}", spent[*s as usize]))
        .collect();
    if !phases.is_empty() {
        eprintln!("  phases (ns or counts): {}", phases.join(", "));
    }
}

/// The scene's most work in a tick (Newton iterations summed over its bodies) over its first
/// 240 ticks, for eight runs whose bodies start a nanometre apart (`SCENE`; rocks if unset): a
/// pile is chaotic, and one run's most work says little of a change (G1.8).
#[test]
#[ignore = "a bench, run by hand"]
fn work_ensemble() {
    let (scene, _) = scene();
    let mut most = Vec::new();
    for k in 0..8u32 {
        let mut sb = PhysicsSandbox::new(scene, DT);
        for b in &mut sb.world_mut().bodies {
            for p in &mut b.structure.position {
                p.x += f64::from(k) * 1e-9;
            }
        }
        let (mut done, mut worst) = (sb.iterations(), 0);
        for _ in 0..240 {
            sb.step(DT);
            let now = sb.iterations();
            worst = worst.max(now - done);
            done = now;
        }
        most.push(worst);
    }
    eprintln!("most work in a tick, eight runs: {most:?}");
    most.sort_unstable();
    eprintln!("median {}, worst {}", most[4], most[7]);
}
