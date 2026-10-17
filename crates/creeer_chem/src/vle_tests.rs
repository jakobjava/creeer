//! Vapour pressures, boiling and liquid densities against references: water against
//! IAPWS-95's saturation line (W. Wagner and A. Pruß, J. Phys. Chem. Ref. Data 31, 2002),
//! organic liquids against their measured boiling points and densities (CRC Handbook).

use crate::real::critical;
use crate::vle::{Volatile, bubble_point, flash};
use creeer_materials::species::SpeciesTable;
use creeer_math::det;

fn volatile<'a>(table: &'a SpeciesTable, gas: &str, liquid: &str) -> Volatile<'a> {
    Volatile::new(table.get(gas).unwrap(), table.get(liquid).unwrap())
}

/// The ISA standard atmosphere's pressure at altitude `h` (m) in the troposphere (Pa).
fn atmosphere(h: f64) -> f64 {
    101_325.0 * det::pow(1.0 - 2.255_77e-5 * h, 5.255_88)
}

/// Water's vapour pressure from its gas's and its liquid's Gibbs energies, its vapour's
/// second virial coefficient and its liquid's Poynting factor, against IAPWS-95. Measured:
/// 0.01 % at 300 K, 0.13 % at 400 K, 0.35 % at 450 K, 0.9 % at 500 K, 1.9 % at 550 K (the
/// third virial term grows towards the critical point). As an ideal gas it was 0.9 % low
/// at 350 K and 21 % at 550 K.
#[test]
fn water_vapour_pressure_follows_iapws() {
    let table = SpeciesTable::reference();
    let w = volatile(&table, "H2O", "H2O(L)");
    for (t, iapws, within) in [
        (300.0, 3_536.8, 0.002),
        (350.0, 41_682.0, 0.002),
        (400.0, 245_770.0, 0.002),
        (450.0, 932_030.0, 0.005),
        (500.0, 2_639_200.0, 0.012),
        (550.0, 6_117_800.0, 0.025),
    ] {
        let p = w.vapour_pressure(t);
        assert!(
            (p / iapws - 1.0).abs() < within,
            "{t} K: {p} Pa, IAPWS {iapws}"
        );
    }
}

/// Water boils at 373.12 K at sea level, 363.1 K at 3000 m, 343.3 K on Everest's summit
/// (IAPWS-95's saturation temperatures at the ISA pressures).
#[test]
fn water_boils_lower_up_a_mountain() {
    let table = SpeciesTable::reference();
    let w = volatile(&table, "H2O", "H2O(L)");
    for (h, iapws) in [(0.0, 373.124), (3000.0, 363.13), (8849.0, 343.35)] {
        let t = w.boiling_point(atmosphere(h)).unwrap();
        assert!(
            (t - iapws).abs() < 0.1,
            "{h} m: boils at {t} K, IAPWS {iapws}"
        );
    }
}

/// Organic liquids' normal boiling points (CRC): their NASA data are the CEA program's
/// reactants' (TRC fits of 1984–86, made for their heats of combustion), their entropies a
/// little off their gases': benzene boils 2.1 K high, toluene 5.2 K, ethanol 4.4 K
/// (without a virial term: its hydrogen bonds), methanol 0.8 K (measured; distillation-grade
/// vapour pressures are G5's).
#[test]
fn organic_liquids_boil_near_their_measured_points() {
    let table = SpeciesTable::reference();
    for (gas, liquid, measured, within) in [
        ("C6H6", "C6H6(L)", 353.24, 2.5),
        ("C7H8", "C7H8(L)", 383.75, 5.5),
        ("C2H5OH", "C2H5OH(L)", 351.44, 4.8),
        ("CH3OH", "CH3OH(L)", 337.85, 1.0),
    ] {
        let t = volatile(&table, gas, liquid)
            .boiling_point(101_325.0)
            .unwrap();
        assert!(
            (t - measured).abs() < within,
            "{gas} boils at {t} K, measured {measured}"
        );
    }
}

/// Raoult's law and the flash: an equimolar benzene–toluene liquid bubbles where its
/// components' vapour pressures, weighted by their fractions, make the pressure, the first
/// vapour rich in the lighter benzene; between its bubble and dew points a flash splits it
/// with each component's balance kept.
#[test]
fn raoult_and_the_flash_keep_their_balances() {
    let table = SpeciesTable::reference();
    let pair = [
        volatile(&table, "C6H6", "C6H6(L)"),
        volatile(&table, "C7H8", "C7H8(L)"),
    ];
    let atm = 101_325.0;
    let t = bubble_point(&pair, &[0.5, 0.5], atm, 300.0, 400.0);
    let total = 0.5 * pair[0].vapour_pressure(t) + 0.5 * pair[1].vapour_pressure(t);
    assert!((total / atm - 1.0).abs() < 1e-9);
    let y = 0.5 * pair[0].vapour_pressure(t) / atm;
    assert!(
        y > 0.65 && y < 0.75,
        "{y} benzene in the first vapour (Antoine: 0.71)"
    );
    let split = flash(&pair, &[0.5, 0.5], t + 3.0, atm);
    assert!(split.vapour > 0.0 && split.vapour < 1.0, "{}", split.vapour);
    assert!(split.vapour_fractions[0] > 0.5 && split.liquid_fractions[0] < 0.5);
    for i in 0..2 {
        let z = (1.0 - split.vapour) * split.liquid_fractions[i]
            + split.vapour * split.vapour_fractions[i];
        assert!((z - 0.5).abs() < 1e-9, "component {i}: {z}");
    }
    assert_eq!(
        flash(&pair, &[0.5, 0.5], t - 5.0, atm).vapour,
        0.0,
        "below its bubble point: liquid"
    );
}

/// Liquid densities: water's along its saturation line (IAPWS-95: 958.35, 890.34 and 755.97
/// kg/m³ at 373.15, 450 and 550 K), organic liquids at their boiling points by Rackett from
/// 298.15 K (CRC: benzene 815, toluene 780, ethanol 737 kg/m³).
#[test]
fn liquid_densities_follow_their_references() {
    let table = SpeciesTable::reference();
    for (gas, t, measured, within) in [
        ("H2O", 373.15, 958.35, 0.001),
        ("H2O", 450.0, 890.34, 0.001),
        ("H2O", 550.0, 755.97, 0.002),
        ("C6H6", 353.24, 815.0, 0.01),
        ("C7H8", 383.75, 780.0, 0.01),
        ("C2H5OH", 351.44, 737.0, 0.025),
    ] {
        let m = table.get(gas).unwrap().molar_mass;
        let rho = m / critical(gas).unwrap().liquid_volume(t, m).unwrap();
        assert!(
            (rho / measured - 1.0).abs() < within,
            "{gas} at {t} K: {rho} kg/m³, measured {measured}"
        );
    }
}
