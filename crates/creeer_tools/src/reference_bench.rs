//! F6.8 reference load (plan.md §5): one 60 Hz tick of everything at once, against the 10 ms
//! tick on the minimum-spec CPU. 20,000 active beams (four 5,000-beam bridges, a lorry
//! crossing each, their islands in parallel); 8 players walking about a yard of crates; and
//! 1,000 machines across all domains, each domain at its own reference load (docs/f6-plan.md):
//! a factory hall's 20,000 thermal nodes at 20 Hz, a 10,000-node DC circuit and a 2,000-bus AC
//! grid, 1,000 cars' drivetrains, 500 furnaces' equilibria at 10 Hz, a town's 5,000-pipe mains
//! at their own rate (their step beside the ticks), 1,000 aero panels on 100 tumbling bodies,
//! 200 floating bodies, and 10,000 gates with 100 Lua scripts.
//!
//! The domains run one after another, each as parallel within as it is: the sum is what a
//! server's tick costs before any domain overlaps another.

use crate::spikes::{Better, DT, MIN_SPEC_CORE_FACTOR, Report, p};
use creeer_core::TimingStats;
use creeer_math::glam::DVec3;
use creeer_physics::structure::{ImplicitSolver, Scheme, Structure};
use creeer_physics::world::World;
use rayon::prelude::*;
use std::time::{Duration, Instant};

/// The tick on the minimum-spec CPU (plan.md §5), and the F6 domains' share of it
/// (docs/f6-plan.md, "All domains").
const TARGET_MS: f64 = 10.0;
const DOMAINS_MS: f64 = 5.0;
/// Ticks measured each way, after a second's warm-up.
const TICKS: usize = 300;

pub(crate) type Part<'a> = (&'static str, Box<dyn FnMut(u32) + Send + 'a>);

/// Four 5,000-beam bridges on their piers, each crossed by a 20 kN load at 20 m/s.
fn bridges() -> Box<dyn FnMut(u32) + Send> {
    use creeer_physics::structure::section::STEEL_S235;
    use creeer_physics::structure::shapes::{Base, space_frame};
    use creeer_physics::structure::{Section, statics};
    let tube = Section::round_tube(0.2, 0.01);
    let mut s = space_frame([130, 2, 4], 2.0, tube, STEEL_S235, Base::Every(10));
    statics::nonlinear(&mut s, 200);
    let lane = crate::solvers::lane(&s);
    let mut islands: Vec<(Structure, ImplicitSolver, usize)> = (0..4)
        .map(|k| {
            let solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
            (s.clone(), solver, k * lane.len() / 4)
        })
        .collect();
    Box::new(move |tick| {
        islands.par_iter_mut().for_each(|(s, solver, start)| {
            let travelled = f64::from(tick) * DT * 20.0 + *start as f64 * 2.0;
            let along = travelled % (lane.len() - 1) as f64;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = along as usize;
            let share = along - k as f64;
            s.load.fill(DVec3::ZERO);
            s.load[lane[k]].y = -20_000.0 * (1.0 - share);
            s.load[lane[k + 1]].y = -20_000.0 * share;
            solver.step(s);
        });
    })
}

/// Eight players walking circles round a yard of crates (ten stacks of ten).
fn players() -> Box<dyn FnMut(u32) + Send> {
    use creeer_sim::character;
    let mut world: World = crate::world_bench::stacks();
    let first = world.bodies.len();
    for k in 0..8 {
        let feet = DVec3::new(f64::from(k) * 16.0, 0.02, 30.0);
        world.add(character::body(feet, DVec3::Y, DT));
    }
    let mut reactions = Vec::new();
    Box::new(move |tick| {
        for k in 0..8 {
            let angle = f64::from(tick) * DT * 0.5 + f64::from(k);
            let (sin, cos) = (creeer_math::det::sin(angle), creeer_math::det::cos(angle));
            let walk = DVec3::new(cos, 0.0, sin) * 3.0;
            let index = first + k as usize;
            character::steer(
                &mut world,
                index,
                (walk, false),
                (DVec3::Y, None),
                &mut reactions,
            );
        }
        world.step();
        character::release(&mut world, &mut reactions);
    })
}

