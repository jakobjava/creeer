//! Contacts of convex hulls (`hull.rs`) with balls and planes, and a hull's face as a patch.
//!
//! A face presses as a patch (`contact/patch.rs`), whose depth is the bilinear blend of four
//! corners of a parallelogram. A hull's face has any number of corners, so its patch is the
//! rectangle of least area around it, and the face (or the part of it that can press) is the
//! rectangle's domain: the depth over the face is the same plane, and only the face presses.
//! The rectangle's corners move with the hull, as material points of it.

use super::contact::law::FACING;
use super::hull::Hull;
use super::shape::{Domain, Follows, PatchVertex, Pose, RawContact};
use super::surface::Feature;
use creeer_math::glam::{DMat3, DVec3};

/// A hull at a pose: its corners, and its faces' normals and offsets, in the world.
pub(super) struct Placed<'a> {
    pub hull: &'a Hull,
    pub pose: Pose,
    pub vertices: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub offsets: Vec<f64>,
}

impl<'a> Placed<'a> {
    pub fn new(hull: &'a Hull, pose: Pose) -> Self {
        let r = DMat3::from_quat(pose.rotation);
        let vertices: Vec<DVec3> = hull
            .vertices()
            .iter()
            .map(|&v| pose.position + r * v)
            .collect();
        let normals: Vec<DVec3> = hull.faces().iter().map(|f| r * f.normal).collect();
        let offsets = hull
            .faces()
            .iter()
            .zip(&normals)
            .map(|(f, n)| n.dot(vertices[f.ring[0]]))
            .collect();
        Self {
            hull,
            pose,
            vertices,
            normals,
            offsets,
        }
    }

    /// The corners of face `f`, in order around it.
    pub fn ring(&self, f: usize) -> Vec<DVec3> {
        self.hull.faces()[f]
            .ring
            .iter()
            .map(|&i| self.vertices[i])
            .collect()
    }

    /// The rectangle of face `f` (`hull::Face::rectangle`), in order around it.
    pub fn rectangle(&self, f: usize) -> [DVec3; 4] {
        self.hull.faces()[f]
            .rectangle
            .map(|c| self.pose.position + self.pose.rotation * c)
    }

    /// The centroid of the corners (inside the hull).
    pub fn inside(&self) -> DVec3 {
        #[allow(clippy::cast_precision_loss)] // a few corners
        let n = self.vertices.len() as f64;
        self.vertices.iter().sum::<DVec3>() / n
    }
}

/// The rectangle of least area around the corners `ring` of a face with normal `normal`
/// (one side along one of the face's edges, as the least one always has): its corners in
/// order, s from corner 0 towards corner 1 and t towards corner 3.
pub(super) fn rectangle(ring: &[DVec3], normal: DVec3) -> [DVec3; 4] {
    let origin = ring[0];
    let mut best = (f64::INFINITY, [DVec3::ZERO; 4]);
    for k in 0..ring.len() {
        let edge = ring[(k + 1) % ring.len()] - ring[k];
        if edge.length_squared() == 0.0 {
            continue;
        }
        let u = edge.normalize();
        let v = normal.cross(u);
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for &p in ring {
            let d = p - origin;
            for (axis, value) in [d.dot(u), d.dot(v)].into_iter().enumerate() {
                lo[axis] = lo[axis].min(value);
                hi[axis] = hi[axis].max(value);
            }
        }
        let area = (hi[0] - lo[0]) * (hi[1] - lo[1]);
        if area < best.0 {
            let at = |s: f64, t: f64| origin + u * s + v * t;
            best = (
                area,
                [
                    at(lo[0], lo[1]),
                    at(hi[0], lo[1]),
                    at(hi[0], hi[1]),
                    at(lo[0], hi[1]),
                ],
            );
        }
    }
    best.1
}

/// `polygon` (in the plane of `rectangle`) in the rectangle's coordinates.
pub(super) fn domain(rectangle: &[DVec3; 4], polygon: &[DVec3]) -> Domain {
    let (u, v) = (rectangle[1] - rectangle[0], rectangle[3] - rectangle[0]);
    let points: Vec<[f64; 2]> = polygon
        .iter()
        .map(|&p| {
            let d = p - rectangle[0];
            [
                (d.dot(u) / u.length_squared()).clamp(0.0, 1.0),
                (d.dot(v) / v.length_squared()).clamp(0.0, 1.0),
            ]
        })
        .collect();
    Domain::part(&points)
}

