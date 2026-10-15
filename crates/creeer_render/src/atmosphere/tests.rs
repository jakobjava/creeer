use super::luts::{transmittance_params, transmittance_uv};
use super::*;

const R: f64 = 500_000.0;
const SUN: [f64; 3] = [3.6, 3.4, 3.1];

fn atmosphere() -> Atmosphere {
    Atmosphere::earthlike(R)
}

fn sun_at_elevation(degrees: f64) -> DVec3 {
    let e = degrees.to_radians();
    let (s, c) = det::sin_cos(e);
    DVec3::new(c, s, 0.0)
}

#[test]
fn heights_are_exact_near_the_surface() {
    // The cancellation-free formula must give the exact height for a horizontal ray:
    // h(t) = √(r² + t²) − R, written without subtracting huge numbers.
    let a = atmosphere();
    let ray = Ray { h: 2.0, mu: 0.0 };
    for t in [0.0, 1.0, 100.0, 10_000.0] {
        let r = R + 2.0;
        let exact = t * t / ((r * r + t * t).sqrt() + r) + 2.0;
        assert!((ray.height_at(&a, t) - exact).abs() < 1e-9, "t = {t}");
    }
}

#[test]
fn horizon_geometry_matches_the_planet() {
    let a = atmosphere();
    // From 2 m up, a ray slightly below the horizon hits the ground ~kilometres away;
    // the geometric horizon distance is √(2Rh) ≈ 1.41 km.
    let horizon_mu = -(2.0 * 2.0 / R).sqrt();
    let hit = Ray {
        h: 2.0,
        mu: horizon_mu * 1.01,
    }
    .ground_distance(&a);
    assert!(hit.is_some_and(|d| d > 1_000.0 && d < 2_000.0), "{hit:?}");
    assert!(Ray { h: 2.0, mu: 0.0 }.ground_distance(&a).is_none());
    assert!(
        Ray { h: 2.0, mu: -1.0 }
            .ground_distance(&a)
            .is_some_and(|d| (d - 2.0).abs() < 1e-6)
    );
}

#[test]
fn transmittance_is_lower_towards_the_horizon_and_for_blue() {
    let a = atmosphere();
    let zenith = transmittance_to_space(&a, 0.0, 1.0);
    let low = transmittance_to_space(&a, 0.0, 0.05);
    for c in 0..3 {
        assert!(low[c] < zenith[c]);
        assert!((0.0..=1.0).contains(&zenith[c]));
    }
    assert!(
        zenith[2] < zenith[1] && zenith[1] < zenith[0],
        "blue is scattered most"
    );
    // Earth's zenith transmittance is about 0.9 (red) to 0.7 (blue).
    assert!(
        zenith[0] > 0.85 && zenith[2] > 0.6 && zenith[2] < 0.8,
        "{zenith:?}"
    );
    // Above the atmosphere nothing is lost.
    assert_eq!(transmittance_to_space(&a, 200_000.0, 0.3), [1.0; 3]);
}

#[test]
fn daytime_sky_is_blue_and_sunset_sun_is_red() {
    let a = atmosphere();
    let noon = local_light(&a, 1.0, DVec3::Y, sun_at_elevation(60.0), SUN);
    assert!(
        noon.sky[2] > noon.sky[1] && noon.sky[1] > noon.sky[0],
        "{:?}",
        noon.sky
    );
    let sunset = local_light(&a, 1.0, DVec3::Y, sun_at_elevation(2.0), SUN);
    let ratio = |l: &LocalLight| l.sun[0] / l.sun[2];
    assert!(ratio(&sunset) > 2.0 * ratio(&noon), "sunset sun not redder");
    let night = local_light(&a, 1.0, DVec3::Y, sun_at_elevation(-30.0), SUN);
    assert!(night.sun.iter().all(|&c| c == 0.0));
    assert!(
        night.sky.iter().all(|&c| c < noon.sky[2] * 0.01),
        "night sky too bright"
    );
}

#[test]
fn sky_brightness_is_plausible_relative_to_the_sun() {
    // Clear-sky luminance is roughly 5–10 % of the sun's illuminance per steradian.
    let a = atmosphere();
    let zenith = sky_radiance(&a, 1.0, DVec3::Y, DVec3::Y, sun_at_elevation(45.0), SUN);
    let ratio = zenith[2] / SUN[2];
    assert!((0.01..0.2).contains(&ratio), "zenith/sun = {ratio}");
}

#[test]
fn looking_at_the_planet_from_space_shows_the_lit_atmosphere() {
    let a = atmosphere();
    let up = DVec3::Y;
    // From 300 km, looking straight down at the day side: some scattered light.
    let down = sky_radiance(&a, 300_000.0, up, -up, up, SUN);
    assert!(down[2] > 0.0);
    // Looking straight up into space: nothing.
    let space = sky_radiance(&a, 300_000.0, up, up, up, SUN);
    assert_eq!(space, [0.0; 3]);
}

#[test]
fn transmittance_table_mapping_round_trips() {
    let a = atmosphere();
    for h in [0.0, 1.8, 500.0, 12_000.0, 79_000.0] {
        for mu in [1.0, 0.5, 0.05, 0.0, -0.01] {
            if (Ray { h, mu }).ground_distance(&a).is_some() {
                continue;
            }
            let (u, v) = transmittance_uv(&a, h, mu);
            let (h2, mu2) = transmittance_params(&a, u, v);
            assert!((h - h2).abs() < 1e-3, "h {h} -> {h2}");
            assert!((mu - mu2).abs() < 1e-6, "mu {mu} -> {mu2} at h {h}");
        }
    }
}

#[test]
fn transmittance_table_matches_direct_integration() {
    let a = atmosphere();
    let luts = atmosphere_luts(&a);
    for h in [0.0, 300.0, 5_000.0, 40_000.0] {
        for mu in [1.0, 0.6, 0.2, 0.03] {
            let table = luts.transmittance(h, mu);
            let direct = transmittance_to_space(&a, h, mu);
            for c in 0..3 {
                assert!(
                    (table[c] - direct[c]).abs() < 0.01,
                    "h {h} mu {mu}: table {table:?} vs direct {direct:?}"
                );
            }
        }
    }
}

/// Without multiple scattering a hazy horizon comes out *darker* than the zenith; real
/// skies are brightest and whitest near the horizon.
#[test]
fn multiple_scattering_makes_the_horizon_bright_and_white() {
    let a = atmosphere();
    let to_sun = sun_at_elevation(40.0);
    let zenith = sky_radiance(&a, 1.8, DVec3::Y, DVec3::Y, to_sun, SUN);
    let horizon = sky_radiance(
        &a,
        1.8,
        DVec3::Y,
        DVec3::new(-1.0, 0.01, 0.0).normalize(),
        to_sun,
        SUN,
    );
    assert!(
        horizon[1] > 2.0 * zenith[1],
        "horizon {horizon:?} zenith {zenith:?}"
    );
    let saturation = |c: [f64; 3]| c[2] / c[0];
    assert!(
        saturation(horizon) < saturation(zenith),
        "horizon not whiter"
    );
    let luts = atmosphere_luts(&a);
    for mu_s in [-0.2, 0.0, 0.5, 1.0] {
        let psi = luts.multiscatter(0.0, mu_s);
        assert!(psi.iter().all(|v| v.is_finite() && *v >= 0.0), "{psi:?}");
    }
}
