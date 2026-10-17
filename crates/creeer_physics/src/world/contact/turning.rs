//! Normals that turn with the bodies within the step (`shape::Follows`).
//!
//! A contact's normal is the normal of a face or feature of one body (a box under a ball, a
//! patch's reference face), or for crossed edges the cross product of both. Frozen for the
//! step, it stayed put while the bodies turned (up to 0.1 rad a substep before the turn check
//! redoes the step, `turn.rs`), and an edge swept past the frozen plane into the other body:
//! a ball smashing into crates ended a step 1.6 mm into a crate's edge with 205 kN, and two
//! crates' crossed edges 567 kN into each other, energy the impact never had (F5). The
//! normal turns with its body instead, n = q·q₀⁻¹·n₀; for crossed edges n = e_A × e_B /
//! |e_A × e_B|, each edge turning with its body. The gap's slope then has the torques of
//! the turning normal on those bodies.

use super::super::hull_narrow::closest_on_hull;
use super::super::narrow::closest_on_box;
use super::super::shape::{Pose, Shape};
use super::{Contacts, Side, point};
use crate::structure::group::MemberState;
use creeer_math::glam::{DQuat, DVec3};

/// How a contact's normal turns within the step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Turning {
    /// Frozen.
    #[default]
    Frozen,
    /// With a body (member, node), from its orientation at the step's start.
    Body {
        /// The body.
        at: (usize, usize),
        /// Its orientation at the step's start.
        from: DQuat,
    },
    /// Crossed edges: their directions at the step's start (world) and their sides'
    /// orientations then (fixed geometry stays put).
    Edges {
        /// Edge on A, edge on B.
        edges: (DVec3, DVec3),
        /// A's and B's orientations at the step's start.
        from: (DQuat, DQuat),
        /// ±1: the sign that makes the cross product the normal from B to A.
        sign: f64,
    },
}

/// A turned normal, and the gap's slope by the turn of up to two bodies (member, node,
/// ∂g/∂θ).
pub(super) struct Turned {
    pub normal: DVec3,
    pub torques: [Option<(usize, usize, DVec3)>; 2],
}

/// The orientation of a side's node now and its (member, node), or none for fixed geometry.
fn node(side: Side, states: &[MemberState<'_>]) -> Option<(usize, usize, DQuat)> {
    match side {
        Side::Node { member, node, .. } => Some((member, node, states[member].orientation[node])),
        Side::Fixed(_) => None,
    }
}

impl Contacts {
    /// Records where the turning normals start (the step's start).
    pub(super) fn start_turning(&mut self, start: &[MemberState<'_>]) {
        for c in &mut self.list {
            let orientation = |side| node(side, start).map_or(DQuat::IDENTITY, |n| n.2);
            match &mut c.turning {
                Turning::Frozen => {}
                Turning::Body { at, from } => *from = start[at.0].orientation[at.1],
                Turning::Edges { edges, from, sign } => {
                    *from = (orientation(c.a), orientation(c.b));
                    *sign = if edges.0.cross(edges.1).dot(c.normal) < 0.0 {
                        -1.0
                    } else {
                        1.0
                    };
                }
            }
        }
    }

    /// The turned normal of contact `i` at `states`, if it turns (the energy needs no more).
    pub(super) fn turned_normal(&self, i: usize, states: &[MemberState<'_>]) -> Option<DVec3> {
        let c = &self.list[i];
        match c.turning {
            Turning::Frozen => None,
            Turning::Body { at, from } => {
                Some(states[at.0].orientation[at.1] * from.conjugate() * c.normal)
            }
            Turning::Edges { .. } => self.turned(i, states).map(|t| t.normal),
        }
    }

    /// The turned normal of contact `i`, if it turns, and the torques of its turning.
    pub(super) fn turned(&self, i: usize, states: &[MemberState<'_>]) -> Option<Turned> {
        let c = &self.list[i];
        let r = || point(c.a, states).0 - point(c.b, states).0;
        match c.turning {
            Turning::Frozen => None,
            Turning::Body { at, from } => {
                let now = states[at.0].orientation[at.1];
                let normal = now * from.conjugate() * c.normal;
                let r = r();
                Some(Turned {
                    normal,
                    torques: [Some((at.0, at.1, normal.cross(r))), None],
                })
            }
            Turning::Edges { edges, from, sign } => {
                let turn = |side, from: DQuat, e: DVec3| {
                    node(side, states).map_or(e, |(_, _, now)| now * from.conjugate() * e)
                };
                let (ea, eb) = (turn(c.a, from.0, edges.0), turn(c.b, from.1, edges.1));
                let u = ea.cross(eb) * sign;
                let length = u.length();
                if length < 1e-6 {
                    return None;
                }
                let normal = u / length;
                let r = r();
                let across = r - normal * normal.dot(r);
                // dn = (I − n·nᵀ)·du/|u|; du = (δθ_A × e_A) × e_B for A, e_A × (δθ_B × e_B) for B.
                let slope_a = ea.cross(eb.cross(across)) * (sign / length);
                let slope_b = eb.cross(across.cross(ea)) * (sign / length);
                let torque = |side, slope| node(side, states).map(|(m, n, _)| (m, n, slope));
                Some(Turned {
                    normal,
                    torques: [torque(c.a, slope_a), torque(c.b, slope_b)],
                })
            }
        }
    }
}

impl Contacts {
    /// For a ball's contact measured to a shape's closest point (`Measure::Closest`): the
    /// shape's normal there (towards the ball), the point, and its arm from the shape's node.
    pub(super) fn closest(
        &self,
        i: usize,
        shape: usize,
        on_a: bool,
        states: &[MemberState<'_>],
    ) -> Option<(DVec3, DVec3, DVec3)> {
        let c = &self.list[i];
        let (shape_side, ball) = if on_a { (c.a, c.b) } else { (c.b, c.a) };
        let Side::Node { member, node, .. } = shape_side else {
            return None;
        };
        let s = &states[member];
        let pose = Pose {
            position: s.position[node],
            rotation: s.orientation[node],
        };
        let centre = point(ball, states).0;
        let (p, normal, _) = match &self.shapes[shape] {
            Shape::Box { half } => closest_on_box(centre, pose, *half),
            Shape::Hull(hull) => closest_on_hull(centre, pose, hull),
            Shape::Sphere { .. } => return None,
        };
        Some((normal, p, p - pose.position))
    }
}
