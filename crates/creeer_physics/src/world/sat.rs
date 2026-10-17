//! Box against box: separating-axis test with face clipping (after D. Gregorius, "The
//! Separating Axis Test between Convex Polyhedra", GDC 2013).
//!
//! The axis of least penetration among the 15 candidates decides the contact: across a face,
//! every face of the other box turned towards it is clipped against the reference face's
//! sides, a contact patch each (`contact/law.rs`); across an edge pair, the closest points
//! of the two edges are a point contact of crossed (rounded) edges.

use super::shape::{Domain, Follows, PatchVertex, Pose, RawContact};
use super::surface::Feature;
use creeer_math::glam::{DMat3, DVec3};

struct OrientedBox {
    center: DVec3,
    axes: [DVec3; 3],
    half: DVec3,
}

impl OrientedBox {
    fn new(pose: Pose, half: DVec3) -> Self {
        let r = DMat3::from_quat(pose.rotation);
        Self {
            center: pose.position,
            axes: [r.x_axis, r.y_axis, r.z_axis],
            half,
        }
    }

    /// Half the extent of the box along `axis`.
    fn radius(&self, axis: DVec3) -> f64 {
        (0..3)
            .map(|k| self.half[k] * self.axes[k].dot(axis).abs())
            .sum()
    }

    /// The four corners of the face whose outward normal is closest to `direction`, and
    /// that face's normal.
    fn face(&self, direction: DVec3) -> ([DVec3; 4], DVec3) {
        let (k, sign) = (0..3)
            .map(|k| (k, self.axes[k].dot(direction)))
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(k, d)| (k, d.signum()))
            .expect("three axes");
        let normal = self.axes[k] * sign;
        let center = self.center + normal * self.half[k];
        let (u, v) = (
            self.axes[(k + 1) % 3] * self.half[(k + 1) % 3],
            self.axes[(k + 2) % 3] * self.half[(k + 2) % 3],
        );
        (
            [
                center + u + v,
                center - u + v,
                center - u - v,
                center + u - v,
            ],
            normal,
        )
    }

    /// The edge along axis `k` that lies furthest in `direction`.
    fn edge(&self, k: usize, direction: DVec3) -> (DVec3, DVec3) {
        let mut middle = self.center;
        for j in (0..3).filter(|&j| j != k) {
            middle += self.axes[j] * (self.half[j] * self.axes[j].dot(direction).signum());
        }
        let along = self.axes[k] * self.half[k];
        (middle - along, middle + along)
    }
}

/// Contacts between box A and box B whose gap is below `margin`.
pub fn box_box(
    pose_a: Pose,
    half_a: DVec3,
    pose_b: Pose,
    half_b: DVec3,
    margin: f64,
) -> Vec<RawContact> {
    let (a, b) = (
        OrientedBox::new(pose_a, half_a),
        OrientedBox::new(pose_b, half_b),
    );
    let t = b.center - a.center;
    // Best axis: largest separation (least penetration). Face axes win ties against edge
    // axes, which are only taken when clearly better (avoids flicker on resting boxes).
    let mut best: Option<(f64, DVec3, usize)> = None;
    let mut consider = |axis: DVec3, kind: usize, bias: f64| {
        let length = axis.length();
        if length < 1e-6 {
            return;
        }
        let axis = axis / length;
        let separation = t.dot(axis).abs() - a.radius(axis) - b.radius(axis);
        if best.is_none_or(|(s, _, _)| separation > s + bias) {
            best = Some((separation, axis, kind));
        }
    };
    for k in 0..3 {
        consider(a.axes[k], 0, 0.0);
    }
    for k in 0..3 {
        consider(b.axes[k], 1, 0.0);
    }
    for i in 0..3 {
        for j in 0..3 {
            consider(a.axes[i].cross(b.axes[j]), 2 + 3 * i + j, 1e-4);
        }
    }
    let Some((separation, axis, kind)) = best else {
        return Vec::new();
    };
    if separation > margin {
        return Vec::new();
    }
    // Normal from B to A.
    let normal = if t.dot(axis) > 0.0 { -axis } else { axis };
    match kind {
        0 => clip(&a, &b, -normal, margin, true),
        1 => clip(&b, &a, normal, margin, false),
        _ => {
            let (i, j) = ((kind - 2) / 3, (kind - 2) % 3);
            let (pa, qa) = a.edge(i, -normal);
            let (pb, qb) = b.edge(j, normal);
            let (point_a, point_b) = closest_points(pa, qa, pb, qb);
            vec![RawContact {
                point_a,
                point_b,
                normal,
                radius_a: 0.0,
                radius_b: 0.0,
                features: [Feature::Edge, Feature::Edge],
                patch: None,
                bend: None,
                follows: Follows::Edges((qa - pa).normalize(), (qb - pb).normalize()),
                inset: 0.0,
            }]
        }
    }
}

