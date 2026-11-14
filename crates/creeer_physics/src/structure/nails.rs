//! What nailing by hand asks of a built structure (G2.11 B, ADR-0036; joints: ADR-0033): how a
//! member lies (to find where two of its members touch, and how thick each is along a nail), and
//! a joint given more fasteners where it is (a second nail at a crossing makes the joint of two,
//! not a second joint beside it).

use super::element::Stiffness;
use super::{JointLaw, NodeId, Structure};
use creeer_math::glam::DVec3;

/// How a member lies now: its two nodes, its section's axes across it (turned with its first
/// node) and their half extents (m), and whether it is round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberFrame {
    pub a: NodeId,
    pub b: NodeId,
    pub y: DVec3,
    pub z: DVec3,
    pub half: [f64; 2],
    pub round: bool,
}

impl MemberFrame {
    /// How far its surface reaches from its axis along `way` (m; `way` across it): a round
    /// member's radius, a board's or a beam's box.
    pub fn reach(&self, way: DVec3) -> f64 {
        if self.round {
            return self.half[0];
        }
        way.dot(self.y).abs() * self.half[0] + way.dot(self.z).abs() * self.half[1]
    }
}

impl Structure {
    /// Member `e` as it lies now, if it is a member (not a joint, not broken).
    pub fn member_frame(&self, e: usize) -> Option<MemberFrame> {
        let el = self.elements.get(e)?;
        if el.is_joint() || el.broken {
            return None;
        }
        let q = self.orientation[el.a];
        let [hy, hz] = el.section.extent;
        // (A round section's area is π·r²; a box's 4·hy·hz.)
        let round =
            (el.section.area - std::f64::consts::PI * hy * hz).abs() < 0.01 * el.section.area;
        Some(MemberFrame {
            a: el.a,
            b: el.b,
            y: q * el.y,
            z: q * el.z,
            half: [hy, hz],
            round,
        })
    }

    /// Joint `e`'s stiffness against turning about its fasteners (N·m/rad), if it is a joint
    /// and holds: `JointLaw::twist` as its equivalent member keeps it.
    pub fn joint_twist(&self, e: usize) -> Option<f64> {
        self.joint_ends(e)?;
        let el = &self.elements[e];
        Some(el.section.torsion * el.material.shear_modulus / el.rest_length)
    }

    /// Joint `e` holding by `law` from now on, where it is (more fasteners driven at it): its
    /// ends, its frames and how far it has slipped kept, its stiffness and capacities the new
    /// law's. The body's solver must be refactored (`ImplicitSolver::refactor`).
    ///
    /// # Panics
    /// If `e` is not a joint, or the law is not physical (`JointLaw::equivalent`).
    pub fn refasten(&mut self, e: usize, law: &JointLaw) {
        let el = &mut self.elements[e];
        let to_b = el.fastened.expect("a joint").to_b;
        let (section, material, kept) = law.equivalent(el.rest_length);
        el.stiffness = Stiffness::new(&section, &material, el.rest_length);
        el.section = section;
        el.material = material;
        el.fastened = Some(super::fastened::Fastened { to_b, ..kept });
    }
}
