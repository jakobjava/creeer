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

use crate::so3::skew;
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
            arm_a: DVec3::ZERO,
            arm_b: DVec3::ZERO,
        }
    }

    /// Normal force at the start of the last step (N).
    pub fn normal_force(&self) -> f64 {
        self.lambda
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
    beta: f64,
    smoothing: f64,
}

impl Contacts {
    /// Contacts with the given list.
    pub fn new(list: Vec<Contact>) -> Self {
        Self {
            list,
            beta: 1.0,
            smoothing: 1.0,
        }
    }

    /// Gap and slip of contact `c` at `states`.
    fn gap_and_slip(&self, c: &Contact, states: &[MemberState<'_>]) -> (f64, DVec3, DVec3, DVec3) {
        let ((pa, ra), (pb, rb)) = (point(c.a, states), point(c.b, states));
        let gap = c.normal.dot(pa - pb) - c.offset;
        let travel = pa - point(c.anchor, states).0;
        let slip = travel - c.normal * c.normal.dot(travel);
        (gap, slip, ra, rb)
    }

    /// Where the anchor of contact `c` must move after the step so that its offset is at
    /// most [`STICK_DISTANCE`] (world point), or `None` if it stuck.
    pub fn moved_anchor(&self, c: &Contact, states: &[MemberState<'_>]) -> Option<DVec3> {
        let (_, slip, _, _) = self.gap_and_slip(c, states);
        let length = slip.length();
        (length > STICK_DISTANCE)
            .then(|| point(c.anchor, states).0 + slip * (1.0 - STICK_DISTANCE / length))
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

impl Contacts {
    /// Gauss-Newton stiffness of a contact on its point (3×3): the penalty (and dashpot) along
    /// the normal while it touches or will touch within the step (closing over its gap: an
    /// impact, whose stiffness is the coupling that matters most), sticking friction across it.
    fn stiffness(&self, c: &Contact) -> DMat3 {
        let normal = if c.gap_hat + c.approach.min(0.0) * self.beta < 0.0 {
            c.stiffness + c.damping / self.beta
        } else {
            0.0
        };
        // Sticking: the stiff spring of the anchor. Sliding: Coulomb's force μ·λ is constant,
        // its secant over the slip of the step is the stiffness the step sees.
        let slip = c.sliding * self.beta;
        let tangent = if slip > self.smoothing {
            c.friction * c.lambda / slip
        } else {
            c.friction * c.lambda * 2.0 / self.smoothing
        };
        let n = c.normal;
        let outer = DMat3::from_cols(n * n.x, n * n.y, n * n.z);
        outer * normal + (DMat3::IDENTITY - outer) * tangent
    }
}

impl Coupling for Contacts {
    fn begin(&mut self, info: &StepInfo<'_>) {
        self.beta = info.beta;
        self.smoothing = STICK_DISTANCE;
        for i in 0..self.list.len() {
            let c = self.list[i];
            let ((pa, arm_a), (pb, arm_b)) = (point(c.a, info.start), point(c.b, info.start));
            let gap_start = c.normal.dot(pa - pb) - c.offset;
            let (ha, hb) = (point(c.a, info.hat).0, point(c.b, info.hat).0);
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
            .map(|c| {
                let (gap, slip, _, _) = self.gap_and_slip(c, states);
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
            .map(|c| {
                let neg = self.gap_and_slip(c, states).0.min(0.0);
                0.5 * c.stiffness * neg * neg
            })
            .sum()
    }

    fn add_gradient(&self, states: &[MemberState<'_>], gradient: &mut [Vec<DVec3>]) {
        for c in &self.list {
            let (gap, slip, ra, rb) = self.gap_and_slip(c, states);
            let (neg, neg_hat) = (gap.min(0.0), c.gap_hat.min(0.0));
            let d_gap = if gap < 0.0 {
                c.stiffness * neg + c.damping / self.beta * (neg - neg_hat)
            } else {
                0.0
            };
            let g =
                c.normal * d_gap + slip * (c.friction * c.lambda * self.smooth(slip.length()).1);
            for (side, g, arm) in [(c.a, g, ra), (c.b, -g, rb)] {
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
            let gap = self.gap_and_slip(&self.list[i], states).0;
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
        let mut out = Vec::new();
        for c in &self.list {
            let (
                Side::Node {
                    member: ma,
                    node: na,
                    ..
                },
                Side::Node {
                    member: mb,
                    node: nb,
                    ..
                },
            ) = (c.a, c.b)
            else {
                continue;
            };
            // The gap moves with J_a·δq_a − J_b·δq_b (J = [I, −[r]×]): the cross term is
            // −J_aᵀ·H·J_b.
            let h = self.stiffness(c);
            let (sa, sb) = (skew(c.arm_a), skew(c.arm_b));
            let blocks = [[h, -(h * sb)], [sa * h, -(sa * h * sb)]];
            let mut m6 = [[0.0; 6]; 6];
            for (bi, row) in blocks.iter().enumerate() {
                for (bj, block) in row.iter().enumerate() {
                    for r in 0..3 {
                        for k in 0..3 {
                            m6[3 * bi + r][3 * bj + k] = -block.col(k)[r];
                        }
                    }
                }
            }
            out.push(((ma, na), (mb, nb), m6));
        }
        out
    }

    fn node_stiffness(&self, member: usize) -> Vec<(usize, DMat3)> {
        let mut blocks = Vec::new();
        for c in &self.list {
            for side in [c.a, c.b] {
                if let Side::Node {
                    member: m, node, ..
                } = side
                    && m == member
                {
                    blocks.push((node, self.stiffness(c)));
                }
            }
        }
        blocks
    }

    fn rigid_stiffness(&self, member: usize) -> [[f64; 6]; 6] {
        let mut h6 = [[0.0; 6]; 6];
        for c in &self.list {
            let h = self.stiffness(c);
            for (side, arm) in [(c.a, c.arm_a), (c.b, c.arm_b)] {
                let Side::Node {
                    member: m, node: 0, ..
                } = side
                else {
                    continue;
                };
                if m != member {
                    continue;
                }
                // δp = δx − [r]×·δθ, so the 6×6 block is Jᵀ·H·J with J = [I, −[r]×]. The
                // lever arm is the one at the start of the step.
                let s = skew(arm);
                let blocks = [[h, -(h * s)], [s * h, -(s * h * s)]];
                for (bi, row) in blocks.iter().enumerate() {
                    for (bj, block) in row.iter().enumerate() {
                        for r in 0..3 {
                            for k in 0..3 {
                                h6[3 * bi + r][3 * bj + k] += block.col(k)[r];
                            }
                        }
                    }
                }
            }
        }
        h6
    }
}
