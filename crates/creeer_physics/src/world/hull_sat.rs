//! Hull against hull (and against box, as a hull): the separating-axis test of convex
//! polyhedra (D. Gregorius, "The Separating Axis Test between Convex Polyhedra", GDC 2013),
//! with face clipping as for boxes (`sat.rs`).
//!
//! The candidates are each hull's face normals and the cross products of edge pairs whose
//! arcs on the Gauss map cross (only those pairs can touch edge to edge). The axis of least
//! penetration decides the contact: across a face, the other hull's face most opposed to it
//! is clipped against the reference face's sides, a patch (`hull_narrow.rs`); across an edge
//! pair, the closest points of the two edges are a point contact of crossed (rounded) edges.

use super::hull::Hull;
use super::hull_narrow::{Placed, domain, patch};
use super::sat::closest_points;
use super::shape::{Follows, Pose, RawContact};
use super::surface::Feature;
use creeer_math::glam::DVec3;

/// Contacts between hull A and hull B whose gap is below `margin`.
pub fn hull_hull(pose_a: Pose, a: &Hull, pose_b: Pose, b: &Hull, margin: f64) -> Vec<RawContact> {
    let (a, b) = (Placed::new(a, pose_a), Placed::new(b, pose_b));
    let (separation_a, face_a) = face_query(&a, &b);
    if separation_a > margin {
        return Vec::new();
    }
    let (separation_b, face_b) = face_query(&b, &a);
    if separation_b > margin {
        return Vec::new();
    }
    let edges = edge_query(&a, &b);
    if edges.is_some_and(|(separation, ..)| separation > margin) {
        return Vec::new();
    }
    let face = separation_a.max(separation_b);
    // Edge axes only when clearly better than the faces (as for boxes: no flicker at rest).
    if let Some((separation, i, j, axis)) = edges
        && separation > face + 1e-4
    {
        let ends = |p: &Placed<'_>, e: usize| {
            let [u, v] = p.hull.edges()[e].ends;
            (p.vertices[u], p.vertices[v])
        };
        let ((pa, qa), (pb, qb)) = (ends(&a, i), ends(&b, j));
        let (point_a, point_b) = closest_points(pa, qa, pb, qb);
        // The axis points out of A, the normal from B to A.
        return vec![RawContact {
            point_a,
            point_b,
            normal: -axis,
            radius_a: 0.0,
            radius_b: 0.0,
            features: [Feature::Edge, Feature::Edge],
            patch: None,
            bend: None,
            follows: Follows::Edges((qa - pa).normalize(), (qb - pb).normalize()),
            inset: 0.0,
        }];
    }
    if separation_a >= separation_b {
        clip(&a, face_a, &b, margin, true)
    } else {
        clip(&b, face_b, &a, margin, false)
    }
}

/// The largest separation of `other` from a face plane of `reference`, and that face.
fn face_query(reference: &Placed<'_>, other: &Placed<'_>) -> (f64, usize) {
    let mut best = (f64::NEG_INFINITY, 0);
    for (f, (&n, &d)) in reference.normals.iter().zip(&reference.offsets).enumerate() {
        let deepest = other
            .vertices
            .iter()
            .map(|v| n.dot(*v))
            .fold(f64::INFINITY, f64::min);
        if deepest - d > best.0 {
            best = (deepest - d, f);
        }
    }
    best
}

