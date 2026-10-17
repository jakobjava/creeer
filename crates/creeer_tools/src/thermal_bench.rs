//! F6.2 thermal networks: the cost of a step of a factory hall's 20,000 nodes, every one of
//! them awake (motors running, parts warming).

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_materials::MaterialTable;
use creeer_thermal::{Enthalpy, Link, Medium, Network, NodeId, Shape};
use std::sync::Arc;

/// Budget for a 20,000-node thermal step (at 20 Hz: three ticks' share of 0.5 ms).
const THERMAL_BUDGET_MS: f64 = 1.5;
const ATMOSPHERE: f64 = 101_325.0;

/// A factory hall: `machines` machines of `parts` steel parts bolted to frames on a concrete
/// floor (one island), and `carts` loose aluminium carts of `cart_parts` parts (an island
/// each). Every part convects to the air, frames and carts radiate to the walls, the floor
/// conducts to the ground; motors and lamps heat a part in four.
pub(crate) fn hall(machines: usize, parts: usize, carts: usize, cart_parts: usize) -> Network {
    let table = MaterialTable::reference();
    let material = |id: &str| table.get(id).expect("in the reference table");
    let (steel, concrete, aluminium) = (
        material("steel_s235"),
        material("concrete_c30"),
        material("aluminium_6061_t6"),
    );
    let enthalpy =
        |m: &creeer_materials::Material| Arc::new(Enthalpy::new(&m.specific_heat, m.melting));
    let (h_steel, h_concrete, h_aluminium) =
        (enthalpy(steel), enthalpy(concrete), enthalpy(aluminium));
    let (k_steel, k_concrete, k_aluminium) = (
        Arc::new(steel.conductivity.clone()),
        Arc::new(concrete.conductivity.clone()),
        Arc::new(aluminium.conductivity.clone()),
    );
    let mut net = Network::new(9.806_65);
    let air = net.add_fixed(295.0);
    let walls = net.add_fixed(290.0);
    let ground = net.add_fixed(285.0);
    let shapes = [
        Shape::Cylinder(0.08),
        Shape::VerticalPlate(0.4),
        Shape::PlateUp(0.2),
        Shape::Sphere(0.1),
    ];
    let convect = |net: &mut Network, node: NodeId, k: usize, area: f64| {
        let (shape, speed) = (shapes[k % 4], 0.5 * (k % 5) as f64);
        net.link(Link::Convection {
            nodes: (node, air),
            medium: Medium::Air,
            area,
            shape,
            speed,
            pressure: ATMOSPHERE,
        });
    };
    let mut slabs: Vec<NodeId> = Vec::with_capacity(machines);
    for m in 0..machines {
        let slab = net.add_mass(2400.0 * 2.0 * 0.2, h_concrete.clone(), 290.0);
        net.link(Link::Conduction {
            nodes: (slab, ground),
            conductivity: k_concrete.clone(),
            area: 2.0,
            length: 0.1,
        });
        if let Some(&last) = slabs.last() {
            net.link(Link::Conduction {
                nodes: (slab, last),
                conductivity: k_concrete.clone(),
                area: 0.4,
                length: 1.4,
            });
        }
        slabs.push(slab);
        let frame = net.add_mass(80.0, h_steel.clone(), 300.0);
        net.link(Link::Conduction {
            nodes: (frame, slab),
            conductivity: k_steel.clone(),
            area: 4e-3,
            length: 0.05,
        });
        net.link(Link::Radiation {
            nodes: (frame, walls),
            factor: 0.8 * 1.5,
        });
        convect(&mut net, frame, m, 1.5);
        let mut previous: Vec<NodeId> = Vec::with_capacity(parts);
        for k in 0..parts {
            let part = net.add_mass(
                0.5 + (k % 7) as f64,
                h_steel.clone(),
                300.0 + (k % 11) as f64,
            );
            net.link(Link::Conduction {
                nodes: (part, frame),
                conductivity: k_steel.clone(),
                area: 2e-4,
                length: 0.1,
            });
            for &other in previous.iter().rev().take(2) {
                net.link(Link::Conduction {
                    nodes: (part, other),
                    conductivity: k_steel.clone(),
                    area: 1e-4,
                    length: 0.05,
                });
            }
            convect(&mut net, part, k + m, 0.05);
            if k % 4 == 0 {
                net.set_source(part, 20.0 + (k % 9) as f64 * 10.0);
            }
            previous.push(part);
        }
    }
    for c in 0..carts {
        let mut previous = None;
        for k in 0..cart_parts {
            let part = net.add_mass(
                1.0 + (k % 3) as f64,
                h_aluminium.clone(),
                293.0 + (c % 13) as f64,
            );
            if let Some(last) = previous {
                net.link(Link::Conduction {
                    nodes: (part, last),
                    conductivity: k_aluminium.clone(),
                    area: 2e-4,
                    length: 0.1,
                });
            }
            convect(&mut net, part, k + c, 0.1);
            net.link(Link::Radiation {
                nodes: (part, walls),
                factor: 0.1 * 0.1,
            });
            if (c + k) % 4 == 0 {
                net.set_source(part, 15.0);
            }
            previous = Some(part);
        }
    }
    net
}

/// Runs the thermal benchmarks.
pub fn run(report: &mut Report) {
    println!("F6.2 thermal network: a factory hall stepped at 20 Hz (ms, p50 / p99)");
    // Its two halves alone: the machines (one island of 11,000 nodes, done in parallel within)
    // and the carts (a thousand islands of ten, in parallel with each other).
    for (name, mut net) in [
        ("machines (1 island)", hall(500, 20, 0, 0)),
        ("carts (1,000 islands)", hall(0, 0, 1000, 10)),
    ] {
        let t = time_ticks(
            || {
                net.step(0.05);
            },
            100,
        );
        println!("  {name:<24} {:.3} / {:.3}", p(&t, 50.0), p(&t, 99.0));
    }
    let mut net = hall(500, 20, 1000, 10);
    let nodes = 500 * 22 + 1000 * 10;
    let (mut iterations, mut factorisations, mut solved, mut steps) = (0, 0, 0, 0);
    let t = time_ticks(
        || {
            let r = net.step(0.05);
            iterations = iterations.max(r.iterations);
            factorisations += r.factorisations;
            solved = r.solved;
            steps += 1;
            assert_eq!(r.unconverged, 0, "the hall's step converges");
        },
        200,
    );
    println!(
        "  hall: {nodes} nodes, {solved} islands awake | {:.3} / {:.3} | Newton <= {iterations}, {:.2} factorisations a step",
        p(&t, 50.0),
        p(&t, 99.0),
        factorisations as f64 / f64::from(steps),
    );
    let rel = net.ledger().relative_imbalance(net.stored());
    println!("  ledger relative imbalance after {steps} steps: {rel:.1e}");
    report.metric("thermal_step_20k_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= THERMAL_BUDGET_MS && rel < 1e-10,
        format!("20k-node thermal step projected to min-spec: {projected:.2} ms <= {THERMAL_BUDGET_MS} ms (ledger closed)"),
    );
    println!();
}
