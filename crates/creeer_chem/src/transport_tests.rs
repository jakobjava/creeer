//! Gas viscosities and conductivities against reference data at 300 K (E. W. Lemmon and
//! R. T. Jacobsen, Int. J. Thermophys. 25, 2004, for N₂, O₂, Ar and air; NIST's REFPROP
//! values for CO₂, CH₄, H₂, He) and steam's at 400 K (IAPWS). Measured: η within 2.5 %, λ of the
//! polyatomic gases 4–8 % high (the modified Eucken correction's bias), the rest within 1 %.

use crate::System;
use crate::transport::{mixture, pure};
use creeer_materials::species::SpeciesTable;

#[test]
fn pure_gases_follow_their_reference_viscosities() {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &["N2", "O2", "Ar", "CO2", "CH4", "H2", "He", "H2O"]).unwrap();
    for (name, t, eta_ref, lambda_ref) in [
        ("N2", 300.0, 17.89e-6, 0.02597),
        ("O2", 300.0, 20.65e-6, 0.02648),
        ("Ar", 300.0, 22.74e-6, 0.01772),
        ("CO2", 300.0, 15.02e-6, 0.01681),
        ("CH4", 300.0, 11.17e-6, 0.03415),
        ("H2", 300.0, 8.96e-6, 0.1853),
        ("He", 300.0, 19.95e-6, 0.1559),
        ("H2O", 400.0, 13.34e-6, 0.02664),
    ] {
        let (eta, lambda) = pure(&sys, sys.index(name).unwrap(), t).unwrap();
        assert!(
            (eta / eta_ref - 1.0).abs() < 0.03,
            "{name}: η {eta}, reference {eta_ref}"
        );
        assert!(
            (lambda / lambda_ref - 1.0).abs() < 0.08,
            "{name}: λ {lambda}, reference {lambda_ref}"
        );
    }
}

/// Air as its nitrogen, oxygen and argon by Wilke's rule: 18.54 µPa·s and 26.24 mW/(m·K) at
/// 300 K (Lemmon et al.).
#[test]
fn air_mixes_to_its_viscosity_and_conductivity() {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &["N2", "O2", "Ar"]).unwrap();
    let mut moles = vec![0.0; 3];
    for (name, x) in [("N2", 0.7812), ("O2", 0.2096), ("Ar", 0.0092)] {
        moles[sys.index(name).unwrap()] = x;
    }
    let (eta, lambda) = mixture(&sys, &moles, 300.0);
    assert!((eta / 18.54e-6 - 1.0).abs() < 0.02, "η {eta}");
    assert!((lambda / 0.02624 - 1.0).abs() < 0.05, "λ {lambda}");
}
