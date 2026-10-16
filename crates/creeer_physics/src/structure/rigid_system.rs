//! A joint preconditioner for the rigid bodies of a group.
//!
//! Each rigid body on its own has an exact 6×6 Newton matrix (its mass, inertia and the
//! stiffness of its contacts). In a pile, though, the bodies push on each other: with
//! separate matrices a push travels one contact per iteration, and a ball hitting a stack
//! of six boxes needed 500–1800 iterations. Here all rigid bodies of the group share one
//! sparse matrix, every contact between two of them coupling their blocks
//! (−β²·J_aᵀ·H·J_b), factorised with the deterministic sparse Cholesky: the whole pile then
//! answers to a push in one solve.

use super::group::{Coupling, Member};
use crate::sparse::{Cholesky, SymbolicCholesky};
use creeer_math::glam::{DMat3, DVec3};

/// The 3×3 block (rows `bi`, columns `bj`; 0 translation, 1 rotation) of a 6×6 matrix.
fn block(m: &[[f64; 6]; 6], bi: usize, bj: usize) -> DMat3 {
    DMat3::from_cols_array_2d(&std::array::from_fn(|col| {
        std::array::from_fn(|row| m[3 * bi + row][3 * bj + col])
    }))
}

/// The rigid bodies of a group, solved together.
pub(super) struct RigidSystem {
    /// Group member index of each rigid body in the system.
    members: Vec<usize>,
    chol: Cholesky,
}

impl RigidSystem {
    /// Builds and factorises the joint matrix from each rigid body's own 6×6 matrix
    /// (`rigid[m]`) and the coupling's cross terms. `None` when fewer than two rigid bodies
    /// touch each other (the separate matrices are then exact).
    pub(super) fn new(
        members: &[Member<'_>],
        coupling: &dyn Coupling,
        rigid: &[Option<[[f64; 6]; 6]>],
    ) -> Option<Self> {
        let mut index = vec![usize::MAX; members.len()];
        let mut list = Vec::new();
        for (m, matrix) in rigid.iter().enumerate() {
            if matrix.is_some() {
                index[m] = list.len();
                list.push(m);
            }
        }
        let crosses: Vec<(usize, usize, [[f64; 6]; 6])> = coupling
            .rigid_coupling()
            .into_iter()
            .filter(|&((a, na), (b, nb), _)| {
                a != b && na == 0 && nb == 0 && index[a] != usize::MAX && index[b] != usize::MAX
            })
            .map(|((a, _), (b, _), m)| (index[a], index[b], m))
            .collect();
        if crosses.is_empty() {
            return None;
        }
        let b2 = members[0].1.work.beta * members[0].1.work.beta;
        let mut edges = Vec::with_capacity(list.len() + 4 * crosses.len());
        let mut off = Vec::with_capacity(edges.capacity());
        let mut diag = Vec::with_capacity(2 * list.len());
        let mut positions = Vec::with_capacity(2 * list.len());
        // Fixed axes (rows and columns of identity in the own matrices) must not couple.
        let masks: Vec<[DMat3; 2]> = list
            .iter()
            .map(|&m| {
                let s = &members[m].0;
                [
                    DMat3::from_diagonal(s.free_translation[0]),
                    DMat3::from_diagonal(s.free_rotation[0]),
                ]
            })
            .collect();
        for (r, &m) in list.iter().enumerate() {
            let own = rigid[m].as_ref().expect("rigid");
            diag.push(block(own, 0, 0));
            diag.push(block(own, 1, 1));
            edges.push((2 * r, 2 * r + 1));
            off.push(block(own, 0, 1));
            let p = members[m].0.position[0];
            positions.extend([p, p]);
        }
        for (a, b, cross) in &crosses {
            for (bi, bj) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                edges.push((2 * a + bi, 2 * b + bj));
                off.push(masks[*a][bi] * block(cross, bi, bj) * masks[*b][bj] * b2);
            }
        }
        let mut chol = Cholesky::new(SymbolicCholesky::new(&positions, &edges));
        chol.factor(&diag, &off).ok()?;
        Some(Self {
            members: list,
            chol,
        })
    }

    /// Whether group member `m` is solved here.
    pub(super) fn contains(&self, m: usize) -> bool {
        self.members.binary_search(&m).is_ok()
    }

    /// Applies the joint inverse to the rigid bodies' parts of `all` (member `m` starts at
    /// `offsets[m]`).
    pub(super) fn solve(&mut self, all: &mut [DVec3], offsets: &[usize]) {
        let mut rhs: Vec<DVec3> = self
            .members
            .iter()
            .flat_map(|&m| [all[offsets[m]], all[offsets[m] + 1]])
            .collect();
        self.chol.solve(&mut rhs);
        for (r, &m) in self.members.iter().enumerate() {
            all[offsets[m]] = rhs[2 * r];
            all[offsets[m] + 1] = rhs[2 * r + 1];
        }
    }
}
