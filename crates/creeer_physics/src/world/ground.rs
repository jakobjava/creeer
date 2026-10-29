//! Contacts with the ground: fixed geometry given by a density field (planet terrain).
//!
//! The physics crate does not know the terrain; it sees a [`Ground`]: a field that is
//! negative inside the solid, with a gradient pointing out of it. The distance to the
//! surface is about density / |gradient|, never more than the density itself where the
//! gradient is shorter than a distance's; contact points are projected onto the surface
//! (`ground/projection.rs`), and the contact normal is the gradient direction there.
//!
//! - **Spheres** (balls, structure joints): one contact, centre to surface.
//! - **Boxes:** every face turned towards the ground within the margin is a contact patch
//!   (`contact/law.rs`), over curved ground pressing with the ground's depth measured across it
//!   (`ground_rise.rs`; on flat ground exactly the box–plane patch). Points on a grid at most
//!   0.5 m apart over the box's surface count too, where the ground rises into the box beyond
//!   what its patches see: a bump, a point contact of that size. It presses
//!   with what it rises beyond the face that already presses there: it starts to touch where
//!   it first counts. (Counted only from 1 mm above the plane, under a face resting on the
//!   ground it began 1 mm deep, and a rock on a hump was kicked every few steps, F5.) A bump
//!   found again keeps the inset it had while it presses (`ground_rise::bump`, G2.0).

use super::ground_rise::{Kept, Pressing, bump, face_gap, none_kept};
use super::ground_sample::{Feet, measure_face};
use super::shape::{Domain, Follows, Pose, RawContact, Shape};
use super::surface::Feature;
use creeer_math::glam::{DMat3, DVec3};

/// Fixed ground described by a density field.
pub trait Ground: Send + Sync + std::fmt::Debug {
    /// The density at `p` (negative inside the ground) and its gradient (pointing out).
    fn density_gradient(&self, p: DVec3) -> (f64, DVec3);
}

/// Spacing of the sample points on a box's surface (m).
pub(super) const BOX_SAMPLE_SPACING: f64 = 0.5;
/// How much farther than the corner plane a surface point must reach to count (m).
pub(super) const BULGE: f64 = 1e-3;

#[cfg(test)]
mod hole_tests;
mod projection;
pub use projection::closest;
pub(super) use projection::{estimate, project};

/// Step of the finite differences for the ground's curvature (m).
const CURVATURE_STEP: f64 = 0.02;

/// The ground's shape operator S at surface point `q` with outward normal `n`: near q the
/// surface rises ½·tᵀ·S·t above its tangent plane at a tangential offset t, and its normal
/// there is n − S·t (to first order). From the normals 2 cm to either side. A contact's gap
/// is measured to this curved surface instead of the tangent plane: on curved ground a step
/// ends pressed in or lifted by κ·d²/2 over its travel d against a plane, and the next step
/// gave that back as energy (a ball rolling over bumps gained energy for tens of steps in a
/// row, measured in F5).
pub fn curvature(ground: &dyn Ground, q: DVec3, n: DVec3) -> Option<DMat3> {
    let (t1, t2) = n.any_orthonormal_pair();
    let mut s = DMat3::ZERO;
    for t in [t1, t2] {
        let (_, plus) = project(ground, q + t * CURVATURE_STEP)?;
        let (_, minus) = project(ground, q - t * CURVATURE_STEP)?;
        let mut st = (minus - plus) / (2.0 * CURVATURE_STEP);
        st -= n * n.dot(st);
        // S = Σ (S·tₖ) ⊗ tₖ.
        s += DMat3::from_cols(st * t.x, st * t.y, st * t.z);
    }
    let s = (s + s.transpose()) * 0.5;
    // Flatter than [`FLAT`]: the differences of the normals are rounding (3·10⁻¹³ m⁻¹ on a
    // plane, from its gradient's), and a face's corners measured half a metre from its
    // centroid saw that rounding: their forces split unevenly at 10⁻⁶ (F5).
    let flat = s.to_cols_array().iter().all(|x| x.abs() < FLAT);
    Some(if flat { DMat3::ZERO } else { s })
}

/// Ground curving less than this is flat (m⁻¹): a sphere a million kilometres across, flatter
/// than any planet; over a metre it sags 10⁻¹⁰ m.
pub(super) const FLAT: f64 = 1e-9;

