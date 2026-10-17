//! Equilibria of reactions with condensed phases and at pressure, against handbook results:
//! limestone's calcination, the Boudouard equilibrium, wüstite reduced by carbon monoxide,
//! the Ellingham crossover of carbon and iron, ammonia's synthesis.

use crate::{Equilibrium, Fixed, System, equilibrate};
use creeer_materials::species::{R, SpeciesTable};
use creeer_math::det;

const ATM: f64 = 101_325.0;
const BAR: f64 = 1e5;

/// The equilibrium of `feed` (species, moles) at `t` and `p` among `names`.
fn at(
    table: &SpeciesTable,
    names: &[&str],
    feed: &[(&str, f64)],
    t: f64,
    p: f64,
) -> (System, Equilibrium) {
    let sys = System::new(table, names).unwrap();
    let mut moles = vec![0.0; sys.species.len()];
    for (name, n) in feed {
        moles[sys.index(name).unwrap()] = *n;
    }
    let b0 = sys.element_moles(&moles);
    let eq = equilibrate(&sys, &moles, Fixed::Tp { t, p }, None).unwrap();
    for (b, e) in b0.iter().zip(sys.element_moles(&eq.moles)) {
        assert!(
            (b - e).abs() < 1e-6 * b.max(1e-30),
            "element balance {b} vs {e}"
        );
    }
    (sys, eq)
}

fn mole_fraction(sys: &System, eq: &Equilibrium, name: &str) -> f64 {
    let gas: f64 = eq.moles[..sys.gases].iter().sum();
    eq.moles[sys.index(name).unwrap()] / gas
}

/// CaCO₃ ⇌ CaO + CO₂: limestone gives off its CO₂ at one atmosphere from 1170 K (the lime
/// kiln's temperature; Gaskell, Introduction to the Thermodynamics of Materials). In a kiln's
/// nitrogen below it, calcite stays under CO₂ at its decomposition pressure K_p·P°; above
/// it, where K_p·P° exceeds the atmosphere, it all turns to lime. (An equilibrium needs
/// some gas, as the CEA program's does: below the temperature, calcite under CO₂ alone has
/// none.)
#[test]
fn limestone_calcines_at_1170_kelvin_under_an_atmosphere_of_co2() {
    let table = SpeciesTable::reference();
    let s = |n: &str| table.get(n).unwrap();
    // ΔG = 0 at p_CO2 = 1 atm: g(CaCO3) = g(CaO) + g(CO2) + ln(1 atm / 1 bar).
    let gap = |t: f64| {
        s("CaO(cr)").g_rt(t) + s("CO2").g_rt(t) + det::ln(ATM / BAR) - s("CaCO3(cr)").g_rt(t)
    };
    let (mut lo, mut hi) = (900.0, 1400.0);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if gap(mid) > 0.0 { lo = mid } else { hi = mid }
    }
    assert!((lo - 1170.0).abs() < 15.0, "calcination at {lo} K");
    let names = ["CO2", "N2", "CaCO3(cr)", "CaO(cr)"];
    // A little nitrogen: holding K_p·P° of CO₂ over it takes a little of the calcite's CO₂.
    let feed = [("CaCO3(cr)", 1.0), ("N2", 0.01)];
    // Below: calcite stays, the CO₂ over it at K_p·P°.
    let t = lo - 30.0;
    let (sys, eq) = at(&table, &names, &feed, t, ATM);
    assert!(
        eq.moles[sys.index("CaCO3(cr)").unwrap()] > 0.5,
        "{t} K: calcite stays"
    );
    let kp = det::exp(-(s("CaO(cr)").g_rt(t) + s("CO2").g_rt(t) - s("CaCO3(cr)").g_rt(t)));
    let p_co2 = mole_fraction(&sys, &eq, "CO2") * ATM / BAR;
    assert!(
        (p_co2 / kp - 1.0).abs() < 1e-4,
        "{t} K: p_CO2 {p_co2} bar, K_p {kp}"
    );
    // Above: all of it lime.
    let t = lo + 30.0;
    let (sys, eq) = at(&table, &names, &feed, t, ATM);
    assert_eq!(
        eq.moles[sys.index("CaCO3(cr)").unwrap()],
        0.0,
        "{t} K: calcite gone"
    );
    assert!((eq.moles[sys.index("CaO(cr)").unwrap()] - 1.0).abs() < 1e-9);
}

