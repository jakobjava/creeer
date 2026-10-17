//! Water's electrolysis against the textbook: 1.229 V reversible and 1.481 V thermoneutral at
//! 25 °C (from ΔG and ΔH of liquid water's formation), 1.18 V at 80 °C, Faraday's masses.

use crate::electrolysis::{Electrochemical, FARADAY};
use crate::kinetics::Reaction;
use crate::system::System;
use creeer_materials::species::SpeciesTable;

fn water() -> (System, Electrochemical) {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &["H2", "O2", "H2O(L)"]).unwrap();
    let reaction = Reaction::new(
        &sys,
        &[("H2O(L)", 1.0)],
        &[("H2", 1.0), ("O2", 0.5)],
        0.0,
        0.0,
        0.0,
    )
    .unwrap();
    (
        sys,
        Electrochemical {
            reaction,
            electrons: 2.0,
        },
    )
}

#[test]
fn splitting_water_takes_1_229_volts_and_1_481_without_heat() {
    let (sys, cell) = water();
    let standard = |_: usize| 1.0;
    let e = cell.reversible_voltage(&sys, 298.15, standard);
    assert!((e - 1.229).abs() < 0.001, "{e} V reversible");
    let tn = cell.thermoneutral_voltage(&sys, 298.15);
    assert!((tn - 1.481).abs() < 0.001, "{tn} V thermoneutral");
    // Warmer, easier: 1.18 V at 80 °C (its entropy, T·ΔS, comes as heat).
    let warm = cell.reversible_voltage(&sys, 353.15, standard);
    assert!((warm - 1.18).abs() < 0.005, "{warm} V at 80 °C");
    // Under 30 bar of product gases the Nernst term adds RT/2F·ln(30^1.5) = 65 mV.
    let pressed = cell.reversible_voltage(&sys, 298.15, |_| 30.0);
    assert!(
        (pressed - e - 0.0655).abs() < 0.001,
        "{} V more under 30 bar",
        pressed - e
    );
}

/// Faraday: an ampere-hour is 37.31 mmol of electrons, two to a hydrogen molecule: 18.66 mmol
/// of hydrogen (37.6 mg) and half as many moles of oxygen.
#[test]
fn an_ampere_hour_makes_38_milligrams_of_hydrogen() {
    let (sys, cell) = water();
    let moles = cell.extent(1.0, 3600.0);
    assert!((moles - 3600.0 / (2.0 * FARADAY)).abs() < 1e-15);
    let grams = moles * sys.species[sys.index("H2").unwrap()].molar_mass * 1e3;
    assert!((grams - 0.0376).abs() < 0.0001, "{grams} g of hydrogen");
}