/// A contact of point `p` (with `offset` for a sphere's radius) with the ground.
pub(super) fn point_contact(
    ground: &dyn Ground,
    p: DVec3,
    offset: f64,
    margin: f64,
) -> Option<RawContact> {
    let (surface, normal) = project(ground, p)?;
    let contact = RawContact {
        point_a: p,
        point_b: surface,
        normal,
        radius_a: offset,
        radius_b: 0.0,
        features: [
            if offset > 0.0 {
                Feature::Round(offset)
            } else {
                Feature::Face
            },
            Feature::Face,
        ],
        patch: None,
        bend: curvature(ground, surface, normal),
        follows: Follows::Neither,
        inset: 0.0,
    };
    (contact.gap() < margin).then_some(contact)
}

/// Contacts of a shape at `pose` with the ground (gap below `margin`).
pub fn contacts(ground: &dyn Ground, pose: Pose, shape: &Shape, margin: f64) -> Vec<RawContact> {
    contacts_kept(ground, pose, shape, margin, &none_kept)
}

/// Contacts of a shape at `pose` with the ground (gap below `margin`), keeping the bumps the
/// last step had (`kept`, `ground_rise::bump`).
pub(super) fn contacts_kept(
    ground: &dyn Ground,
    pose: Pose,
    shape: &Shape,
    margin: f64,
    kept: Kept<'_>,
) -> Vec<RawContact> {
    // Cheap rejection from the centre's distance. The first-order distance can
    // underestimate the true one on steep slopes, never by more than half.
    let Some(center) = estimate(ground, pose.position) else {
        return Vec::new();
    };
    if center * 0.5 > shape.bounding_radius() + margin {
        return Vec::new();
    }
    match shape {
        Shape::Sphere { radius } => point_contact(ground, pose.position, *radius, margin)
            .into_iter()
            .collect(),
        Shape::Box { half } => box_contacts(ground, pose, *half, margin, kept),
        Shape::Hull(hull) => super::ground_hull::hull_contacts(ground, pose, hull, margin, kept),
    }
}

fn box_contacts(
    ground: &dyn Ground,
    pose: Pose,
    half: DVec3,
    margin: f64,
    kept: Kept<'_>,
) -> Vec<RawContact> {
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
    let (lowest, &deepest) = gaps
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .expect("eight corners");
    // Only if the box is close enough for the ground to reach it.
    if deepest > margin + half.max_element() || project(ground, at(corner_s(lowest))).is_none() {
        return Vec::new();
    }
    // The faces that press as patches (`faces`), and other surface points where the ground
    // rises beyond what the patches see.
    let corners: [DVec3; 8] = std::array::from_fn(|c| at(corner_s(c)));
    let (mut found, patched) = faces(ground, &corners, r, margin);
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
                // On a face pressing as a patch, the gap its patch sees there: a bump is what
                // rises beyond.
                let plane = face_gap(&patched, p).unwrap_or(plane);
                let Some(gap) = estimate(ground, p) else {
                    continue;
                };
                found.extend(bump(ground, (p, Feature::Face), (gap, plane), margin, kept));
            }
        }
    }
    found
}

/// The faces of a box turned towards the ground within `margin` of it: a patch each, over
/// curved ground pressing with the ground's depth measured across it (`ground_sample.rs`),
/// on flat ground the box–plane patch.
fn faces(
    ground: &dyn Ground,
    corners: &[DVec3; 8],
    r: DMat3,
    margin: f64,
) -> (Vec<RawContact>, Vec<Pressing>) {
    // The box's corners are measured once though three faces share each.
    let mut feet = Feet::new(ground);
    let mut out = Vec::new();
    let mut patched = Vec::new();
    for k in 0..3 {
        for sign in [-1.0, 1.0] {
            // The face's corners in order around it: (+, +), (−, +), (−, −), (+, −) along the
            // two other axes.
            let (a, b) = ((k + 1) % 3, (k + 2) % 3);
            let side = if sign > 0.0 { 1 << k } else { 0 };
            let ring = [(1, 1), (0, 1), (0, 0), (1, 0)].map(|(i, j)| side | (i << a) | (j << b));
            let parallelogram = ring.map(|c| corners[c]);
            let points = Domain::WHOLE.points();
            let Some(face) =
                measure_face(&mut feet, &parallelogram, points, r.col(k) * sign, margin)
            else {
                continue;
            };
            out.extend(face.under.corners(&parallelogram, &face.domain, &mut feet));
            patched.push(Pressing {
                corners: parallelogram,
                under: face.under,
                domain: face.domain,
            });
        }
    }
    (out, patched)
}
