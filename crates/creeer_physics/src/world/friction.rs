//! Friction points of contacts with spheres (balls, structure joints, wheels later).
//!
//! A sphere touches its partner at a different material point as it rolls, so friction
//! cannot be measured at a material point of the sphere: within one step that point lifts
//! off along a cycloid and slips by about r·φ³/6 (1.5 mm for a 0.25 m ball turning 0.33 rad),
//! fifteen times the stick distance, and the ball would brake as if sliding. Instead, the
//! friction point is the contact point **carried by the body's linearised motion** from the
//! start of the step:
//!
//! p = x + a₀ + φ × a₀,  φ = log(q·q₀⁻¹),
//!
//! with a₀ the arm from the node to the contact point at the start of the step. Pure rolling
//! (Δx = −φ × a₀) then slips exactly zero. The anchor on the partner is carried the same way.
//! Between steps the stick offset (not a material anchor point) is carried over, so the
//! contact travels over both surfaces (`anchor.rs`).
//!
//! Contacts between flat faces, edges and corners (no sphere, radius 0) keep the material
//! points of ADR-0005: a corner that sticks is the same material point.

use super::contact::Side;
use crate::so3;
use crate::structure::group::MemberState;
use creeer_math::glam::{DQuat, DVec3};

/// A point carried by a node's linearised rigid motion from the start of the step, or a
/// fixed point of the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Carried {
    /// Carried by node `node` of member `member`.
    Node {
        /// Member index in the group.
        member: usize,
        /// Node index in the member.
        node: usize,
        /// Arm from the node to the point at the start of the step (world).
        arm0: DVec3,
        /// The node's orientation at the start of the step.
        q0: DQuat,
    },
    /// A fixed point.
    Fixed(DVec3),
}

impl Carried {
    /// The point `point` (world, at the start of the step) carried by `side`'s node.
    pub fn new(side: Side, point: DVec3, start: &[MemberState<'_>]) -> Self {
        match side {
            Side::Node { member, node, .. } => {
                let s = &start[member];
                Self::Node {
                    member,
                    node,
                    arm0: point - s.position[node],
                    q0: s.orientation[node],
                }
            }
            Side::Fixed(_) => Self::Fixed(point),
        }
    }

    /// Arm from its node at the start of the step (zero for a fixed point).
    pub fn arm0(&self) -> DVec3 {
        match self {
            Self::Node { arm0, .. } => *arm0,
            Self::Fixed(_) => DVec3::ZERO,
        }
    }

    /// Where the point is in `states`.
    pub fn position(&self, states: &[MemberState<'_>]) -> DVec3 {
        match *self {
            Self::Node {
                member,
                node,
                arm0,
                q0,
            } => {
                let s = &states[member];
                let phi = so3::log(s.orientation[node] * q0.conjugate());
                s.position[node] + arm0 + phi.cross(arm0)
            }
            Self::Fixed(p) => p,
        }
    }

    /// Adds the gradient of an energy whose gradient at the point is `force`: the force on
    /// the node's translation, and on its rotation J⁻ᵀ(φ)·(a₀ × force) (the left Jacobian of
    /// the logarithm, so that the gradient is exact for the line search).
    pub fn add_gradient(
        &self,
        states: &[MemberState<'_>],
        force: DVec3,
        gradient: &mut [Vec<DVec3>],
    ) {
        if let Self::Node {
            member,
            node,
            arm0,
            q0,
        } = *self
        {
            let phi = so3::log(states[member].orientation[node] * q0.conjugate());
            let half = gradient[member].len() / 2;
            gradient[member][node] += force;
            gradient[member][half + node] +=
                so3::inverse_left_jacobian(phi).transpose() * arm0.cross(force);
        }
    }
}