/// C + CO₂ ⇌ 2 CO at 1000 K, 1 atm: K_p = 1.752 (from JANAF's ΔG_f: CO −200.275, CO₂
/// −395.886 kJ/mol), x_CO²·P/(x_CO₂·P°) = K_p: 74.7 % CO over carbon.
#[test]
fn the_boudouard_equilibrium_over_carbon() {
    let table = SpeciesTable::reference();
    let names = ["CO", "CO2", "O2", "C(gr)"];
    let (sys, eq) = at(&table, &names, &[("CO2", 1.0), ("C(gr)", 2.0)], 1000.0, ATM);
    let k = (2.0 * 200_275.0 - 395_886.0) / (R * 1000.0);
    let kp = det::exp(k) * BAR / ATM;
    let x = 0.5 * (-kp + (kp * kp + 4.0 * kp).sqrt());
    let co = mole_fraction(&sys, &eq, "CO");
    assert!((co - x).abs() < 0.005, "{co} CO, JANAF {x}");
    assert!(eq.moles[sys.index("C(gr)").unwrap()] > 0.0, "carbon left");
}

/// Fe₀.₉₄₇O + CO ⇌ 0.947 Fe + CO₂: the gas over wüstite and iron, log₁₀ K = 688/T − 0.90
/// (Darken and Gurry, 1945): 62 % CO at 1000 K, 70 % at 1273 K, the blast furnace's
/// boundary between reducing and not.
#[test]
fn wustite_is_reduced_by_carbon_monoxide() {
    let table = SpeciesTable::reference();
    let names = [
        "CO",
        "CO2",
        "O2",
        "Fe(a)",
        "Fe(c)",
        "Fe.947O(cr)",
        "Fe3O4(cr)",
    ];
    for t in [1000.0, 1273.0] {
        // Enough oxide and gas that both solids remain: the gas is the one in equilibrium.
        let (sys, eq) = at(
            &table,
            &names,
            &[("Fe.947O(cr)", 2.0), ("Fe(a)", 1.0), ("CO", 1.0)],
            t,
            ATM,
        );
        let k = det::pow(10.0, 688.0 / t - 0.90);
        let expected = 1.0 / (1.0 + k);
        let co = mole_fraction(&sys, &eq, "CO");
        assert!(
            (co - expected).abs() < 0.03,
            "{t} K: {co} CO, Darken & Gurry {expected}"
        );
    }
}

/// The Ellingham diagram: 2 C + O₂ → 2 CO falls with temperature, 2 Fe + O₂ → 2 FeO rises;
/// above their crossover (985 K from Gaskell's fits, −223.4 − 0.1753·T and −519.2 + 0.1251·T
/// kJ) carbon takes oxygen from iron.
#[test]
fn carbon_reduces_iron_oxide_above_the_ellingham_crossover() {
    let table = SpeciesTable::reference();
    let s = |n: &str| table.get(n).unwrap();
    let fe = |t: f64| if t < 1184.0 { s("Fe(a)") } else { s("Fe(c)") };
    // Per mole of O₂: 2/0.947 ... as 2 Fe₀.₉₄₇O from 1.894 Fe.
    let iron =
        |t: f64| R * t * (2.0 * s("Fe.947O(cr)").g_rt(t) - 1.894 * fe(t).g_rt(t) - s("O2").g_rt(t));
    let carbon =
        |t: f64| R * t * (2.0 * s("CO").g_rt(t) - 2.0 * s("C(gr)").g_rt(t) - s("O2").g_rt(t));
    let (mut lo, mut hi) = (700.0, 1400.0);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if carbon(mid) > iron(mid) {
            lo = mid
        } else {
            hi = mid
        }
    }
    assert!((lo - 985.0).abs() < 40.0, "crossover at {lo} K");
}

