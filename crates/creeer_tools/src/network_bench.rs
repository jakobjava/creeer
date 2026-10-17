//! F6 network solvers (`creeer_math::sparse`): the cost of a step of a large circuit (LU,
//! refactorised with the pivots kept, then solved) and of a large conduction network
//! (Cholesky), each one island.

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_core::TimingStats;
use creeer_math::sparse::{Assembly, Cholesky, Lu};

/// Budget for a 10,000-node circuit step on the minimum-spec CPU (docs/f6-plan.md).
const CIRCUIT_BUDGET_MS: f64 = 1.0;
/// Budget for a 20,000-node thermal step (at 20 Hz: three ticks' share of 0.5 ms).
const THERMAL_BUDGET_MS: f64 = 1.5;

/// A factory's wiring: a ring main of `feeders` feeders, each a radial line of machines, every
/// machine a small circuit (cable, breaker, a motor's windings to ground), and `sources`
/// generators on the ring as ideal voltage sources (MNA rows with a zero diagonal).
fn factory(feeders: usize, machines: usize, sources: usize, scale: f64) -> (Assembly, Vec<f64>) {
    let (mut positions, mut values) = (Vec::new(), Vec::new());
    let mut n = 0;
    let mut node = || {
        n += 1;
        n - 1
    };
    let wire =
        |positions: &mut Vec<(usize, usize)>, values: &mut Vec<f64>, i: usize, j: usize, g: f64| {
            positions.extend([(i, i), (j, j), (i, j), (j, i)]);
            values.extend([g, g, -g, -g]);
        };
    let ring: Vec<usize> = (0..feeders).map(|_| node()).collect();
    for f in 0..feeders {
        wire(
            &mut positions,
            &mut values,
            ring[f],
            ring[(f + 1) % feeders],
            200.0 * scale,
        );
        let mut previous = ring[f];
        for _ in 0..machines {
            // Along the feeder, then into the machine: cable, breaker, two windings to ground.
            let tap = node();
            wire(&mut positions, &mut values, previous, tap, 80.0 * scale);
            let (cable, breaker, a, b) = (node(), node(), node(), node());
            wire(&mut positions, &mut values, tap, cable, 20.0 * scale);
            wire(&mut positions, &mut values, cable, breaker, 1000.0 * scale);
            wire(&mut positions, &mut values, breaker, a, 5.0 * scale);
            wire(&mut positions, &mut values, a, b, 3.0 * scale);
            for k in [a, b] {
                positions.push((k, k));
                values.push(2.0 * scale);
            }
            previous = tap;
        }
    }
    let nodes = n;
    for s in 0..sources {
        let (at, branch) = (ring[(s * feeders) / sources], nodes + s);
        positions.extend([(at, branch), (branch, at)]);
        values.extend([1.0, 1.0]);
    }
    (Assembly::new(nodes + sources, &positions), values)
}

/// Machines' parts: `machines` clusters of `parts` parts (a part conducts to the next two and
/// to the machine's frame), every frame to the floor, the floor a line of slabs, and every
/// part to the air.
fn machine_parts(machines: usize, parts: usize, scale: f64) -> (Assembly, Vec<f64>) {
    let (mut positions, mut values) = (Vec::new(), Vec::new());
    let mut conduct = |i: usize, j: usize, g: f64| {
        positions.extend([(i, i), (j, j), (i, j), (j, i)]);
        values.extend([g, g, -g, -g]);
    };
    let floor = machines;
    let per = parts + 1;
    for m in 0..floor {
        if m + 1 < floor {
            conduct(m, m + 1, 40.0 * scale);
        }
        let frame = floor + m * per;
        conduct(m, frame, 10.0 * scale);
        for k in 1..per {
            let part = frame + k;
            conduct(frame, part, 2.0 * scale);
            for d in [1, 2] {
                if k + d < per {
                    conduct(part, part + d, 0.5 * scale);
                }
            }
        }
    }
    let n = floor + machines * per;
    let (mut positions, mut values) = (positions, values);
    for i in 0..n {
        positions.push((i, i));
        values.push(0.05 * scale);
    }
    (Assembly::new(n, &positions), values)
}

/// A wiring mesh: nodes on a side × side grid, wires to most neighbours, a load from every
/// node to ground, and `sources` ideal voltage sources (MNA rows with a zero diagonal).
fn circuit(side: usize, sources: usize, scale: f64) -> (Assembly, Vec<f64>) {
    let n = side * side;
    let id = |x: usize, y: usize| y * side + x;
    let (mut positions, mut values) = (Vec::new(), Vec::new());
    let mut wire = |i: usize, j: usize, g: f64| {
        positions.extend([(i, i), (j, j), (i, j), (j, i)]);
        values.extend([g, g, -g, -g]);
    };
    for y in 0..side {
        for x in 0..side {
            let i = id(x, y);
            // Most neighbours wired (a deterministic pattern leaves a quarter out).
            if x + 1 < side && (x * 7 + y * 3) % 4 != 0 {
                wire(i, id(x + 1, y), 50.0 * scale);
            }
            if y + 1 < side && (x * 5 + y * 11) % 4 != 0 {
                wire(i, id(x, y + 1), 50.0 * scale);
            }
        }
    }
    for i in 0..n {
        positions.push((i, i));
        values.push(0.01 * scale);
    }
    for s in 0..sources {
        let (node, branch) = ((s * 97) % n, n + s);
        positions.extend([(node, branch), (branch, node)]);
        values.extend([1.0, 1.0]);
    }
    (Assembly::new(n + sources, &positions), values)
}

