//! Contacts with the ground: fixed geometry given by a density field (planet terrain).
//!
//! The physics crate does not know the terrain; it sees a [`Ground`]: a field that is
//! negative inside the solid, with a gradient pointing out of it. The distance to the
//! surface is about density / |gradient|; contact points are refined by one projection
//! step, and the contact normal is the gradient direction at the surface point.
//!
//! - **Spheres** (balls, structure joints): one contact, centre to surface.
//! - **Boxes:** points on the box surface, on a grid at most 0.5 m apart. The corners
//!   always count (on flat ground that is exactly the box–plane contact of ADR-0005).
//!   Other points count only where the ground rises into the box beyond the plane
//!   through the corners (their gap is below the corners' gaps interpolated to them), so a
//!   box on flat ground keeps four contacts, but one resting on a bump touches the bump.

use super::shape::{Pose, RawContact, Shape};
use creeer_math::glam::{DMat3, DVec3};

/// Fixed ground described by a density field.
pub trait Ground: Send + Sync + std::fmt::Debug {
    /// The density at `p` (negative inside the ground) and its gradient (pointing out).
    fn density_gradient(&self, p: DVec3) -> (f64, DVec3);
}

/// Spacing of the sample points on a box's surface (m).
const BOX_SAMPLE_SPACING: f64 = 0.5;
/// How much farther than the corner plane a surface point must reach to count (m).
const BULGE: f64 = 1e-3;

/// Distance (m) from `p` to the ground's surface (negative inside), to first order.
fn estimate(ground: &dyn Ground, p: DVec3) -> Option<f64> {
    let (d, g) = ground.density_gradient(p);
    let length = g.length();
    (length > 1e-9).then(|| d / length)
}

/// The surface point nearest `p` and the outward normal there: two projection steps.
fn project(ground: &dyn Ground, p: DVec3) -> Option<(DVec3, DVec3)> {
    let mut q = p;
    let mut normal = DVec3::Y;
    for _ in 0..2 {
        let (d, g) = ground.density_gradient(q);
        let length = g.length();
        if length <= 1e-9 {
            return None;
        }
        normal = g / length;
        q -= normal * (d / length);
    }
    Some((q, normal))
}

/// A contact of point `p` (with `offset` for a sphere's radius) with the ground.
fn point_contact(ground: &dyn Ground, p: DVec3, offset: f64, margin: f64) -> Option<RawContact> {
    let (surface, normal) = project(ground, p)?;
    let contact = RawContact {
        point_a: p,
        point_b: surface,
        normal,
        radius_a: offset,
        radius_b: 0.0,
    };
    (contact.gap() < margin).then_some(contact)
}

/// Contacts of a shape at `pose` with the ground (gap below `margin`).
pub fn contacts(ground: &dyn Ground, pose: Pose, shape: Shape, margin: f64) -> Vec<RawContact> {
    // Cheap rejection from the centre's distance. The first-order distance can
    // underestimate the true one on steep slopes, never by more than half.
    let Some(center) = estimate(ground, pose.position) else {
        return Vec::new();
    };
    if center * 0.5 > shape.bounding_radius() + margin {
        return Vec::new();
    }
    match shape {
        Shape::Sphere { radius } => point_contact(ground, pose.position, radius, margin)
            .into_iter()
            .collect(),
        Shape::Box { half } => box_contacts(ground, pose, half, margin),
    }
}

fn box_contacts(ground: &dyn Ground, pose: Pose, half: DVec3, margin: f64) -> Vec<RawContact> {
    let r = DMat3::from_quat(pose.rotation);
    let at = |s: DVec3| pose.position + r * (half * s);
    // Gaps of the eight corners (s = ±1 per axis), corner c has bits x, y, z.
    let corner_s = |c: usize| {
        DVec3::new(
            if c & 1 == 0 { -1.0 } else { 1.0 },
            if c & 2 == 0 { -1.0 } else { 1.0 },
            if c & 4 == 0 { -1.0 } else { 1.0 },
        )
    };
    let mut gaps = [0.0; 8];
    for (c, gap) in gaps.iter_mut().enumerate() {
        *gap = estimate(ground, at(corner_s(c))).unwrap_or(f64::INFINITY);
    }
    let mut found = Vec::new();
    for (c, &gap) in gaps.iter().enumerate() {
        if gap < margin + BULGE {
            found.extend(point_contact(ground, at(corner_s(c)), 0.0, margin));
        }
    }
    // Other surface points: only if the box is close enough for the ground to reach them.
    let lowest = gaps.iter().copied().fold(f64::INFINITY, f64::min);
    if lowest > margin + half.max_element() {
        return found;
    }
    let steps: [u32; 3] = half.to_array().map(|h| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, positive
        let n = ((2.0 * h / BOX_SAMPLE_SPACING).ceil() as u32).clamp(2, 16);
        n
    });
    let coordinate = |i: u32, n: u32| -1.0 + 2.0 * f64::from(i) / f64::from(n);
    for k in 0..=steps[2] {
        for j in 0..=steps[1] {
            for i in 0..=steps[0] {
                let on = [i, j, k]
                    .iter()
                    .zip(steps)
                    .filter(|&(&a, n)| a == 0 || a == n)
                    .count();
                // Surface points that are not corners (corners have all three extreme).
                if on == 0 || on == 3 {
                    continue;
                }
                let s = DVec3::new(
                    coordinate(i, steps[0]),
                    coordinate(j, steps[1]),
                    coordinate(k, steps[2]),
                );
                // The corners' gaps interpolated trilinearly to s: the gap a flat ground
                // would have here.
                let w = (s + DVec3::ONE) * 0.5;
                let plane: f64 = (0..8)
                    .map(|c| {
                        let f = |bit: usize, t: f64| if c & bit == 0 { 1.0 - t } else { t };
                        gaps[c] * f(1, w.x) * f(2, w.y) * f(4, w.z)
                    })
                    .sum();
                let p = at(s);
                let Some(gap) = estimate(ground, p) else {
                    continue;
                };
                if gap < margin && gap < plane - BULGE {
                    found.extend(point_contact(ground, p, 0.0, margin));
                }
            }
        }
    }
    found
}
