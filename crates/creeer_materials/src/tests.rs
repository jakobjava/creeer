use crate::elements::{ELEMENTS, atomic_number, atomic_weight};
use crate::{Formula, FormulaError};

fn molar_mass_g(text: &str) -> f64 {
    Formula::parse(text)
        .unwrap_or_else(|e| panic!("{e}"))
        .molar_mass()
        * 1e3
}

#[test]
fn element_table_is_ordered_and_complete() {
    assert_eq!(ELEMENTS.len(), 92);
    assert_eq!(atomic_number("H"), Some(1));
    assert_eq!(atomic_number("Fe"), Some(26));
    assert_eq!(atomic_number("U"), Some(92));
    assert_eq!(atomic_number("Xx"), None);
    // Atomic weights rise with atomic number except for the known inversions
    // (Ar/K, Co/Ni, Te/I, Th/Pa).
    let inversions = ["K", "Ni", "I", "Pa"];
    for pair in ELEMENTS.windows(2) {
        if !inversions.contains(&pair[1].0) {
            assert!(pair[1].1 > pair[0].1, "{} after {}", pair[1].0, pair[0].0);
        }
    }
    assert!((atomic_weight(8) - 15.999).abs() < 1e-9);
}

#[test]
fn molar_masses_of_known_compounds() {
    // Reference values (g/mol) from the standard atomic weights.
    for (formula, expected) in [
        ("H2O", 18.015),
        ("SiO2", 60.083),
        ("CaCO3", 100.086),
        ("CuFeS2", 183.521),
        ("Fe3O4", 231.533),
        ("Al(OH)3", 78.003),
        ("CaMg(CO3)2", 184.398),
        ("CaSO4·2H2O", 172.168),
        ("KAlSi3O8", 278.33),
        ("Ca5(PO4)3F", 504.30),
    ] {
        let m = molar_mass_g(formula);
        assert!((m - expected).abs() < 0.01, "{formula}: {m} g/mol");
    }
}

#[test]
fn parentheses_hydrates_and_solid_solutions() {
    let gypsum = Formula::parse("CaSO4·2H2O").expect("parses");
    assert_eq!(gypsum.atoms[&1], 4.0); // H
    assert_eq!(gypsum.atoms[&8], 6.0); // O
    assert_eq!(Formula::parse("CaSO4*2H2O").expect("parses"), gypsum);
    let pentlandite = Formula::parse("Fe4.5Ni4.5S8").expect("parses");
    assert_eq!(pentlandite.atoms[&26], 4.5);
    assert_eq!(pentlandite.atoms[&28], 4.5);
    let nested = Formula::parse("K(Mg3)(AlSi3O10)(OH)2").expect("parses");
    assert_eq!(nested.atoms[&12], 3.0);
    assert_eq!(nested.atoms[&8], 12.0);
}

#[test]
fn mass_fractions() {
    // Chalcopyrite is 34.6 % copper by mass; magnetite 72.4 % iron.
    let cpy = Formula::parse("CuFeS2").expect("parses");
    assert!((cpy.mass_fraction(29) - 0.3463).abs() < 1e-3);
    let mag = Formula::parse("Fe3O4").expect("parses");
    assert!((mag.mass_fraction(26) - 0.7236).abs() < 1e-3);
    let total: f64 = cpy.atoms.keys().map(|&z| cpy.mass_fraction(z)).sum();
    assert!((total - 1.0).abs() < 1e-12);
}

#[test]
fn malformed_formulas_are_rejected() {
    for text in ["", "cu", "Xx2", "(H2O", "H2O)", "()", "2", "Fe··O"] {
        assert!(
            matches!(Formula::parse(text), Err(FormulaError(_))),
            "{text}"
        );
    }
}
