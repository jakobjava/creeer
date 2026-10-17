//! Validation of the surroundings and of melting: water's properties and convection, the
//! radiation helpers against closed forms, joints against their measured range, and the
//! Stefan problem against Neumann's solution.

use crate::contact::Joint;
use crate::environment::*;
use crate::*;
use creeer_materials::{Curve, Melting};
use creeer_math::det;
use std::sync::Arc;

const ATMOSPHERE: f64 = 101_325.0;
const G: f64 = 9.806_65;

/// Water at 300 K against Incropera's Table A.6: ρ 997, μ 855e-6, k 0.613, Pr 5.83.
#[test]
fn water_properties() {
    let w = Fluid::water(300.0);
    assert_eq!(
        (w.density, w.viscosity, w.conductivity),
        (997.0, 855e-6, 0.613)
    );
    assert!((w.prandtl() / 5.83 - 1.0).abs() < 2e-3, "{}", w.prandtl());
    // Densest at 4 °C: it expands as it cools below.
    assert!(Fluid::water(275.0).expansion < 0.0 && Fluid::water(280.0).expansion > 0.0);
}

/// A warm 10 cm wall in still water (Ra 3·10⁸: laminar), against Ostrach's boundary-layer
/// solution; and water
/// takes far more heat than air.
#[test]
fn convection_in_water() {
    let (l, ts, tf) = (0.1, 310.0, 300.0);
    let w = Fluid::water(305.0);
    let (nu, alpha, pr) = (w.kinematic_viscosity(), w.diffusivity(), w.prandtl());
    let ra = G * w.expansion * (ts - tf) * l * l * l / (nu * alpha);
    let g_pr = 0.75 * pr.sqrt() / det::pow(0.609 + 1.221 * pr.sqrt() + 1.238 * pr, 0.25);
    let ostrach = 4.0 / 3.0 * det::pow(ra / pr / 4.0, 0.25) * g_pr * w.conductivity / l;
    let h = coefficient(
        Medium::Water,
        Shape::VerticalPlate(l),
        ts,
        tf,
        ATMOSPHERE,
        0.0,
        G,
    );
    assert!((h / ostrach - 1.0).abs() < 0.03, "{h} vs {ostrach}");
    let air = coefficient(
        Medium::Air,
        Shape::VerticalPlate(l),
        ts,
        tf,
        ATMOSPHERE,
        0.0,
        G,
    );
    assert!((50.0..200.0).contains(&(h / air)), "water/air: {}", h / air);
}

/// A plate colder than the fluid facing up is a warmer one facing down (the same film, the
/// same difference): the fluid lies against both. Water below 4 °C turns that over.
#[test]
fn which_face_a_plate_shows() {
    let h = |shape, ts, tf, medium| coefficient(medium, shape, ts, tf, ATMOSPHERE, 0.0, G);
    let cold_up = h(Shape::PlateUp(0.5), 280.0, 300.0, Medium::Air);
    assert_eq!(cold_up, h(Shape::PlateDown(0.5), 300.0, 280.0, Medium::Air));
    assert!(cold_up < h(Shape::PlateUp(0.5), 300.0, 280.0, Medium::Air));
    // Water at 1–2 °C sinks as it warms: a warmer plate facing up holds it against itself,
    // as a colder one facing down does.
    let up = h(Shape::PlateUp(0.5), 275.5, 274.0, Medium::Water);
    assert_eq!(up, h(Shape::PlateDown(0.5), 274.0, 275.5, Medium::Water));
    assert!(up < h(Shape::PlateUp(0.5), 274.0, 275.5, Medium::Water));
}

#[test]
fn sunlight_and_sky() {
    assert!((sunlight(AU) - SOLAR_CONSTANT).abs() < 1e-9);
    assert!(
        (sunlight(1.524 * AU) - 586.0).abs() < 1.0,
        "Mars: {}",
        sunlight(1.524 * AU)
    );
    // Kasten–Young: 1 straight up, sec z while the Earth's curvature does not matter (within
    // 1 % to 60°), ~38 at the horizon, nothing below.
    assert!((air_mass(0.0).unwrap() - 1.0).abs() < 1e-3);
    for z in [20.0f64, 40.0, 60.0] {
        let secant = 1.0 / det::cos(z.to_radians());
        assert!(
            (air_mass(z.to_radians()).unwrap() / secant - 1.0).abs() < 0.01,
            "{z}°"
        );
    }
    assert!((37.0..39.0).contains(&air_mass(90f64.to_radians()).unwrap()));
    assert_eq!(air_mass(91f64.to_radians()), None);
    // Meinel: 953 W/m² of direct sunlight with the Sun overhead, none in vacuum lost.
    let overhead = direct_sunlight(SOLAR_CONSTANT, 0.0, Some(EARTH_CLEAR_SKY));
    assert!((overhead - 952.7).abs() < 1.0, "{overhead}");
    assert_eq!(direct_sunlight(SOLAR_CONSTANT, 1.0, None), SOLAR_CONSTANT);
    let tilted = absorbed(0.5, 2.0, [0.0, 0.0, 1.0], [0.0, 0.6, 0.8], 1000.0);
    assert!((tilted - 800.0).abs() < 1e-9);
    assert_eq!(
        absorbed(0.5, 2.0, [0.0, 0.0, -1.0], [0.0, 0.6, 0.8], 1000.0),
        0.0
    );
    // Swinbank: a clear night over 20 °C air radiates like 277 K; overcast, like the air.
    assert!((sky_temperature(293.15, 0.0) - 277.1).abs() < 0.1);
    assert!((sky_temperature(293.15, 1.0) - 293.15).abs() < 1e-9);
}

