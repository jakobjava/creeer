//! Validation against closed-form solutions, and the ledger's closure.

use crate::*;
use creeer_materials::{Curve, MaterialTable};
use creeer_math::det;
use std::sync::Arc;

const ATMOSPHERE: f64 = 101_325.0;
const G: f64 = 9.806_65;

fn constant(cp: f64) -> Arc<Enthalpy> {
    Arc::new(Enthalpy::constant(cp))
}

fn steps(net: &mut Network, dt: f64, n: usize) {
    for _ in 0..n {
        net.step(dt);
    }
}

/// A mass cooling through a conductance: backward Euler's exact recurrence, and the exponential.
#[test]
fn lumped_cooling() {
    let (m, cp, g, t0, air) = (2.0, 500.0, 4.0, 400.0, 300.0);
    let mut net = Network::new(G);
    let part = net.add_mass(m, constant(cp), t0);
    let outside = net.add_fixed(air);
    net.link(Link::Conductance {
        nodes: (part, outside),
        conductance: g,
    });
    let dt = 1.0;
    let a = g * dt / (m * cp);
    let mut expected = t0;
    for _ in 0..100 {
        net.step(dt);
        expected = (expected + a * air) / (1.0 + a);
    }
    assert!(
        (net.temperature(part) - expected).abs() < 1e-9,
        "{} vs {expected}",
        net.temperature(part)
    );
    // Small steps: the exponential (τ = 250 s; backward Euler's error is O(Δt/τ)).
    let mut net = Network::new(G);
    let part = net.add_mass(m, constant(cp), t0);
    let outside = net.add_fixed(air);
    net.link(Link::Conductance {
        nodes: (part, outside),
        conductance: g,
    });
    steps(&mut net, 0.05, 5000);
    let exact = air + (t0 - air) * det::exp(-g * 250.0 / (m * cp));
    assert!(
        (net.temperature(part) - exact).abs() < 0.01,
        "{} vs {exact}",
        net.temperature(part)
    );
}

/// A rod between two held temperatures settles to a straight profile, its flow k·A·ΔT/L.
#[test]
fn steady_rod() {
    let (n, k, area, length) = (20, 50.0, 1e-4, 1.0);
    let dx = length / n as f64;
    let mut net = Network::new(G);
    let hot = net.add_fixed(400.0);
    let cold = net.add_fixed(300.0);
    let nodes: Vec<_> = (0..n)
        .map(|_| net.add_mass(7850.0 * area * dx, constant(450.0), 300.0))
        .collect();
    let conduction = |a, b, l| Link::Conduction {
        nodes: (a, b),
        conductivity: Arc::new(Curve::constant(k)),
        area,
        length: l,
    };
    net.link(conduction(nodes[0], hot, 0.5 * dx));
    for w in nodes.windows(2) {
        net.link(conduction(w[0], w[1], dx));
    }
    net.link(conduction(nodes[n - 1], cold, 0.5 * dx));
    steps(&mut net, 1e5, 200);
    for (i, &node) in nodes.iter().enumerate() {
        let x = (i as f64 + 0.5) * dx;
        assert!((net.temperature(node) - (400.0 - 100.0 * x / length)).abs() < 1e-6);
    }
    // In steady state what comes in at the hot end leaves at the cold: the ledger's dissipated
    // power is the net of both, ~0, and the store no longer changes.
    let before = net.stored();
    net.step(1.0);
    assert!((net.stored() - before).abs() < 1e-6);
}

/// A thick wall whose face is suddenly held hotter: erfc's profile into a semi-infinite solid.
#[test]
fn semi_infinite_slab() {
    let (k, rho, cp, dx, cells) = (50.0, 7850.0, 450.0, 1e-3, 200);
    let (ti, ts) = (300.0, 500.0);
    let mut net = Network::new(G);
    let face = net.add_fixed(ts);
    let cells: Vec<_> = (0..cells)
        .map(|_| net.add_mass(rho * dx, constant(cp), ti))
        .collect();
    net.link(Link::Conductance {
        nodes: (cells[0], face),
        conductance: k / (0.5 * dx),
    });
    for w in cells.windows(2) {
        net.link(Link::Conductance {
            nodes: (w[0], w[1]),
            conductance: k / dx,
        });
    }
    let (dt, n) = (0.05, 1200);
    steps(&mut net, dt, n);
    let alpha = k / (rho * cp);
    let span = 2.0 * (alpha * dt * n as f64).sqrt();
    for (i, &cell) in cells.iter().enumerate().step_by(10) {
        let x = (i as f64 + 0.5) * dx;
        let exact = ts + (ti - ts) * det::erf(x / span);
        assert!(
            (net.temperature(cell) - exact).abs() < 0.005 * (ts - ti),
            "x = {x}: {} vs {exact}",
            net.temperature(cell)
        );
    }
}

