//! The physics sandbox's scenes against their budgets (performance and maximum realism,
//! together: plan.md §1): every tick's time and work.
//!
//! - **Work** (CI, `physics_bench_tests.rs`): the Newton iterations a tick costs, summed over
//!   the bodies. Deterministic, the same on every machine: no tick may cost more than its
//!   scene's ceiling.
//! - **Time** (`creeer_client --physics-bench [--metrics <file>]`, run by `cargo xtask perf`):
//!   each scene's total, p99 and worst tick (ms, on this machine), the best of three runs
//!   (a run's work is the same each time; what differs is the machine's), against ceilings
//!   that come down as the solver gets faster (`docs/future-improvements.md`'s target: 2–3 ms
//!   a tick on the development machine).

use crate::physics_sandbox::{PhysicsSandbox, PhysicsScene};
use creeer_core::Simulation;
use std::fmt::Write as _;

/// The tick.
const DT: f64 = 1.0 / 60.0;

/// What one run of a scene cost: each tick's time (ms) and work (Newton iterations).
pub struct Run {
    pub ms: Vec<f64>,
    pub work: Vec<u64>,
}

/// Runs `scene` for `ticks` ticks.
pub fn run(scene: PhysicsScene, ticks: usize) -> Run {
    let mut sandbox = PhysicsSandbox::new(scene, DT);
    let (mut ms, mut work) = (Vec::with_capacity(ticks), Vec::with_capacity(ticks));
    let mut done = sandbox.iterations();
    for _ in 0..ticks {
        sandbox.step(DT);
        let now = sandbox.iterations();
        ms.push(sandbox.step_ms());
        work.push(now.saturating_sub(done));
        done = now;
    }
    Run { ms, work }
}

/// The scenes the bench times, their names in metrics, and their ceilings (ms on the
/// development machine): the worst tick and the p99 tick. Set 2026-10-18 a quarter above
/// what they measured (best of three: worst 38, 20.7, 7.0, 38 ms; p99 6.8, 15.9, 2.0, 2.8 ms),
/// and brought down with every gain on the way to 2–3 ms.
pub const TIMED: [(PhysicsScene, &str, f64, f64); 4] = [
    (PhysicsScene::Bridge, "bridge", 48.0, 9.0),
    (PhysicsScene::Rocks, "rocks", 26.0, 20.0),
    (PhysicsScene::Stacks, "stacks", 9.0, 2.6),
    (PhysicsScene::Crash, "crash", 48.0, 3.6),
];

/// Seconds each scene is timed for.
const SECONDS: usize = 15;

/// `creeer_client --physics-bench [--metrics <file>]`: times the scenes, appends their
/// metrics to the file (`name value better` lines, as `creeer_tools bench-spikes` writes),
/// and fails if any misses its ceilings.
pub fn bench(args: &[String]) -> Result<(), String> {
    let metrics = args
        .iter()
        .position(|a| a == "--metrics")
        .and_then(|i| args.get(i + 1));
    let mut lines = String::new();
    let mut missed = Vec::new();
    println!("physics sandbox scenes, {SECONDS} s each, best of three (ms):");
    for (scene, name, worst_ceiling, p99_ceiling) in TIMED {
        let runs: Vec<Run> = (0..3).map(|_| run(scene, SECONDS * 60)).collect();
        let stat = |run: &Run, q: f64| {
            let mut sorted = run.ms.clone();
            sorted.sort_by(f64::total_cmp);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            sorted[((sorted.len() - 1) as f64 * q) as usize]
        };
        let best = |f: &dyn Fn(&Run) -> f64| runs.iter().map(f).fold(f64::INFINITY, f64::min);
        let total = best(&|r| r.ms.iter().sum());
        let p99 = best(&|r| stat(r, 0.99));
        let worst = best(&|r| stat(r, 1.0));
        let work = runs[0].work.iter().copied().max().unwrap_or(0);
        println!(
            "  {name:<8} total {total:7.0} | p99 {p99:6.2} (≤ {p99_ceiling}) | worst {worst:6.2} (≤ {worst_ceiling}) | most work a tick {work}"
        );
        let _ = writeln!(lines, "physics_{name}_total_ms {total:.3} lower");
        let _ = writeln!(lines, "physics_{name}_p99_ms {p99:.3} lower");
        let _ = writeln!(lines, "physics_{name}_worst_ms {worst:.3} info");
        if worst > worst_ceiling || p99 > p99_ceiling {
            missed.push(name);
        }
    }
    if let Some(path) = metrics {
        let mut text = std::fs::read_to_string(path).unwrap_or_default();
        text.push_str(&lines);
        std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?;
    }
    if missed.is_empty() {
        println!("  budgets: every scene within its ceilings -> OK");
        Ok(())
    } else {
        Err(format!(
            "physics scenes over their ceilings: {}",
            missed.join(", ")
        ))
    }
}

#[cfg(test)]
#[path = "physics_bench_tests.rs"]
mod tests;
