//! Electrolysis cells between a circuit and a reactor (F6.5): the cell's reversible voltage
//! from the reactor's chemistry (ΔG/zF at its temperature and its gases' pressures), the
//! circuit's current through it, and the reaction that current drives by Faraday's law in
//! the reactor, which takes all the electrical energy the cell took (the reaction's work and
//! the cell's heat).
//!
//! The circuit steps first, the reactor after with what the circuit did: the charge and the
//! energy of the step exactly as the circuit booked them, so the hydrogen made is the charge
//! that passed and the energy the circuit gave is the energy the reactor got.

use creeer_chem::System;
use creeer_chem::electrolysis::Electrochemical;
use creeer_chem::reactor::Reactor;
use creeer_electric::{Circuit, ComponentId};
use creeer_materials::species::STANDARD_PRESSURE;

/// A cell stack in a circuit, running a reaction in a reactor.
#[derive(Clone, Debug)]
pub struct CellLink {
    pub cell: ComponentId,
    /// Its cells in series, and the reaction it runs (on the reactor's system).
    pub cells: f64,
    pub electrochemical: Electrochemical,
}

/// The reversible voltage per cell of `link`'s reaction in `reactor` now (V).
pub fn reversible_voltage(link: &CellLink, reactor: &Reactor, sys: &System) -> f64 {
    let gas: f64 = reactor.moles[..sys.gases].iter().sum();
    let p = reactor.pressure(sys);
    // Each gas at its partial pressure (bar), a trace at least (a gas not there yet would be
    // made at no cost: its log has no floor).
    let pressure =
        |j: usize| (reactor.moles[j] / gas.max(1e-300) * p / STANDARD_PRESSURE).max(1e-6);
    link.electrochemical
        .reversible_voltage(sys, reactor.t, pressure)
}

/// Advances `circuit` and `reactor` by `h` seconds together, `links`' cells between them.
pub fn step(
    circuit: &mut Circuit,
    reactor: &mut Reactor,
    sys: &System,
    links: &[CellLink],
    h: f64,
) {
    let emf = prepare(circuit, reactor, sys, links);
    circuit.step(h);
    finish(circuit, reactor, sys, links, &emf, h);
}

/// [`step`]'s first half: each cell's reversible voltage and temperature handed to the circuit,
/// for a circuit other couplings step too; the voltages, for [`finish`].
pub fn prepare(
    circuit: &mut Circuit,
    reactor: &Reactor,
    sys: &System,
    links: &[CellLink],
) -> Vec<f64> {
    let mut emf = Vec::with_capacity(links.len());
    for link in links {
        let e = reversible_voltage(link, reactor, sys);
        circuit.set_temperature(link.cell, reactor.t);
        circuit.set_emf(link.cell, e);
        emf.push(e);
    }
    emf
}

/// [`step`]'s second half, after the circuit's step: the reactions its cells drove, and the
/// reactor's step.
pub fn finish(
    circuit: &mut Circuit,
    reactor: &mut Reactor,
    sys: &System,
    links: &[CellLink],
    emf: &[f64],
    h: f64,
) {
    reactor.driven.clear();
    let mut power = 0.0;
    for (link, &e) in links.iter().zip(emf) {
        // The work the circuit booked is cells·E·(the charge, as its rule took it).
        let work = circuit.take_work(link.cell);
        let heat = circuit.take_heat(link.cell);
        let charge = work / (link.cells * e);
        let extent = link.electrochemical.extent(link.cells * charge, 1.0) / h;
        reactor
            .driven
            .push((link.electrochemical.reaction.clone(), extent));
        power += (work + heat) / h;
    }
    reactor.power = power;
    reactor.step(sys, h);
}
