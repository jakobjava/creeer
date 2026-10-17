//! The circuit bench's tests.

use super::*;

/// Every bench runs with its controls worked, its traces finite and its energy accounted.
#[test]
fn every_bench_runs_and_balances() {
    for (bench, name) in Bench::ALL {
        let mut b = CircuitBench::build(bench);
        for k in 0..1200 {
            b.on = (k / 300) % 2 == 0;
            b.advance(TICK);
        }
        assert!(
            b.traces
                .iter()
                .all(|t| t.points.iter().all(|p| p[1].is_finite())),
            "{name}"
        );
        let (ledger, stored) = match &b.rig {
            Rig::Dc { circuit, .. } => (*circuit.ledger(), circuit.stored()),
            Rig::Ac { grid, .. } => (*grid.ledger(), grid.stored()),
        };
        assert!(
            ledger.relative_imbalance(stored) < 1e-8,
            "{name}: {:e}",
            ledger.relative_imbalance(stored)
        );
    }
}

/// The solar bench's day: dark at midnight, bright at noon, the bank charging by day.
#[test]
fn the_solar_bench_has_a_day() {
    let mut b = CircuitBench::build(Bench::Solar);
    // To noon: 12 h at 360× is 2 minutes of ticks.
    for _ in 0..(12 * 3600 / 360 * 60) {
        b.advance(TICK);
    }
    let Rig::Dc { circuit, parts, .. } = &b.rig else {
        unreachable!()
    };
    assert!(
        circuit.irradiance(parts[0]) > 600.0,
        "noon: {}",
        circuit.irradiance(parts[0])
    );
    assert!(
        circuit.current(parts[1]) > 1.0,
        "charging at noon: {} A",
        circuit.current(parts[1])
    );
}