/// The four contacts of a patch: the rectangle's corners on body A (`corner_on_a`) or B, each
/// with its point on the other side from `other`, along `normal` (from B to A), turning with
/// `follows`.
pub(super) fn patch(
    rectangle: &[DVec3; 4],
    domain: &Domain,
    (normal, follows): (DVec3, Follows),
    corner_on_a: bool,
    other: impl Fn(DVec3) -> DVec3,
) -> Vec<RawContact> {
    rectangle
        .iter()
        .enumerate()
        .map(|(i, &corner)| {
            let (point_a, point_b) = if corner_on_a {
                (corner, other(corner))
            } else {
                (other(corner), corner)
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
                    corner_on_a,
                    domain: domain.clone(),
                    facing: None,
                }),
                bend: None,
                follows,
                inset: 0.0,
            }
        })
        .collect()
}

/// A hull (A) against the solid below the plane n·x = offset: every face turned towards the
/// plane with a corner within `margin` of it is a patch (as a box's, `narrow::box_plane`).
pub fn hull_plane(
    pose: Pose,
    hull: &Hull,
    normal: DVec3,
    offset: f64,
    margin: f64,
) -> Vec<RawContact> {
    let placed = Placed::new(hull, pose);
    let mut out = Vec::new();
    for f in 0..hull.faces().len() {
        if placed.normals[f].dot(normal) >= -FACING {
            continue;
        }
        let ring = placed.ring(f);
        if ring.iter().all(|&v| normal.dot(v) - offset >= margin) {
            continue;
        }
        let rectangle = placed.rectangle(f);
        let domain = domain(&rectangle, &ring);
        out.extend(patch(
            &rectangle,
            &domain,
            (normal, Follows::Neither),
            true,
            |p| p - normal * (normal.dot(p) - offset),
        ));
    }
    out
}

/// A ball (centre `c`, A) against a hull: the hull's closest point, or, from inside, the
/// nearest face. The hull touches with a face, an edge or a corner.
pub fn sphere_hull(c: DVec3, radius: f64, pose: Pose, hull: &Hull, margin: f64) -> Vec<RawContact> {
    let (point_b, normal, feature) = closest_on_hull(c, pose, hull);
    let contact = RawContact {
        point_a: c,
        point_b,
        normal,
        radius_a: radius,
        radius_b: 0.0,
        features: [Feature::Round(radius), feature],
        patch: None,
        bend: None,
        follows: Follows::ShapeB,
        inset: 0.0,
    };
    if contact.gap() < margin {
        vec![contact]
    } else {
        Vec::new()
    }
}

/// The point of a hull closest to `c`, or from inside the nearest point of its nearest face;
/// the normal there towards `c` (out of the hull); and the feature it lies on. Worked out in
/// the hull's own frame (no placed copy: it is measured at every evaluation of a step).
pub(super) fn closest_on_hull(c: DVec3, pose: Pose, hull: &Hull) -> (DVec3, DVec3, Feature) {
    let r = DMat3::from_quat(pose.rotation);
    let local = r.transpose() * (c - pose.position);
    let faces = hull.faces();
    let distance = |f: usize| faces[f].normal.dot(local) - faces[f].offset;
    let (nearest, outermost) = (0..faces.len())
        .map(|f| (f, distance(f)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a hull has faces");
    let (point, normal, feature) = if outermost <= 0.0 {
        // Centre inside: push out through the nearest face.
        let n = faces[nearest].normal;
        (local - n * outermost, n, Feature::Face)
    } else {
        // The closest point of the faces the centre is outside of.
        let mut best = (f64::INFINITY, DVec3::ZERO, Feature::Face);
        for f in (0..faces.len()).filter(|&f| distance(f) > 0.0) {
            let ring: Vec<DVec3> = hull.ring(f).collect();
            let (point, feature) = closest_on_face(&ring, faces[f].normal, local);
            let d = (local - point).length_squared();
            if d < best.0 {
                best = (d, point, feature);
            }
        }
        (best.1, (local - best.1).normalize(), best.2)
    };
    (pose.position + r * point, r * normal, feature)
}

/// The point of the convex polygon `ring` (normal `normal`) closest to `p`, and whether it
/// lies inside the face, on an edge or at a corner.
fn closest_on_face(ring: &[DVec3], normal: DVec3, p: DVec3) -> (DVec3, Feature) {
    let on_plane = p - normal * normal.dot(p - ring[0]);
    let len = ring.len();
    let inside = (0..len).all(|k| {
        let (a, b) = (ring[k], ring[(k + 1) % len]);
        (b - a).cross(on_plane - a).dot(normal) >= 0.0
    });
    if inside {
        return (on_plane, Feature::Face);
    }
    let mut best = (f64::INFINITY, DVec3::ZERO, Feature::Edge);
    for k in 0..len {
        let (a, b) = (ring[k], ring[(k + 1) % len]);
        let d = b - a;
        let t = ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0);
        let point = a + d * t;
        let distance = (p - point).length_squared();
        if distance < best.0 {
            let feature = if t > 0.0 && t < 1.0 {
                Feature::Edge
            } else {
                Feature::Corner
            };
            best = (distance, point, feature);
        }
    }
    (best.1, best.2)
}
