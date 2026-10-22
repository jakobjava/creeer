//! The shipped materials load, follow their standards, and obey the physics that ties their
//! properties together.

use crate::{Curve, Kind, MaterialTable};

const C: f64 = 273.15;

#[test]
fn the_shipped_materials_load() {
    let table = MaterialTable::reference();
    assert_eq!(table.all().len(), 10);
    for id in [
        "steel_s235",
        "aluminium_6061_t6",
        "copper_c11000",
        "spruce_c24",
        "spruce_bast",
        "concrete_c30",
        "ice",
    ] {
        assert!(table.get(id).is_some(), "{id}");
    }
}

#[test]
fn steel_follows_en_1993_1_2_in_a_fire() {
    let table = MaterialTable::reference();
    let s235 = table.get("steel_s235").unwrap();
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs();
    // Reduction factors (Table 3.1): yield 0.47 and stiffness 0.31 at 600 °C.
    assert!(close(s235.yield_strength.at(600.0 + C), 0.47 * 235e6));
    assert!(close(s235.youngs_modulus.at(600.0 + C), 0.31 * 210e9));
    // Conductivity 54 − 0.0333·θ, straight between its ends: exact anywhere on the line.
    assert!((s235.conductivity.at(400.0 + C) - (54.0 - 0.0333 * 400.0)).abs() < 0.01);
    // The heat-capacity peak of the austenite transition.
    assert!(close(s235.specific_heat.at(735.0 + C), 5000.0));
    // Interpolated between table points, constant beyond.
    assert!(close(
        s235.yield_strength.at(450.0 + C),
        0.5 * (235e6 + 183.3e6)
    ));
    assert!(close(s235.yield_strength.at(2000.0 + C), 4.7e6));
    assert!(close(s235.yield_strength.at(-40.0 + C), 235e6));
}

#[test]
fn good_conductors_obey_wiedemann_franz() {
    // Electrons carry both heat and charge in metals: k·ρ/T = L = 2.44e-8 W·Ω/K² (within the
    // scatter of alloys, whose lattice adds a little heat conduction).
    let lorenz = 2.44e-8;
    let table = MaterialTable::reference();
    for m in table.all().iter().filter(|m| m.kind == Kind::Metal) {
        let t = 20.0 + C;
        let (k, rho) = (m.conductivity.at(t), m.resistivity.at(t));
        if k < 50.0 {
            continue; // titanium alloys: lattice conduction dominates
        }
        let ratio = k * rho / t / lorenz;
        assert!(
            (0.8..1.25).contains(&ratio),
            "{}: k·ρ/(L·T) = {ratio:.2}",
            m.id
        );
    }
}

#[test]
fn isotropic_materials_have_a_poisson_ratio_between_0_and_one_half() {
    let table = MaterialTable::reference();
    for m in table.all().iter().filter(|m| m.kind != Kind::Wood) {
        for &(t, e) in m.youngs_modulus.points() {
            let nu = e / (2.0 * m.shear_modulus.at(t)) - 1.0;
            assert!(
                (0.15..0.45).contains(&nu),
                "{} at {:.0} K: ν = {nu:.3}",
                m.id,
                t
            );
        }
    }
}

#[test]
fn heat_spreads_through_metals_far_faster_than_through_wood() {
    let table = MaterialTable::reference();
    let a = |id: &str| table.get(id).unwrap().diffusivity(293.15);
    // Copper ≈ 1.1e-4 m²/s, steel ≈ 1.5e-5, wood ≈ 2e-7.
    assert!((1.0e-4..1.3e-4).contains(&a("copper_c11000")));
    assert!((1.3e-5..1.7e-5).contains(&a("steel_s235")));
    assert!((1e-7..3e-7).contains(&a("spruce_c24")));
}

#[test]
fn bad_data_is_refused() {
    let good = include_str!("../../../data/materials/engineering.ron");
    let start = good.find("    (\n        id: \"steel_s235\"").unwrap();
    let end = start + good[start..].find("\n    ),").unwrap() + 6;
    let twice = format!("[{0}, {0}]", &good[start..end]);
    assert!(MaterialTable::parse(&twice).unwrap_err().contains("twice"));
    let wrong_unit = good.replacen("density: \"7850 kg/m^3\"", "density: \"7850 kg\"", 1);
    assert!(MaterialTable::parse(&wrong_unit).is_err());
    let weak = good.replacen(
        "ultimate_strength: \"220 MPa\"",
        "ultimate_strength: \"20 MPa\"",
        1,
    );
    assert!(
        MaterialTable::parse(&weak)
            .unwrap_err()
            .contains("ultimate")
    );
    assert!(Curve::new(vec![(300.0, 1.0), (300.0, 2.0)]).is_err());
}

#[test]
fn steel_lengthens_as_en_1993_1_2_says() {
    // EN 1993-1-2 3.4.1.1: Δl/l against θ (°C), from 20 °C.
    let en = |c: f64| {
        if c < 750.0 {
            1.2e-5 * c + 0.4e-8 * c * c - 2.416e-4
        } else if c <= 860.0 {
            1.1e-2
        } else {
            2e-5 * c - 6.2e-3
        }
    };
    let table = MaterialTable::reference();
    let steel = table.get("steel_s235").unwrap();
    for c in [100.0, 400.0, 700.0, 800.0, 1000.0, 1200.0] {
        let ours = steel.expansion.integral(293.15, 273.15 + c);
        assert!((ours - en(c)).abs() < 5e-5, "{c} °C: {ours} vs {}", en(c));
    }
    // Cooling back down undoes it, exactly.
    let up = steel.expansion.integral(293.15, 900.0);
    assert!((up + steel.expansion.integral(900.0, 293.15)).abs() < 1e-18);
}
