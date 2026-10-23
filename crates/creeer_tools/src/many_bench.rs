//! P2 (plan.md, performance track): many structures stepping at once. Eight 5,000-beam
//! bridges, one per core (each on a pool of one thread, as a server's islands are spread),
//! against one alone on one thread: what each step costs when the structures share the
//! machine's caches and memory (ADR-0021: bound by memory). Target: at most 1.3× alone.

use crate::spikes::{Better, DT, Report};
use creeer_math::glam::DVec3;
use creeer_physics::structure::{ImplicitSolver, Scheme, Structure, Timings};
use std::sync::Barrier;
use std::time::{Duration, Instant};

/// Steps measured per bridge, after a warm-up.
const STEPS: u32 = 120;
const WARM_UP: u32 = 20;
/// The bridges stepping at once.
const AT_ONCE: usize = 8;
/// plan.md's target for P2.
const TARGET_RATIO: f64 = 1.3;

/// A bridge on its piers with its lane, settled under its weight.
fn bridge() -> (Structure, Vec<usize>) {
    use creeer_physics::structure::section::STEEL_S235;
    use creeer_physics::structure::shapes::{Base, space_frame};
    use creeer_physics::structure::{Section, statics};
    let tube = Section::round_tube(0.2, 0.01);
    let mut s = space_frame([130, 2, 4], 2.0, tube, STEEL_S235, Base::Every(10));
    statics::nonlinear(&mut s, 200);
    let lane = crate::solvers::lane(&s);
    (s, lane)
}

/// What the steps of one bridge cost: the mean step and its parts.
#[derive(Default)]
struct Cost {
    total: Duration,
    parts: Timings,
}

/// Steps one bridge on a pool of one thread, its load (20 kN at 20 m/s) starting `offset`
/// nodes along the lane; with a `barrier`, every step starts with the others'.
fn run(s: &Structure, lane: &[usize], offset: usize, barrier: Option<&Barrier>) -> Cost {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .expect("a pool of one thread");
    pool.install(|| {
        let mut s = s.clone();
        let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
        let mut cost = Cost::default();
        for tick in 0..WARM_UP + STEPS {
            let travelled = f64::from(tick) * DT * 20.0 + offset as f64 * 2.0;
            let along = travelled % (lane.len() - 1) as f64;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = along as usize;
            let share = along - k as f64;
            s.load.fill(DVec3::ZERO);
            s.load[lane[k]].y = -20_000.0 * (1.0 - share);
            s.load[lane[k + 1]].y = -20_000.0 * share;
            if let Some(b) = barrier {
                b.wait();
            }
            let start = Instant::now();
            solver.step(&mut s);
            if tick >= WARM_UP {
                cost.total += start.elapsed();
                let t = solver.timings;
                let p = &mut cost.parts;
                p.guard += t.guard;
                p.predict += t.predict;
                p.residual += t.residual;
                p.objective += t.objective;
                p.solve += t.solve;
            }
        }
        cost
    })
}

fn print(name: &str, costs: &[Cost]) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let per = f64::from(STEPS) * costs.len() as f64;
    let ms = |f: &dyn Fn(&Cost) -> Duration| {
        costs.iter().map(|c| f(c).as_secs_f64()).sum::<f64>() * 1e3 / per
    };
    let total = ms(&|c| c.total);
    println!(
        "  {name:<34} {total:.3} ms a step | guard {:.3}, prediction {:.3}, forces {:.3}, line search {:.3}, solves {:.3}",
        ms(&|c| c.parts.guard),
        ms(&|c| c.parts.predict),
        ms(&|c| c.parts.residual),
        ms(&|c| c.parts.objective),
        ms(&|c| c.parts.solve),
    );
    total
}

/// Runs the comparison and reports the ratio.
pub fn run_many(report: &mut Report) {
    println!("P2: 5,000-beam bridges stepping at once, one thread each (ms a step, mean)");
    let (s, lane) = bridge();
    let alone = print("one alone", &[run(&s, &lane, 0, None)]);
    let barrier = Barrier::new(AT_ONCE);
    let costs: Vec<Cost> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..AT_ONCE)
            .map(|k| {
                let (s, lane, barrier) = (&s, &lane, &barrier);
                scope.spawn(move || run(s, lane, k * lane.len() / AT_ONCE, Some(barrier)))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a bridge's thread"))
            .collect()
    });
    let many = print("eight at once, each", &costs);
    let ratio = many / alone;
    println!("  ratio {ratio:.2} (target <= {TARGET_RATIO})");
    // What the memory gives: a factor's size streamed by one thread, and by eight at once
    // (each its own), the bound the eight bridges' solves meet (ADR-0035).
    let (one, eight) = (bandwidth(1), bandwidth(AT_ONCE));
    println!(
        "  streaming 12 MB arrays: one thread {one:.0} GB/s, eight at once {eight:.0} GB/s together"
    );
    report.metric("structure_bridge_alone_1t_ms", alone, Better::Lower);
    report.metric("structure_bridges_8_ratio", ratio, Better::Info);
    println!();
}

/// GB/s that `threads` threads read together, each summing its own 12 MB array (a bridge's
/// factor) over and over.
fn bandwidth(threads: usize) -> f64 {
    const VALUES: usize = 12 << 17; // 12 MB of f64
    const ROUNDS: usize = 20;
    let barrier = Barrier::new(threads);
    let seconds: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|k| {
                let barrier = &barrier;
                scope.spawn(move || {
                    #[allow(clippy::cast_precision_loss)]
                    let data: Vec<f64> = (0..VALUES).map(|i| (i + k) as f64).collect();
                    let mut sum = 0.0;
                    barrier.wait();
                    let start = Instant::now();
                    for _ in 0..ROUNDS {
                        // Four sums, so that the adds' latency does not bound the reads.
                        let mut s = [0.0; 4];
                        for c in data.as_chunks::<4>().0 {
                            for (s, v) in s.iter_mut().zip(c) {
                                *s += v;
                            }
                        }
                        sum += s.iter().sum::<f64>();
                    }
                    std::hint::black_box(sum);
                    start.elapsed().as_secs_f64()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a reader"))
            .collect()
    });
    let longest = seconds.iter().copied().fold(0.0, f64::max);
    #[allow(clippy::cast_precision_loss)]
    let bytes = (threads * VALUES * ROUNDS * 8) as f64;
    bytes / longest / 1e9
}
