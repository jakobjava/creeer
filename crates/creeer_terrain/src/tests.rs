//! Tests of the reference planet's data and elevation model.

use crate::Planet;
use crate::elevation::fibonacci_sphere;

fn home() -> Planet {
    Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron loads")
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
    let ocean = p.ocean.expect("home has an ocean");
    assert_eq!(
        (
            ocean.density,
            ocean.viscosity,
            ocean.sound,
            ocean.temperature
        ),
        (1025.97, 1.22e-3, 1506.7, 288.15)
    );
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
    // (new generator or data), update the constant. F5: the continent curve and its CDF
    // interpolate monotone cubics instead of straight lines, and ridge crests are rounded to
    // the sample spacing (both removed creases that showed as lines in low sunlight).
    let p = home();
    let hash = hash_bits(fibonacci_sphere(256).map(|d| p.elevation.height(d, 1.0)));
    assert_eq!(hash, 0xc296_11e6_f70a_1ea1, "elevation hash {hash:#018x}");
}

#[test]
fn the_planet_centre_is_rock() {
    // It has no direction to look a height up in: it gave NaN, read as air, and the coarsest
    // chunks around it meshed a hidden bubble.
    let terrain = crate::Terrain::new(home());
    let centre = creeer_math::glam::DVec3::ZERO;
    assert!(terrain.density(centre) < -400_000.0);
    let radius = terrain.planet().datum.polar_radius;
    for x in [-1, 0] {
        let key = crate::chunk::ChunkKey {
            level: 15,
            pos: creeer_math::glam::IVec3::new(x, -1, 0),
        };
        let mesh = terrain
            .mesh_chunk(key)
            .expect("the coarsest chunks hold the surface");
        for p in &mesh.positions[..mesh.surface_vertices] {
            let r = (key.origin() + p.as_dvec3()).length();
            assert!(r > 0.9 * radius, "a vertex {r} m from the centre");
        }
    }
}