/// The domains' reference loads, each stepping as its domain does (at its rate).
pub(crate) fn machines<'a>() -> Vec<Part<'a>> {
    let mut parts: Vec<Part<'a>> = Vec::new();
    let mut hall = crate::thermal_bench::hall(500, 20, 1000, 10);
    parts.push((
        "thermal: 20,000 nodes at 20 Hz",
        Box::new(move |tick| {
            if tick % 3 == 0 {
                hall.step(0.05);
            }
        }),
    ));
    let (mut dc, _) = crate::electric_bench::factory(10, 333);
    let (mut ac, _) = crate::electric_bench::town(20, 100);
    for _ in 0..600 {
        dc.step(DT);
    }
    for _ in 0..300 {
        ac.step(DT);
    }
    parts.push((
        "electric: 10,000-node DC",
        Box::new(move |_| {
            dc.step(DT);
        }),
    ));
    parts.push((
        "electric: 2,000-bus AC",
        Box::new(move |_| {
            ac.step(DT);
        }),
    ));
    let mut cars: Vec<_> = (0..1000).map(crate::mech_bench::car).collect();
    parts.push((
        "drivetrains: 1,000",
        Box::new(move |tick| cars.par_iter_mut().for_each(|c| c.drive(tick))),
    ));
    let mut chem = crate::chem_bench::load();
    parts.push(("chemistry: 500 at 10 Hz", Box::new(move |_| chem())));
    let (mut mains, junctions) = crate::fluids_bench::mains();
    let mut tracer = mains.tracer(&[0.0, 288.0]);
    parts.push((
        "pipes: 5,000 at their rate",
        Box::new(move |tick| {
            crate::fluids_bench::mains_tick(&mut mains, &mut tracer, &junctions, tick);
        }),
    ));
    let (sky, mut air, mut frames) = crate::fluids_bench::aero();
    parts.push((
        "aero: 1,000 panels, 100 bodies",
        Box::new(move |_| {
            creeer_sim::aero::push(&sky, None, &mut air, &mut frames);
            air.step();
            creeer_sim::aero::settle(&mut air, &mut frames);
        }),
    ));
    let (sea, mut water, mut floaters) = crate::fluids_bench::sea();
    parts.push((
        "floating: 200 bodies",
        Box::new(move |_| {
            creeer_sim::sea::push(&sea, &mut water, &mut floaters);
            water.step();
            creeer_sim::sea::settle(&mut water, &mut floaters, DT);
        }),
    ));
    let (mut logic, sensors, _) = crate::logic_bench::controls();
    parts.push((
        "logic: 10,000 gates, 100 scripts",
        Box::new(move |tick| crate::logic_bench::tick(&mut logic, &sensors, tick)),
    ));
    parts
}

/// Runs the reference load.
pub fn run(report: &mut Report) {
    println!("F6.8 reference load: one tick of everything (ms, p50 / p99)");
    let mut parts: Vec<Part<'_>> = vec![
        ("structures: 20,000 beams", bridges()),
        ("8 players, 100 crates", players()),
    ];
    parts.extend(machines());
    let mut tick = 0u32;
    for _ in 0..60 {
        for (_, part) in &mut parts {
            part(tick);
        }
        tick += 1;
    }
    // One after another, each part timed alone.
    let mut spent = vec![Duration::ZERO; parts.len()];
    let mut alone = TimingStats::new(TICKS);
    for _ in 0..TICKS {
        let start = Instant::now();
        for ((_, part), spent) in parts.iter_mut().zip(&mut spent) {
            let at = Instant::now();
            part(tick);
            *spent += at.elapsed();
        }
        alone.push(start.elapsed());
        tick += 1;
    }
    for ((name, _), spent) in parts.iter().zip(&spent) {
        let mean = spent.as_secs_f64() * 1e3 / TICKS as f64;
        println!("  {name:<34} {mean:.3} ms a tick alone (mean)");
    }
    println!(
        "  one after another: {:.3} / {:.3}",
        p(&alone, 50.0),
        p(&alone, 99.0)
    );
    // The F6 domains alone, side by side: their share of the tick (docs/f6-plan.md: ≈ 5 ms).
    let domains = side_by_side(&mut parts[2..], &mut tick);
    let (p50, p99) = (p(&domains, 50.0), p(&domains, 99.0));
    println!("  the F6 domains side by side: {p50:.3} / {p99:.3}");
    report.metric("reference_domains_p50_ms", p50, Better::Lower);
    let projected = p50 / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= DOMAINS_MS,
        format!(
            "the F6 domains' share projected to min-spec: {projected:.2} ms <= {DOMAINS_MS} ms"
        ),
    );
    // Everything side by side.
    let stats = side_by_side(&mut parts, &mut tick);
    let (p50, p99) = (p(&stats, 50.0), p(&stats, 99.0));
    println!("  everything side by side: {p50:.3} / {p99:.3}");
    report.metric("reference_alone_p50_ms", p(&alone, 50.0), Better::Info);
    report.metric("reference_tick_p50_ms", p50, Better::Lower);
    report.metric("reference_tick_p99_ms", p99, Better::Info);
    // Not a budget the F6 domains can meet alone: the beams' solver takes three quarters of the
    // tick's work (ADR-0016, docs/future-improvements.md). Reported against plan.md's target.
    let projected = p50 / MIN_SPEC_CORE_FACTOR;
    println!(
        "  target (plan.md §5): the whole tick projected to min-spec {projected:.2} ms <= {TARGET_MS} ms -> {}",
        if projected <= TARGET_MS {
            "met"
        } else {
            "not met yet"
        }
    );
    println!();
}

/// Ticks of `parts` side by side: every part a task on the work-stealing pool (a server's
/// tick: its domains independent within it, coupled between ticks), each still parallel
/// within.
fn side_by_side(parts: &mut [Part<'_>], tick: &mut u32) -> TimingStats {
    let mut stats = TimingStats::new(TICKS);
    for _ in 0..TICKS {
        let start = Instant::now();
        rayon::scope(|s| {
            for (_, part) in parts.iter_mut() {
                let at = *tick;
                s.spawn(move |_| part(at));
            }
        });
        stats.push(start.elapsed());
        *tick += 1;
    }
    stats
}