/// View factors against Incropera's figures, and a cube's faces seeing all of its inside.
#[test]
fn view_factors() {
    assert!((coaxial_discs(1.0, 1.0, 1.0) - (3.0 - 5f64.sqrt()) / 2.0).abs() < 1e-12);
    let opposite = aligned_rectangles(1.0, 1.0, 1.0);
    let side = perpendicular_rectangles(1.0, 1.0, 1.0);
    assert!((opposite - 0.1998).abs() < 1e-4 && (side - 0.2000).abs() < 1e-4);
    assert!(
        (opposite + 4.0 * side - 1.0).abs() < 1e-9,
        "a cube's face sees the rest of it"
    );
    // Reciprocity: A₁F₁₂ = A₂F₂₁ for discs of different sizes.
    let (r1, r2) = (0.5, 2.0);
    assert!(
        (r1 * r1 * coaxial_discs(r1, r2, 1.0) - r2 * r2 * coaxial_discs(r2, r1, 1.0)).abs() < 1e-12
    );
    // Grey exchange: black bodies exchange A·F; large parallel plates 1/(1/ε₁ + 1/ε₂ − 1).
    assert!((grey_exchange(2.0, 1.0, 3.0, 1.0, 0.4) - 0.8).abs() < 1e-12);
    let plates = grey_exchange(1.0, 0.8, 1.0, 0.5, 1.0);
    assert!((plates - 1.0 / (1.0 / 0.8 + 1.0 / 0.5 - 1.0)).abs() < 1e-12);
}

/// A bolted steel flange conducts within the measured range of such joints (1–10 kW/(m²·K)),
/// better pressed harder, worse in vacuum.
#[test]
fn joints() {
    let joint = Joint::machined_steel_in_air();
    let h = joint.conductance(1e6);
    assert!((1e3..1e4).contains(&h), "{h}");
    assert!(joint.conductance(1e7) > h);
    let vacuum = Joint {
        gas: (0.0, 0.0),
        ..joint
    };
    assert!(vacuum.conductance(1e6) < 0.6 * h);
}

/// One-phase Stefan problem: a solid at its melting point, its face suddenly held 30 K above.
/// The melt front moves as s = 2λ·√(α·t), λ·e^(λ²)·erf(λ) = St/√π (Neumann's solution).
#[test]
fn stefan_problem() {
    let (k, rho, cp, latent, tm, dt_face) = (1.0, 1000.0, 1000.0, 3.0e5, 300.0, 30.0);
    let melting = Melting {
        solidus: tm,
        liquidus: tm,
        latent_heat: latent,
    };
    let enthalpy = Arc::new(Enthalpy::new(&Curve::constant(cp), Some(melting)));
    let (lo, _) = enthalpy.melting().unwrap();
    let (dx, cells) = (0.5e-3, 200);
    let mut net = Network::new(G);
    let face = net.add_fixed(tm + dt_face);
    let cells: Vec<_> = (0..cells)
        .map(|_| net.add_mass(rho * dx, enthalpy.clone(), lo))
        .collect();
    net.link(Link::Conductance {
        nodes: (cells[0], face),
        conductance: k / (0.5 * dx),
    });
    for w in cells.windows(2) {
        net.link(Link::Conductance {
            nodes: (w[0], w[1]),
            conductance: k / dx,
        });
    }
    let (time, steps) = (3600.0, 3600);
    for _ in 0..steps {
        net.step(time / f64::from(steps));
    }
    // How far it has melted: each cell's melted fraction, from its enthalpy.
    let melted: f64 = cells
        .iter()
        .map(|&c| ((net.specific_enthalpy(c) - enthalpy.at(lo)) / latent).clamp(0.0, 1.0) * dx)
        .sum();
    // Neumann's λ, by bisection.
    let st = cp * dt_face / latent;
    let f = |l: f64| l * det::exp(l * l) * det::erf(l) - st / std::f64::consts::PI.sqrt();
    let (mut a, mut b) = (0.0, 2.0);
    for _ in 0..100 {
        let m = 0.5 * (a + b);
        if f(m) > 0.0 { b = m } else { a = m }
    }
    let front = 2.0 * a * (k / (rho * cp) * time).sqrt();
    // The sensible heat stored in the melt counts its cells as only partly melted; within 3 %.
    assert!(
        (melted / front - 1.0).abs() < 0.03,
        "{melted} m vs {front} m"
    );
}
