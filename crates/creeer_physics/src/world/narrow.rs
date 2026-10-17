//! Contact detection between pairs of shapes (gap below `margin`).
//!
//! Every contact says what touches on each side (face, edge, corner, ball: its curvature
//! sets the Hertz law, `contact/law.rs`). A box's face pressed against a flat is a patch:
//! all corners of the face, so the pressure can be integrated over it.

use super::contact::law::FACING;
use super::hull::Hull;
use super::hull_narrow::{hull_plane, sphere_hull};
use super::hull_sat::hull_hull;
use super::sat::box_box;
use super::shape::{Domain, Follows, PatchVertex, Pose, RawContact, Shape, StaticShape};
use super::surface::Feature;
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// Contacts of shape `a` with shape `b` (normal from b to a).
pub fn shapes(a: (Pose, &Shape), b: (Pose, &Shape), margin: f64) -> Vec<RawContact> {
    match (a.1, b.1) {
        (Shape::Sphere { radius: ra }, Shape::Sphere { radius: rb }) => {
            let diff = a.0.position - b.0.position;
            let distance = diff.length();
            let normal = if distance > 1e-12 {
                diff / distance
            } else {
                DVec3::Y
            };
            keep(
                RawContact {
                    point_a: a.0.position,
                    point_b: b.0.position,
                    normal,
                    radius_a: *ra,
                    radius_b: *rb,
                    features: [Feature::Round(*ra), Feature::Round(*rb)],
                    patch: None,
                    bend: None,
                    follows: Follows::Neither,
                    inset: 0.0,
                },
                margin,
            )
        }
        (Shape::Sphere { radius }, Shape::Box { half }) => {
            sphere_box(a.0.position, *radius, b.0, *half, margin)
        }
        (Shape::Box { half }, Shape::Sphere { radius }) => {
            sphere_box(b.0.position, *radius, a.0, *half, margin)
                .into_iter()
                .map(flip)
                .collect()
        }
        (Shape::Box { half: ha }, Shape::Box { half: hb }) => box_box(a.0, *ha, b.0, *hb, margin),
        (Shape::Sphere { radius }, Shape::Hull(hull)) => {
            sphere_hull(a.0.position, *radius, b.0, hull, margin)
        }
        (Shape::Hull(hull), Shape::Sphere { radius }) => {
            sphere_hull(b.0.position, *radius, a.0, hull, margin)
                .into_iter()
                .map(flip)
                .collect()
        }
        (Shape::Hull(ha), Shape::Hull(hb)) => hull_hull(a.0, ha, b.0, hb, margin),
        // A box against a hull is a hull against a hull.
        (Shape::Hull(hull), Shape::Box { half }) => {
            hull_hull(a.0, hull, b.0, &Hull::cuboid(*half), margin)
        }
        (Shape::Box { half }, Shape::Hull(hull)) => {
            hull_hull(a.0, &Hull::cuboid(*half), b.0, hull, margin)
        }
    }
}

/// Contacts of shape `a` with fixed geometry.
pub fn with_static(a: (Pose, &Shape), b: &StaticShape, margin: f64) -> Vec<RawContact> {
    match (*b, a.1) {
        (StaticShape::Plane { normal, offset }, Shape::Sphere { radius }) => {
            let c = a.0.position;
            keep(
                RawContact {
                    point_a: c,
                    point_b: c - normal * (normal.dot(c) - offset),
                    normal,
                    radius_a: *radius,
                    radius_b: 0.0,
                    features: [Feature::Round(*radius), Feature::Face],
                    patch: None,
                    bend: None,
                    follows: Follows::Neither,
                    inset: 0.0,
                },
                margin,
            )
        }
        (StaticShape::Plane { normal, offset }, Shape::Box { half }) => {
            box_plane(a.0, *half, normal, offset, margin)
        }
        (StaticShape::Plane { normal, offset }, Shape::Hull(hull)) => {
            hull_plane(a.0, hull, normal, offset, margin)
        }
        (
            StaticShape::Box {
                center,
                rotation,
                half,
            },
            _,
        ) => shapes(a, (pose(center, rotation), &Shape::Box { half }), margin),
    }
}

