//! Placing a member's new joint, and the joints at its ends, so that a split changes no
//! joint's motion (`buckling.rs`).
//!
//! Placed on the member's cubic alone, a new joint with a kink, in a member already carrying
//! its load, was out of balance by twice the force times the kink's slope (4 kN at the foot
//! of a 120 m tower), and the member's ends by the moments the kink adds; Newmark's
//! trapezoidal rule does not damp such stiff modes but rings on, and the tower took 30
//! iterations a step for seconds (F5). So the joints are moved until the members' forces on
//! each are what its motion needs: on an end, what they were before the split, adjusted for
//! the mass the end gave to the new joint; on the new joint, its mass times its
//! acceleration less its weight. The members around them hold the rest of the structure
//! where it is.

use super::element::MEASURES;
use super::{NodeId, Structure};
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// The gradient of the members' energy at `nodes` (minus the forces and torques they exert
/// on each), from every member at any of them.
pub(super) fn gradients(s: &Structure, nodes: &[NodeId]) -> Vec<[DVec3; 2]> {
    let mut out = vec![[DVec3::ZERO; 2]; nodes.len()];
    each_member(s, nodes, |el_slots, g, force, _| {
        for &(k, slot) in el_slots {
            for i in 0..MEASURES {
                out[k][0] += g[i][slot] * force[i];
                out[k][1] += g[i][slot + 1] * force[i];
            }
        }
    });
    out
}