/// A plate facing the Sun at 1 AU, radiating from both faces into space: (S/2σ)^¼ ≈ 331 K.
#[test]
fn radiative_equilibrium() {
    let (solar, emissivity, area) = (1361.0, 0.9, 1.0);
    let mut net = Network::new(0.0);
    let plate = net.add_mass(2700.0 * 0.002 * area, constant(900.0), 250.0);
    let space = net.add_fixed(2.725);
    net.set_source(plate, emissivity * solar * area);
    net.link(Link::Radiation {
        nodes: (plate, space),
        factor: emissivity * 2.0 * area,
    });
    steps(&mut net, 10.0, 1000);
    let exact = det::pow(
        solar / (2.0 * STEFAN_BOLTZMANN) + det::pow(2.725, 4.0),
        0.25,
    );
    assert!(
        (net.temperature(plate) - exact).abs() < 1e-4,
        "{} vs {exact}",
        net.temperature(plate)
    );
    assert!((330.5..331.5).contains(&exact), "{exact}");
    assert_eq!(net.awake(), 0, "settled: asleep");
    // A change wakes it.
    net.set_source(plate, 0.0);
    net.step(10.0);
    assert_eq!(net.awake(), 1);
    assert!(net.temperature(plate) < exact);
}

/// Ice heated steadily: its temperature waits at 0 °C for exactly its heat of fusion.
#[test]
fn melting_plateau() {
    let table = MaterialTable::reference();
    let ice = table.get("ice").unwrap();
    let enthalpy = Arc::new(Enthalpy::new(&ice.specific_heat, ice.melting));
    let (lo, hi) = enthalpy.melting().unwrap();
    let (m, power, dt) = (1.0, 1000.0, 1.0);
    let mut net = Network::new(G);
    let block = net.add_mass(m, enthalpy.clone(), 263.15);
    net.set_source(block, power);
    let (mut start, mut end, mut iterations) = (None, None, 0);
    for k in 1..=600 {
        iterations = iterations.max(net.step(dt).iterations);
        let t = net.temperature(block);
        let time = k as f64 * dt;
        if start.is_none() && t >= lo {
            start = Some(time);
        }
        if end.is_none() && t >= hi {
            end = Some(time);
        }
    }
    let plateau = end.unwrap() - start.unwrap();
    let expected = m * (enthalpy.at(hi) - enthalpy.at(lo)) / power;
    assert!(
        (plateau - expected).abs() <= dt,
        "{plateau} s vs {expected} s"
    );
    assert!(
        expected > 333.55e3 * m / power,
        "the plateau holds the whole heat of fusion"
    );
    assert!(
        iterations <= 5,
        "Newton across the corners: {iterations} iterations"
    );
    // Every joule supplied is in the water now.
    let gained = m * (enthalpy.at(net.temperature(block)) - enthalpy.at(263.15));
    assert!((gained - power * 600.0).abs() < 1e-6 * gained);
    // 600 kJ: 20 kJ warm the ice, 334 kJ melt it, the rest warms the water ~59 K.
    assert!(
        (330.0..334.0).contains(&net.temperature(block)),
        "{}",
        net.temperature(block)
    );
}

