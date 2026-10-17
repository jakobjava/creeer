//! F6.3 electrical networks: a factory's DC wiring (10,000 nodes, nonlinear, stepped at
//! 60 Hz) and a town's AC grid (2,000 buses, constant-power loads and motors).

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_electric::{
    Battery, Branch, Circuit, DcMachine, Diode, GROUND, Grid, InductionMotor, Kind,
};
use creeer_math::sparse::Complex;

/// Budgets on the minimum-spec CPU (docs/f6-plan.md): a 10,000-node DC/transient network
/// and a 2,000-bus AC grid, each 1 ms a tick.
const DC_BUDGET_MS: f64 = 1.0;
const AC_BUDGET_MS: f64 = 1.0;

/// A factory's 48 V DC wiring: a battery bank, `feeders` feeders of `machines` stations each
/// along a cable (a wire segment between stations), every station a breaker, a motor with a
/// capacitor across it, and an indicator LED behind its resistor.
pub(crate) fn factory(feeders: usize, machines: usize) -> (Circuit, usize) {
    let mut c = Circuit::new();
    let bus = c.node();
    for _ in 0..4 {
        c.battery(bus, GROUND, Battery::lead_acid_12v_100ah());
    }
    // Four 12 V batteries in parallel stand for a 48 V bank: the source behind them.
    let plus = c.node();
    c.voltage_source(plus, bus, 36.0);
    let mut nodes = 2;
    for f in 0..feeders {
        let mut along = plus;
        for m in 0..machines {
            let next = c.node();
            c.resistor(along, next, 0.002);
            let fed = c.node();
            c.add(
                Kind::Breaker {
                    rated: 20.0,
                    thermal: 30.0,
                    magnetic: 7.5,
                },
                next,
                fed,
            );
            let motor = c.machine(fed, GROUND, DcMachine::SCOOTER_24V);
            c.set_load(motor, 0.2 + 0.05 * ((f + m) % 7) as f64);
            c.capacitor(fed, GROUND, 2.2e-3);
            let lamp = c.node();
            c.resistor(fed, lamp, 2200.0);
            c.diode(lamp, GROUND, Diode::RED_LED);
            along = next;
            nodes += 3;
        }
    }
    (c, nodes)
}

/// A town's grid: a 33 kV source feeding `feeders` 11 kV feeders through a substation
/// transformer, each a line of `buses` buses, every fifth with a distribution transformer to
/// a 400 V bus with a constant-power load and an induction motor.
pub(crate) fn town(feeders: usize, buses: usize) -> (Grid, usize) {
    let mut g = Grid::new(50.0);
    let source = g.bus();
    g.add(Branch::Source {
        a: source,
        emf: Complex::new(33e3, 0.0),
        z: Complex::new(0.2, 2.0),
    });
    let sub = g.bus();
    let base = 11e3 * 11e3 / 20e6;
    g.add(Branch::Transformer {
        a: source,
        b: sub,
        ratio: 3.0,
        z: Complex::new(0.005, 0.08) * Complex::from(base * 9.0),
        y: Complex::new(0.001, -0.005) / Complex::from(base * 9.0),
    });
    let mut count = 2;
    for f in 0..feeders {
        let mut along = sub;
        for k in 0..buses {
            let next = g.bus();
            g.add(Branch::Line {
                a: along,
                b: next,
                z: Complex::new(0.05, 0.08),
            });
            count += 1;
            if k % 5 == 4 {
                let low = g.bus();
                let base = 11e3 * 11e3 / 250e3;
                g.add(Branch::Transformer {
                    a: next,
                    b: low,
                    ratio: 11e3 / 400.0,
                    z: Complex::new(0.0097, 0.0388) * Complex::from(base),
                    y: Complex::new(0.002, -0.015) / Complex::from(base),
                });
                g.add(Branch::Load {
                    a: low,
                    power: Complex::new(40e3 + 1e3 * ((f + k) % 9) as f64, 12e3),
                });
                let motor = g.add(Branch::Motor {
                    a: low,
                    model: InductionMotor::KW7_5,
                    speed: 150.0,
                    load: 0.0,
                });
                g.set_load(motor, 30.0);
                count += 1;
            }
            along = next;
        }
    }
    (g, count)
}

/// Runs the electrical benchmarks.
pub fn run(report: &mut Report) {
    println!("F6.3 electrical networks: stepped at 60 Hz (ms, p50 / p99)");
    let (mut dc, nodes) = factory(10, 333);
    // Past the batteries' and motors' start (ten seconds): the factory running.
    for _ in 0..600 {
        dc.step(1.0 / 60.0);
    }
    let (mut iterations, mut factorisations, mut steps) = (0, 0, 0u32);
    let t = time_ticks(
        || {
            let r = dc.step(1.0 / 60.0);
            iterations = iterations.max(r.iterations);
            factorisations += r.factorisations;
            steps += 1;
            assert_eq!(r.unconverged, 0, "the factory's step converges");
        },
        200,
    );
    println!(
        "  factory DC wiring: {nodes} nodes | {:.3} / {:.3} | Newton <= {iterations}, {:.1} factorisations a step",
        p(&t, 50.0),
        p(&t, 99.0),
        factorisations as f64 / f64::from(steps)
    );
    report.metric("electric_dc_10k_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= DC_BUDGET_MS,
        format!(
            "10k-node DC circuit step projected to min-spec: {projected:.2} ms <= {DC_BUDGET_MS} ms"
        ),
    );
    let (mut ac, buses) = town(20, 100);
    for _ in 0..300 {
        ac.step(1.0 / 60.0);
    }
    let mut most = 0;
    let t = time_ticks(
        || {
            let r = ac.step(1.0 / 60.0);
            most = most.max(r.iterations);
            assert!(r.converged, "the town's grid converges");
        },
        200,
    );
    println!(
        "  town AC grid: {buses} buses | {:.3} / {:.3} | load iterations <= {most}",
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric("electric_ac_2k_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= AC_BUDGET_MS,
        format!(
            "2k-bus AC grid step projected to min-spec: {projected:.2} ms <= {AC_BUDGET_MS} ms"
        ),
    );
    println!();
}
