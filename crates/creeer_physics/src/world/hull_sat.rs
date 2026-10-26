//! Hull against hull (and against box, as a hull): the separating-axis test of convex
//! polyhedra (D. Gregorius, "The Separating Axis Test between Convex Polyhedra", GDC 2013),
//! with face clipping as for boxes (`sat.rs`).
//!
//! The candidates are each hull's face normals and the cross products of edge pairs whose
//! arcs on the Gauss map cross (only those pairs can touch edge to edge). The axis of least
//! penetration decides the contact: across a face, the other hull's face that presses on it
//! (`pressing`) is clipped against the reference face's sides, a patch (`hull_narrow.rs`);
//! across an edge pair lying across a face, that face's patch (`edge_on_face`); across
//! crossed edges, the closest points of the two edges are a point contact of crossed
//! (rounded) edges.

use super::contact::law::FACING;
use super::hull::Hull;
use super::hull_narrow::{Placed, domain, patch};
use super::sat::{ACROSS, BIAS, closest_points};
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
    // An edge lying across a face, wherever the edges are within the bias of the faces: the
    // face's patch whichever axis the test would take, so the contact does not change with it.
    if let Some((separation, i, j, axis)) = edges
        && separation > face - BIAS
        && let Some(patch) = edge_on_face(&a, (i, j), &b, (separation, axis), margin)
    {
        return patch;
    }
    // Edge axes only when clearly better than the faces (as for boxes: no flicker at rest).
    if let Some((separation, i, j, axis)) = edges
        && separation > face + BIAS
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
    let pressing = if separation_a >= separation_b {
        pressing(&a, face_a, &b, &[], margin).map(|p| (p, true))
    } else {
        pressing(&b, face_b, &a, &[], margin).map(|p| (p, false))
    };
    pressing.map_or_else(Vec::new, |(p, reference_is_a)| {
        let (reference, incident) = if reference_is_a { (&a, &b) } else { (&b, &a) };
        clip_flat(reference, incident, &p, reference_is_a, margin)
    })
}

/// An edge pair taken as the axis (`i` of A, `j` of B, `separation` along it), seen as the
/// face beside one of the edges that the other edge lies across: that face's patch, if it
/// is pressed (where it presses: its own polygon's deepest point) within the bias of the
/// edges.
///
/// A long hull's face axis is set by the other hull's vertices metres away, beyond the
/// face, below its plane (a felled log lying across another: the face's separation swung
/// by a millimetre for a turn of 0.3 mrad), so the edges win the test. But an edge lying
/// across a face touches it along a line, from where it crosses one of the face's edges to
/// where it crosses the next. The edge pair names only the deeper end: as the log rocked,
/// the other end became the deeper one, found 66 µm in where the last held 27 µm, and the
/// log never slept (ADR-0024, "G2.0: rest"). The face's patch holds the whole line.
fn edge_on_face(
    a: &Placed<'_>,
    (i, j): (usize, usize),
    b: &Placed<'_>,
    (separation, axis): (f64, DVec3),
    margin: f64,
) -> Option<Vec<RawContact>> {
    let beside = |p: &Placed<'_>, e: usize| p.hull.edges()[e].faces;
    let mut best: Option<(f64, Pressing, bool)> = None;
    for (reference, edge, incident, other, is_a) in [(a, i, b, j, true), (b, j, a, i, false)] {
        // The face beside one edge, pressed by a face beside the edge lying across it.
        for f in beside(reference, edge) {
            let Some(p) = pressing(reference, f, incident, &beside(incident, other), margin) else {
                continue;
            };
            // As deep as the edges, and of those the face the other edge lies flattest
            // across: its normal nearest the edges' axis (either face beside an edge is as
            // deep where the edges cross, and taking the nearer in depth flipped a trunk's
            // patch from one face to the other and back, G2.0).
            let aligned = reference.normals[f].dot(axis).abs();
            if (p.separation - separation).abs() <= BIAS
                && aligned >= ACROSS
                && best.as_ref().is_none_or(|(x, ..)| aligned > *x)
            {
                best = Some((aligned, p, is_a));
            }
        }
    }
    let (_, p, reference_is_a) = best?;
    let (reference, incident) = if reference_is_a { (a, b) } else { (b, a) };
    Some(clip_flat(reference, incident, &p, reference_is_a, margin))
}

