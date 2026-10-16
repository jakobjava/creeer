use crate::Planet;
use crate::cube::CubeCoord;
use crate::elevation::fibonacci_sphere;
use crate::tiles::{
    HeightCache, HeightTile, MAX_LEVEL, TILE_CELLS, TileKey, level_for_spacing, level_spacing,
};
use creeer_math::glam::DVec3;
use std::sync::Arc;

fn cache() -> HeightCache {
    let planet = Planet::load(&crate::data_path("planets/home.ron")).expect("loads");
    HeightCache::new(Arc::clone(&planet.elevation), 256)
}

#[test]
fn levels_match_requested_spacing() {
    let r = 500_000.0;
    // The finest level for 1 m voxels on a 500 km planet: cells just under a metre.
    let level = level_for_spacing(r, 1.0);
    assert!(level_spacing(r, level) <= 1.0);
    assert!(level_spacing(r, level - 1) > 1.0);
    assert_eq!(level, 14);
    for spacing in [0.1, 3.0, 50.0, 2000.0, 1e5] {
        let l = level_for_spacing(r, spacing);
        assert!(level_spacing(r, l) <= spacing || l == 0, "{spacing}");
    }
    assert!(level_spacing(r, MAX_LEVEL) < 0.05);
}

#[test]
fn tiles_reproduce_the_elevation_at_their_samples() {
    let cache = cache();
    let key = TileKey {
        face: 4,
        level: 9,
        i: 300,
        j: 211,
    };
    let tile = HeightTile::generate(cache.elevation(), key);
    let spacing = level_spacing(500_000.0, key.level);
    let cell = 2.0 / (TILE_CELLS as f64 * 512.0);
    let (s0, t0) = (-1.0 + 300.0 * 64.0 * cell, -1.0 + 211.0 * 64.0 * cell);
    for (a, b) in [(0, 0), (5, 9), (64, 64), (63, 1), (32, 17)] {
        let c = CubeCoord {
            face: 4,
            s: s0 + f64::from(a) * cell,
            t: t0 + f64::from(b) * cell,
        };
        let direct = cache.elevation().height(c.direction(), spacing);
        let sampled = tile.sample(c.s, c.t);
        // Stored as f32: exact to well below a millimetre.
        assert!(
            (direct - sampled).abs() < 1e-3,
            "({a},{b}): {direct} vs {sampled}"
        );
    }
}

#[test]
fn interpolation_stays_close_to_the_elevation() {
    let cache = cache();
    for spacing in [1.0, 20.0, 500.0] {
        let mut sampler = cache.sampler(spacing);
        let tile_spacing = level_spacing(500_000.0, sampler.level);
        let mut worst = 0.0f64;
        // Points scattered within a few km, so a few tiles are involved.
        for (k, d) in fibonacci_sphere(400).enumerate() {
            let base = DVec3::new(0.3, 0.8, 0.5).normalize();
            #[allow(clippy::cast_precision_loss)]
            let dir =
                (base + d * (spacing * 40.0 / 500_000.0) * (1.0 + k as f64 / 400.0)).normalize();
            let direct = cache.elevation().height(dir, tile_spacing);
            worst = worst.max((sampler.height(dir) - direct).abs());
        }
        // Octaves near two samples per wavelength are only partly resolved; the surface
        // still stays within a fraction of a sample of the band-limited elevation.
        assert!(
            worst < 0.15 * tile_spacing,
            "spacing {spacing}: error {worst}"
        );
    }
}

#[test]
fn heights_are_continuous_across_tile_and_face_boundaries() {
    let cache = cache();
    let mut sampler = cache.sampler(2.0);
    let level = sampler.level;
    let spacing = level_spacing(500_000.0, level);
    // Walk across a tile boundary and across the +Z/+X face edge in small steps; the height
    // may change only as much as the slope allows, never jump.
    let walks = [
        // Across a tile boundary inside face +Z.
        (
            CubeCoord {
                face: 4,
                s: 0.25 - 1e-6,
                t: 0.1,
            },
            DVec3::X,
        ),
        // Across the edge between +Z and +X.
        (
            CubeCoord {
                face: 4,
                s: 1.0 - 4e-6,
                t: 0.3,
            },
            DVec3::X,
        ),
    ];
    for (start, axis) in walks {
        let mut dir = start.direction();
        let step = 0.05 / 500_000.0;
        let mut previous = sampler.height(dir);
        let mut faces = std::collections::BTreeSet::new();
        for _ in 0..400 {
            dir = (dir + axis * step).normalize();
            faces.insert(CubeCoord::from_direction(dir).face);
            let h = sampler.height(dir);
            // 5 cm steps: even a 45° slope moves 5 cm; allow the face-edge mismatch, which
            // is the interpolation error (a small fraction of a sample).
            assert!(
                (h - previous).abs() < 0.05 + 0.05 * spacing,
                "jump {previous} -> {h}"
            );
            previous = h;
        }
        assert!(!faces.is_empty());
    }
}

#[test]
fn cache_reuses_tiles_and_evicts_old_ones() {
    let cache = cache();
    let key = |i| TileKey {
        face: 0,
        level: 10,
        i,
        j: 7,
    };
    let a = cache.tile(key(1));
    let again = cache.tile(key(1));
    assert!(Arc::ptr_eq(&a, &again), "cached tile is shared");
    for i in 2..300 {
        cache.tile(key(i));
    }
    assert!(cache.len() <= 256, "capacity respected: {}", cache.len());
    // An evicted tile regenerates bit-identically.
    let b = cache.tile(key(1));
    for (s, t) in [(-0.99, -0.98), (-0.995, -0.99)] {
        assert_eq!(a.sample(s, t).to_bits(), b.sample(s, t).to_bits());
    }
}

#[test]
#[ignore = "measurement: run in release with --nocapture"]
fn print_tile_costs() {
    use std::time::Instant;
    let cache = cache();
    let start = Instant::now();
    for i in 0..20 {
        HeightTile::generate(
            cache.elevation(),
            TileKey {
                face: 2,
                level: 14,
                i: 5000 + i,
                j: 7000,
            },
        );
    }
    println!(
        "tile generation: {:.2} ms",
        start.elapsed().as_secs_f64() * 1e3 / 20.0
    );
    let base = DVec3::new(0.3, 0.8, 0.5).normalize();
    let dirs: Vec<DVec3> = (0..36_000)
        .map(|k| {
            let k = f64::from(k);
            {
                use creeer_math::det::{cos, sin};
                (base + DVec3::new(sin(k), cos(k * 0.7), sin(k * 1.3)) * 3e-5).normalize()
            }
        })
        .collect();
    let mut sampler = cache.sampler(1.0);
    let primed: f64 = dirs.iter().map(|&d| sampler.height(d)).sum(); // generates the tiles
    assert!(primed.is_finite());
    let start = Instant::now();
    let sum: f64 = dirs.iter().map(|&d| sampler.height(d)).sum();
    println!(
        "36k tile samples: {:.2} ms (sum {sum:.0})",
        start.elapsed().as_secs_f64() * 1e3
    );
    let start = Instant::now();
    let sum: f64 = dirs[..3600]
        .iter()
        .map(|&d| cache.elevation().height(d, 0.79))
        .sum();
    println!(
        "36k direct heights (from 3.6k): {:.2} ms (sum {sum:.0})",
        start.elapsed().as_secs_f64() * 1e4
    );
}
