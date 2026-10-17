//! The ground under a face pressing on it (`ground.rs`, `ground_hull.rs`).
//!
//! A face takes the ground under its centroid, a point that moves with it smoothly: the
//! surface point there, its normal and its shape operator, a quadric (`FaceGround`). The face
//! presses if it is turned towards that normal, and its patch's four corners
//! (`contact/patch.rs`) are measured against the quadric: they share out the face's load
//! for friction and the step's matrix. Over curved ground the patch presses with the depth
//! measured at the corners of its triangles, each to the ground under it
//! (`Domain::Sampled`): the ground's own, however it curves under the face, and moving within
//! the step as the ground under each point does, so a step ends where the next one finds
//! it. (With one quadric for the depth across the face, taken where the body was deepest,
//! where the face pressed or under its centroid, the ground was millimetres off it 20 cm
//! away on 10 cm bumps 2 m apart, and the error moved with the quadric: a rock was thrown
//! off a face with 5 kJ every third step, then kicked every few steps, crept, and turned on
//! a bump past where its face pressed for seconds, F5.)

use super::ground::BULGE;
use super::ground_sample::Feet;
use super::shape::{Domain, Follows, PatchVertex, RawContact};
use super::surface::Feature;
use creeer_math::glam::{DMat3, DVec3};

/// The ground under a face as its corners see it: the quadric through `at` with outward
/// normal `up` and shape operator `bend` (`ground_sample::measure_face`; none: its tangent
/// plane).
#[derive(Clone, Copy, Debug)]
pub(super) struct FaceGround {
    /// The surface point under the face's centroid.
    pub at: DVec3,
    /// The outward normal there.
    pub up: DVec3,
    /// The shape operator there.
    pub bend: Option<DMat3>,
}

impl FaceGround {
    /// How far `p` is above the quadric along `up`: up·r − ½·tᵀ·S·t, r = p − at and t its
    /// part across `up` (as `Measure::Curved` measures it from `at`).
    pub fn gap(&self, p: DVec3) -> f64 {
        let r = p - self.at;
        let t = r - self.up * self.up.dot(r);
        self.up.dot(r) - self.bend.map_or(0.0, |s| 0.5 * t.dot(s * t))
    }

    /// The quadric's normal under `p`: (up − S·t)/|up − S·t|, the gap's slope.
    pub fn normal(&self, p: DVec3) -> DVec3 {
        let r = p - self.at;
        let t = r - self.up * self.up.dot(r);
        (self.up - self.bend.map_or(DVec3::ZERO, |s| s * t)).normalize()
    }

    /// The four contacts of a face pressing on it as a patch: the parallelogram `corners` (on
    /// A, in order) with the domain `domain`, each corner measured to the ground under it (`feet`)
    /// (the quadric's point under it where there is none), with its normal there: its
    /// friction acts across the ground under it, and faces sharing a corner see the same
    /// ground there. (Across the normal under the face's centroid, a box resting in a hollow
    /// pressed a corner with two faces whose grounds' normals were 14° apart; their friction
    /// planes crossed, and it crept 2 cm in 5 s on a 12° slope with μ = 0.7. Across the
    /// quadric's normal there, a 2 m slab over a bump had its corners' friction tilted by 45°
    /// over flat ground, F5.)
    pub fn corners(
        &self,
        corners: &[DVec3; 4],
        domain: &Domain,
        feet: &mut Feet<'_>,
    ) -> Vec<RawContact> {
        corners
            .iter()
            .enumerate()
            .map(|(i, &corner)| {
                // On flat ground (an unmeasured domain), the plane under the face, as a plane's
                // contact is measured: one normal for all four corners.
                let on_plane = || {
                    (
                        corner - self.normal(corner) * self.gap(corner),
                        self.normal(corner),
                    )
                };
                let (point_b, normal) = match domain.sampled() {
                    Some(_) => feet.at(corner).unwrap_or_else(on_plane),
                    None => on_plane(),
                };
                (i, corner, point_b, normal)
            })
            .map(|(i, corner, point_b, normal)| RawContact {
                point_a: corner,
                point_b,
                normal,
                radius_a: 0.0,
                radius_b: 0.0,
                features: [Feature::Face, Feature::Face],
                patch: Some(PatchVertex {
                    index: u8::try_from(i).expect("four"),
                    corner_on_a: true,
                    domain: domain.clone(),
                    facing: Some(self.up),
                }),
                bend: self.bend,
                follows: Follows::Neither,
                inset: 0.0,
            })
            .collect()
    }
}