/// The patch of `pressing` and of the reference hull's other faces in its plane (within
/// `ACROSS`) that the same incident face presses: one flat region of the hull, each face's
/// part of it its own patch.
///
/// A felled trunk's faces along its length lie in nearly one plane, bent a little at each
/// ring. An edge of a player's body box lying across such a trunk where two of its rings meet
/// presses on both faces; the edge pair named one end of the line or the other as the trunk
/// rocked, and with it one face (16, then 17: 362 µm in at one, 388 µm found at the other).
/// The face's patch held its part of the line alone: each time the choice changed the other
/// part was found pressed in, 0.34 J beyond what the last step left, every ten ticks for good
/// (ADR-0024, "G2.0: at rest"). Each face clips the incident face to its own sides, so the
/// patches cover disjoint parts of the overlap.
fn clip_flat(
    reference: &Placed<'_>,
    incident: &Placed<'_>,
    pressing_face: &Pressing,
    reference_is_a: bool,
    margin: f64,
) -> Vec<RawContact> {
    let mut contacts = clip(reference, incident, pressing_face, reference_is_a);
    let normal = reference.normals[pressing_face.face];
    for (g, n) in reference.normals.iter().enumerate() {
        if g == pressing_face.face || n.dot(normal) < ACROSS {
            continue;
        }
        if let Some(p) = pressing(reference, g, incident, &[pressing_face.incident], margin) {
            contacts.extend(clip(reference, incident, &p, reference_is_a));
        }
    }
    contacts
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

/// A face of the reference hull, the incident hull's face that presses on it, and the part
/// of that face over it.
struct Pressing {
    /// The reference face.
    face: usize,
    /// The incident face.
    incident: usize,
    /// The incident face clipped against the reference face's sides.
    polygon: Vec<DVec3>,
    /// The polygon's least height above the reference face's plane (m).
    separation: f64,
}

/// The face of `incident` that presses on face `f` of `reference`: of the faces `among`
/// (none: all), clipped against the face's sides and within `margin` of it, the most
/// opposed to it of those that reach within the bias of the deepest.
///
/// For boxes the most opposed of all faces is the one (`sat.rs`). A long hull has many
/// nearly parallel faces along its length, and the most opposed may lie metres from the
/// face: a log lying across another found it clipped away, and no contact, and went 1 mm
/// into the other in the next step (G2.0). Taken as the most opposed that reaches over the
/// face at all, it was a face of the next ring 66 µm short of where the edges crossed, and
/// the patch changed from step to step with the edges' (an edge lying across a face is
/// pressed in by either face beside it, `edge_on_face`). The deepest decides as the edges
/// do; of the faces beside the edge, both as deep, the most opposed.
fn pressing(
    reference: &Placed<'_>,
    f: usize,
    incident: &Placed<'_>,
    among: &[usize],
    margin: f64,
) -> Option<Pressing> {
    let (normal, offset) = (reference.normals[f], reference.offsets[f]);
    let depth = |p: DVec3| normal.dot(p) - offset;
    let facing = |g: usize| incident.normals[g].dot(normal);
    let mut order: Vec<usize> = if among.is_empty() {
        (0..incident.normals.len()).collect()
    } else {
        among.to_vec()
    };
    order.sort_by(|&x, &y| facing(x).total_cmp(&facing(y)));
    let ring = reference.ring(f);
    let mut found: Vec<Pressing> = Vec::new();
    for (k, &opposed) in order.iter().enumerate() {
        let first = k == 0 && among.is_empty();
        if !first && facing(opposed) >= -FACING {
            break;
        }
        let mut polygon = incident.ring(opposed);
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
        let separation = polygon
            .iter()
            .map(|&p| depth(p))
            .fold(f64::INFINITY, f64::min);
        if polygon.len() >= 3 && separation < margin {
            found.push(Pressing {
                face: f,
                incident: opposed,
                polygon,
                separation,
            });
        }
    }
    let deepest = found
        .iter()
        .map(|p| p.separation)
        .fold(f64::INFINITY, f64::min);
    found.into_iter().find(|p| p.separation <= deepest + BIAS)
}

/// The patch of `pressing`: its corners on the incident hull. `reference_is_a` says which
/// is hull A.
fn clip(
    reference: &Placed<'_>,
    incident: &Placed<'_>,
    pressing: &Pressing,
    reference_is_a: bool,
) -> Vec<RawContact> {
    let f = pressing.face;
    let (normal, offset) = (reference.normals[f], reference.offsets[f]);
    let depth = |p: DVec3| normal.dot(p) - offset;
    let rectangle = incident.rectangle(pressing.incident);
    let domain = domain(&rectangle, &pressing.polygon);
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
