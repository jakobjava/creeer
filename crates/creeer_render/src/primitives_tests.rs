use super::*;
use std::collections::HashMap;

/// Every edge is used once each way: the mesh is closed and its triangles agree on a side.
fn assert_closed(mesh: &MeshData) {
    let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        for edge in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            assert_ne!(edge.0, edge.1, "a triangle repeats a vertex");
            *edges.entry(edge).or_default() += 1;
        }
    }
    for (&(a, b), &count) in &edges {
        assert_eq!(count, 1, "edge {a}→{b} used {count} times one way");
        assert_eq!(
            edges.get(&(b, a)),
            Some(&1),
            "edge {a}→{b} not used the other way"
        );
    }
}

/// Drawn at `scale`, every triangle of a convex shape faces away from its centre, and the
/// shape lies within its box, reaching its sides along X and Y (and Z where `fills_z`).
fn assert_convex_at(mesh: &MeshData, scale: Vec3, fills_z: bool) {
    let points: Vec<Vec3> = mesh.vertices.iter().map(|v| shaped(v, scale)).collect();
    for t in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| points[i as usize]);
        let face = (b - a).cross(c - a);
        if face.length() < 1e-9 {
            continue;
        }
        assert!(
            face.normalize().dot((a + b + c) / 3.0) > 0.0,
            "a triangle faces inwards at {scale}"
        );
    }
    let (lo, hi) = points.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| {
        (lo.min(p), hi.max(p))
    });
    let half = 0.5 * scale;
    assert!(lo.cmpge(-half - 1e-6).all() && hi.cmple(half + 1e-6).all());
    let reach = |i: usize| (lo[i] + half[i]).abs() < 1e-6 && (hi[i] - half[i]).abs() < 1e-6;
    assert!(
        reach(0) && reach(1) && (reach(2) || !fills_z),
        "{lo} {hi} at {scale}"
    );
}

const SCALES: [Vec3; 4] = [
    Vec3::ONE,
    Vec3::new(0.1, 3.0, 0.4),
    Vec3::new(0.01, 0.01, 2.0),
    Vec3::new(4.0, 0.25, 6.0),
];

#[test]
fn rounds_keep_their_size_in_metres() {
    assert!((bevel_radius(Vec3::ONE) - 0.025).abs() < 1e-7, "capped");
    assert!((bevel_radius(Vec3::new(0.1, 5.0, 0.2)) - 0.008).abs() < 1e-7);
    // The box's +X face, drawn 0.1 × 3 × 0.4: inset by the same 8 mm from both edges, though
    // the box is thirty times as tall as it is deep.
    let scale = Vec3::new(0.1, 3.0, 0.4);
    let r = bevel_radius(scale);
    for k in BOX_LEVELS {
        let mesh = MeshData::rounded_box(k);
        let face = mesh.vertices.iter().filter(|v| v.normal[0] > 1.0 - 1e-6);
        assert_eq!(face.clone().count(), 4);
        for v in face {
            let p = shaped(v, scale);
            assert!((p.x - 0.05).abs() < 1e-6);
            assert!((p.y.abs() - (1.5 - r)).abs() < 1e-6);
            assert!((p.z.abs() - (0.2 - r)).abs() < 1e-6);
        }
        // Every rounded vertex lies on its corner's sphere of the bevel radius.
        for v in &mesh.vertices {
            let inner = Vec3::from(v.position).signum() * (0.5 * scale - r);
            assert!((shaped(v, scale).distance(inner) - r).abs() < 1e-6);
        }
    }
    // Procedural vertices that are not bevelled are only scaled.
    let plain = MeshVertex::plain([0.5, 0.5, 0.5], [1.0, 0.0, 0.0]);
    assert_eq!(shaped(&plain, scale), Vec3::new(0.05, 1.5, 0.2));
}

#[test]
fn rounded_boxes_are_closed_and_convex_at_any_scale() {
    for k in BOX_LEVELS {
        let mesh = MeshData::rounded_box(k);
        assert_eq!(
            mesh.indices.len() / 3,
            (8 * k * k + 24 * k + 12) as usize,
            "{k}"
        );
        assert!(mesh.vertices.iter().all(MeshVertex::is_bevelled));
        assert_closed(&mesh);
        for scale in SCALES {
            assert_convex_at(&mesh, scale, true);
        }
    }
}

#[test]
fn rounded_cylinders_are_closed_and_convex_with_round_sides() {
    for n in LEVELS {
        let rim = rim_segments(n);
        let mesh = MeshData::rounded_cylinder(n, rim);
        let per_segment = if rim == 0 { 4 } else { 4 * rim + 4 };
        assert_eq!(mesh.indices.len() / 3, (per_segment * n) as usize);
        if rim > 0 {
            assert_closed(&mesh);
        }
        for scale in SCALES {
            assert_convex_at(&mesh, scale, n % 4 == 0);
        }
        // Drawn 0.2 across and 3 long: the sides at radius 0.1, the caps inset by 16 mm.
        let scale = Vec3::new(0.2, 3.0, 0.2);
        let r = if rim == 0 { 0.0 } else { bevel_radius(scale) };
        for v in &mesh.vertices {
            let p = shaped(v, scale);
            let radius = Vec3::new(p.x, 0.0, p.z).length();
            if v.normal[1] == 0.0 {
                assert!((radius - 0.1).abs() < 1e-6 && p.y.abs() <= 1.5 - r + 1e-6);
            } else if (v.normal[1].abs() - 1.0).abs() < 1e-6 && radius > 1e-6 {
                assert!((radius - (0.1 - r)).abs() < 1e-6 && (p.y.abs() - 1.5).abs() < 1e-6);
            }
        }
    }
}