/// The incident box's face most opposed to the reference box's face (outward normal
/// `towards`, pointing at the incident box) is a patch (`contact/law.rs`): its four corners,
/// and as its domain the face clipped against the reference face's sides.
///
/// Only that face: the other faces turned towards the reference face are steep to it, their
/// depth changes by their whole width as they turn, and the reference face is finite, so
/// the axis may change by the next step. Taken as well, a crate nearly at right angles to
/// the one below it ended a step 2 cm 'inside' it with 27 kJ that the next step's contacts did
/// not have (measured in F5). Edges crossing edges are point contacts.
/// `reference_is_a` says which is box A.
fn clip(
    reference: &OrientedBox,
    incident: &OrientedBox,
    towards: DVec3,
    margin: f64,
    reference_is_a: bool,
) -> Vec<RawContact> {
    let (ref_face, ref_normal) = reference.face(towards);
    let middle = (ref_face[0] + ref_face[1] + ref_face[2] + ref_face[3]) * 0.25;
    let ref_point = ref_face[0];
    let depth = |p: DVec3| ref_normal.dot(p - ref_point);
    let (corners, _) = incident.face(-towards);
    let polygon = clip_to(&ref_face, ref_normal, middle, corners.to_vec());
    if polygon.len() < 3 || polygon.iter().all(|&p| depth(p) >= margin) {
        return Vec::new();
    }
    // The domain in the face's coordinates: s towards corner 1, t towards corner 3.
    let (u, v) = (corners[1] - corners[0], corners[3] - corners[0]);
    let points: Vec<[f64; 2]> = polygon
        .iter()
        .map(|&p| {
            let d = p - corners[0];
            [
                (d.dot(u) / u.length_squared()).clamp(0.0, 1.0),
                (d.dot(v) / v.length_squared()).clamp(0.0, 1.0),
            ]
        })
        .collect();
    let domain = Domain::part(&points);
    corners
        .iter()
        .enumerate()
        .map(|(i, &corner)| {
            let on_reference = corner - ref_normal * depth(corner);
            let (point_a, point_b, normal) = if reference_is_a {
                (on_reference, corner, -ref_normal)
            } else {
                (corner, on_reference, ref_normal)
            };
            RawContact {
                point_a,
                point_b,
                normal,
                radius_a: 0.0,
                radius_b: 0.0,
                features: [Feature::Face, Feature::Face],
                patch: Some(PatchVertex {
                    index: u8::try_from(i).expect("four"),
                    corner_on_a: !reference_is_a,
                    domain: domain.clone(),
                    facing: None,
                }),
                bend: None,
                follows: if reference_is_a {
                    Follows::A
                } else {
                    Follows::B
                },
                inset: 0.0,
            }
        })
        .collect()
}

/// `polygon` clipped by the side planes of the face `ref_face` (normal `ref_normal`, centre
/// `middle`).
fn clip_to(
    ref_face: &[DVec3; 4],
    ref_normal: DVec3,
    middle: DVec3,
    mut polygon: Vec<DVec3>,
) -> Vec<DVec3> {
    for k in 0..4 {
        let (p, q) = (ref_face[k], ref_face[(k + 1) % 4]);
        // Side plane through edge p–q, normal pointing out of the face (the corner order
        // depends on which way the face points, so orient it explicitly).
        let mut side = (q - p).cross(ref_normal).normalize();
        if side.dot(p - middle) < 0.0 {
            side = -side;
        }
        let distance = |v: DVec3| side.dot(v - p);
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
        polygon = clipped;
    }
    // Coinciding edges produce the same corner twice.
    polygon.dedup_by(|a, b| (*a - *b).length() < 1e-9);
    if polygon.len() > 1 && (polygon[0] - polygon[polygon.len() - 1]).length() < 1e-9 {
        polygon.pop();
    }
    polygon
}

/// Closest points between segments p1–q1 and p2–q2.
pub(super) fn closest_points(p1: DVec3, q1: DVec3, p2: DVec3, q2: DVec3) -> (DVec3, DVec3) {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (c, b) = (d1.dot(r), d1.dot(d2));
    let denom = a * e - b * b;
    let mut s = if denom > 1e-12 {
        ((b * f - c * e) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (b * s + f) / e;
    if t < 0.0 {
        t = 0.0;
        s = (-c / a).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - c) / a).clamp(0.0, 1.0);
    }
    (p1 + d1 * s, p2 + d2 * t)
}