/// The largest separation along the cross product of an edge of `a` and an edge of `b` that
/// can touch (their arcs on the Gauss map cross): (separation, edge of a, edge of b, axis
/// pointing out of a).
fn edge_query(a: &Placed<'_>, b: &Placed<'_>) -> Option<(f64, usize, usize, DVec3)> {
    let centre = a.inside();
    let mut best: Option<(f64, usize, usize, DVec3)> = None;
    for (i, ea) in a.hull.edges().iter().enumerate() {
        let (pa, qa) = (a.vertices[ea.ends[0]], a.vertices[ea.ends[1]]);
        let (u1, u2) = (a.normals[ea.faces[0]], a.normals[ea.faces[1]]);
        for (j, eb) in b.hull.edges().iter().enumerate() {
            let (pb, qb) = (b.vertices[eb.ends[0]], b.vertices[eb.ends[1]]);
            // B's Gauss map is mirrored: the Minkowski difference A − B.
            let (w1, w2) = (-b.normals[eb.faces[0]], -b.normals[eb.faces[1]]);
            if !minkowski_face(u1, u2, w1, w2) {
                continue;
            }
            let (da, db) = (qa - pa, qb - pb);
            let cross = da.cross(db);
            let length = cross.length();
            // Parallel edges: their faces' axes cover them.
            if length < 1e-6 * (da.length() * db.length()) {
                continue;
            }
            let mut axis = cross / length;
            if axis.dot(pa - centre) < 0.0 {
                axis = -axis;
            }
            let separation = axis.dot(pb - pa);
            if best.is_none_or(|(s, ..)| separation > s) {
                best = Some((separation, i, j, axis));
            }
        }
    }
    best
}

/// Whether the arcs a–b and c–d on the unit sphere (each between the normals of an edge's
/// two faces) cross: then the edges' cross product is a face of the Minkowski difference.
fn minkowski_face(a: DVec3, b: DVec3, c: DVec3, d: DVec3) -> bool {
    let (bxa, dxc) = (b.cross(a), d.cross(c));
    let (cba, dba) = (c.dot(bxa), d.dot(bxa));
    let (adc, bdc) = (a.dot(dxc), b.dot(dxc));
    cba * dba < 0.0 && adc * bdc < 0.0 && cba * bdc > 0.0
}

/// The face of `incident` most opposed to face `f` of `reference`, clipped against that
/// face's sides: a patch, its corners on the incident hull. `reference_is_a` says which is
/// hull A.
fn clip(
    reference: &Placed<'_>,
    f: usize,
    incident: &Placed<'_>,
    margin: f64,
    reference_is_a: bool,
) -> Vec<RawContact> {
    let (normal, offset) = (reference.normals[f], reference.offsets[f]);
    let depth = |p: DVec3| normal.dot(p) - offset;
    let opposed = (0..incident.normals.len())
        .min_by(|&x, &y| {
            incident.normals[x]
                .dot(normal)
                .total_cmp(&incident.normals[y].dot(normal))
        })
        .expect("a hull has faces");
    let face = incident.ring(opposed);
    let ring = reference.ring(f);
    let mut polygon = face.clone();
    for k in 0..ring.len() {
        let (p, q) = (ring[k], ring[(k + 1) % ring.len()]);
        // The side plane through edge p–q, normal out of the face (rings run
        // counter-clockwise seen from outside).
        let side = (q - p).cross(normal).normalize();
        polygon = clip_by(&polygon, |v| side.dot(v - p));
    }
    polygon.dedup_by(|x, y| (*x - *y).length() < 1e-9);
    if polygon.len() > 1 && (polygon[0] - polygon[polygon.len() - 1]).length() < 1e-9 {
        polygon.pop();
    }
    if polygon.len() < 3 || polygon.iter().all(|&p| depth(p) >= margin) {
        return Vec::new();
    }
    let rectangle = incident.rectangle(opposed);
    let domain = domain(&rectangle, &polygon);
    // The normal from B to A.
    let along = if reference_is_a { -normal } else { normal };
    let follows = if reference_is_a {
        Follows::A
    } else {
        Follows::B
    };
    patch(
        &rectangle,
        &domain,
        (along, follows),
        !reference_is_a,
        |p| p - normal * depth(p),
    )
}

/// `polygon` without the part where `distance` is positive.
fn clip_by(polygon: &[DVec3], distance: impl Fn(DVec3) -> f64) -> Vec<DVec3> {
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    for m in 0..polygon.len() {
        let (u, w) = (polygon[m], polygon[(m + 1) % polygon.len()]);
        let (du, dw) = (distance(u), distance(w));
        if du <= 0.0 {
            clipped.push(u);
        }
        if (du < 0.0) != (dw < 0.0) {
            clipped.push(u + (w - u) * (du / (du - dw)));
        }
    }
    clipped
}
