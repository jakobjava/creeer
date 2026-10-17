//! Fluids and convection against their tables and independent correlations.

use crate::*;
use creeer_math::det;

const ATMOSPHERE: f64 = 101_325.0;
const G: f64 = 9.806_65;

/// Air at 300 K, 1 atm, against Incropera's Table A.4 (density against the ideal gas).
#[test]
fn air_properties() {
    let air = Fluid::air(300.0, ATMOSPHERE);
    let near = |x: f64, y: f64, tol: f64| (x / y - 1.0).abs() < tol;
    assert!(near(air.density, 1.177, 1e-3), "{}", air.density);
    assert!(near(air.viscosity, 184.6e-7, 5e-3), "{}", air.viscosity);
    assert!(
        near(air.conductivity, 26.3e-3, 5e-3),
        "{}",
        air.conductivity
    );
    assert!(near(air.prandtl(), 0.707, 5e-3), "{}", air.prandtl());
    // 600 K: μ 305.8e-7, k 46.9e-3 (Table A.4).
    let hot = Fluid::air(600.0, ATMOSPHERE);
    assert!(near(hot.viscosity, 305.8e-7, 1e-9) && near(hot.conductivity, 46.9e-3, 1e-9));
    // Beyond the table, continuous with it.
    let (edge, past) = (
        Fluid::air(1000.0, ATMOSPHERE),
        Fluid::air(1000.0 + 1e-9, ATMOSPHERE),
    );
    assert!(
        near(past.viscosity, edge.viscosity, 1e-9)
            && near(past.conductivity, edge.conductivity, 1e-9)
    );
    assert!(near(past.specific_heat, edge.specific_heat, 1e-9));
}

/// The correlations against independent ones, within the scatter they are quoted with.
#[test]
fn correlations() {
    let film = Fluid::air(325.0, ATMOSPHERE);
    let (nu, alpha, pr, k) = (
        film.kinematic_viscosity(),
        film.diffusivity(),
        film.prandtl(),
        film.conductivity,
    );
    let h = |shape, speed| coefficient(Medium::Air, shape, 350.0, 300.0, ATMOSPHERE, speed, G);
    // A vertical plate 0.5 m, laminar: Ostrach's similarity solution of the boundary layer,
    // Nu = (4/3)·(Gr/4)^¼·g(Pr) (Incropera eqs. 9.20–9.21).
    let l = 0.5;
    let ra = G * film.expansion * 50.0 * l * l * l / (nu * alpha);
    let g_pr = 0.75 * pr.sqrt() / det::pow(0.609 + 1.221 * pr.sqrt() + 1.238 * pr, 0.25);
    let ostrach = 4.0 / 3.0 * det::pow(ra / pr / 4.0, 0.25) * g_pr;
    let plate = h(Shape::VerticalPlate(l), 0.0);
    assert!(
        (plate / (ostrach * k / l) - 1.0).abs() < 0.02,
        "{plate} vs {}",
        ostrach * k / l
    );
    assert!(
        (3.0..6.0).contains(&plate),
        "a warm wall in still air: {plate} W/(m²·K)"
    );
    // A 10 m wall 50 K above the air, turbulent (Ra ~ 3·10¹²): McAdams's 0.10·Ra^⅓.
    let l = 10.0;
    let ra = G * film.expansion * 50.0 * l * l * l / (nu * alpha);
    let wall = h(Shape::VerticalPlate(l), 0.0);
    let mcadams = 0.10 * det::cbrt(ra) * k / l;
    assert!((wall / mcadams - 1.0).abs() < 0.15, "{wall} vs {mcadams}");
    // Through the transition, Nu rises steadily with Ra.
    let mut last = 0.0;
    for k in 0..=40 {
        let l = 0.5 + 0.05 * k as f64;
        let nu = h(Shape::VerticalPlate(l), 0.0) * l;
        assert!(nu > last);
        last = nu;
    }
    // Wind 5 m/s along a 1 m plate 2 K above the air (Re 3·10⁵, laminar): Blasius's average,
    // natural convection negligible beside it (it adds 0.2 %).
    let air = Fluid::air(300.0, ATMOSPHERE);
    let re = 5.0 * 1.0 / air.kinematic_viscosity();
    let blasius = 0.664 * re.sqrt() * det::cbrt(air.prandtl()) * air.conductivity;
    let windy = coefficient(
        Medium::Air,
        Shape::VerticalPlate(1.0),
        301.0,
        299.0,
        ATMOSPHERE,
        5.0,
        G,
    );
    assert!(
        (windy / blasius - 1.0).abs() < 0.005,
        "{windy} vs {blasius}"
    );
    // Past Re 5·10⁵ the boundary layer turns turbulent along the plate's end: more heat.
    let turbulent = coefficient(
        Medium::Air,
        Shape::VerticalPlate(4.0),
        301.0,
        299.0,
        ATMOSPHERE,
        5.0,
        G,
    );
    assert!(
        turbulent * 4.0 / air.conductivity > 0.664 * (4.0 * re).sqrt() * det::cbrt(air.prandtl())
    );
    // A 2 cm cylinder in a 5 m/s crossflow: Hilpert (C 0.193, m 0.618 for Re 4e3–4e4).
    let d = 0.02;
    let re = 5.0 * d / nu;
    let hilpert = 0.193 * det::pow(re, 0.618) * det::cbrt(pr) * k / d;
    let cylinder = h(Shape::Cylinder(d), 5.0);
    assert!(
        (cylinder / hilpert - 1.0).abs() < 0.15,
        "{cylinder} vs {hilpert}"
    );
    // A 5 cm sphere at 2 m/s: Ranz–Marshall, 2 + 0.6·Re^½·Pr^⅓.
    let d = 0.05;
    let re = 2.0 * d / nu;
    let ranz = (2.0 + 0.6 * re.sqrt() * det::cbrt(pr)) * k / d;
    let sphere = h(Shape::Sphere(d), 2.0);
    assert!((sphere / ranz - 1.0).abs() < 0.2, "{sphere} vs {ranz}");
    // A sphere in still air at its own temperature conducts: Nu = 2.
    let still = coefficient(
        Medium::Air,
        Shape::Sphere(d),
        300.0,
        300.0,
        ATMOSPHERE,
        0.0,
        G,
    );
    let k300 = Fluid::air(300.0, ATMOSPHERE).conductivity;
    assert!((still - 2.0 * k300 / d).abs() < 1e-12, "{still}");
    // No gravity, no wind: only the sphere's conduction is left.
    assert_eq!(
        coefficient(
            Medium::Air,
            Shape::VerticalPlate(1.0),
            350.0,
            300.0,
            ATMOSPHERE,
            0.0,
            0.0
        ),
        0.0
    );
}
