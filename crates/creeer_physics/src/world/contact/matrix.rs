//! The Gauss-Newton stiffness of contacts: their blocks in the Newton matrices of the
//! structures and rigid bodies they join (see `mod.rs` for the energy terms).

use super::{Contact, Contacts, Side};
use crate::so3::skew;
use crate::structure::group::NodeCoupling;
use creeer_math::glam::{DMat3, DVec3};

impl Contacts {
    /// Gauss-Newton stiffness of a contact on its point (3×3): the penalty (and dashpot) along
    /// the normal while it touches or will touch within the step (closing over its gap: an
    /// impact, whose stiffness is the coupling that matters most), sticking friction across it.
    fn stiffness(&self, c: &Contact) -> DMat3 {
        let (normal, tangent) = self.stiffness_parts(c);
        normal + tangent
    }

    /// The normal and the tangential (friction) part of [`Self::stiffness`].
    fn stiffness_parts(&self, c: &Contact) -> (DMat3, DMat3) {
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
        (outer * normal, (DMat3::IDENTITY - outer) * tangent)
    }

    /// The stiffness blocks of a contact with the lever arms they act at: one block at the
    /// contact points, or (contacts with spheres) the normal part at the contact points and
    /// the friction part at the carried friction points.
    /// (No allocation: this runs for every contact at every matrix assembly.)
    fn arms_and_blocks(&self, i: usize) -> impl Iterator<Item = (DVec3, DVec3, DMat3)> {
        let c = &self.list[i];
        let blocks = match self.carried(i) {
            None => [Some((c.arm_a, c.arm_b, self.stiffness(c))), None],
            Some([fa, fb]) => {
                let (normal, tangent) = self.stiffness_parts(c);
                [
                    Some((c.arm_a, c.arm_b, normal)),
                    Some((fa.arm0(), fb.arm0(), tangent)),
                ]
            }
        };
        blocks.into_iter().flatten()
    }

    /// The cross blocks between the rigid bodies a contact joins.
    pub(super) fn coupling_blocks(&self) -> Vec<NodeCoupling> {
        let mut out = Vec::new();
        for (i, c) in self.list.iter().enumerate() {
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
            let mut m6 = [[0.0; 6]; 6];
            for (arm_a, arm_b, h) in self.arms_and_blocks(i) {
                let (sa, sb) = (skew(arm_a), skew(arm_b));
                let blocks = [[h, -(h * sb)], [sa * h, -(sa * h * sb)]];
                for (bi, row) in blocks.iter().enumerate() {
                    for (bj, block) in row.iter().enumerate() {
                        for r in 0..3 {
                            for k in 0..3 {
                                m6[3 * bi + r][3 * bj + k] -= block.col(k)[r];
                            }
                        }
                    }
                }
            }
            out.push(((ma, na), (mb, nb), m6));
        }
        out
    }

    /// The translation blocks of a structure's nodes.
    pub(super) fn node_blocks(&self, member: usize) -> Vec<(usize, DMat3)> {
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

    /// The 6×6 block of a rigid body.
    pub(super) fn rigid_block(&self, member: usize) -> [[f64; 6]; 6] {
        let mut h6 = [[0.0; 6]; 6];
        for (i, c) in self.list.iter().enumerate() {
            for (arm_a, arm_b, h) in self.arms_and_blocks(i) {
                for (side, arm) in [(c.a, arm_a), (c.b, arm_b)] {
                    let Side::Node {
                        member: m, node: 0, ..
                    } = side
                    else {
                        continue;
                    };
                    if m != member {
                        continue;
                    }
                    // δp = δx − [r]×·δθ, so the 6×6 block is Jᵀ·H·J with J = [I, −[r]×].
                    // The lever arm is the one at the start of the step.
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
        }
        h6
    }
}
