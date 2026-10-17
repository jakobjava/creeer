//! Equilibria against reference results: adiabatic flame temperatures (NASA CEA,
//! S. R. Turns, An Introduction to Combustion), the Gibbs energy's minimum.

use crate::{Fixed, System, equilibrate};
use creeer_materials::species::SpeciesTable;

const ATM: f64 = 101_325.0;
const ROOM: f64 = 298.15;

/// The adiabatic flame of `reactants` (species, moles) at 1 atm from 298.15 K, over every
/// species their elements can form (as the CEA program takes its products).
fn flame(table: &SpeciesTable, reactants: &[(&str, f64)]) -> (System, crate::Equilibrium) {
    let names: Vec<&str> = reactants.iter().map(|r| r.0).collect();
    let sys = System::of_elements(table, &names).unwrap();
    let mut moles = vec![0.0; sys.species.len()];
    for (name, n) in reactants {
        moles[sys.index(name).unwrap()] = *n;
    }
    let h = sys.enthalpy(&moles, ROOM);
    let b0 = sys.element_moles(&moles);
    let eq = equilibrate(&sys, &moles, Fixed::Hp { h, p: ATM }, None).unwrap();
    // The elements and the enthalpy are where they were.
    for (b, e) in b0.iter().zip(sys.element_moles(&eq.moles)) {
        assert!(
            (b - e).abs() < 1e-6 * b.max(1e-30),
            "element balance {b} vs {e}"
        );
    }
    assert!(
        (sys.enthalpy(&eq.moles, eq.t) - h).abs() < 1e-4 * 1e5,
        "enthalpy"
    );
    (sys, eq)
}

/// Methane in air, stoichiometric: 2226 K (CEA; air as 3.76 N₂ per O₂).
#[test]
fn methane_burns_in_air_at_2226_kelvin() {
    let table = SpeciesTable::reference();
    let (sys, eq) = flame(&table, &[("CH4", 1.0), ("O2", 2.0), ("N2", 7.52)]);
    assert!(
        (eq.t - 2226.0).abs() < 5.0,
        "{} K after {} iterations",
        eq.t,
        eq.iterations
    );
    // Its products: water and CO₂ above all, a little CO and OH from dissociation.
    let x = |name: &str| eq.moles[sys.index(name).unwrap()];
    assert!(
        x("CO") / x("CO2") > 0.05 && x("CO") / x("CO2") < 0.15,
        "CO/CO2 {}",
        x("CO") / x("CO2")
    );
}

/// Hydrogen in oxygen, stoichiometric: 3080 K, heavily dissociated (Turns, Table 2.? and CEA).
#[test]
fn hydrogen_burns_in_oxygen_near_3080_kelvin() {
    let table = SpeciesTable::reference();
    let (sys, eq) = flame(&table, &[("H2", 2.0), ("O2", 1.0)]);
    assert!(
        (eq.t - 3080.0).abs() < 15.0,
        "{} K after {} iterations",
        eq.t,
        eq.iterations
    );
    let total: f64 = eq.moles[..sys.gases].iter().sum();
    let water = eq.moles[sys.index("H2O").unwrap()] / total;
    assert!(water > 0.5 && water < 0.6, "water {water}");
}
