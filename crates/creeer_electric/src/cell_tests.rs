//! The electrolysis cell in a circuit: its polarisation curve as Butler–Volmer gives it,
//! running backwards below its reversible voltage, its books.

use crate::cell::Electrolyzer;
use crate::{Circuit, GROUND};
use creeer_math::det;

/// The voltage the cell needs for `i` amps at `e` volts reversible and `t` K:
/// E + b·asinh(i/(2·i₀·A)) + i·R.
fn polarisation(cell: &Electrolyzer, i: f64, e: f64, t: f64) -> f64 {
    let x = i / (2.0 * cell.exchange * cell.area);
    // asinh, odd: ln(x + √(x² + 1)) cancels for large negative x.
    let asinh = x.signum() * det::ln(x.abs() + (x * x + 1.0).sqrt());
    e + cell.slope(t) * asinh + i * cell.ohmic
}

/// An alkaline cell at 80 °C (1.18 V reversible) on 1.901 V draws 90 A, 0.3 A/cm²; the
/// circuit's Newton solution is the polarisation curve's to its tolerance.
#[test]
fn the_cell_follows_its_polarisation_curve() {
    let cell = Electrolyzer::ALKALINE_CELL;
    let mut c = Circuit::new();
    let top = c.node();
    c.voltage_source(top, GROUND, 1.901);
    let id = c.electrolyzer(top, GROUND, cell);
    c.set_temperature(id, 353.15);
    c.set_emf(id, 1.18);
    for _ in 0..10 {
        c.step(1.0 / 60.0);
    }
    let i = c.current(id);
    assert!((i - 90.0).abs() < 1.0, "{i} A");
    let v = polarisation(&cell, i, 1.18, 353.15);
    assert!((v - 1.901).abs() < 1e-6, "{v} V on the curve at {i} A");
}

/// Below its reversible voltage it runs backwards, a fuel cell, current out of its plus, on
/// the same curve: 0.43 V below 1.229 V drives 0.59 A out (its slow electrodes).
#[test]
fn below_its_reversible_voltage_it_is_a_fuel_cell() {
    let mut c = Circuit::new();
    let top = c.node();
    c.voltage_source(top, GROUND, 0.8);
    let id = c.electrolyzer(top, GROUND, Electrolyzer::ALKALINE_CELL);
    c.set_emf(id, 1.229);
    c.step(1.0 / 60.0);
    let i = c.current(id);
    assert!(i < 0.0, "{i} A");
    let v = polarisation(&Electrolyzer::ALKALINE_CELL, i, 1.229, 298.15);
    assert!((v - 0.8).abs() < 1e-6, "{v} V on the curve at {i} A");
    let (_, _, exported, imported) = c.ledger().totals();
    assert!(
        imported > 0.0 && exported == 0.0,
        "the reaction's work comes in"
    );
}

/// Its books: what it takes is the reaction's work (exported: cells·E·∫i·dt) and heat; the
/// ledger closes.
#[test]
fn the_cell_books_the_reaction_and_its_heat() {
    let mut c = Circuit::new();
    let top = c.node();
    c.voltage_source(top, GROUND, 1.9);
    let id = c.electrolyzer(top, GROUND, Electrolyzer::ALKALINE_CELL);
    c.set_emf(id, 1.229);
    let mut charge = 0.0;
    for _ in 0..600 {
        c.step(1.0 / 60.0);
        charge += c.current(id) / 60.0;
    }
    let (_, heat, exported, _) = c.ledger().totals();
    assert!(
        (exported / (1.229 * charge) - 1.0).abs() < 1e-3,
        "{exported} J of work for {charge} C"
    );
    assert!(heat > 0.0);
    assert!(c.ledger().relative_imbalance(c.stored()) < 1e-9);
}
