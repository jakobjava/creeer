//! Reactors against closed forms (first and second order, a CSTR's steady state) and against
//! the equilibrium solver (kinetics with detailed balance end where equilibrium is), their
//! books closed.

use crate::kinetics::Reaction;
use crate::reactor::{Holding, Reactor};
use crate::{Fixed, System, equilibrate};
use creeer_materials::species::SpeciesTable;
use creeer_math::det;

const ATM: f64 = 101_325.0;

fn system(table: &SpeciesTable, names: &[&str]) -> System {
    System::new(table, names).unwrap()
}

fn feed(sys: &System, amounts: &[(&str, f64)]) -> Vec<f64> {
    let mut moles = vec![0.0; sys.species.len()];
    for (name, n) in amounts {
        moles[sys.index(name).unwrap()] = *n;
    }
    moles
}

fn run(r: &mut Reactor, sys: &System, seconds: f64, h: f64) {
    let mut t = 0.0;
    while t < seconds - 0.5 * h {
        t += h;
        assert!(r.step(sys, h).converged);
    }
}

fn closes(r: &Reactor, sys: &System) -> f64 {
    r.ledger().relative_imbalance(r.stored(sys))
}

/// A first-order decomposition, N₂O₅ → N₂O₄ + ½ O₂ (irreversible, k = 0.01 /s), held at
/// 300 K in a fixed volume: n = n₀·e^(−kt), to BDF2's error at k·h = 0.005.
#[test]
fn a_first_order_reaction_decays_exponentially() {
    let table = SpeciesTable::reference();
    let sys = system(&table, &["N2O5", "N2O4", "O2"]);
    let mut reaction = Reaction::new(
        &sys,
        &[("N2O5", 1.0)],
        &[("N2O4", 1.0), ("O2", 0.5)],
        0.01,
        0.0,
        0.0,
    )
    .unwrap();
    reaction.reversible = false;
    let mut r = Reactor::new(
        &sys,
        feed(&sys, &[("N2O5", 1.0)]),
        300.0,
        Holding::Volume(1.0),
        vec![reaction],
    );
    r.isothermal = true;
    run(&mut r, &sys, 100.0, 0.5);
    let left = r.moles[sys.index("N2O5").unwrap()];
    assert!(
        (left / det::exp(-1.0) - 1.0).abs() < 1e-4,
        "{left} mol left, e⁻¹ = {}",
        det::exp(-1.0)
    );
    assert!(closes(&r, &sys) < 1e-9, "ledger {}", closes(&r, &sys));
}

/// A second-order one, 2 NO₂ → 2 NO + O₂ (rate k·c², k = 0.01 m³/(mol·s)): 1/c = 1/c₀ + 2kt.
#[test]
fn a_second_order_reaction_follows_its_hyperbola() {
    let table = SpeciesTable::reference();
    let sys = system(&table, &["NO2", "NO", "O2"]);
    let mut reaction = Reaction::new(
        &sys,
        &[("NO2", 2.0)],
        &[("NO", 2.0), ("O2", 1.0)],
        0.01,
        0.0,
        0.0,
    )
    .unwrap();
    reaction.reversible = false;
    let mut r = Reactor::new(
        &sys,
        feed(&sys, &[("NO2", 1.0)]),
        300.0,
        Holding::Volume(1.0),
        vec![reaction],
    );
    r.isothermal = true;
    run(&mut r, &sys, 100.0, 0.25);
    let c = r.moles[sys.index("NO2").unwrap()];
    assert!((c - 1.0 / 3.0).abs() < 1e-4, "{c} mol/m³, closed form 1/3");
}