/// How far the ground can rise above the plane through a face's corners `corners` (a
/// parallelogram) curving as it does `under` it: the quadric's most, and a [`BULGE`] more for
/// how the ground strays from it.
pub(super) fn reach(corners: &[DVec3; 4], under: &FaceGround) -> f64 {
    let edges = (corners[1] - corners[0], corners[3] - corners[0]);
    let sag = |e: DVec3| under.bend.map_or(0.0, |s| e.dot(s * e).abs() / 8.0);
    sag(edges.0) + sag(edges.1) + BULGE
}

/// A face pressing as a patch: its rectangle, the ground under it, and its domain.
pub(super) struct Pressing {
    /// The rectangle's corners, in order (s from 0 towards 1, t from 0 towards 3).
    pub corners: [DVec3; 4],
    /// The ground its corners are measured against.
    pub under: FaceGround,
    /// Its domain, with the ground under its triangles' corners.
    pub domain: Domain,
}

impl Pressing {
    /// The gap the face's patch sees at the point (s, t) of its rectangle: the gaps of the
    /// corners of the triangle it lies in, interpolated there (on flat ground, the plane's).
    fn gap(&self, st: [f64; 2]) -> f64 {
        let (u, v) = (
            self.corners[1] - self.corners[0],
            self.corners[3] - self.corners[0],
        );
        let at = |[s, t]: [f64; 2]| self.corners[0] + u * s + v * t;
        let Some(sampled) = self.domain.sampled() else {
            return self.under.gap(at(st));
        };
        let gap = |i: usize| {
            let (foot, n) = sampled.feet[i];
            n.dot(at(sampled.vertices[i]) - foot)
        };
        sampled
            .triangles
            .iter()
            .find_map(|t| {
                let w = barycentric(&t.map(|i| sampled.vertices[i]), st)?;
                Some(w[0] * gap(t[0]) + w[1] * gap(t[1]) + w[2] * gap(t[2]))
            })
            .unwrap_or_else(|| self.under.gap(at(st)))
    }
}

/// The weights of `p` in the triangle `t` (none outside it).
fn barycentric(t: &[[f64; 2]; 3], p: [f64; 2]) -> Option<[f64; 3]> {
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let area = cross(t[0], t[1], t[2]);
    if area == 0.0 {
        return None;
    }
    let w = [
        cross(p, t[1], t[2]) / area,
        cross(t[0], p, t[2]) / area,
        cross(t[0], t[1], p) / area,
    ];
    w.iter().all(|&x| x >= -1e-9).then_some(w)
}

/// The gap the patch of a face of `pressing` sees at its point `p` (none off them all): a
/// bump is what the ground rises beyond it.
pub(super) fn face_gap(pressing: &[Pressing], p: DVec3) -> Option<f64> {
    pressing.iter().find_map(|face| {
        let (u, v) = (
            face.corners[1] - face.corners[0],
            face.corners[3] - face.corners[0],
        );
        let d = p - face.corners[0];
        let normal = u.cross(v).normalize_or_zero();
        let (s, t) = (d.dot(u) / u.length_squared(), d.dot(v) / v.length_squared());
        let inside = (-1e-9..=1.0 + 1e-9).contains(&s) && (-1e-9..=1.0 + 1e-9).contains(&t);
        (inside && d.dot(normal).abs() < 1e-6 * u.length().max(v.length()))
            .then(|| face.gap([s.clamp(0.0, 1.0), t.clamp(0.0, 1.0)]))
    })
}
