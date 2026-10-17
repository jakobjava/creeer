//! The Gauss-Newton stiffness of contacts: their blocks in the Newton matrices of the
//! structures and rigid bodies they join (see `mod.rs` for the energy terms).

use super::{Contact, Contacts, Law, Side};
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
        // A patch's corners are coupled: their stiffness comes as the patch's blocks.
        let own = if matches!(c.law, Law::Patch { .. }) {
            0.0
        } else {
            c.stiffness
        };
        let normal = if self.touching(c) {
            own + c.damping / self.beta
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

    /// The cross blocks between the rigid bodies the contacts join: one per pair of nodes
    /// in a row, so a patch couples its two bodies through one block (its corners and their
    /// pairs), not twenty (the joint matrix's analysis grew with every block, F5).
    pub(super) fn coupling_blocks(&self) -> Vec<NodeCoupling> {
        let mut out: Vec<NodeCoupling> = Vec::new();
        // The gap moves with J_a·δq_a − J_b·δq_b (J = [I, −[r]×]): the cross term is
        // −J_aᵀ·H·J_b.
        for (i, c) in self.list.iter().enumerate() {
            for (arm_a, arm_b, h) in self.arms_and_blocks(i) {
                if let Some(m6) = merged(&mut out, c.a, c.b) {
                    add_block(m6, arm_a, arm_b, h, -1.0);
                }
            }
            // A patch couples every corner on A with every corner's point on B.
            if let Law::Patch { patch, vertex: 3 } = c.law
                && let Some(stiffness) = &self.pairs[patch]
            {
                let first = self.patches[patch].first;
                let (on_a, on_b) = (self.levers(first, true), self.levers(first, false));
                let corners = &self.list[first..first + 4];
                let one_b = corners.iter().all(|c| node(c.b) == node(corners[0].b));
                for (k, row) in stiffness.iter().enumerate() {
                    if one_b {
                        // All on one node of B: Σₗ K_kl·gₗ first, one product per corner.
                        let mut u = [0.0; 6];
                        for (l, &kl) in row.iter().enumerate() {
                            for (ui, gi) in u.iter_mut().zip(on_b[l]) {
                                *ui += kl * gi;
                            }
                        }
                        if let Some(m6) = merged(&mut out, corners[k].a, corners[0].b) {
                            add_outer(m6, on_a[k], u, -1.0);
                        }
                        continue;
                    }
                    for (l, &kl) in row.iter().enumerate() {
                        if let Some(m6) = merged(&mut out, corners[k].a, corners[l].b) {
                            add_outer(m6, on_a[k], on_b[l], -kl);
                        }
                    }
                }
            }
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

    /// The 6×6 blocks of the rigid bodies (by member; zero for members without contacts),
    /// all in one pass over the contacts (made when the step begins).
    pub(super) fn rigid_blocks(&self) -> Vec<[[f64; 6]; 6]> {
        let members = self
            .list
            .iter()
            .flat_map(|c| [on_rigid(c.a), on_rigid(c.b)])
            .flatten()
            .max()
            .map_or(0, |m| m + 1);
        let mut h6 = vec![[[0.0; 6]; 6]; members];
        for (i, c) in self.list.iter().enumerate() {
            for (arm_a, arm_b, h) in self.arms_and_blocks(i) {
                for (side, arm) in [(c.a, arm_a), (c.b, arm_b)] {
                    // δp = δx − [r]×·δθ, so the 6×6 block is Jᵀ·H·J with J = [I, −[r]×].
                    // The lever arm is the one at the start of the step.
                    if let Some(m) = on_rigid(side) {
                        add_block(&mut h6[m], arm, arm, h, 1.0);
                    }
                }
            }
        }
        for (p, stiffness) in self.patches.iter().zip(&self.pairs) {
            let Some(stiffness) = stiffness else { continue };
            for on_a in [true, false] {
                let levers = self.levers(p.first, on_a);
                let side = |k: usize| {
                    let c = &self.list[p.first + k];
                    on_rigid(if on_a { c.a } else { c.b })
                };
                // Σₖₗ K_kl·gₖ·gₗᵀ as Σₖ gₖ·(Σₗ K_kl·gₗ)ᵀ, over the corners on the same body.
                for (k, row) in stiffness.iter().enumerate() {
                    let Some(m) = side(k) else { continue };
                    let mut u = [0.0; 6];
                    for (l, &kl) in row.iter().enumerate() {
                        if side(l) == Some(m) {
                            for (ui, gi) in u.iter_mut().zip(levers[l]) {
                                *ui += kl * gi;
                            }
                        }
                    }
                    add_outer(&mut h6[m], levers[k], u, 1.0);
                }
            }
        }
        h6
    }

    /// Jᵀ·n of a patch's corners on side A (or B): how each corner's gap moves with its
    /// body, (n, r × n). A patch's stiffness n_k·n_lᵀ·K_kl between corners k and l then adds
    /// K_kl·(Jₖᵀ·nₖ)·(Jₗᵀ·nₗ)ᵀ to the matrix: rank one per pair, a sixteenth of the work of
    /// the 3×3 products (measured in F5).
    fn levers(&self, first: usize, on_a: bool) -> [[f64; 6]; 4] {
        std::array::from_fn(|k| {
            let c = &self.list[first + k];
            let (n, arm) = (c.normal, if on_a { c.arm_a } else { c.arm_b });
            let turn = arm.cross(n);
            [n.x, n.y, n.z, turn.x, turn.y, turn.z]
        })
    }
}

impl Contacts {
    /// Whether a contact counts in the step's matrix (set when the step begins).
    fn touching(&self, c: &Contact) -> bool {
        c.touching
    }

    /// Each patch's stiffness between its corners where it touches (`None` where not): K_kl
    /// for corners k and l, the integrated pressure's (`law.rs`), acting along the corners'
    /// normals. Made once when the step begins, for every matrix of the step.
    pub(super) fn patch_pairs(&self) -> Vec<Option<[[f64; 4]; 4]>> {
        self.patches
            .iter()
            .map(|p| {
                (0..4)
                    .any(|k| self.touching(&self.list[p.first + k]))
                    .then(|| std::array::from_fn(|k| std::array::from_fn(|l| p.coupling(k, l))))
            })
            .collect()
    }
}

/// Adds sign·J_kᵀ·H·J_l to a 6×6 block, J = [I, −[r]×] at lever arms `arm_k` and `arm_l`.
fn add_block(m6: &mut [[f64; 6]; 6], arm_k: DVec3, arm_l: DVec3, h: DMat3, sign: f64) {
    let (sk, sl) = (skew(arm_k), skew(arm_l));
    let blocks = [[h, -(h * sl)], [sk * h, -(sk * h * sl)]];
    for (bi, row) in blocks.iter().enumerate() {
        for (bj, block) in row.iter().enumerate() {
            for r in 0..3 {
                for k in 0..3 {
                    m6[3 * bi + r][3 * bj + k] += sign * block.col(k)[r];
                }
            }
        }
    }
}

/// The node of a side (member, node), or `None` for fixed geometry.
fn node(side: Side) -> Option<(usize, usize)> {
    match side {
        Side::Node { member, node, .. } => Some((member, node)),
        Side::Fixed(_) => None,
    }
}

/// The member of a rigid body's side (its one node), if it is one.
fn on_rigid(side: Side) -> Option<usize> {
    match side {
        Side::Node {
            member, node: 0, ..
        } => Some(member),
        _ => None,
    }
}

/// The cross block between the nodes of sides `a` and `b`: the last one if it joins the
/// same nodes, else a new one; `None` when either side is fixed.
fn merged(out: &mut Vec<NodeCoupling>, a: Side, b: Side) -> Option<&mut [[f64; 6]; 6]> {
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
    ) = (a, b)
    else {
        return None;
    };
    let key = ((ma, na), (mb, nb));
    if out.last().is_none_or(|last| (last.0, last.1) != key) {
        out.push((key.0, key.1, [[0.0; 6]; 6]));
    }
    out.last_mut().map(|last| &mut last.2)
}

/// Adds s·u·vᵀ to a 6×6 block.
fn add_outer(m6: &mut [[f64; 6]; 6], u: [f64; 6], v: [f64; 6], s: f64) {
    for (row, ui) in m6.iter_mut().zip(u) {
        for (x, vj) in row.iter_mut().zip(v) {
            *x += s * ui * vj;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A patch's pair stiffness n_k·n_lᵀ·K_kl through the corners' lever arms is the rank-one
    /// K_kl·(Jₖᵀ·nₖ)·(Jₗᵀ·nₗ)ᵀ: the same blocks as the full 3×3 products, normals and arms
    /// all different.
    #[test]
    fn rank_one_pairs_are_the_full_products() {
        let arms = [
            DVec3::new(0.5, -0.5, 0.5),
            DVec3::new(-0.4, -0.5, 0.6),
            DVec3::new(-0.5, -0.45, -0.5),
            DVec3::new(0.3, -0.5, -0.7),
        ];
        let normals = [
            DVec3::Y,
            DVec3::new(0.1, 1.0, 0.0).normalize(),
            DVec3::new(0.0, 1.0, -0.2).normalize(),
            DVec3::new(-0.1, 1.0, 0.1).normalize(),
        ];
        let stiffness = |k: usize, l: usize| 1e7 * (1.0 + (k + l) as f64) / (1.0 + (k * l) as f64);
        let (mut full, mut rank_one) = ([[0.0; 6]; 6], [[0.0; 6]; 6]);
        let lever = |k: usize| {
            let turn = arms[k].cross(normals[k]);
            let n = normals[k];
            [n.x, n.y, n.z, turn.x, turn.y, turn.z]
        };
        for k in 0..4 {
            for l in 0..4 {
                let (nk, nl) = (normals[k], normals[l]);
                let h = DMat3::from_cols(nk * nl.x, nk * nl.y, nk * nl.z) * stiffness(k, l);
                add_block(&mut full, arms[k], arms[l], h, 1.0);
                add_outer(&mut rank_one, lever(k), lever(l), stiffness(k, l));
            }
        }
        for (a, b) in full.iter().flatten().zip(rank_one.iter().flatten()) {
            assert!((a - b).abs() <= 1e-12 * 1e8, "{full:?}\n{rank_one:?}");
        }
    }
}
