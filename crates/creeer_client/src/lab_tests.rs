//! The lab's experiments as the physics says: its Ellingham diagram ordered as the
//! textbook's, its furnace walking the ore down to iron, its relaxing gas settling at its
//! equilibrium, its presets equilibrating.

use crate::lab_runs::{Furnace, Relaxation};
use creeer_materials::species::SpeciesTable;

fn at(line: &[[f64; 2]], celsius: f64) -> f64 {
    line.iter()
        .min_by(|a, b| (a[0] - celsius).abs().total_cmp(&(b[0] - celsius).abs()))
        .unwrap()[1]
}

/// Aluminium's and magnesium's oxides lowest, copper's highest, carbon monoxide's line
/// falling and crossing iron's near 700 °C (Gaskell's fits: 712 °C).
#[test]
fn the_ellingham_diagram_is_ordered_as_the_textbooks() {
    let table = SpeciesTable::reference();
    let lines = crate::lab_ellingham::lines(&table);
    let line = |prefix: &str| &lines.iter().find(|l| l.0.starts_with(prefix)).unwrap().1;
    let (al, cu, c, fe) = (line("4/3 Al"), line("4 Cu"), line("2 C +"), line("2 Fe"));
    for celsius in [200.0, 800.0, 1400.0] {
        assert!(
            at(al, celsius) < at(fe, celsius) && at(fe, celsius) < at(cu, celsius),
            "{celsius} °C"
        );
    }
    assert!(
        at(c, 1400.0) < at(c, 200.0),
        "C → CO falls with temperature"
    );
    assert!(
        at(c, 500.0) > at(fe, 500.0) && at(c, 900.0) < at(fe, 900.0),
        "carbon takes iron's oxygen above ~700 °C"
    );
}

/// Carbon monoxide through hot hematite turns it all to iron, through magnetite and wüstite.
#[test]
fn the_furnace_reduces_its_ore_to_iron() {
    let table = SpeciesTable::reference();
    let mut furnace = Furnace::new(&table);
    let mut most_wustite = 0.0f64;
    for _ in 0..3000 {
        furnace.tick(0.1);
        most_wustite = most_wustite.max(furnace.solids[2].back().unwrap()[1]);
    }
    let iron = furnace.solids[3].back().unwrap()[1];
    assert!(
        (iron - 4.0).abs() < 1e-6,
        "{iron} mol of iron from 2 of hematite"
    );
    assert!(most_wustite > 1.0, "through wüstite");
}

/// The brown gas settles where its equilibrium is, warmer more dissociated.
#[test]
fn the_relaxing_gas_settles_at_its_equilibrium() {
    let table = SpeciesTable::reference();
    let mut r = Relaxation::new(&table);
    for t in [300.0, 380.0] {
        r.t = t;
        for _ in 0..600 {
            r.tick(0.1);
        }
        let x = r.trace.back().unwrap()[1];
        assert!(
            (x - r.equilibrium).abs() < 1e-6,
            "{t} K: {x} against {}",
            r.equilibrium
        );
    }
    assert!(
        r.equilibrium > 0.8,
        "mostly NO₂ at 380 K: {}",
        r.equilibrium
    );
}
