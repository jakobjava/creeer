//! F2 solver comparison (ADR-0004): accuracy against the exact references and the cost of
//! one 60 Hz tick for a single 5,000-beam structure.

use crate::spikes::{Better, DT, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_physics::spike::{
    BridgeSpec, ImplicitTruss, Scheme, Truss, VbdTruss, explicit_reference_response,
    linear_static_sag, oscillation, rayleigh_frequency,
};

/// ADR-0002 budget: ms per tick for 5,000 beams in one island on the minimum-spec CPU.
const SINGLE_ISLAND_BUDGET_MS: f64 = 4.0;
/// Four seconds of response: about seven periods of the first mode.
const TICKS: usize = 240;

/// Frequency ratio and damping ratio of a simulated response to switching on gravity.
fn dynamics(mut step: impl FnMut() -> f64) -> Option<(f64, f64)> {
    let spec = BridgeSpec::REFERENCE;
    let samples: Vec<f64> = (0..TICKS).map(|_| step()).collect();
    let reference = oscillation(
        &explicit_reference_response(spec, TICKS),
        DT,
        linear_static_sag(spec),
    )?;
    let o = oscillation(&samples, DT, linear_static_sag(spec))?;
    Some((o.frequency / reference.frequency, o.damping_ratio))
}

fn print_dynamics(name: &str, result: Option<(f64, f64)>) {
    match result {
        Some((ratio, zeta)) => println!(
            "  {name:<28} | {:>+8.1} % | {:>9.2} %",
            (ratio - 1.0) * 100.0,
            zeta * 100.0
        ),
        None => println!("  {name:<28} | no clean oscillation (falls through / overdamped)"),
    }
}

pub fn run(report: &mut Report) {
    let spec = BridgeSpec::REFERENCE;
    println!(
        "F2 solver comparison: first vertical mode of the reference bridge (Rayleigh {:.3} Hz)",
        rayleigh_frequency(spec)
    );
    println!("  candidate                    | frequency | damping ζ");
    let bridge = || Truss::box_bridge(Default::default(), spec);
    print_dynamics(
        "explicit reference",
        dynamics({
            let samples = explicit_reference_response(spec, TICKS);
            let mut i = 0;
            move || {
                i += 1;
                samples[i - 1]
            }
        }),
    );
    for substeps in [20, 100] {
        let mut t = bridge();
        print_dynamics(
            &format!("XPBD {substeps} substeps"),
            dynamics(|| {
                t.step(DT, substeps);
                t.midspan_sag()
            }),
        );
    }
    for iterations in [50, 200] {
        let mut v = VbdTruss::new(bridge(), DT, 1, iterations);
        print_dynamics(
            &format!("VBD {iterations} iterations"),
            dynamics(|| {
                v.step();
                v.truss.midspan_sag()
            }),
        );
    }
    for (scheme, substeps) in [
        (Scheme::BackwardEuler, 1),
        (Scheme::Bdf2, 1),
        (Scheme::Bdf2, 2),
    ] {
        let mut s = ImplicitTruss::new(bridge(), scheme, DT, substeps);
        let result = dynamics(|| {
            s.step();
            s.truss.midspan_sag()
        });
        print_dynamics(&format!("implicit {scheme:?} x{substeps}"), result);
        if scheme == Scheme::Bdf2 && substeps == 1 {
            // No clean oscillation at all counts as failing the budget.
            let error = result.map_or(f64::INFINITY, |(ratio, _)| ratio - 1.0);
            report.metric(
                "implicit_bdf2_frequency_error_pct",
                error.abs() * 100.0,
                Better::Lower,
            );
            report.budget(
                error.abs() < 0.05,
                format!("BDF2 first frequency within ±5 %: {:+.2} %", error * 100.0),
            );
        }
    }

    println!("\n  cost of one tick, single island (ms, p50 / p99):");
    let space_frame = Truss::space_frame(23);
    println!(
        "  space-frame roof: {} beams, {} nodes",
        space_frame.beam_count(),
        space_frame.node_count()
    );
    let mut xpbd = space_frame.clone();
    let x = time_ticks(|| xpbd.step(DT, 100), 50);
    println!(
        "  XPBD 100 substeps            | {:.3} / {:.3}",
        p(&x, 50.0),
        p(&x, 99.0)
    );
    let mut vbd = VbdTruss::new(space_frame.clone(), DT, 1, 200);
    let v = time_ticks(|| vbd.step(), 30);
    println!(
        "  VBD 200 iterations           | {:.3} / {:.3}",
        p(&v, 50.0),
        p(&v, 99.0)
    );
    for substeps in [1, 2] {
        let mut sim = ImplicitTruss::new(space_frame.clone(), Scheme::Bdf2, DT, substeps);
        let s = time_ticks(|| sim.step(), 300);
        println!(
            "  {:<28} | {:.3} / {:.3}",
            format!("implicit BDF2 x{substeps}"),
            p(&s, 50.0),
            p(&s, 99.0)
        );
        if substeps == 1 {
            report.metric(
                "implicit_bdf2_5k_single_island_p50_ms",
                p(&s, 50.0),
                Better::Lower,
            );
            let projected = p(&s, 99.0) / MIN_SPEC_CORE_FACTOR;
            report.budget(
                projected <= SINGLE_ISLAND_BUDGET_MS,
                format!(
                    "implicit BDF2, 5k beams in one island, p99 projected to min-spec: {projected:.2} ms <= {SINGLE_ISLAND_BUDGET_MS} ms"
                ),
            );
        }
    }
    println!("  (findings and decision: docs/adr/0004-structural-dynamics-solver.md)\n");
}

/// F2 beam structures (6 degrees of freedom per node, welded joints): cost of one tick for
/// a structure in its static equilibrium, disturbed by a 20 kN load moving along it.
pub fn run_structures(report: &mut Report) {
    use creeer_physics::structure::section::STEEL_S235;
    use creeer_physics::structure::shapes::{Base, space_frame};
    use creeer_physics::structure::{ImplicitSolver, Scheme as StructureScheme, Section, statics};
    println!("F2 beam structures: one island, Newmark, one step per tick (ms, p50 / p99)");
    let tube = Section::round_tube(0.2, 0.01);
    // Real supports: a bridge on piers every 20 m, a building on columns every 12 m (held
    // only at its corners, a 260 m steel bridge of these tubes collapses under its own weight,
    // which the solver, now with plasticity, faithfully shows).
    for (name, cells, base, metric) in [
        (
            "bridge 260 x 4 x 8 m, piers",
            [130, 2, 4],
            Base::Every(10),
            Some("structure_bridge_5k_p50_ms"),
        ),
        (
            "tower 8 x 120 x 8 m",
            [4, 60, 4],
            Base::Corners,
            Some("structure_tower_4k_p50_ms"),
        ),
        ("chassis 24 x 4 x 8 m", [12, 2, 4], Base::Corners, None),
        (
            "welded block 36 x 8 x 36 m",
            [18, 4, 18],
            Base::Every(6),
            Some("structure_block_5k_p50_ms"),
        ),
    ] {
        let mut s = space_frame(cells, 2.0, tube, STEEL_S235, base);
        statics::nonlinear(&mut s, 200);
        let mut solver = ImplicitSolver::new(&s, StructureScheme::Newmark, DT, 1);
        let lane = lane(&s);
        let mut tick = 0u32;
        let mut sum = creeer_physics::structure::Timings::default();
        let mut iterations = 0u32;
        let mut ticks = 0u32;
        let t = time_ticks(
            || {
                // A 20 kN load (about a 2 t vehicle) driving at 20 m/s along the lane,
                // shared between the two nodes around it.
                let travelled = f64::from(tick) * DT * 20.0;
                let along = travelled % (lane.len() - 1) as f64;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let k = along as usize;
                let share = along - k as f64;
                s.load.fill(Default::default());
                s.load[lane[k]].y = -20_000.0 * (1.0 - share);
                s.load[lane[k + 1]].y = -20_000.0 * share;
                tick += 1;
                solver.step(&mut s);
                let t = solver.timings;
                sum.guard += t.guard;
                sum.predict += t.predict;
                sum.residual += t.residual;
                sum.objective += t.objective;
                sum.solve += t.solve;
                sum.total += t.total;
                iterations += solver.last_iterations;
                ticks += 1;
            },
            100,
        );
        println!(
            "  {name:<28} {:>5} beams | {:.3} / {:.3}",
            s.element_count(),
            p(&t, 50.0),
            p(&t, 99.0)
        );
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0 / f64::from(ticks);
        println!(
            "    mean per tick: {:.2} iterations | energy guard {:.3}, prediction {:.3}, forces {:.3}, line search {:.3}, solves {:.3} ms",
            f64::from(iterations) / f64::from(ticks),
            ms(sum.guard),
            ms(sum.predict),
            ms(sum.residual),
            ms(sum.objective),
            ms(sum.solve)
        );
        if let Some(metric) = metric {
            report.metric(metric, p(&t, 50.0), Better::Lower);
        }
        if cells == [130, 2, 4] {
            let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
            report.budget(
                projected <= SINGLE_ISLAND_BUDGET_MS,
                format!("5k-beam bridge, p50 projected to min-spec: {projected:.2} ms <= {SINGLE_ISLAND_BUDGET_MS} ms"),
            );
        }
    }
    println!();
}

/// Top-layer nodes along the structure's longest horizontal axis, at the middle of the
/// other one: where a vehicle would drive (nodes 2 m apart in these lattices).
fn lane(s: &creeer_physics::structure::Structure) -> Vec<usize> {
    let top = s
        .position
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let (lo, hi) = s.position.iter().fold(
        (
            f64::INFINITY * creeer_math::glam::DVec3::ONE,
            f64::NEG_INFINITY * creeer_math::glam::DVec3::ONE,
        ),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let (along, across) = if hi.x - lo.x >= hi.z - lo.z {
        (0, 2)
    } else {
        (2, 0)
    };
    let middle = 0.5 * (lo[across] + hi[across]);
    let mut nodes: Vec<usize> = (0..s.node_count())
        .filter(|&i| s.position[i].y > top - 0.5)
        .collect();
    let nearest = nodes
        .iter()
        .map(|&i| (s.position[i][across] - middle).abs())
        .fold(f64::INFINITY, f64::min);
    nodes.retain(|&i| (s.position[i][across] - middle).abs() <= nearest + 1e-9);
    nodes.sort_by(|&a, &b| s.position[a][along].total_cmp(&s.position[b][along]));
    nodes
}
