//! How a contact's gap is measured within the step (`mod.rs`): against B's plane, a curved
//! surface, the ground or a shape, found afresh at every evaluation.

use super::super::ground::closest;
use super::patch::{Patch, weights};
use super::{Contacts, MATRIX_DEPTH, Side, point};
use crate::structure::group::MemberState;

use creeer_math::glam::{DMat3, DVec3};

/// How a contact's gap is measured during the step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Measure {
    /// Against the plane through B's point, normal to the contact (frozen for the step).
    Frozen,
    /// Against the curved surface through B's point with this shape operator (to second
    /// order; for the path of A's point, `ground.rs`).
    Curved(DMat3),
    /// From A's point to the closest point of the ground, found afresh at every evaluation
    /// (round bodies rolling over it: `ground::closest`).
    Ground,
    /// From a ball's centre to the closest point of the other side's shape (a box's, a
    /// hull's), found afresh at every evaluation: `shape` indexes [`Contacts::shapes`], `on_a`
    /// says whether the shape is side A's. (Frozen, or turning with the box, the plane of an
    /// edge let a ball smashing into crates end a step 19 cm inside one, F5.)
    Closest {
        /// The shape.
        shape: usize,
        /// Whether it is side A's.
        on_a: bool,
    },
}

/// A contact's gap as measured, its gradient with respect to A's point, and both points'
/// lever arms.
struct Measured {
    gap: f64,
    direction: DVec3,
    arm_a: DVec3,
    arm_b: DVec3,
}

