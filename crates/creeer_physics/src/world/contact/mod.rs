//! Contacts as energy terms of the implicit step (ADR-0005).
//!
//! A contact joins a point of A and a point of B (or of the fixed world), with a normal from
//! B to A that is frozen for the step. Its gap is g = n·(p_A − p_B) − offset. The energy is
//!
//! - elastic: ½·k·min(g, 0)² (a stiff penalty: 1 mm of penetration per 100 kN at 1e8 N/m);
//! - damping: c/(2β)·(min(g, 0) − min(ĝ, 0))², a dashpot on the rate of penetration, strong
//!   enough (c = k·h) to absorb within the step the contact's own vibration, which a 60 Hz
//!   step cannot resolve (about 1 ms for steel). Bouncing then follows Newton's impact law
//!   with the materials' restitution (`impact.rs`);
//! - friction: μ·λ·f₀(|u|), with u the tangential offset of A's point from its anchor on B
//!   and λ the normal force (lagged, refreshed during the step as in IPC, Li et al. 2020).
//!   u is measured from an anchor point on B, and f₀ is |u| smoothed below 0.1 mm: sticking
//!   is a stiff spring that never creeps, sliding is Coulomb friction μ·λ exactly.

mod matrix;

use super::friction::Carried;
use crate::structure::group::{Coupling, MemberState, NodeCoupling, StepInfo};
use creeer_math::glam::{DMat3, DVec3};

/// Static friction is a stiff spring between the contact point and an anchor on the other
/// body, up to this offset (m); beyond it the contact slides with force μ·λ and the anchor
/// follows. Measuring from an anchor instead of from the start of each step means a
/// sticking contact holds a constant offset and never creeps.
pub const STICK_DISTANCE: f64 = 1.0e-4;

/// One end of a contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    /// A point attached to a node (`local` in the node's frame), of member `member` of the
    /// group.
    Node {
        /// Member index in the group.
        member: usize,
        /// Node index in the member.
        node: usize,
        /// Point in the node's frame.
        local: DVec3,
    },
    /// A fixed point of the world.
    Fixed(DVec3),
}

/// A contact and its material parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// Side A (the normal points towards it).
    pub a: Side,
    /// Side B.
    pub b: Side,
    /// Unit normal from B to A, frozen for the step.
    pub normal: DVec3,
    /// Radii subtracted from the distance (spheres).
    pub offset: f64,
    /// Penalty stiffness k (N/m).
    pub stiffness: f64,
    /// Dashpot c (N·s/m).
    pub damping: f64,
    /// Coulomb friction coefficient μ.
    pub friction: f64,
    /// Coefficient of restitution for the impact law after the step.
    pub restitution: f64,
    /// Normal velocity of A relative to B at the start of the step (m/s, negative when
    /// approaching).
    pub approach: f64,
    /// Tangential speed of A relative to B at the start of the step (m/s): whether it
    /// slides, for the Newton matrix.
    pub sliding: f64,
    gap_hat: f64,
    lambda: f64,
    /// The material point of B that A's point sticks to (friction measures slip from it).
    pub anchor: Side,
    /// Radii of the two sides at the contact (spheres). With either above zero, friction
    /// acts at carried points (`friction.rs`) instead of material points.
    pub radius_a: f64,
    /// Radius of side B.
    pub radius_b: f64,
    /// Lever arms (world) of both points from their nodes at the start of the step.
    arm_a: DVec3,
    arm_b: DVec3,
}

impl Contact {
    /// A contact between two sides.
    pub fn new(
        a: Side,
        b: Side,
        normal: DVec3,
        offset: f64,
        stiffness: f64,
        damping: f64,
        friction: f64,
    ) -> Self {
        Self {
            a,
            b,
            normal,
            offset,
            stiffness,
            damping,
            friction,
            restitution: 0.0,
            approach: 0.0,
            sliding: 0.0,
            gap_hat: 0.0,
            lambda: 0.0,
            anchor: b,
            radius_a: 0.0,
            radius_b: 0.0,
            arm_a: DVec3::ZERO,
            arm_b: DVec3::ZERO,
        }
    }

