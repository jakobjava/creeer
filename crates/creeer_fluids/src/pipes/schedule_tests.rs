//! Each network's own rate: a small one steps every tick, a 5,000-pipe meshed grid picks a
//! slower rate that keeps it within its budget; the choice is the structure's alone.

use super::schedule::{BUDGET_NS, RATES};
use super::{LinkKind, Liquid, Network, NodeKind};

const G: f64 = 9.80665;
const DT: f64 = 1.0 / 60.0;

fn pipe(length: f64, diameter: f64) -> LinkKind {
    LinkKind::Pipe {
        length,
        diameter,
        roughness: 1e-4,
        minor: 0.5,
        check: false,
    }
}

/// A house's water: the mains, a pump up to a roof tank, twenty taps on a branching line.
fn house() -> Network {
    let mut net = Network::new(Liquid::WATER_20C, G);
    let mains = net.node(0.0, NodeKind::Reservoir, 30.0);
    let tank = net.node(
        12.0,
        NodeKind::Tank {
            area: 1.0,
            floor: 12.0,
        },
        12.5,
    );
    let pump = LinkKind::Pump {
        shutoff: 25.0,
        resistance: 4e4,
        speed: 1.0,
        best_flow: 0.002,
        best_efficiency: 0.6,
    };
    net.link(mains, tank, pump);
    let mut last = tank;
    for k in 0..20 {
        let tap = net.node(
            10.0 - f64::from(k % 5),
            NodeKind::Junction { demand: 5e-5 },
            0.0,
        );
        net.link(if k % 4 == 0 { tank } else { last }, tap, pipe(3.0, 0.02));
        last = tap;
    }
    net
}

/// A town's mains: a 50 × 50 grid of junctions, 4,900 pipes, fed by two pumped reservoirs and
/// a tank (as the fluids bench has it).
pub(super) fn town() -> Network {
    let mut net = Network::new(Liquid::WATER_20C, G);
    let n = 50;
    let junctions: Vec<usize> = (0..n * n)
        .map(|k| {
            net.node(
                f64::from(u8::try_from(k % 7).expect("small")) * 2.0,
                NodeKind::Junction { demand: 2e-4 },
                30.0,
            )
        })
        .collect();
    for i in 0..n {
        for j in 0..n {
            let k = i * n + j;
            if j + 1 < n {
                net.link(junctions[k], junctions[k + 1], pipe(120.0, 0.15));
            }
            if i + 1 < n {
                net.link(junctions[k], junctions[k + n], pipe(120.0, 0.15));
            }
        }
    }
    let pump = LinkKind::Pump {
        shutoff: 60.0,
        resistance: 400.0,
        speed: 1.0,
        best_flow: 0.3,
        best_efficiency: 0.8,
    };
    for corner in [0, n * n - 1] {
        let lake = net.node(0.0, NodeKind::Reservoir, 0.0);
        net.link(lake, junctions[corner], pump);
    }
    let tank = net.node(
        40.0,
        NodeKind::Tank {
            area: 400.0,
            floor: 40.0,
        },
        45.0,
    );
    net.link(tank, junctions[n * n / 2 + n / 2], pipe(200.0, 0.4));
    net
}

/// A house's 21 links fit their share at 60 Hz: the network steps every tick.
#[test]
fn a_small_network_steps_every_tick() {
    let mut net = house();
    assert_eq!(
        net.ticks_per_step(),
        1,
        "cost {} ns, share {} ns",
        net.estimated_cost(),
        net.budget_share()
    );
    for _ in 0..10 {
        let (report, h) = net.tick(DT).expect("every tick");
        assert!(report.converged);
        assert_eq!(h, DT);
    }
}

/// The 4,900-pipe grid's step does not fit its share every tick; it picks a slower rate (20 or
/// 10 Hz) whose cost a tick is within its share of the 1 ms budget, and steps on every third
/// or sixth tick by that interval.
#[test]
fn a_large_meshed_grid_picks_a_slower_rate_within_its_budget() {
    let mut net = town();
    let every = net.ticks_per_step();
    let (cost, share) = (net.estimated_cost(), net.budget_share());
    assert!(every > 1 && RATES.contains(&every), "every {every} ticks");
    assert!(
        cost > share,
        "too costly for every tick: {cost} ns against {share} ns"
    );
    assert!(
        cost / f64::from(every) <= share && share <= BUDGET_NS,
        "{cost} ns every {every} ticks, share {share} ns"
    );
    let stepped: Vec<bool> = (0..2 * every).map(|_| net.tick(DT).is_some()).collect();
    for (k, &s) in stepped.iter().enumerate() {
        assert_eq!(s, k % every as usize == 0, "tick {k}");
    }
}

/// The rate is the structure's: the same network built again picks the same; a link added
/// has it worked out again.
#[test]
fn the_rate_is_the_structures_alone() {
    let (mut a, mut b) = (town(), town());
    assert_eq!(a.ticks_per_step(), b.ticks_per_step());
    assert_eq!(a.estimated_cost().to_bits(), b.estimated_cost().to_bits());
    let mut small = house();
    assert_eq!(small.ticks_per_step(), 1);
    let (x, y) = (small.links[1].b, small.links[2].b);
    let before = small.estimated_cost();
    small.link(x, y, pipe(2.0, 0.02));
    assert!(small.solver.is_none(), "a new link starts the solver again");
    assert!(small.estimated_cost() > before);
}

#[test]
#[ignore = "calibration: prints measured step times against the structure's counts"]
fn calibrate() {
    for (name, mut net) in [("house", house()), ("town", town())] {
        let mut tracer = net.tracer(&[0.0, 288.0]);
        let mut solver = net
            .solver
            .take()
            .unwrap_or_else(|| super::liquid::Solver::new(&net));
        let (nnz, flops) = solver.pattern_size(&net);
        net.solver = Some(solver);
        for _ in 0..20 {
            net.step(0.1);
            net.carry(&mut tracer, 0.1);
        }
        let ticks = if name == "house" { 2000 } else { 50 };
        let (mut steps, mut carry, mut iterations) = (0.0, 0.0, 0usize);
        for t in 0..ticks {
            let swing = 1.0
                + 0.5 * creeer_math::det::sin(f64::from(t) * 0.1 * std::f64::consts::TAU / 600.0);
            for node in &mut net.nodes {
                if let NodeKind::Junction { demand } = &mut node.kind {
                    *demand = 2e-4 * swing;
                }
            }
            let s = std::time::Instant::now();
            let r = net.step(0.1);
            steps += s.elapsed().as_secs_f64();
            iterations += r.iterations;
            let s = std::time::Instant::now();
            net.carry(&mut tracer, 0.1);
            carry += s.elapsed().as_secs_f64();
        }
        let n = f64::from(ticks);
        eprintln!(
            "{name}: links {} nnz {nnz} flops {flops}: step {:.1} us, carry {:.1} us, {} iterations",
            net.links.len(),
            steps / n * 1e6,
            carry / n * 1e6,
            iterations
        );
    }
}