/// A conduction sheet: side × side nodes, each to its neighbours and to the surroundings.
fn sheet(side: usize, scale: f64) -> (Assembly, Vec<f64>) {
    let id = |x: usize, y: usize| y * side + x;
    let (mut positions, mut values) = (Vec::new(), Vec::new());
    for y in 0..side {
        for x in 0..side {
            let i = id(x, y);
            positions.push((i, i));
            values.push(0.2 * scale);
            for (dx, dy) in [(1, 0), (0, 1)] {
                if x + dx < side && y + dy < side {
                    let j = id(x + dx, y + dy);
                    positions.extend([(i, i), (j, j), (i, j), (j, i)]);
                    values.extend([3.0 * scale, 3.0 * scale, -3.0 * scale, -3.0 * scale]);
                }
            }
        }
    }
    (Assembly::new(side * side, &positions), values)
}

/// Steps of an LU-solved network: values a little changed each step (same pattern), the
/// pivots kept, then a solve. Returns the timings, the unknowns and the entries of L and U.
fn lu_steps(assembly: &Assembly, values: &[f64], ticks: usize) -> (TimingStats, usize, usize) {
    let a = assembly.matrix(values);
    let mut lu = Lu::new(&a).expect("solvable");
    let (mut tick, mut b) = (0u32, vec![0.0; a.n]);
    let mut scaled = assembly.zeros::<f64>();
    let mut contributions = values.to_vec();
    let t = time_ticks(
        || {
            tick += 1;
            let s = 1.0 + 1e-3 * f64::from(tick % 10);
            for (c, v) in contributions.iter_mut().zip(values) {
                *c = v * s;
            }
            assembly.fill(&contributions, &mut scaled.values);
            lu.refactor(&scaled).expect("solvable");
            b.fill(1.0);
            lu.solve(&mut b);
        },
        ticks,
    );
    (t, a.n, lu.nnz())
}

/// The same with a Cholesky factorisation.
fn cholesky_steps(
    assembly: &Assembly,
    values: &[f64],
    ticks: usize,
) -> (TimingStats, usize, usize) {
    let a = assembly.matrix(values);
    let mut chol = Cholesky::new(&a).expect("positive definite");
    let (mut tick, mut b) = (0u32, vec![0.0; a.n]);
    let mut scaled = assembly.zeros::<f64>();
    let mut contributions = values.to_vec();
    let t = time_ticks(
        || {
            tick += 1;
            let s = 1.0 + 1e-3 * f64::from(tick % 10);
            for (c, v) in contributions.iter_mut().zip(values) {
                *c = v * s;
            }
            assembly.fill(&contributions, &mut scaled.values);
            chol.refactor(&scaled).expect("positive definite");
            b.fill(1.0);
            chol.solve(&mut b);
        },
        ticks,
    );
    (t, a.n, chol.nnz())
}

fn line(name: &str, (t, n, nnz): &(TimingStats, usize, usize)) {
    println!(
        "  {name:<34} {n:>6} unknowns, {nnz:>7} in the factor | {:.3} / {:.3}",
        p(t, 50.0),
        p(t, 99.0)
    );
}

/// Runs the network benchmarks.
pub fn run(report: &mut Report) {
    println!("F6 networks: one island, refactorised and solved each step (ms, p50 / p99)");
    // A factory's wiring: 2,000 machines on 10 feeders, 4 generators.
    let (assembly, values) = factory(10, 200, 4, 1.0);
    let wiring = lu_steps(&assembly, &values, 200);
    line("factory wiring (LU)", &wiring);
    report.metric(
        "network_circuit_10k_p50_ms",
        p(&wiring.0, 50.0),
        Better::Lower,
    );
    let projected = p(&wiring.0, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= CIRCUIT_BUDGET_MS,
        format!("10k-node circuit step projected to min-spec: {projected:.2} ms <= {CIRCUIT_BUDGET_MS} ms"),
    );
    // 1,000 machines of 20 parts on a floor.
    let (assembly, values) = machine_parts(1000, 20, 1.0);
    let thermal = cholesky_steps(&assembly, &values, 200);
    line("machines' parts (Cholesky)", &thermal);
    report.metric(
        "network_thermal_20k_p50_ms",
        p(&thermal.0, 50.0),
        Better::Lower,
    );
    let projected = p(&thermal.0, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= THERMAL_BUDGET_MS,
        format!("20k-node thermal step projected to min-spec: {projected:.2} ms <= {THERMAL_BUDGET_MS} ms"),
    );
    // Meshes (every node wired to its neighbours: a 2D grid): the direct solvers' worst case,
    // informational (large meshed thermal islands are solved iteratively, F6.2).
    let (assembly, values) = circuit(100, 100, 1.0);
    let mesh = lu_steps(&assembly, &values, 30);
    line("wiring mesh 100 x 100 (LU)", &mesh);
    report.metric("network_mesh_10k_p50_ms", p(&mesh.0, 50.0), Better::Info);
    let (assembly, values) = sheet(100, 1.0);
    let sheet = cholesky_steps(&assembly, &values, 30);
    line("conduction sheet 100 x 100 (Cholesky)", &sheet);
    report.metric("network_sheet_10k_p50_ms", p(&sheet.0, 50.0), Better::Info);
    println!();
}