/// How far inside the sphere of radius `r` about `centre`, on which its corners lie, a
/// triangle's plane reaches, as a share of `r`.
fn depth(points: [Vec3; 3], centre: Vec3, r: f32) -> f32 {
    let [a, b, c] = points;
    1.0 - (b - a).cross(c - a).normalize().dot(a - centre).abs() / r
}

#[test]
fn level_errors_match_the_drawn_geometry() {
    let mut last = (f32::MAX, f32::MAX);
    for n in LEVELS {
        // A circle's sagitta: the chords' midpoints, measured on the drawn cylinder's side.
        let mesh = MeshData::rounded_cylinder(n, rim_segments(n));
        let position = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
        let deepest = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|t| {
                t.iter()
                    .all(|&i| mesh.vertices[i as usize].normal[1] == 0.0)
            })
            .map(|t| {
                let [a, b, c] = t.map(position);
                // Its distance from the axis, the side being parallel to it.
                let axis_point = Vec3::new(0.0, a.y, 0.0);
                depth([a, b, c], axis_point, 0.5)
            })
            .fold(0.0_f32, f32::max);
        assert!((deepest - sagitta(n)).abs() < 1e-5, "{n}: {deepest}");
        // Its rims stray no further than its sides, at the largest rim a cylinder has.
        let rim = rim_segments(n);
        let rim_stray = if rim == 0 {
            2_f32.sqrt() - 1.0
        } else {
            round_error(rim)
        };
        assert!(RIM_SHARE * rim_stray <= sagitta(n) + 1e-7, "{n}: rim {rim}");
        // A sphere's deepest face, against the bound used for it.
        let sphere = MeshData::uv_sphere(1.0, n, n / 2);
        let deepest = sphere
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                depth(
                    t.map(|i| Vec3::from(sphere.vertices[i as usize].position)),
                    Vec3::ZERO,
                    1.0,
                )
            })
            .fold(0.0_f32, f32::max);
        let bound = sphere_error(n);
        assert!(
            deepest <= bound + 1e-5 && deepest > 0.9 * bound,
            "{n}: {deepest} {bound}"
        );
        // Each level finer than the last.
        assert!(sagitta(n) < last.0 && sphere_error(n) < last.1);
        last = (sagitta(n), sphere_error(n));
    }
    // The box's rounds, drawn at scale 1 (radius 2.5 cm): the deepest facet of its corners,
    // or its edges' sagitta, is the error used for its level.
    let mut last = 3_f32.sqrt() - 1.0;
    let r = bevel_radius(Vec3::ONE);
    for k in BOX_LEVELS {
        let mesh = MeshData::rounded_box(k);
        let deepest = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter_map(|t| {
                let vertices = t.map(|i| mesh.vertices[i as usize]);
                // A facet of one corner's round: all its vertices at that corner, not all
                // facing one way (a face).
                let corner = Vec3::from(vertices[0].position);
                let one = vertices
                    .iter()
                    .all(|v| Vec3::from(v.position).abs_diff_eq(corner, 0.0));
                let first = Vec3::from(vertices[0].normal);
                let flat = vertices
                    .iter()
                    .all(|v| Vec3::from(v.normal).abs_diff_eq(first, 0.0));
                (one && !flat).then(|| {
                    let inner = corner.signum() * (0.5 - r);
                    depth(vertices.map(|v| shaped(&v, Vec3::ONE)), inner, r)
                })
            })
            .fold(round_error(k), f32::max);
        let error = rounded_box_error(k);
        assert!(
            (deepest - error).abs() < 1e-4,
            "{k}: {deepest} against {error}"
        );
        assert!(error < last, "{k} rounder than the level before");
        last = error;
    }
    // A sharp box strays √3 − 1 of the radius at its corners: the coarsest level.
    let levels = box_levels();
    assert_eq!(levels.len(), BOX_LEVELS.len() + 1);
    assert!(!levels[0].0.vertices[0].is_bevelled());
    // At 24 segments, the facets lie within 1 % of the radius (`art-style.md` §7).
    assert!(sagitta(24) < 0.01);
    // The finest box meets `model_check`'s rule: its rounds within 1 mm at 2.5 cm.
    assert!(rounded_box_error(BOX_LEVELS[BOX_LEVELS.len() - 1]) * BEVEL_MAX < 0.001);
    assert_eq!(cylinder_levels().len(), LEVELS.len());
    assert_eq!(sphere_levels().len(), LEVELS.len());
}

#[test]
fn the_shaders_round_as_the_primitives_do() {
    let wgsl = include_str!("shaders/globals.wgsl");
    for constant in [
        format!("const BEVEL_SHARE: f32 = {BEVEL_SHARE};"),
        format!("const BEVEL_MAX: f32 = {BEVEL_MAX};"),
        format!("const NO_TOKEN: u32 = 0x{:x}u;", crate::mesh::NO_TOKEN),
    ] {
        assert!(wgsl.contains(&constant), "globals.wgsl lacks `{constant}`");
    }
    assert!(wgsl.contains(
        "return position * scale + (normal - 2.0 * position) * select(0.0, radius, bevelled);"
    ));
}
