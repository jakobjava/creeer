//! Joule heating: a circuit's heat into a thermal network, the network's temperature back
//! into the circuit's resistance.

use creeer_electric::{Circuit, GROUND, Kind};
use creeer_materials::MaterialTable;
use creeer_thermal::{Enthalpy, Link, Network};
use std::sync::Arc;

/// A 1 m copper wire of 0.5 mm² carrying current from a 0.5 V source, cooled to 20 °C air
/// through 0.05 W/K (a thin wire in still air): it warms until I²·R(T) = G·(T − T_air), its
/// resistance rising with it (so
/// it carries less as it warms), and every joule of its heat reaches the network.
#[test]
fn a_wire_heats_itself_to_its_balance() {
    let copper = MaterialTable::reference()
        .get("copper_c11000")
        .unwrap()
        .clone();
    let (length, area, volts, cooling) = (1.0, 0.5e-6, 0.5, 0.05);
    let mut circuit = Circuit::new();
    let top = circuit.node();
    circuit.voltage_source(top, GROUND, volts);
    let wire = circuit.add(
        Kind::Wire {
            resistivity: Arc::new(copper.resistivity.clone()),
            length,
            area,
        },
        top,
        GROUND,
    );
    let mut heat = Network::new(9.81);
    let mass = copper.density * length * area;
    let node = heat.add_mass(
        mass,
        Arc::new(Enthalpy::new(&copper.specific_heat, copper.melting)),
        293.15,
    );
    let air = heat.add_fixed(293.15);
    heat.link(Link::Conductance {
        nodes: (node, air),
        conductance: cooling,
    });
    let dt = 0.05;
    let mut given = 0.0;
    // Its time constant m·c/G is 34 s: twelve of them.
    for _ in 0..8000 {
        circuit.step(dt);
        let joules = circuit.take_heat(wire);
        given += joules;
        heat.receive(node, joules);
        heat.step(dt);
        circuit.set_temperature(wire, heat.temperature(node));
    }
    // The balance, solved by bisection: V²/R(T) = G·(T − T_air).
    let resistance = |t: f64| copper.resistivity.at(t) * length / area;
    let (mut lo, mut hi) = (293.15, 1000.0);
    for _ in 0..100 {
        let m = 0.5 * (lo + hi);
        if volts * volts / resistance(m) > cooling * (m - 293.15) {
            lo = m
        } else {
            hi = m
        }
    }
    let t = heat.temperature(node);
    assert!((t - lo).abs() < 0.05, "{t} K vs {lo} K");
    assert!(t > 350.0, "it heats noticeably: {t} K");
    let current = circuit.current(wire);
    assert!((current - volts / resistance(t)).abs() < 1e-6 * current);
    // Everything the circuit dissipated went into the network, and from it to the air.
    let (_, dissipated, _, _) = circuit.ledger().totals();
    let (_, _, _, received) = heat.ledger().totals();
    assert!((received - given).abs() < 1e-9 * given && (dissipated - given).abs() < 1e-6 * given);
}
