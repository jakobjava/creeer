//! Rigid bodies resting on structures, solved jointly with them (a block on a bridge,
//! cargo on a frame).
//!
//! Each structure has its own factorised matrix and each rigid body its 6×6 one, but the
//! contacts between them couple the two, and without that coupling the step had to find it
//! by iterating: a bridge on its piers converged in 7 iterations a step, with a 10 t block on
//! it in 15.5 (F5). They couple only through the translations of the structure's joints in
//! contact (E selects them): the joint matrix is
//!
//!   [A_s + E·D·Eᵀ, E·C; Cᵀ·Eᵀ, A_r],
//!
//! A_s the structure's factorised matrix and D the change of its contacts' stiffness since
//! it was factorised (it is refactorised only when that stiffness changes by half: against
//! the old stiffness alone, a block on a bridge gave a Schur matrix that was not positive
//! definite). Eliminating the structure leaves a small dense system on the joints in contact
//! and the bodies, with G = Eᵀ·A_s⁻¹·E (three solves with the structure's factorisation per
//! joint, once per step):
//!
//!   [I + G·D, G·C; Cᵀ, A_r]·[u; z_r] = [Eᵀ·A_s⁻¹·r_s; r_r],
//!   z_s = A_s⁻¹·r_s − A_s⁻¹·E·(D·u + C·z_r).
//!
//! Rigid bodies that also touch other rigid bodies stay in the rigid system
//! (`rigid_system.rs`); and past [`MOST`] joints in contact the solves cost more than the
//! iterations they save.

use super::group::{Coupling, Member};
use super::rigid_system::RigidSystem;
use creeer_math::glam::{DMat3, DVec3};

/// Most structure joints in contact with rigid bodies solved this way.
const MOST: usize = 48;

/// The rigid bodies of a group resting on its structures, solved jointly with them.
pub(super) struct Coupled {
    /// Group member index of each coupled rigid body.
    rigid: Vec<usize>,
    /// The joints in contact: structure member and node.
    joints: Vec<(usize, usize)>,
    /// β²·C by (joint, rigid body): the joint's translation rows, the body's columns.
    cross: Vec<(usize, usize, [[f64; 6]; 3])>,
    /// D by joint: β²·(the contact stiffness now − the one factorised).
    change: Vec<DMat3>,
    /// A_s⁻¹·E: per joint, three full vectors of its structure.
    columns: Vec<[Vec<DVec3>; 3]>,
    /// The dense system's LU factors (row-major) and pivots.
    lu: Vec<f64>,
    pivots: Vec<usize>,
}