/// N₂ + 3 H₂ ⇌ 2 NH₃ at 700 K: K_p = exp(−ΔG_f/RT) with JANAF's ΔG_f(NH₃, 700 K) = 27.190
/// kJ/mol; as an ideal gas from a 1 : 3 feed, the yield at 1 bar and at 300 bar (Le
/// Chatelier: pressure drives it).
#[test]
fn ammonia_yield_rises_with_pressure() {
    let table = SpeciesTable::reference();
    let names = ["N2", "H2", "NH3"];
    let kp = det::exp(-27_190.0 / (R * 700.0));
    for (p, low, high) in [(BAR, 0.0, 0.05), (300.0 * BAR, 0.2, 0.5)] {
        // x_NH3 / (x_N2^½ · x_H2^{3/2} · P/P°) = K_p, from 1 N₂ + 3 H₂ forming 2y NH₃.
        let ratio = |y: f64| {
            let total = 4.0 - 2.0 * y;
            let (a, n, h) = (2.0 * y / total, (1.0 - y) / total, (3.0 - 3.0 * y) / total);
            a / (n.sqrt() * det::pow(h, 1.5) * p / BAR)
        };
        let (mut lo, mut hi) = (0.0, 1.0 - 1e-12);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if ratio(mid) < kp { lo = mid } else { hi = mid }
        }
        let expected = 2.0 * lo / (4.0 - 2.0 * lo);
        let (sys, eq) = at(&table, &names, &[("N2", 1.0), ("H2", 3.0)], 700.0, p);
        let nh3 = mole_fraction(&sys, &eq, "NH3");
        assert!(
            (nh3 / expected - 1.0).abs() < 0.02,
            "{p} Pa: {nh3} NH3, from JANAF {expected}"
        );
        assert!(nh3 > low && nh3 < high, "{p} Pa: {nh3}");
    }
}

/// Hematite reduced by ever more carbon monoxide at 1100 K walks down iron's oxides in order:
/// hematite and magnetite, magnetite, magnetite and wüstite, wüstite, wüstite and iron, iron
/// (the Baur–Glaessner sequence): never three iron phases at once (the phase rule; a third
/// replaces one), the oxygen left in the solids only falling.
#[test]
fn iron_ore_walks_down_its_oxides() {
    let table = SpeciesTable::reference();
    let names = [
        "CO",
        "CO2",
        "O2",
        "N2",
        "Fe(a)",
        "Fe(c)",
        "Fe.947O(cr)",
        "Fe3O4(cr)",
        "Fe2O3(cr)",
    ];
    let iron = ["Fe(a)", "Fe(c)", "Fe.947O(cr)", "Fe3O4(cr)", "Fe2O3(cr)"];
    let mut last_oxygen = f64::INFINITY;
    let mut seen = Vec::new();
    for k in 0..=40 {
        let co = 0.25 * f64::from(k);
        let (sys, eq) = at(
            &table,
            &names,
            &[("Fe2O3(cr)", 1.0), ("CO", co), ("N2", 1.0)],
            1100.0,
            ATM,
        );
        let present: Vec<&str> = iron
            .iter()
            .copied()
            .filter(|n| eq.moles[sys.index(n).unwrap()] > 1e-9)
            .collect();
        assert!(present.len() <= 2, "{co} mol CO: {present:?}");
        let oxygen: f64 = present
            .iter()
            .map(|n| {
                eq.moles[sys.index(n).unwrap()]
                    * sys.species[sys.index(n).unwrap()]
                        .formula
                        .atoms
                        .get(&8)
                        .copied()
                        .unwrap_or(0.0)
            })
            .sum();
        assert!(
            oxygen <= last_oxygen + 1e-9,
            "{co} mol CO: the solids gained oxygen"
        );
        last_oxygen = oxygen;
        if seen.last() != Some(&present) {
            seen.push(present);
        }
    }
    let expected: Vec<Vec<&str>> = vec![
        vec!["Fe2O3(cr)"],
        vec!["Fe3O4(cr)", "Fe2O3(cr)"],
        vec!["Fe3O4(cr)"],
        vec!["Fe.947O(cr)", "Fe3O4(cr)"],
        vec!["Fe.947O(cr)"],
        vec!["Fe(a)", "Fe.947O(cr)"],
        vec!["Fe(a)"],
    ];
    assert_eq!(seen, expected);
}