    /// Normal force at the start of the last step (N).
    pub fn normal_force(&self) -> f64 {
        self.lambda
    }

    /// Whether a side is a sphere, whose friction acts at carried points.
    pub fn rolls(&self) -> bool {
        self.radius_a > 0.0 || self.radius_b > 0.0
    }
}

/// World position of a side, and the lever arm from its node.
fn point(side: Side, states: &[MemberState<'_>]) -> (DVec3, DVec3) {
    match side {
        Side::Node {
            member,
            node,
            local,
        } => {
            let s = &states[member];
            let arm = s.orientation[node] * local;
            (s.position[node] + arm, arm)
        }
        Side::Fixed(p) => (p, DVec3::ZERO),
    }
}

/// The contacts of one group.
#[derive(Clone, Debug, Default)]
pub struct Contacts {
    /// The contacts.
    pub list: Vec<Contact>,
    /// For each contact of the step, its carried friction points (contacts with spheres;
    /// `friction.rs`). Kept beside the contacts: most contacts have none, and a larger
    /// `Contact` made every step slower.
    carried: Vec<Option<[Carried; 2]>>,
    beta: f64,
    smoothing: f64,
}

impl Contacts {
    /// Contacts with the given list.
    pub fn new(list: Vec<Contact>) -> Self {
        Self {
            list,
            carried: Vec::new(),
            beta: 1.0,
            smoothing: 1.0,
        }
    }

    /// The carried friction points of contact `i`, if it has them.
    fn carried(&self, i: usize) -> Option<[Carried; 2]> {
        self.carried.get(i).copied().flatten()
    }

    /// Gap and slip of contact `i` at `states`.
    fn gap_and_slip(&self, i: usize, states: &[MemberState<'_>]) -> (f64, DVec3, DVec3, DVec3) {
        let c = &self.list[i];
        let ((pa, ra), (pb, rb)) = (point(c.a, states), point(c.b, states));
        let gap = c.normal.dot(pa - pb) - c.offset;
        let travel = match self.carried(i) {
            Some([fa, fb]) => fa.position(states) - fb.position(states),
            None => pa - point(c.anchor, states).0,
        };
        let slip = travel - c.normal * c.normal.dot(travel);
        (gap, slip, ra, rb)
    }

    /// Where the anchor of contact `i` must move after the step so that its offset is at
    /// most [`STICK_DISTANCE`] (world point), or `None` if it stuck.
    pub fn moved_anchor(&self, i: usize, states: &[MemberState<'_>]) -> Option<DVec3> {
        let (_, slip, _, _) = self.gap_and_slip(i, states);
        let length = slip.length();
        let anchor = self.list[i].anchor;
        (length > STICK_DISTANCE)
            .then(|| point(anchor, states).0 + slip * (1.0 - STICK_DISTANCE / length))
    }

    /// The stick offset of contact `i` after the step, at most [`STICK_DISTANCE`] long: what
    /// a rolling contact carries into the next step (`anchor.rs`).
    pub fn end_slip(&self, i: usize, states: &[MemberState<'_>]) -> DVec3 {
        let (_, slip, _, _) = self.gap_and_slip(i, states);
        let length = slip.length();
        if length > STICK_DISTANCE {
            slip * (STICK_DISTANCE / length)
        } else {
            slip
        }
    }

    /// f₀ and f₁(x)/x of the friction smoothing.
    fn smooth(&self, x: f64) -> (f64, f64) {
        let e = self.smoothing;
        if x >= e {
            (x, 1.0 / x)
        } else {
            (
                -x * x * x / (3.0 * e * e) + x * x / e + e / 3.0,
                2.0 / e - x / (e * e),
            )
        }
    }
}

impl Coupling for Contacts {
    fn begin(&mut self, info: &StepInfo<'_>) {
        self.beta = info.beta;
        self.smoothing = STICK_DISTANCE;
        self.carried.clear();
        for i in 0..self.list.len() {
            let c = self.list[i];
            let ((pa, arm_a), (pb, arm_b)) = (point(c.a, info.start), point(c.b, info.start));
            let gap_start = c.normal.dot(pa - pb) - c.offset;
            let (ha, hb) = (point(c.a, info.hat).0, point(c.b, info.hat).0);
            // Contacts with spheres: friction at A's surface point and at the anchor, both
            // carried by their bodies' linearised motion (`friction.rs`).
            let carried = c.rolls().then(|| {
                let surface = pa - c.normal * c.radius_a;
                let anchor = point(c.anchor, info.start).0;
                [
                    Carried::new(c.a, surface, info.start),
                    Carried::new(c.anchor, anchor, info.start),
                ]
            });
            self.carried.push(carried);
            let c = &mut self.list[i];
            c.arm_a = arm_a;
            c.arm_b = arm_b;
            c.gap_hat = c.normal.dot(ha - hb) - c.offset;
            c.lambda = c.stiffness * (-gap_start).max(0.0);
        }
    }

    fn energy(&self, states: &[MemberState<'_>]) -> f64 {
        self.list
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let (gap, slip, _, _) = self.gap_and_slip(i, states);
                let (neg, neg_hat) = (gap.min(0.0), c.gap_hat.min(0.0));
                0.5 * c.stiffness * neg * neg
                    + c.damping / (2.0 * self.beta) * (neg - neg_hat) * (neg - neg_hat)
                    + c.friction * c.lambda * self.smooth(slip.length()).0
            })
            .sum()
    }

    fn elastic_energy(&self, states: &[MemberState<'_>]) -> f64 {
        self.list
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let neg = self.gap_and_slip(i, states).0.min(0.0);
                0.5 * c.stiffness * neg * neg
            })
            .sum()
    }

    fn add_gradient(&self, states: &[MemberState<'_>], gradient: &mut [Vec<DVec3>]) {
        for (i, c) in self.list.iter().enumerate() {
            let (gap, slip, ra, rb) = self.gap_and_slip(i, states);
            let (neg, neg_hat) = (gap.min(0.0), c.gap_hat.min(0.0));
            let d_gap = if gap < 0.0 {
                c.stiffness * neg + c.damping / self.beta * (neg - neg_hat)
            } else {
                0.0
            };
            let friction = slip * (c.friction * c.lambda * self.smooth(slip.length()).1);
            // The normal force acts at the contact points; friction at the material points
            // too (one force), or for spheres at the carried friction points.
            let at_points = match self.carried(i) {
                None => c.normal * d_gap + friction,
                Some([fa, fb]) => {
                    fa.add_gradient(states, friction, gradient);
                    fb.add_gradient(states, -friction, gradient);
                    c.normal * d_gap
                }
            };
            for (side, g, arm) in [(c.a, at_points, ra), (c.b, -at_points, rb)] {
                if let Side::Node { member, node, .. } = side {
                    let half = gradient[member].len() / 2;
                    gradient[member][node] += g;
                    gradient[member][half + node] += arm.cross(g);
                }
            }
        }
    }

    fn update_lag(&mut self, states: &[MemberState<'_>]) -> bool {
        let mut changed = false;
        for i in 0..self.list.len() {
            let gap = self.gap_and_slip(i, states).0;
            let c = &mut self.list[i];
            let force = c.stiffness * (-gap).max(0.0);
            // Only friction uses λ: contacts without friction never need a refresh.
            if c.friction > 0.0 && (force - c.lambda).abs() > 0.05 * force.max(c.lambda) {
                changed = true;
            }
            c.lambda = force;
        }
        changed
    }

    fn rigid_coupling(&self) -> Vec<NodeCoupling> {
        self.coupling_blocks()
    }

    fn node_stiffness(&self, member: usize) -> Vec<(usize, DMat3)> {
        self.node_blocks(member)
    }

    fn rigid_stiffness(&self, member: usize) -> [[f64; 6]; 6] {
        self.rigid_block(member)
    }
}