impl Coupled {
    /// The joint solve of the group's rigid bodies that rest only on its structures, or
    /// `None` if there are none (or too many joints, or the system is singular).
    pub(super) fn new(
        members: &mut [Member<'_>],
        coupling: &dyn Coupling,
        rigid: &[Option<[[f64; 6]; 6]>],
        system: Option<&RigidSystem>,
    ) -> Option<Self> {
        let structure = |m: usize| rigid[m].is_none() && !members[m].0.elements.is_empty();
        let alone = |m: usize| rigid[m].is_some() && !system.is_some_and(|j| j.contains(m));
        let mut found: Vec<(usize, usize, usize, [[f64; 6]; 6])> = Vec::new();
        for ((ma, na), (mb, nb), m6) in coupling.rigid_coupling() {
            // (structure, joint, rigid body, block with the structure's rows)
            if structure(ma) && alone(mb) && nb == 0 {
                found.push((ma, na, mb, m6));
            } else if structure(mb) && alone(ma) && na == 0 {
                let t = std::array::from_fn(|i| std::array::from_fn(|j| m6[j][i]));
                found.push((mb, nb, ma, t));
            }
        }
        let mut bodies: Vec<usize> = found.iter().map(|b| b.2).collect();
        bodies.sort_unstable();
        bodies.dedup();
        let mut joints: Vec<(usize, usize)> = found.iter().map(|b| (b.0, b.1)).collect();
        joints.sort_unstable();
        joints.dedup();
        if bodies.is_empty() || joints.len() > MOST {
            return None;
        }
        let b2 = members[0].1.work.beta * members[0].1.work.beta;
        let free = |members: &[Member<'_>], m: usize, n: usize, k: usize| {
            let s = &members[m].0;
            if k < 3 {
                s.free_translation[n][k]
            } else {
                s.free_rotation[n][k - 3]
            }
        };
        // β²·C, merged by (joint, body).
        let mut cross: Vec<(usize, usize, [[f64; 6]; 3])> = Vec::new();
        for (s, node, r, block) in found {
            let (j, b) = (
                joints.binary_search(&(s, node)).expect("listed"),
                bodies.binary_search(&r).expect("listed"),
            );
            let scaled: [[f64; 6]; 3] = std::array::from_fn(|i| {
                std::array::from_fn(|k| {
                    block[i][k] * b2 * free(members, s, node, i) * free(members, r, 0, k)
                })
            });
            match cross.iter_mut().find(|c| c.0 == j && c.1 == b) {
                Some(c) => {
                    for (row, add) in c.2.iter_mut().zip(scaled) {
                        for (x, a) in row.iter_mut().zip(add) {
                            *x += a;
                        }
                    }
                }
                None => cross.push((j, b, scaled)),
            }
        }
        // D: the contacts' stiffness at each joint now, less the one factorised.
        let change: Vec<DMat3> = joints
            .iter()
            .map(|&(s, node)| {
                let now: DMat3 = coupling
                    .node_stiffness(s)
                    .iter()
                    .filter(|b| b.0 == node)
                    .map(|b| b.1)
                    .sum();
                let (st, v) = &members[s];
                let then: DMat3 = v
                    .contact_blocks
                    .iter()
                    .filter(|b| b.0 == node)
                    .map(|b| b.1)
                    .sum();
                // What the factorisation holds of the old stiffness: scaled by the β of the
                // solver's own scheme it was made with (Newmark's h/2), not this step's (BDF2's
                // 2h/3 for resting contacts, backward Euler's h for impacts), and applied turned
                // with the node (`precondition`). Taken as β_now²·(old) instead, D took away
                // more than the matrix ever had (78 % more under BDF2): the joint matrix was not
                // positive definite and its solve went uphill, which ended steps as converged
                // where they began: a frame froze 3 mm above its piers under a block (F6).
                let (turn, factored) = (v.turns[node], v.beta() * v.beta());
                let p = DMat3::from_diagonal(st.block_mask(node));
                p * (now * b2 - turn * then * turn.transpose() * factored) * p
            })
            .collect();
        // A_s⁻¹·E.
        let columns: Vec<[Vec<DVec3>; 3]> = joints
            .iter()
            .map(|&(s, node)| {
                let (st, v) = &mut members[s];
                std::array::from_fn(|k| {
                    let mut column = vec![DVec3::ZERO; 2 * st.node_count()];
                    column[node][k] = 1.0;
                    v.precondition(st, &mut column, None);
                    column
                })
            })
            .collect();
        let matrix = dense(&joints, &cross, &change, &columns, &bodies, rigid);
        let (lu, pivots) = factor(matrix, 3 * joints.len() + 6 * bodies.len())?;
        Some(Self {
            rigid: bodies,
            joints,
            cross,
            change,
            columns,
            lu,
            pivots,
        })
    }

    /// Whether group member `m` is solved here.
    pub(super) fn contains(&self, m: usize) -> bool {
        self.rigid.binary_search(&m).is_ok()
    }

    /// The coupled bodies' parts of `all` (member `m` from `offsets[m]`), to keep before the
    /// members are preconditioned.
    pub(super) fn residuals(&self, all: &[DVec3], offsets: &[usize]) -> Vec<[f64; 6]> {
        self.rigid
            .iter()
            .map(|&m| {
                let (t, r) = (all[offsets[m]], all[offsets[m] + 1]);
                [t.x, t.y, t.z, r.x, r.y, r.z]
            })
            .collect()
    }

    /// Turns `all`, with the structures already preconditioned (A_s⁻¹·r_s) and `residuals`
    /// the coupled bodies' parts as they were, into the joint solve.
    pub(super) fn solve(&self, all: &mut [DVec3], residuals: &[[f64; 6]], offsets: &[usize]) {
        let nj = 3 * self.joints.len();
        let mut rhs: Vec<f64> = self
            .joints
            .iter()
            .flat_map(|&(s, node)| {
                let y = all[offsets[s] + node];
                [y.x, y.y, y.z]
            })
            .collect();
        rhs.extend(residuals.iter().flatten());
        let x = substitute(&self.lu, &self.pivots, rhs);
        // w = D·u + C·z_r by joint; then z_s = A_s⁻¹·r_s − A_s⁻¹·E·w.
        let mut w: Vec<DVec3> = self
            .change
            .iter()
            .enumerate()
            .map(|(j, d)| *d * DVec3::new(x[3 * j], x[3 * j + 1], x[3 * j + 2]))
            .collect();
        for &(j, body, c) in &self.cross {
            let z = &x[nj + 6 * body..nj + 6 * body + 6];
            for q in 0..3 {
                w[j][q] += (0..6).map(|col| c[q][col] * z[col]).sum::<f64>();
            }
        }
        for (j, &(s, _)) in self.joints.iter().enumerate() {
            for (k, column) in self.columns[j].iter().enumerate() {
                let weight = w[j][k];
                if weight == 0.0 {
                    continue;
                }
                for (a, c) in all[offsets[s]..offsets[s] + column.len()]
                    .iter_mut()
                    .zip(column)
                {
                    *a -= *c * weight;
                }
            }
        }
        for (b, &m) in self.rigid.iter().enumerate() {
            let at = |k: usize| x[nj + 6 * b + k];
            all[offsets[m]] = DVec3::new(at(0), at(1), at(2));
            all[offsets[m] + 1] = DVec3::new(at(3), at(4), at(5));
        }
    }
}

/// The dense system [I + G·D, G·C; Cᵀ, A_r] (row-major), G = Eᵀ·A_s⁻¹·E read off the
/// columns (zero between joints of different structures).
fn dense(
    joints: &[(usize, usize)],
    cross: &[(usize, usize, [[f64; 6]; 3])],
    change: &[DMat3],
    columns: &[[Vec<DVec3>; 3]],
    bodies: &[usize],
    rigid: &[Option<[[f64; 6]; 6]>],
) -> Vec<f64> {
    let g = |a: usize, k: usize, b: usize, l: usize| -> f64 {
        if joints[a].0 == joints[b].0 {
            columns[b][l][joints[a].1][k]
        } else {
            0.0
        }
    };
    let nj = 3 * joints.len();
    let size = nj + 6 * bodies.len();
    let mut m = vec![0.0; size * size];
    for a in 0..joints.len() {
        for k in 0..3 {
            let row = 3 * a + k;
            m[row * size + row] += 1.0;
            for (b, d) in change.iter().enumerate() {
                for l in 0..3 {
                    let gd: f64 = (0..3).map(|q| g(a, k, b, q) * d.col(l)[q]).sum();
                    m[row * size + 3 * b + l] += gd;
                }
            }
            for &(j, body, c) in cross {
                for col in 0..6 {
                    let gc: f64 = (0..3).map(|q| g(a, k, j, q) * c[q][col]).sum();
                    m[row * size + nj + 6 * body + col] += gc;
                }
            }
        }
    }
    for &(j, body, c) in cross {
        for col in 0..6 {
            for (q, row) in c.iter().enumerate() {
                m[(nj + 6 * body + col) * size + 3 * j + q] += row[col];
            }
        }
    }
    for (b, &r) in bodies.iter().enumerate() {
        let own = rigid[r].expect("rigid");
        for (i, row) in own.iter().enumerate() {
            for (k, v) in row.iter().enumerate() {
                m[(nj + 6 * b + i) * size + nj + 6 * b + k] += v;
            }
        }
    }
    m
}

/// LU factors with partial pivoting of the `size` × `size` matrix `a` (row-major), or
/// `None` if singular.
fn factor(mut a: Vec<f64>, size: usize) -> Option<(Vec<f64>, Vec<usize>)> {
    let mut pivots: Vec<usize> = (0..size).collect();
    for j in 0..size {
        let p =
            (j..size).max_by(|&x, &y| a[x * size + j].abs().total_cmp(&a[y * size + j].abs()))?;
        if a[p * size + j].abs() <= 0.0 {
            return None;
        }
        if p != j {
            for k in 0..size {
                a.swap(p * size + k, j * size + k);
            }
            pivots.swap(p, j);
        }
        let d = a[j * size + j];
        for i in j + 1..size {
            let f = a[i * size + j] / d;
            a[i * size + j] = f;
            if f != 0.0 {
                for k in j + 1..size {
                    a[i * size + k] -= f * a[j * size + k];
                }
            }
        }
    }
    Some((a, pivots))
}

/// Solves with the factors of [`factor`].
fn substitute(lu: &[f64], pivots: &[usize], b: Vec<f64>) -> Vec<f64> {
    let size = pivots.len();
    let mut x: Vec<f64> = pivots.iter().map(|&p| b[p]).collect();
    for i in 0..size {
        for k in 0..i {
            x[i] -= lu[i * size + k] * x[k];
        }
    }
    for i in (0..size).rev() {
        for k in i + 1..size {
            x[i] -= lu[i * size + k] * x[k];
        }
        x[i] /= lu[i * size + i];
    }
    x
}
