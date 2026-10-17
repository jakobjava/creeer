//! Water split by an alkaline cell between a circuit and a reactor: Faraday's law to the
//! mole, every joule the circuit gave received by the reactor, the heat above the
//! thermoneutral voltage given off.

use crate::electrochem::{self, CellLink};
use creeer_chem::System;
use creeer_chem::electrolysis::{Electrochemical, FARADAY};
use creeer_chem::kinetics::Reaction;
use creeer_chem::reactor::{Holding, Reactor};
use creeer_electric::{Circuit, Electrolyzer, GROUND};
use creeer_materials::species::SpeciesTable;

#[test]
fn a_cell_splits_water_by_faradays_law_with_every_joule_booked() {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &["H2O(L)", "H2", "O2", "N2"]).unwrap();
    let mut moles = vec![0.0; sys.species.len()];
    moles[sys.index("H2O(L)").unwrap()] = 100.0;
    moles[sys.index("N2").unwrap()] = 1.0;
    let mut reactor = Reactor::new(
        &sys,
        moles,
        353.15,
        Holding::Pressure(101_325.0),
        Vec::new(),
    );
    reactor.isothermal = true;
    let split = Reaction::new(
        &sys,
        &[("H2O(L)", 1.0)],
        &[("H2", 1.0), ("O2", 0.5)],
        0.0,
        0.0,
        0.0,
    )
    .unwrap();
    let mut circuit = Circuit::new();
    let top = circuit.node();
    circuit.voltage_source(top, GROUND, 1.95);
    let cell = circuit.electrolyzer(top, GROUND, Electrolyzer::ALKALINE_CELL);
    let links = [CellLink {
        cell,
        cells: 1.0,
        electrochemical: Electrochemical {
            reaction: split,
            electrons: 2.0,
        },
    }];
    let h = 0.1;
    let mut charge = 0.0;
    for _ in 0..6000 {
        electrochem::step(&mut circuit, &mut reactor, &sys, &links, h);
        charge += circuit.current(cell) * h;
    }
    let at = |name: &str| reactor.moles[sys.index(name).unwrap()];
    // Faraday: the hydrogen is the charge over 2F (to the rule's averaging of the current).
    let hydrogen = at("H2");
    assert!(
        (hydrogen * 2.0 * FARADAY / charge - 1.0).abs() < 1e-3,
        "{hydrogen} mol H₂ for {charge} C"
    );
    assert!((at("O2") - hydrogen / 2.0).abs() < 1e-12 * hydrogen);
    assert!((100.0 - at("H2O(L)") - hydrogen).abs() < 1e-9, "water used");
    // Every joule: what the circuit gave the cell is what the reactor took in.
    let (_, _, _, imported) = reactor.ledger().totals();
    // (The cell is the circuit's only dissipator but for GMIN's leak: picojoules.)
    let (_, cell_heat, cell_work, _) = circuit.ledger().totals();
    let given = cell_work + cell_heat;
    assert!(
        (imported - given).abs() < 1e-6 * imported,
        "{imported} J in, the circuit gave {given}"
    );
    // Above the thermoneutral voltage (1.48 V) the cell runs hot: the reactor, held at 80 °C,
    // gives off what came in less what the products hold, ΔH·ξ.
    let (_, dh) = links[0].electrochemical.reaction.delta_rt(&sys, 353.15);
    let held = dh * creeer_materials::species::R * 353.15 * hydrogen;
    let (_, _, given_off, _) = reactor.ledger().totals();
    assert!(
        (given_off - (imported - held)).abs() < 1e-3 * imported,
        "{given_off} J off, expected {}",
        imported - held
    );
    assert!(given_off > 0.2 * imported, "the cell runs hot at 1.95 V");
    assert!(reactor.ledger().relative_imbalance(reactor.stored(&sys)) < 1e-9);
}
