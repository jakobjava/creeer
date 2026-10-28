//! The joints' Gauss–Newton blocks for the step's matrices (`group.rs`): each scalar's weight
//! times its rows' outer product, into the rigid bodies' own 6×6 blocks, the blocks between
//! two bodies, and structures' nodes' translations.

use super::group::Joints;
use super::rows::Row;
use crate::structure::group::{MemberState, NodeCoupling};
use creeer_math::glam::{DMat3, DVec3};

/// Adds w·u·vᵀ. (Dense and branch-free, so it vectorises: where a row's entry is nought it
/// adds ±0 to an entry that is a sum of nonzero products from +0, never −0, which leaves it
/// as it was; the same bits as skipping the rows' zeros, as this did before.)
fn outer(m6: &mut [[f64; 6]; 6], u: &Row, v: &Row, w: f64) {
    for (row, &ur) in m6.iter_mut().zip(u) {
        let wu = w * ur;
        for (x, &vc) in row.iter_mut().zip(v) {
            *x += wu * vc;
        }
    }
}

fn translation(u: &Row, w: f64) -> DMat3 {
    let t = DVec3::new(u[0], u[1], u[2]);
    DMat3::from_cols(t * t.x, t * t.y, t * t.z) * w
}

impl Joints {
    /// The Gauss–Newton blocks at `states`.
    pub(crate) fn make_blocks(&mut self, states: &[MemberState<'_>]) {
        let timer = crate::profile::start();
        let members = (self.list.iter())
            .flat_map(|(_, j)| [Some(j.a.0), j.b.map(|b| b.0)])
            .chain(
                self.struts
                    .iter()
                    .flat_map(|g| [Some(g.strut.a.0), g.strut.b.map(|b| b.0)]),
            )
            .chain(self.grips.iter().map(|g| Some(g.grip.a.0)))
            .flatten()
            .max()
            .map_or(0, |m| m + 1);
        // (The last blocks' buffers again: four allocations a joint solve's iteration.)
        let mut rigid = std::mem::take(&mut self.rigid);
        rigid.clear();
        rigid.resize(members, [[0.0; 6]; 6]);
        let mut nodes = std::mem::take(&mut self.nodes);
        nodes.clear();
        // One cross block per joint between two bodies (merged by their nodes where used).
        let slots: Vec<Option<usize>> = (self.list.iter())
            .scan(0, |n, (_, j)| {
                Some(j.b.map(|_| {
                    *n += 1;
                    *n - 1
                }))
            })
            .collect();
        let mut cross: Vec<NodeCoupling> = std::mem::take(&mut self.cross);
        cross.clear();
        cross.extend((self.list.iter()).filter_map(|(_, j)| j.b.map(|b| (j.a, b, [[0.0; 6]; 6]))));
        let sides = &self.sides;
        self.terms(states, true, |j, s, _, w| {
            if w == 0.0 {
                return;
            }
            let joint = &self.list[j].1;
            for (side, row, rigid_side) in [
                (Some(joint.a), &s.a, sides[j][0].2),
                (joint.b, &s.b, sides[j][1].2),
            ] {
                let Some((m, n)) = side else { continue };
                if rigid_side {
                    outer(&mut rigid[m], row, row, w);
                } else {
                    nodes.push((m, n, translation(row, w)));
                }
            }
            if let Some(slot) = slots[j] {
                outer(&mut cross[slot].2, &s.a, &s.b, w);
            }
        });
        // The struts' alike (their cross blocks after the joints').
        self.strut_terms(states, |k, s, _, w| {
            if w == 0.0 {
                return;
            }
            let [(a, rigid_a), (b, rigid_b)] = self.strut_sides(k);
            for (side, row, rigid_side) in [(a, &s.a, rigid_a), (b, &s.b, rigid_b)] {
                let Some((m, n)) = side else { continue };
                if rigid_side {
                    outer(&mut rigid[m], row, row, w);
                } else {
                    nodes.push((m, n, translation(row, w)));
                }
            }
            if let (Some(a), Some(b)) = (a, b) {
                let mut m6 = [[0.0; 6]; 6];
                outer(&mut m6, &s.a, &s.b, w);
                cross.push((a, b, m6));
            }
        });
        // The grips' (on their one side).
        self.grip_terms(states, |k, s, _, w| {
            if w == 0.0 {
                return;
            }
            let ((m, n), rigid_side) = self.grip_side(k);
            if rigid_side {
                outer(&mut rigid[m], &s.a, &s.a, w);
            } else {
                nodes.push((m, n, translation(&s.a, w)));
            }
        });
        (self.rigid, self.cross, self.nodes) = (rigid, cross, nodes);
        timer.stop(crate::profile::Slot::JointTerms);
    }

    /// The translation blocks at the nodes of member `member`.
    pub(crate) fn node_blocks(&self, member: usize) -> impl Iterator<Item = (usize, DMat3)> + '_ {
        (self.nodes.iter())
            .filter(move |(m, _, _)| *m == member)
            .map(|&(_, n, b)| (n, b))
    }
}