/// N₂O₄ ⇌ 2 NO₂ with detailed balance runs to the very equilibrium the Gibbs minimiser finds,
/// and steps far longer than the reaction's time land on it rather than past it.
#[test]
fn reversible_kinetics_end_at_equilibrium() {
    let table = SpeciesTable::reference();
    let sys = system(&table, &["N2O4", "NO2"]);
    let reaction = Reaction::new(&sys, &[("N2O4", 1.0)], &[("NO2", 2.0)], 50.0, 0.0, 0.0).unwrap();
    let start = feed(&sys, &[("N2O4", 1.0)]);
    let eq = equilibrate(&sys, &start, Fixed::Tp { t: 320.0, p: ATM }, None).unwrap();
    for h in [0.001, 10.0] {
        let mut r = Reactor::new(
            &sys,
            start.clone(),
            320.0,
            Holding::Pressure(ATM),
            vec![reaction.clone()],
        );
        r.isothermal = true;
        run(&mut r, &sys, 20.0, h);
        let no2 = r.moles[sys.index("NO2").unwrap()];
        let target = eq.moles[sys.index("NO2").unwrap()];
        assert!(
            (no2 / target - 1.0).abs() < 1e-6,
            "h {h}: {no2} mol NO₂, equilibrium {target}"
        );
        assert!(closes(&r, &sys) < 1e-9, "ledger {}", closes(&r, &sys));
    }
}

/// Hydrogen burning adiabatically at constant pressure through one stiff global step (its
/// rate millions of times its step): it ends at the adiabatic equilibrium of the same species
/// (the Gibbs minimiser's, at the same enthalpy) and its enthalpy never moves.
#[test]
fn an_adiabatic_burn_ends_at_the_adiabatic_equilibrium() {
    let table = SpeciesTable::reference();
    let sys = system(&table, &["H2", "O2", "H2O"]);
    let reaction = Reaction::new(
        &sys,
        &[("H2", 2.0), ("O2", 1.0)],
        &[("H2O", 2.0)],
        1e6,
        0.0,
        0.0,
    )
    .unwrap();
    let start = feed(&sys, &[("H2", 2.0), ("O2", 1.0)]);
    let mut r = Reactor::new(
        &sys,
        start.clone(),
        1000.0,
        Holding::Pressure(ATM),
        vec![reaction],
    );
    let h0 = r.stored(&sys);
    run(&mut r, &sys, 1.0, 0.05);
    let eq = equilibrate(&sys, &start, Fixed::Hp { h: h0, p: ATM }, None).unwrap();
    assert!(
        (r.t - eq.t).abs() < 0.01 * 1.0,
        "{} K, adiabatic equilibrium {} K",
        r.t,
        eq.t
    );
    assert!(
        (r.stored(&sys) - h0).abs() < 1e-6 * h0.abs().max(1.0),
        "enthalpy moved"
    );
    assert!(closes(&r, &sys) < 1e-9, "ledger {}", closes(&r, &sys));
}

/// A CSTR: N₂O₅ fed at 0.1 mol/s into 1 m³ held at 300 K, residence 10 s, decomposing at
/// k = 0.1 /s: steady state n = F·τ/(1 + k·τ) = 0.5 mol.
#[test]
fn a_stirred_tank_reaches_its_steady_state() {
    let table = SpeciesTable::reference();
    let sys = system(&table, &["N2O5", "N2O4", "O2"]);
    let mut reaction = Reaction::new(
        &sys,
        &[("N2O5", 1.0)],
        &[("N2O4", 1.0), ("O2", 0.5)],
        0.1,
        0.0,
        0.0,
    )
    .unwrap();
    reaction.reversible = false;
    let mut r = Reactor::new(
        &sys,
        vec![0.0; 3],
        300.0,
        Holding::Volume(1.0),
        vec![reaction],
    );
    r.isothermal = true;
    r.feed = Some((feed(&sys, &[("N2O5", 0.1)]), 300.0));
    r.residence = Some(10.0);
    run(&mut r, &sys, 300.0, 0.5);
    let n = r.moles[sys.index("N2O5").unwrap()];
    assert!((n - 0.5).abs() < 1e-6, "{n} mol, steady state 0.5");
    assert!(closes(&r, &sys) < 1e-9, "ledger {}", closes(&r, &sys));
}