impl Contacts {
    /// Gap of contact `i` at `states`, its dashpot's reference gap, its gradient with
    /// respect to A's point (the normal force's direction), slip, and both points' lever
    /// arms. Against a curved B the gap is measured to its surface to second order:
    /// n·r − ½·tᵀ·S·t, t the tangential part of r.
    pub(super) fn gap_and_slip(
        &self,
        i: usize,
        states: &[MemberState<'_>],
    ) -> (f64, f64, DVec3, DVec3, DVec3, DVec3) {
        let c = &self.list[i];
        let normal = self.turned_normal(i, states).unwrap_or(c.normal);
        let m = self.measured(i, states, normal);
        let travel = match self.carried(i) {
            Some([fa, fb]) => fa.position(states) - fb.position(states),
            None => point(c.a, states).0 - point(c.anchor, states).0,
        };
        let slip = travel - c.normal * c.normal.dot(travel);
        // The dashpot damps the penetration rate: the gap's own, from its reference.
        let reference = self.references.get(i).copied().unwrap_or(m.gap);
        (m.gap, reference, m.direction, slip, m.arm_a, m.arm_b)
    }

    /// The gaps of the contacts at `states` as measured with their normals as found (an
    /// impact's, before and after the step: before it, the normals have not turned).
    pub fn measured_gaps(&self, states: &[MemberState<'_>]) -> Vec<f64> {
        (0..self.list.len())
            .map(|i| self.measured(i, states, self.list[i].normal).gap)
            .collect()
    }

    /// Contact `i`'s gap at `states` as its measure says, `normal` its normal there.
    fn measured(&self, i: usize, states: &[MemberState<'_>], normal: DVec3) -> Measured {
        let c = &self.list[i];
        let ((pa, mut arm_a), (pb, mut arm_b)) = (point(c.a, states), point(c.b, states));
        let r = pa - pb;
        let (mut gap, mut direction) = (normal.dot(r) - c.offset, normal);
        match self.measures.get(i).copied().unwrap_or(Measure::Frozen) {
            Measure::Frozen => {}
            Measure::Curved(s) => {
                let t = r - c.normal * c.normal.dot(r);
                let st = s * t;
                gap -= 0.5 * t.dot(st);
                direction -= st;
            }
            Measure::Ground => {
                let found = self.ground.as_deref().and_then(|g| closest(g, pa, pb));
                if let Some((q, n)) = found {
                    gap = n.dot(pa - q) - c.offset;
                    direction = n;
                }
            }
            Measure::Closest { shape, on_a } => {
                if let Some((normal, p, arm)) = self.closest(i, shape, on_a, states) {
                    // The normal from B to A; the force on the shape acts at its closest point.
                    let centre = if on_a { pb } else { pa };
                    gap = normal.dot(centre - p) - c.offset;
                    direction = if on_a { -normal } else { normal };
                    if on_a {
                        arm_a = arm;
                    } else {
                        arm_b = arm;
                    }
                }
            }
        }
        Measured {
            gap,
            direction,
            arm_a,
            arm_b,
        }
    }
}

impl Contacts {
    /// The depths measured at the corners of each patch's triangles over curved ground
    /// (`Sampled`) at `states`, each a point of the face measured to the ground under it to
    /// second order (as `Measure::Curved`); none for a patch on a plane.
    pub(super) fn measured_depths(&self, states: &[MemberState<'_>]) -> Vec<Vec<f64>> {
        self.patches
            .iter()
            .map(|p| {
                self.measured_points(p, states)
                    .iter()
                    .map(|m| -m.0)
                    .collect()
            })
            .collect()
    }

    /// The points of patch `p`'s triangles over curved ground at `states` (none on a plane):
    /// each one's gap, the gap's slope (the force's direction), and its lever arm from the
    /// face's node. Each point is its corners' blend, a material point of the face.
    pub(super) fn measured_points(
        &self,
        p: &Patch,
        states: &[MemberState<'_>],
    ) -> Vec<(f64, DVec3, DVec3)> {
        let Some(s) = p.sampled() else {
            return Vec::new();
        };
        let corners: [(DVec3, DVec3); 4] =
            std::array::from_fn(|k| point(self.list[p.first + k].a, states));
        s.vertices
            .iter()
            .zip(&s.feet)
            .map(|(&v, &(foot, n))| {
                let w = weights(v);
                let (mut at, mut arm) = (DVec3::ZERO, DVec3::ZERO);
                for (&wk, &(pk, ak)) in w.iter().zip(&corners) {
                    at += pk * wk;
                    arm += ak * wk;
                }
                let r = at - foot;
                let t = r - n * n.dot(r);
                let st = s.bend * t;
                (n.dot(r) - 0.5 * t.dot(st), n - st, arm)
            })
            .collect()
    }
}

impl Contacts {
    /// The step along `direction` (each member's translation blocks, then its rotation
    /// blocks, from `offsets[member]`) at which the first contact now apart is pressed
    /// [`MATRIX_DEPTH`] deep, its gap followed linearly, if that is within a full step.
    ///
    /// A Newton step from a state where a contact is still open does not know that contact's
    /// stiffness: it runs into it, far too long, and backtracking takes a tenth of it again and
    /// again while the gap closes geometrically (a rock coming down onto the ramp: ten
    /// iterations). Taken to where the contact closes, the next iteration has its stiffness
    /// and converges as Newton does.
    pub(super) fn closing_step(
        &self,
        states: &[MemberState<'_>],
        direction: &[DVec3],
        offsets: &[usize],
    ) -> Option<f64> {
        let moved = |side: Side, arm: DVec3| match side {
            Side::Node { member, node, .. } => {
                let n = states[member].position.len();
                let at = offsets[member];
                direction[at + node] + direction[at + n + node].cross(arm)
            }
            Side::Fixed(_) => DVec3::ZERO,
        };
        let geometry = self.geometry(states);
        let mut first: Option<f64> = None;
        for (i, c) in self.list.iter().enumerate() {
            let (gap, _, normal, _, arm_a, arm_b) = geometry.at[i];
            if gap <= 0.0 {
                continue;
            }
            let rate = normal.dot(moved(c.a, arm_a) - moved(c.b, arm_b));
            if rate < 0.0 {
                let alpha = (gap + MATRIX_DEPTH) / -rate;
                if alpha < 1.0 && first.is_none_or(|f| alpha < f) {
                    first = Some(alpha);
                }
            }
        }
        first
    }
}
