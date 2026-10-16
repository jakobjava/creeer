//! Contact detection between pairs of shapes (gap below `margin`).

use super::sat::box_box;
use super::shape::{Pose, RawContact, Shape, StaticShape};
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// Contacts of shape `a` with shape `b` (normal from b to a).
pub fn shapes(a: (Pose, Shape), b: (Pose, Shape), margin: f64) -> Vec<RawContact> {
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
                    offset: ra + rb,
                },
                margin,
            )
        }
        (Shape::Sphere { radius }, Shape::Box { half }) => {
            sphere_box(a.0.position, radius, b.0, half, margin)
        }
        (Shape::Box { half }, Shape::Sphere { radius }) => {
            sphere_box(b.0.position, radius, a.0, half, margin)
                .into_iter()
                .map(flip)
                .collect()
        }
        (Shape::Box { half: ha }, Shape::Box { half: hb }) => box_box(a.0, ha, b.0, hb, margin),
    }
}

/// Contacts of shape `a` with fixed geometry.
pub fn with_static(a: (Pose, Shape), b: &StaticShape, margin: f64) -> Vec<RawContact> {
    match (*b, a.1) {
        (StaticShape::Plane { normal, offset }, Shape::Sphere { radius }) => {
            let c = a.0.position;
            keep(
                RawContact {
                    point_a: c,
                    point_b: c - normal * (normal.dot(c) - offset),
                    normal,
                    offset: radius,
                },
                margin,
            )
        }
        (StaticShape::Plane { normal, offset }, Shape::Box { half }) => {
            let r = DMat3::from_quat(a.0.rotation);
            (0..8u8)
                .filter_map(|corner| {
                    let signs = DVec3::new(sign(corner & 1), sign(corner & 2), sign(corner & 4));
                    let v = a.0.position + r * (half * signs);
                    let gap = normal.dot(v) - offset;
                    (gap < margin).then(|| RawContact {
                        point_a: v,
                        point_b: v - normal * gap,
                        normal,
                        offset: 0.0,
                    })
                })
                .collect()
        }
        (
            StaticShape::Box {
                center,
                rotation,
                half,
            },
            _,
        ) => shapes(a, (pose(center, rotation), Shape::Box { half }), margin),
    }
}

fn sign(bit: u8) -> f64 {
    if bit == 0 { -1.0 } else { 1.0 }
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
        offset: c.offset,
    }
}

/// Sphere (centre `c`) against a box: the closest point on the box, or, from inside, the
/// nearest face.
fn sphere_box(c: DVec3, radius: f64, pose: Pose, half: DVec3, margin: f64) -> Vec<RawContact> {
    let r = DMat3::from_quat(pose.rotation);
    let local = r.transpose() * (c - pose.position);
    let clamped = local.clamp(-half, half);
    let (point_b, normal) = if local == clamped {
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
        (pose.position + r * face, r * n)
    } else {
        let world = pose.position + r * clamped;
        (world, (c - world).normalize())
    };
    keep(
        RawContact {
            point_a: c,
            point_b,
            normal,
            offset: radius,
        },
        margin,
    )
}

/// A pose from a position and rotation.
pub fn pose(position: DVec3, rotation: DQuat) -> Pose {
    Pose { position, rotation }
}
