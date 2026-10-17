//! Contacts of convex hulls (`hull_narrow.rs`, `hull_sat.rs`): a hull shaped as a box touches
//! exactly as the box does, and a triangular face presses as a punch of its area.

use super::contact::cap::Cap;
use super::contact::patch::Patch;
use super::hull::Hull;
use super::hull_narrow::{hull_plane, sphere_hull};
use super::hull_sat::hull_hull;
use super::narrow::{box_plane, pose, shapes};
use super::sat::box_box;
use super::shape::{RawContact, Shape};
use super::*;
use std::sync::Arc;

/// A pseudo-random generator for poses (deterministic).
pub(super) fn numbers(seed: u64) -> impl FnMut() -> f64 {
    let mut state = seed;
    move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_precision_loss)] // 53 bits of it
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        unit
    }
}

/// A patch's pressing polygon in the world (its domain through the bilinear map of its
/// corners), its area and centroid, and its normal; or a point contact's points.
pub(super) fn summary(contacts: &[RawContact]) -> Vec<(DVec3, f64, DVec3)> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < contacts.len() {
        let c = &contacts[k];
        if let Some(v) = &c.patch {
            let corners: Vec<DVec3> = contacts[k..k + 4]
                .iter()
                .map(|c| if v.corner_on_a { c.point_a } else { c.point_b })
                .collect();
            let at = |[s, t]: [f64; 2]| {
                corners[0] * ((1.0 - s) * (1.0 - t))
                    + corners[1] * (s * (1.0 - t))
                    + corners[2] * (s * t)
                    + corners[3] * ((1.0 - s) * t)
            };
            let points: Vec<DVec3> = v.domain.points().iter().map(|&p| at(p)).collect();
            let (mut area, mut moment) = (0.0, DVec3::ZERO);
            for i in 1..points.len() - 1 {
                let a = (points[i] - points[0])
                    .cross(points[i + 1] - points[0])
                    .length()
                    * 0.5;
                area += a;
                moment += (points[0] + points[i] + points[i + 1]) * (a / 3.0);
            }
            out.push((moment / area, area, c.normal));
            k += 4;
        } else {
            out.push((c.point_a, (c.point_a - c.point_b).length(), c.normal));
            k += 1;
        }
    }
    out
}

/// The smallest gap of `contacts`: of a patch, over its pressing polygon's corners (its
/// rectangle's corners may lie beyond the face).
pub(super) fn deepest(contacts: &[RawContact]) -> f64 {
    let mut gap = f64::INFINITY;
    let mut k = 0;
    while k < contacts.len() {
        let c = &contacts[k];
        let Some(v) = &c.patch else {
            gap = gap.min(c.gap());
            k += 1;
            continue;
        };
        let bilinear = |points: [DVec3; 4], [s, t]: [f64; 2]| {
            points[0] * ((1.0 - s) * (1.0 - t))
                + points[1] * (s * (1.0 - t))
                + points[2] * (s * t)
                + points[3] * ((1.0 - s) * t)
        };
        let a = std::array::from_fn(|i| contacts[k + i].point_a);
        let b = std::array::from_fn(|i| contacts[k + i].point_b);
        for &p in v.domain.points() {
            gap = gap.min(c.normal.dot(bilinear(a, p) - bilinear(b, p)));
        }
        k += 4;
    }
    gap
}

/// Whether two lists of [`summary`] hold the same contacts, in any order.
pub(super) fn same(a: &[(DVec3, f64, DVec3)], b: &[(DVec3, f64, DVec3)]) -> bool {
    let order = |v: &[(DVec3, f64, DVec3)]| {
        let mut v = v.to_vec();
        v.sort_by(|x, y| x.0.to_array().partial_cmp(&y.0.to_array()).expect("finite"));
        v
    };
    let (a, b) = (order(a), order(b));
    a.len() == b.len()
        && a.iter().zip(&b).all(|(x, y)| {
            (x.0 - y.0).length() < 1e-9 && (x.1 - y.1).abs() < 1e-9 && (x.2 - y.2).length() < 1e-9
        })
}

