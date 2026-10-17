//! The shrinking-core model against its closed forms (Levenspiel, ch. 25).

use crate::shrinking::Particle;

const NONE: f64 = f64::INFINITY;

fn particle(film: f64, diffusivity: f64, surface: f64) -> Particle {
    // Hematite-like: 2 mm, 32 kmol/m³ of Fe₂O₃ units, a third of one per mole of CO.
    Particle {
        radius: 2e-3,
        density: 32_000.0,
        b: 1.0 / 3.0,
        film,
        diffusivity,
        surface,
    }
}

/// Reaction control alone: X = 1 − (1 − t/τ)³, τ = ρ·R/(b·k_s·C).
#[test]
fn reaction_control_follows_its_cube() {
    let p = particle(NONE, NONE, 1e-4);
    let c = 10.0;
    let tau = p.density * p.radius / (p.b * p.surface * c);
    for share in [0.1, 0.5, 0.9] {
        let x = p.convert(0.0, c, share * tau);
        let exact = 1.0 - (1.0 - share) * (1.0 - share) * (1.0 - share);
        assert!(
            (x - exact).abs() < 1e-12,
            "{share}·τ: X {x}, closed form {exact}"
        );
    }
    assert_eq!(p.convert(0.0, c, 1.1 * tau), 1.0);
}

/// Product-layer diffusion alone: t/τ = 1 − 3(1 − X)^(2/3) + 2(1 − X), τ = ρ·R²/(6·b·D_e·C);
/// and taken in many steps it converts as in one (the time each conversion stands for).
#[test]
fn layer_diffusion_follows_its_closed_form_in_any_steps() {
    let p = particle(NONE, 1e-6, NONE);
    let c = 10.0;
    let tau = p.density * p.radius * p.radius / (6.0 * p.b * p.diffusivity * c);
    let x = p.convert(0.0, c, 0.5 * tau);
    let t = 1.0 - 3.0 * creeer_math::det::pow(1.0 - x, 2.0 / 3.0) + 2.0 * (1.0 - x);
    assert!((t - 0.5).abs() < 1e-12, "t/τ {t}");
    let mut stepped = 0.0;
    for _ in 0..100 {
        stepped = p.convert(stepped, c, 0.005 * tau);
    }
    assert!(
        (stepped - x).abs() < 1e-9,
        "{stepped} in steps, {x} at once"
    );
}

/// In series the times add, and without a driving force (the gas in equilibrium) nothing
/// converts.
#[test]
fn resistances_add_and_equilibrium_stops_it() {
    let (film, layer, reaction) = (
        particle(0.05, NONE, NONE),
        particle(NONE, 1e-6, NONE),
        particle(NONE, NONE, 1e-4),
    );
    let all = particle(0.05, 1e-6, 1e-4);
    for x in [0.2, 0.7, 1.0] {
        let sum = film.time_to(x, 10.0) + layer.time_to(x, 10.0) + reaction.time_to(x, 10.0);
        assert!((all.time_to(x, 10.0) / sum - 1.0).abs() < 1e-12);
    }
    assert_eq!(all.convert(0.3, 0.0, 100.0), 0.3);
    assert_eq!(all.convert(0.3, -2.0, 100.0), 0.3);
}
