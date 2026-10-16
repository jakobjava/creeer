use super::*;

fn home() -> PlanetGravity {
    // 500 km, 9.81 m/s² at the datum, a 6-hour day about +Y.
    PlanetGravity::planet(
        9.81 * 500_000.0 * 500_000.0,
        DVec3::Y * (std::f64::consts::TAU / 21_600.0),
    )
}

#[test]
fn surface_gravity_and_the_centrifugal_reduction() {
    let g = home();
    let pole = g.acceleration(DVec3::Y * 500_000.0);
    assert!((pole.length() - 9.81).abs() < 1e-9);
    assert!(pole.normalize().dot(DVec3::NEG_Y) > 1.0 - 1e-15);
    // At the equator the centrifugal acceleration ω²R = 0.0423 m/s² points outward.
    let equator = g.acceleration(DVec3::X * 500_000.0);
    let omega = std::f64::consts::TAU / 21_600.0;
    assert!((equator.length() - (9.81 - omega * omega * 500_000.0)).abs() < 1e-9);
}

#[test]
fn acceleration_is_minus_the_gradient_of_the_potential() {
    let g = home();
    let x = DVec3::new(210_000.0, 380_000.0, -240_000.0);
    let h = 0.5;
    let numeric = DVec3::new(
        g.potential_difference(x + DVec3::X * h, x - DVec3::X * h),
        g.potential_difference(x + DVec3::Y * h, x - DVec3::Y * h),
        g.potential_difference(x + DVec3::Z * h, x - DVec3::Z * h),
    ) / (2.0 * h);
    assert!((numeric + g.acceleration(x)).length() < 1e-9);
}

#[test]
fn potential_differences_are_free_of_cancellation() {
    let g = home();
    let x = DVec3::new(0.0, 500_123.0, 0.0);
    let y = x + DVec3::new(1e-4, 2e-3, -5e-5);
    // For a tiny step the midpoint rule ΔΦ ≈ −a((x + y)/2)·(y − x) is exact to third
    // order (an error of ~10⁻²⁰ J/kg here).
    let expected = -g.acceleration((x + y) * 0.5).dot(y - x);
    let difference = g.potential_difference(y, x);
    assert!(
        (difference - expected).abs() < 1e-15,
        "{difference} vs {expected}"
    );
    // Subtracting the two 4.9 MJ/kg potentials directly loses about 10 digits.
    let naive = g.potential(y) - g.potential(x);
    assert!(
        (naive - expected).abs() > 1e-12,
        "the naive difference is not exact"
    );
    assert_eq!(g.potential_difference(x, x), 0.0);
}

#[test]
fn coriolis_does_no_work_and_deflects_east_when_falling() {
    let g = home();
    let v = DVec3::new(3.0, -40.0, 2.0);
    assert!(g.coriolis(v).dot(v).abs() < 1e-12);
    // Falling at the equator (+X, spin +Y): deflected towards −Z, which is east when the
    // planet turns counter-clockwise about +Y (the surface moves towards −Z at +X).
    let falling = g.coriolis(DVec3::NEG_X * 10.0);
    let east = g.spin.cross(DVec3::X).normalize();
    assert!(falling.dot(east) > 0.0);
}

/// The Sun 13.6 Gm away along +X and a moon 12,000 km away along +Z, seen from Home.
fn with_tides() -> PlanetGravity {
    let mut g = home();
    g.tidal = vec![
        Tidal {
            mu: 1.172_332_8e18,
            at: DVec3::X * 1.36e10,
        },
        Tidal {
            mu: 6.5e10,
            at: DVec3::Z * 1.2e7,
        },
    ]
    .into();
    g
}

#[test]
fn tides_are_the_difference_of_the_pulls_here_and_at_the_centre() {
    let g = with_tides();
    let x = DVec3::new(500_000.0, 1_000.0, -2_000.0);
    let tidal = g.acceleration(x) - home().acceleration(x);
    let pull = |s: &Tidal, p: DVec3| {
        let r = (s.at - p).length();
        (s.at - p) * (s.mu / (r * r * r))
    };
    let expected: DVec3 = g
        .tidal
        .iter()
        .map(|s| pull(s, x) - pull(s, DVec3::ZERO))
        .sum();
    // Both sides subtract accelerations near 10 m/s²: equal to their rounding.
    assert!((tidal - expected).length() < 1e-14, "{tidal} vs {expected}");
    // The Sun alone stretches Home along the line to it by 2μR/d³ ≈ 4.7e-7 m/s²; the
    // moon squeezes it 40 times harder across the line to the moon.
    let sun = &g.tidal[0];
    let stretch = sun.acceleration(DVec3::X * 500_000.0).x;
    let expected = 2.0 * sun.mu * 500_000.0 / (1.36e10 * 1.36e10 * 1.36e10);
    assert!(
        (stretch / expected - 1.0).abs() < 1e-4,
        "{stretch} vs {expected}"
    );
}

#[test]
fn tidal_and_uniform_terms_have_a_potential() {
    let mut g = with_tides();
    g.uniform = DVec3::new(1e-6, -2e-6, 3e-7);
    for mu in [9.81 * 500_000.0 * 500_000.0, 0.0] {
        g.mu = mu;
        let x = DVec3::new(300_000.0, -420_000.0, 150_000.0);
        let h = 0.5;
        let numeric = DVec3::new(
            g.potential_difference(x + DVec3::X * h, x - DVec3::X * h),
            g.potential_difference(x + DVec3::Y * h, x - DVec3::Y * h),
            g.potential_difference(x + DVec3::Z * h, x - DVec3::Z * h),
        ) / (2.0 * h);
        assert!(
            (numeric + g.acceleration(x)).length() < 1e-9,
            "μ = {mu}: {numeric} vs {}",
            g.acceleration(x)
        );
        // Free fall (no central body, no spin): nothing at the origin.
        if mu == 0.0 {
            g.spin = DVec3::ZERO;
            g.uniform = DVec3::ZERO;
            assert_eq!(g.acceleration(DVec3::ZERO), DVec3::ZERO);
            assert_eq!(g.potential_difference(DVec3::ZERO, DVec3::ZERO), 0.0);
        }
    }
}