/// A hull built from a box's corners touches planes, boxes and balls exactly as the box:
/// the same patches (pressing polygon, area and normal), edge contacts and ball contacts, in
/// 2,000 random poses, from a centimetre apart to a centimetre into each other. (Deeper in,
/// two edge axes may tie within 10⁻⁴ m, and the box takes the first, the hull the larger.)
#[test]
fn a_hull_of_a_box_touches_as_the_box() {
    let mut next = numbers(11);
    let (ha, hb) = (DVec3::new(0.5, 0.3, 0.2), DVec3::new(0.4, 0.4, 0.25));
    let (hull_a, hull_b) = (Hull::cuboid(ha), Hull::cuboid(hb));
    let shape_a = Shape::Hull(Arc::new(hull_a.clone()));
    let mut seen = [0; 3];
    for _ in 0..2000 {
        let mut turn = || {
            rotation(
                DVec3::new(next() - 0.5, next() - 0.5, next() - 0.5),
                6.0 * next(),
            )
        };
        let (qa, qb) = (turn(), turn());
        let pa = pose(DVec3::new(next(), next(), next()) * 0.4, qa);
        // B along a random direction from A, from just touching (bisected) to ±1 cm.
        let away = DVec3::new(next() - 0.5, next() - 0.5, next() - 0.5).normalize();
        let at = |d: f64| pose(pa.position + away * d, qb);
        let (mut lo, mut hi) = (0.0, 2.0);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if box_box(pa, ha, at(mid), hb, 0.0).is_empty() {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let pb = at(hi + 0.02 * (next() - 0.5));
        let margin = 0.005;
        let boxes = box_box(pa, ha, pb, hb, margin);
        let hulls = hull_hull(pa, &hull_a, pb, &hull_b, margin);
        assert!(
            same(&summary(&boxes), &summary(&hulls)),
            "{pa:?} {pb:?}\n{boxes:?}\n{hulls:?}"
        );
        // A hull against a box is a hull against the box's hull.
        let mixed = shapes((pa, &shape_a), (pb, &Shape::Box { half: hb }), margin);
        assert!(same(&summary(&boxes), &summary(&mixed)));
        seen[usize::from(!boxes.is_empty())] += 1;
        seen[2] += usize::from(boxes.iter().any(|c| c.patch.is_none()));
        let plane = (
            DVec3::new(next() - 0.5, 1.0, next() - 0.5).normalize(),
            0.3 * next(),
        );
        let on_plane = box_plane(pa, ha, plane.0, plane.1, margin);
        let hull_on_plane = hull_plane(pa, &hull_a, plane.0, plane.1, margin);
        assert!(same(&summary(&on_plane), &summary(&hull_on_plane)));
        let ball = DVec3::new(next() - 0.5, next() - 0.5, next() - 0.5) * 1.6;
        let as_box = shapes(
            (pose(ball, DQuat::IDENTITY), &Shape::Sphere { radius: 0.2 }),
            (pa, &Shape::Box { half: ha }),
            1.0,
        );
        let as_hull = sphere_hull(ball, 0.2, pa, &hull_a, 1.0);
        assert_eq!(as_box.len(), as_hull.len());
        for (x, y) in as_box.iter().zip(&as_hull) {
            assert!(
                (x.point_b - y.point_b).length() < 1e-9 && (x.normal - y.normal).length() < 1e-9
            );
            assert_eq!(x.features, y.features, "ball at {ball}");
        }
    }
    // Both apart and touching, with faces and with edges.
    assert!(seen.iter().all(|&n| n > 50), "{seen:?}");
}

/// A tetrahedron's face lying on a plane is a patch of the triangle's area, as stiff as a
/// flat punch of that area pressed in evenly: 2·E*·√(A/π).
#[test]
fn a_triangular_face_presses_as_a_punch_of_its_area() {
    let points = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.2, 0.0, 0.1),
        DVec3::new(0.3, 0.0, 0.9),
        DVec3::new(0.4, 0.8, 0.3),
    ];
    let (hull, centroid) = Hull::new(&points).expect("a tetrahedron");
    let at = pose(centroid, DQuat::IDENTITY);
    let contacts = hull_plane(at, &hull, DVec3::Y, 0.0, 1e-3);
    assert_eq!(contacts.len(), 4, "one face, one patch");
    let v = contacts[0].patch.clone().expect("a patch");
    let corners = std::array::from_fn(|k| contacts[k].point_a);
    let mut patch = Patch::new(0, corners, &v.domain, DVec3::Y, 1.0e9);
    patch.set_cap(Cap::NONE);
    let area = 0.5
        * (points[1] - points[0])
            .cross(points[2] - points[0])
            .length();
    assert!(
        (patch.area / area - 1.0).abs() < 1e-12,
        "{} m², {area} m²",
        patch.area
    );
    let depth = 1e-6;
    let (energy, forces) = patch.evaluate([depth; 4]);
    let stiffness = 2.0e9 * (area / std::f64::consts::PI).sqrt();
    assert!((energy / (0.5 * stiffness * depth * depth) - 1.0).abs() < 1e-12);
    assert!((forces.iter().sum::<f64>() / (stiffness * depth) - 1.0).abs() < 1e-12);
}