/// A box (A) against the solid below the plane n·x = offset: every face turned towards the
/// plane is a patch of its four corners, if any of them is within `margin`. Together the
/// faces turned down are the box's underside, each counted with its area seen along the
/// normal, so the pressure is integrated over the box's whole footprint.
pub fn box_plane(
    pose: Pose,
    half: DVec3,
    normal: DVec3,
    offset: f64,
    margin: f64,
) -> Vec<RawContact> {
    let r = DMat3::from_quat(pose.rotation);
    let mut out = Vec::new();
    for k in 0..3 {
        for sign in [-1.0, 1.0] {
            let outward = r.col(k) * sign;
            if outward.dot(normal) >= -FACING {
                continue;
            }
            let corners = face(pose.position, r, half, k, sign);
            if corners.iter().all(|&v| normal.dot(v) - offset >= margin) {
                continue;
            }
            for (i, &v) in corners.iter().enumerate() {
                out.push(RawContact {
                    point_a: v,
                    point_b: v - normal * (normal.dot(v) - offset),
                    normal,
                    radius_a: 0.0,
                    radius_b: 0.0,
                    features: [Feature::Face, Feature::Face],
                    patch: Some(PatchVertex {
                        index: u8::try_from(i).expect("four"),
                        corner_on_a: true,
                        domain: Domain::WHOLE,
                        facing: None,
                    }),
                    bend: None,
                    follows: Follows::Neither,
                    inset: 0.0,
                });
            }
        }
    }
    out
}

/// The corners, in order around it, of a box's face across axis `k` on side `sign`.
pub(super) fn face(center: DVec3, r: DMat3, half: DVec3, k: usize, sign: f64) -> [DVec3; 4] {
    let middle = center + r.col(k) * (half[k] * sign);
    let (u, v) = (
        r.col((k + 1) % 3) * half[(k + 1) % 3],
        r.col((k + 2) % 3) * half[(k + 2) % 3],
    );
    [
        middle + u + v,
        middle - u + v,
        middle - u - v,
        middle + u - v,
    ]
}

fn keep(contact: RawContact, margin: f64) -> Vec<RawContact> {
    if contact.gap() < margin {
        vec![contact]
    } else {
        Vec::new()
    }
}

fn flip(c: RawContact) -> RawContact {
    RawContact {
        point_a: c.point_b,
        point_b: c.point_a,
        normal: -c.normal,
        radius_a: c.radius_b,
        radius_b: c.radius_a,
        features: [c.features[1], c.features[0]],
        patch: c.patch,
        bend: None,
        follows: match c.follows {
            Follows::A => Follows::B,
            Follows::B => Follows::A,
            Follows::Edges(a, b) => Follows::Edges(b, a),
            Follows::ShapeA => Follows::ShapeB,
            Follows::ShapeB => Follows::ShapeA,
            Follows::Neither => Follows::Neither,
        },
        inset: c.inset,
    }
}

/// Sphere (centre `c`) against a box: the closest point on the box, or, from inside, the
/// nearest face. The box touches with a face, an edge or a corner, as the closest point
/// lies on one, two or three of its face planes.
fn sphere_box(c: DVec3, radius: f64, pose: Pose, half: DVec3, margin: f64) -> Vec<RawContact> {
    let (point_b, normal, feature) = closest_on_box(c, pose, half);
    keep(
        RawContact {
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
        },
        margin,
    )
}

/// The point of a box closest to `c`, or from inside the nearest point of its nearest face;
/// the normal there towards `c` (out of the box); and the feature it lies on.
pub(super) fn closest_on_box(c: DVec3, pose: Pose, half: DVec3) -> (DVec3, DVec3, Feature) {
    let r = DMat3::from_quat(pose.rotation);
    let local = r.transpose() * (c - pose.position);
    let clamped = local.clamp(-half, half);
    let (point_b, normal, feature) = if local == clamped {
        // Centre inside: push out through the nearest face.
        let room = half - local.abs();
        let k = if room.x <= room.y && room.x <= room.z {
            0
        } else if room.y <= room.z {
            1
        } else {
            2
        };
        let mut face = local;
        face[k] = half[k] * local[k].signum();
        let mut n = DVec3::ZERO;
        n[k] = local[k].signum();
        (pose.position + r * face, r * n, Feature::Face)
    } else {
        let world = pose.position + r * clamped;
        let outside = (0..3).filter(|&k| local[k].abs() > half[k]).count();
        let feature = match outside {
            1 => Feature::Face,
            2 => Feature::Edge,
            _ => Feature::Corner,
        };
        (world, (c - world).normalize(), feature)
    };
    (point_b, normal, feature)
}

/// A pose from a position and rotation.
pub fn pose(position: DVec3, rotation: DQuat) -> Pose {
    Pose { position, rotation }
}
