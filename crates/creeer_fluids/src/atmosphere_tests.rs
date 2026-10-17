//! The atmosphere on an Earth-sized planet against the US Standard Atmosphere 1976 (NOAA,
//! NASA, USAF): its base pressures by geopotential altitude, sea level's density, speed of
//! sound and viscosity. (The standard uses R* = 8.31432 and M = 28.9644 g/mol; this uses
//! the exact R and the gases' molar masses: 5·10⁻⁴ apart at 71 km.)

use crate::atmosphere::Atmosphere;
use creeer_materials::species::SpeciesTable;
use creeer_terrain::AtmosphereData;

/// The standard's Earth: r₀ = 6356.766 km, g₀ = 9.80665 m/s².
const R0: f64 = 6_356_766.0;
const G0: f64 = 9.80665;

pub(crate) fn standard() -> AtmosphereData {
    AtmosphereData {
        composition: vec![
            ("N2".into(), 0.78084),
            ("O2".into(), 0.20946),
            ("Ar".into(), 0.00934),
            ("CO2".into(), 0.00036),
        ],
        surface_pressure: 101_325.0,
        surface_temperature: 288.15,
        layers: vec![
            (11_000.0, -6.5e-3),
            (20_000.0, 0.0),
            (32_000.0, 1.0e-3),
            (47_000.0, 2.8e-3),
            (51_000.0, 0.0),
            (71_000.0, -2.8e-3),
            (84_852.0, -2.0e-3),
        ],
    }
}

#[test]
fn the_standard_atmosphere_on_an_earth_sized_planet() {
    let table = SpeciesTable::reference();
    let air = Atmosphere::new(&standard(), R0, G0 * R0 * R0, &table).unwrap();
    // Base pressures at geopotential altitudes 11, 20, 32, 47, 51, 71 km.
    for (h, pb) in [
        (11_000.0, 22_632.06),
        (20_000.0, 5_474.889),
        (32_000.0, 868.0187),
        (47_000.0, 110.9063),
        (51_000.0, 66.93887),
        (71_000.0, 3.956_420),
    ] {
        let z = R0 * h / (R0 - h);
        let p = air.at(z).p;
        assert!(
            (p / pb - 1.0).abs() < 1e-3,
            "H {h} m: {p} Pa, standard {pb}"
        );
    }
    let sea = air.at(0.0);
    assert!((sea.density - 1.2250).abs() < 2e-4, "ρ {}", sea.density);
    assert!((sea.sound - 340.294).abs() < 0.2, "a {}", sea.sound);
    // The standard's Sutherland viscosity; this is Wilke's mixture of its gases, measured
    // 0.7 % above Lemmon and Jacobsen's air at 300 K (`creeer_chem::transport`): 1.1 % here.
    assert!(
        (sea.viscosity / 1.7894e-5 - 1.0).abs() < 0.015,
        "μ {}",
        sea.viscosity
    );
    let high = air.at(R0 * 11_000.0 / (R0 - 11_000.0));
    assert!(
        (high.t - 216.65).abs() < 1e-9 && (high.density - 0.36392).abs() < 3e-4,
        "{high:?}"
    );
}

/// On home (500 km, 9.81 m/s² at the datum) gravity weakens with height ten times faster
/// than on Earth: the same pressure lies higher (half the sea level's at 5.6 km, Earth's
/// 5.5 km) and the air thins slower.
#[test]
fn a_small_planets_air_reaches_higher() {
    let table = SpeciesTable::reference();
    let (r, g) = (500_000.0, 9.81);
    let small = Atmosphere::new(&standard(), r, g * r * r, &table).unwrap();
    let earth = Atmosphere::new(&standard(), R0, G0 * R0 * R0, &table).unwrap();
    let half = |a: &Atmosphere| {
        let (mut lo, mut hi) = (0.0, 20_000.0);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if a.at(mid).p > 0.5 * 101_325.0 {
                lo = mid
            } else {
                hi = mid
            }
        }
        lo
    };
    assert!(
        half(&small) > half(&earth),
        "{} vs {}",
        half(&small),
        half(&earth)
    );
    assert!(small.at(80_000.0).density > earth.at(80_000.0).density);
}
