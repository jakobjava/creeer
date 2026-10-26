//! Rounded edits (`edits.rs`, G1.3c): the smooth maximum and minimum, and dug rims drawn as
//! rounds instead of zigzags of cell-sized facets (`docs/art-style.md` pillar 9).

use crate::chunk::ChunkKey;
use crate::edits::{Brush, Edit, round, smooth_max, smooth_min};
use crate::elevation::fibonacci_sphere;
use crate::surface_nets::TerrainMesh;
use crate::{Planet, Terrain};
use creeer_math::glam::DVec3;
use std::collections::HashMap;

#[test]
fn smooth_extremes_round_only_near_the_crease() {
    // Apart by the round or more: exactly min and max.
    assert_eq!(smooth_min(1.0, 5.0, 2.0), 1.0);
    assert_eq!(smooth_max(1.0, 5.0, 2.0), 5.0);
    assert_eq!(smooth_min(-3.0, -1.0, 2.0), -3.0);
    // Equal: a quarter of the round beyond.
    assert!((smooth_min(0.0, 0.0, 2.0) + 0.5).abs() < 1e-12);
    assert!((smooth_max(0.0, 0.0, 2.0) - 0.5).abs() < 1e-12);
    // No round: the plain extremes.
    assert_eq!(smooth_min(0.3, 0.2, 0.0), 0.2);
    // Continuous in value and slope across the edge of the round (h = 0).
    let (k, e) = (2.0, 1e-7);
    let f = |b: f64| smooth_min(0.0, b, k);
    assert!((f(k - e) - f(k + e)).abs() < 1e-9);
    let slope = |b: f64| (f(b + e) - f(b - e)) / (2.0 * e);
    assert!((slope(k - 1e-4) - slope(k + 1e-4)).abs() < 1e-3);
    assert_eq!(round(1.0), 3.0);
}

/// A point of the surface on gentle land, in the middle third of its chunk at `level`.
fn gentle(t: &Terrain, level: u8) -> DVec3 {
    fibonacci_sphere(20_000)
        .filter(|&d| (50.0..300.0).contains(&t.surface_height(d)))
        .map(|up| up * (t.planet().datum.radius(up) + t.surface_height(up)))
        .find(|&p| {
            let key = ChunkKey::containing(p, level);
            let local = (p - key.origin()) / key.size();
            local.min_element() > 0.33 && local.max_element() < 0.67
        })
        .expect("gentle land in the middle of a chunk")
}

/// The angle (degrees) each pair of the mesh's surface triangles sharing an edge folds by,
/// for the pairs within `reach` of `centre` (in the chunk's frame).
fn folds_near(m: &TerrainMesh, centre: DVec3, reach: f64) -> Vec<f64> {
    let tris = m.indices[..m.surface_indices].as_chunks::<3>().0;
    let corner = |i: u32| m.positions[i as usize].as_dvec3();
    let face = |t: &[u32; 3]| {
        let [a, b, c] = t.map(corner);
        (b - a).cross(c - a).normalize_or_zero()
    };
    let mut by_edge: HashMap<(u32, u32), usize> = HashMap::new();
    let mut out = Vec::new();
    for (n, t) in tris.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if let Some(&other) = by_edge.get(&(b, a)) {
                let mid = 0.5 * (corner(a) + corner(b));
                if mid.distance(centre) < reach {
                    let cos = face(t).dot(face(&tris[other])).clamp(-1.0, 1.0);
                    out.push(creeer_math::det::acos(cos).to_degrees());
                }
            }
            by_edge.insert((a, b), n);
        }
    }
    out
}

/// A ball of 4.5 m radius dug half into gentle ground: around its rim, neighbouring triangles
/// fold gently on every level a player sees it at. Measured (G1.3c): with exact CSG the worst
/// fold was 150–154° (flaps), with 52, 27 and 21 pairs beyond 45° on levels 0, 1 and 2; with
/// rounds of two cells 44–54°, and 6, 0 and 3 pairs.
#[test]
fn dug_rims_are_rounds_not_zigzags() {
    let t = Terrain::new(
        Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron"),
    );
    let ground = gentle(&t, 5);
    let up = ground.normalize();
    let centre = ground - up * 2.0;
    t.edit(Edit::Dig(Brush::Sphere {
        center: centre,
        radius: 4.5,
    }));
    for level in [0u8, 1, 2, 3, 4, 5] {
        let key = ChunkKey::containing(ground, level);
        let m = t.mesh_chunk(key).expect("ground with a hole");
        let cell = key.cell_size();
        // The rim: about 4 m from the ball's centre along the ground.
        let mut folds = folds_near(&m, centre - key.origin(), 4.5 + 2.0 * cell);
        folds.sort_by(f64::total_cmp);
        let worst = folds.last().copied().unwrap_or(0.0);
        let sharp = folds.iter().filter(|&&a| a > 45.0).count();
        println!(
            "level {level}: {} folds near the rim, worst {worst:.0}°, {sharp} beyond 45°",
            folds.len()
        );
        assert!(folds.len() > 100, "level {level}: the rim was not meshed");
        assert!(worst < 50.0, "level {level}: a fold of {worst:.0}°");
        assert!(
            sharp * 100 <= 3 * folds.len(),
            "level {level}: {sharp} folds beyond 45°"
        );
    }
}