/// Every kind of link, sources, heat received, varying conductivity, a fixed temperature that
/// changes: the ledger closes to rounding.
#[test]
fn ledger_closes() {
    let table = MaterialTable::reference();
    let steel = table.get("steel_s235").unwrap();
    let h_steel = Arc::new(Enthalpy::new(&steel.specific_heat, steel.melting));
    let k_steel = Arc::new(steel.conductivity.clone());
    let mut net = Network::new(G);
    let air = net.add_fixed(293.15);
    let sky = net.add_fixed(250.0);
    let parts: Vec<_> = (0..12)
        .map(|k| net.add_mass(1.0 + k as f64, h_steel.clone(), 300.0 + 40.0 * k as f64))
        .collect();
    for (k, w) in parts.windows(2).enumerate() {
        net.link(Link::Conduction {
            nodes: (w[0], w[1]),
            conductivity: k_steel.clone(),
            area: 1e-3,
            length: 0.05,
        });
        if k % 3 == 0 {
            net.link(Link::Radiation {
                nodes: (w[0], w[1]),
                factor: 0.05,
            });
        }
    }
    for (k, &p) in parts.iter().enumerate() {
        let shape = [
            Shape::VerticalPlate(0.3),
            Shape::Cylinder(0.05),
            Shape::Sphere(0.1),
            Shape::PlateUp(0.2),
        ][k % 4];
        net.link(Link::Convection {
            nodes: (p, air),
            medium: Medium::Air,
            area: 0.1,
            shape,
            speed: (k % 3) as f64,
            pressure: ATMOSPHERE,
        });
        net.link(Link::Radiation {
            nodes: (p, sky),
            factor: 0.08,
        });
    }
    // A second island, a lone part over a fire.
    let lone = net.add_mass(3.0, h_steel, 290.0);
    net.link(Link::Convection {
        nodes: (lone, air),
        medium: Medium::Air,
        area: 0.2,
        shape: Shape::PlateDown(0.3),
        speed: 0.0,
        pressure: ATMOSPHERE,
    });
    net.set_source(lone, 5000.0);
    net.set_source(parts[3], 200.0);
    for k in 0..400 {
        if k % 50 == 0 {
            net.receive(parts[7], 1000.0);
            net.set_fixed(air, 293.15 + 0.01 * k as f64);
        }
        net.step(0.05);
    }
    let rel = net.ledger().relative_imbalance(net.stored());
    assert!(rel < 1e-10, "relative imbalance {rel:e}");
    let (supplied, _, _, imported) = net.ledger().totals();
    assert!((supplied - 5200.0 * 20.0).abs() < 1e-6 && (imported - 8000.0).abs() < 1e-9);
}

/// Islands step in parallel and give the same bits as one by one: the step is deterministic.
#[test]
fn deterministic() {
    let run = || {
        let mut net = Network::new(G);
        let air = net.add_fixed(293.15);
        let mut last = Vec::new();
        for island in 0..64 {
            let a = net.add_mass(1.0, constant(450.0), 300.0 + island as f64);
            let b = net.add_mass(2.0, constant(900.0), 500.0);
            net.link(Link::Conductance {
                nodes: (a, b),
                conductance: 3.0,
            });
            net.link(Link::Convection {
                nodes: (b, air),
                medium: Medium::Air,
                area: 0.1,
                shape: Shape::Cylinder(0.1),
                speed: 1.0,
                pressure: ATMOSPHERE,
            });
            net.link(Link::Radiation {
                nodes: (a, air),
                factor: 0.1,
            });
            last.push(b);
        }
        steps(&mut net, 0.05, 100);
        (
            last.iter()
                .map(|&n| net.temperature(n).to_bits())
                .collect::<Vec<_>>(),
            net.stored().to_bits(),
        )
    };
    let one = run();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    assert_eq!(one, pool.install(run));
    assert_eq!(one, run());
}

/// A network of fixed conductances and constant heat capacities is linear: one factorisation
/// for good, and one solve a step.
#[test]
fn linear_island_one_solve() {
    let mut net = Network::new(G);
    let outside = net.add_fixed(280.0);
    let nodes: Vec<_> = (0..50)
        .map(|k| net.add_mass(1.0, constant(450.0), 300.0 + k as f64))
        .collect();
    for w in nodes.windows(2) {
        net.link(Link::Conductance {
            nodes: (w[0], w[1]),
            conductance: 5.0,
        });
    }
    net.link(Link::Conductance {
        nodes: (nodes[0], outside),
        conductance: 1.0,
    });
    let mut factorisations = 0;
    for _ in 0..20 {
        let report = net.step(0.05);
        assert_eq!(
            (report.solved, report.iterations, report.unconverged),
            (1, 1, 0)
        );
        factorisations += report.factorisations;
    }
    assert_eq!(factorisations, 1);
}