/// Moves the free ones among `nodes` (each with its free translation and rotation axes)
/// until the gradient of the members' energy at each equals its `target` (Gauss–Newton);
/// the members' other ends stay. Returns how far each moved.
///
/// A settle corrects by millimetres. In a structure already crushed out of shape the targets
/// may be out of reach, and Gauss–Newton alone ran away (a frame's joint flung 10⁶ m, F5):
/// so each step moves a joint by a tenth of the longest member (and turns it by 0.1 rad) at
/// most, and the joints end where the residual was least (where they started, if it never
/// fell).
pub(super) fn settle(s: &mut Structure, nodes: &[NodeId], target: &[[DVec3; 2]]) -> Vec<DVec3> {
    let n = 6 * nodes.len();
    let start: Vec<DVec3> = nodes.iter().map(|&i| s.position[i]).collect();
    let state = |s: &Structure| -> Vec<(DVec3, DQuat)> {
        nodes
            .iter()
            .map(|&i| (s.position[i], s.orientation[i]))
            .collect()
    };
    let mut best = (f64::INFINITY, state(s));
    let scale = s.elements.iter().map(|e| e.rest_length).fold(0.0, f64::max);
    let free: Vec<f64> = nodes
        .iter()
        .flat_map(|&i| {
            let (t, r) = (s.free_translation[i], s.free_rotation[i]);
            [t.x, t.y, t.z, r.x, r.y, r.z]
        })
        .collect();
    let mut converged = false;
    for iteration in 0..=50 {
        let mut rhs = vec![0.0; n];
        let mut matrix = vec![0.0; n * n];
        each_member(s, nodes, |slots, g, force, stiffness| {
            for &(k, slot) in slots {
                for d in 0..2 {
                    let v: DVec3 = (0..MEASURES).map(|i| g[i][slot + d] * force[i]).sum();
                    for c in 0..3 {
                        rhs[6 * k + 3 * d + c] -= v[c];
                    }
                }
            }
            for &(k, p) in slots {
                for &(l, q) in slots {
                    for i in 0..MEASURES {
                        for j in 0..MEASURES {
                            let kij = stiffness(i, j);
                            if kij == 0.0 {
                                continue;
                            }
                            for (dp, dq) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                                let (u, w) = (g[i][p + dp], g[j][q + dq]);
                                for a in 0..3 {
                                    for b in 0..3 {
                                        let (row, col) = (6 * k + 3 * dp + a, 6 * l + 3 * dq + b);
                                        matrix[row * n + col] += u[a] * w[b] * kij;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        for (k, t) in target.iter().enumerate() {
            for d in 0..2 {
                for c in 0..3 {
                    rhs[6 * k + 3 * d + c] += t[d][c];
                }
            }
        }
        // A little stiffness on every axis, for the motions nothing resists (a member pinned at
        // both ends turning about its axis): the forces have no part along them.
        let floor = 1e-9 * (0..n).map(|d| matrix[d * n + d]).fold(0.0, f64::max);
        for d in 0..n {
            matrix[d * n + d] += floor;
        }
        // Held axes stay where they are.
        for (dof, &f) in free.iter().enumerate() {
            if f == 0.0 {
                for j in 0..n {
                    matrix[dof * n + j] = 0.0;
                    matrix[j * n + dof] = 0.0;
                }
                matrix[dof * n + dof] = 1.0;
                rhs[dof] = 0.0;
            }
        }
        // The residual here (torques over the longest member: forces too), kept if least.
        let residual: f64 = (0..n)
            .map(|d| rhs[d] / if d % 6 < 3 { 1.0 } else { scale })
            .map(|r| r * r)
            .sum();
        if residual < best.0 {
            best = (residual, state(s));
        }
        if iteration == 50 || converged {
            break;
        }
        let Some(step) = solve(&mut matrix, &mut rhs, n) else {
            break;
        };
        let parts: Vec<(DVec3, DVec3)> = (0..nodes.len())
            .map(|k| {
                let shift = DVec3::new(step[6 * k], step[6 * k + 1], step[6 * k + 2]);
                let turn = DVec3::new(step[6 * k + 3], step[6 * k + 4], step[6 * k + 5]);
                (shift, turn)
            })
            .collect();
        if parts
            .iter()
            .any(|(shift, turn)| !(shift.is_finite() && turn.is_finite()))
        {
            break;
        }
        // At most a tenth of the longest member, and 0.1 rad, a step.
        let size = parts
            .iter()
            .map(|(shift, turn)| (shift.length() / (0.1 * scale)).max(turn.length() / 0.1))
            .fold(0.0, f64::max);
        let cut = if size > 1.0 { 1.0 / size } else { 1.0 };
        let mut largest: f64 = 0.0;
        for (&i, (shift, turn)) in nodes.iter().zip(parts) {
            s.position[i] += shift * cut;
            s.orientation[i] = (DQuat::from_scaled_axis(turn * cut) * s.orientation[i]).normalize();
            largest = largest
                .max(shift.length() * cut / scale)
                .max(turn.length() * cut);
        }
        converged = largest <= 1e-12;
    }
    for (&i, (p, q)) in nodes.iter().zip(best.1) {
        s.position[i] = p;
        s.orientation[i] = q;
    }
    nodes
        .iter()
        .zip(start)
        .map(|(&i, p)| s.position[i] - p)
        .collect()
}

/// Calls `f` for every unbroken member at any of `nodes`: with the slots (index into
/// `nodes`, and the member's own slot of that node's position, 0 or 2), the measures'
/// gradients, the member's forces, and its stiffness.
fn each_member(
    s: &Structure,
    nodes: &[NodeId],
    mut f: impl FnMut(
        &[(usize, usize)],
        &[[DVec3; 4]; MEASURES],
        &[f64; MEASURES],
        &dyn Fn(usize, usize) -> f64,
    ),
) {
    let turn = |i: NodeId| DMat3::from_quat(s.orientation[i]);
    for el in s.elements.iter().filter(|e| !e.broken) {
        let mut slots = Vec::with_capacity(2);
        for (k, &node) in nodes.iter().enumerate() {
            if el.a == node {
                slots.push((k, 0));
            }
            if el.b == node {
                slots.push((k, 2));
            }
        }
        if slots.is_empty() {
            continue;
        }
        let (measures, g) =
            el.kinematics(s.position[el.a], turn(el.a), s.position[el.b], turn(el.b));
        let force = el.stiffness.apply(&measures);
        f(&slots, &g, &force, &|i, j| el.stiffness.entry(i, j));
    }
}

/// Solves the n×n system in `matrix` (row-major) for `rhs` by Gaussian elimination with
/// partial pivoting; none if it is singular.
fn solve(matrix: &mut [f64], rhs: &mut [f64], n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let pivot = (col..n).max_by(|&a, &b| {
            matrix[a * n + col]
                .abs()
                .total_cmp(&matrix[b * n + col].abs())
        })?;
        if matrix[pivot * n + col].abs() < 1e-300 {
            return None;
        }
        if pivot != col {
            for j in 0..n {
                matrix.swap(pivot * n + j, col * n + j);
            }
            rhs.swap(pivot, col);
        }
        for row in col + 1..n {
            let factor = matrix[row * n + col] / matrix[col * n + col];
            if factor == 0.0 {
                continue;
            }
            for j in col..n {
                matrix[row * n + j] -= factor * matrix[col * n + j];
            }
            rhs[row] -= factor * rhs[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let sum: f64 = (row + 1..n).map(|j| matrix[row * n + j] * x[j]).sum();
        x[row] = (rhs[row] - sum) / matrix[row * n + row];
    }
    Some(x)
}
