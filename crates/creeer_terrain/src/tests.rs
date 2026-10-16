//! Tests of the reference planet's data and elevation model.

use crate::Planet;
use crate::elevation::fibonacci_sphere;

fn home() -> Planet {
    Planet::load(&crate::data_path("planets/home.ron")).expect("home.ron loads")
}

#[test]
fn reference_planet_loads_with_si_values() {
    let p = home();
    assert_eq!(p.name, "Home");
    assert_eq!(p.radius, 500_000.0);
    // μ = g·R²
    assert!((p.mu - 9.81 * 500_000.0f64 * 500_000.0).abs() < 1.0);
    assert_eq!(p.sidereal_day, 21_600.0);
    assert!((p.axial_tilt.to_degrees() - 23.5).abs() < 1e-9);
    assert_eq!(p.elevation.layers.len(), 3);
}

fn quantiles(mut values: Vec<f64>) -> Vec<(f64, f64)> {
    values.sort_by(f64::total_cmp);
    [
        0.0, 0.05, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 0.95, 0.99, 1.0,
    ]
    .into_iter()
    .map(|q| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = ((values.len() - 1) as f64 * q) as usize;
        (q, values[i])
    })
    .collect()
}

#[test]
#[ignore = "exploration: prints the elevation distribution"]
fn print_elevation_distribution() {
    let p = home();
    let heights: Vec<f64> = fibonacci_sphere(20_000)
        .map(|d| p.elevation.height(d, 100.0))
        .collect();
    let land = heights.iter().filter(|&&h| h > 0.0).count();
    for (q, h) in quantiles(heights) {
        println!("height q{q:>4.2}: {h:>8.1} m");
    }
    println!("land fraction {:.3}", land as f64 / 20_000.0);
}

#[test]
fn land_fraction_follows_the_coast_in_the_data() {
    // The coast sits at surface fraction 0.54; relief layers move it a little either way.
    let p = home();
    let n = 8000;
    let land = fibonacci_sphere(n)
        .filter(|&d| p.elevation.height(d, 100.0) > 0.0)
        .count();
    let fraction = land as f64 / f64::from(n);
    assert!((fraction - 0.46).abs() < 0.03, "land fraction {fraction}");
}

#[test]
fn heights_stay_within_the_model_bounds() {
    let p = home();
    let (lo, hi) = (p.elevation.min_height(), p.elevation.max_height());
    for d in fibonacci_sphere(4000) {
        let h = p.elevation.height(d, 1.0);
        assert!(lo <= h && h <= hi, "{h} outside [{lo}, {hi}]");
    }
}

#[test]
fn changing_resolution_moves_the_surface_far_less_than_a_sample() {
    // Band-limiting fades octaves in smoothly, so a small change of resolution must not make
    // features pop, and dropping the detail below a resolution must keep the surface within
    // one sample of the full-detail one (LOD levels agree).
    let p = home();
    for spacing in [1.0, 10.0, 100.0, 1000.0] {
        let (mut pop, mut lod) = (0.0f64, 0.0f64);
        for d in fibonacci_sphere(2000) {
            let h = p.elevation.height(d, spacing);
            pop = pop.max((h - p.elevation.height(d, spacing * 1.1)).abs());
            lod = lod.max((h - p.elevation.height(d, spacing / 8.0)).abs());
        }
        assert!(pop < 0.05 * spacing, "spacing {spacing}: pop {pop}");
        assert!(lod < spacing, "spacing {spacing}: LOD difference {lod}");
    }
}

/// FNV-1a over the bits of a sequence of values.
pub(crate) fn hash_bits(values: impl Iterator<Item = f64>) -> u64 {
    values.fold(0xcbf2_9ce4_8422_2325, |h, v| {
        v.to_bits()
            .to_le_bytes()
            .iter()
            .fold(h, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
    })
}

#[test]
fn elevation_is_bit_identical_across_platforms() {
    // Golden hash: CI runs this on Windows, Linux and macOS. If it changes on purpose
    // (new generator or data), update the constant.
    let p = home();
    let hash = hash_bits(fibonacci_sphere(256).map(|d| p.elevation.height(d, 1.0)));
    assert_eq!(hash, 0xfb5c_3d9f_bdb8_555e, "elevation hash {hash:#018x}");
}
