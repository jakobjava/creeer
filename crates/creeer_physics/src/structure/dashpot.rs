//! Dashpots: loads that depend on a node's motion (air on a wing, water on a hull), as
//! whatever exerts them linearised them at the step's start. Each step the load and torque it
//! wrote are the force at the motion it saw, (v₀, ω₀); the dashpot is their slope, so the
//! step feels F₀ − D·((v, ω) − (v₀, ω₀)) at its new motion: implicit in the part that damps
//! (stable however light the node and strong the air), exact where the motion holds.
//!
//! In the step's objective a dashpot is the potential ½/β·δᵀ·D·δ of the offset δ from where
//! the node would end moving at (v₀, ω₀): x_ref = x̂ + β·v₀, q_ref = exp(β·ω₀)·q̂. Only a
//! symmetric positive semidefinite D has one; the rest of a load's slope (a wing's lift turns
//! with the motion, not against it) stays explicit, in the load.

use super::solver::Work;
use super::{NodeId, Structure};
use crate::so3;
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// A node's load and torque as they depend on its velocity and angular velocity this step:
/// they fall by `matrix`·((v, ω) − (`velocity`, `angular`)) from their values in `load` and
/// `torque` (world frame). `matrix` is symmetric positive semidefinite, rows and columns v
/// then ω (N·s/m, N·s, N·m·s). Set before each step by whatever exerts the load; not saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dashpot {
    pub node: NodeId,
    pub matrix: [[f64; 6]; 6],
    pub velocity: DVec3,
    pub angular: DVec3,
}

fn times(m: &[[f64; 6]; 6], d: [f64; 6]) -> [f64; 6] {
    std::array::from_fn(|r| (0..6).map(|c| m[r][c] * d[c]).sum())
}

fn block(m: &[[f64; 6]; 6], at: usize) -> DMat3 {
    DMat3::from_cols_array_2d(&std::array::from_fn(|c| {
        std::array::from_fn(|r| m[at + r][at + c])
    }))
}

impl Work {
    /// Where each dashpot's node would end moving at its reference motion (each substep).
    pub(super) fn refer_dashpots(&mut self, s: &Structure) {
        let beta = self.beta;
        // Newmark takes a force as the mean of the step's ends: a stiff dashpot (h·D ≫ m)
        // would ring, its velocity's sign flipping each step (the trapezoidal rule is not
        // L-stable: a 10 g body in 100 N·s/m did). Taken h/β times, its correction is
        // backward Euler's, which damps it as the air does, at first order in h.
        #[allow(clippy::float_cmp)] // Newmark's τ is β itself
        let newmark = self.tau == beta;
        self.dashpot_scale = if newmark { self.h / beta } else { 1.0 };
        self.referred = true;
        self.dashpot_ref.clear();
        self.dashpot_ref.extend(s.dashpots.iter().map(|d| {
            let n = d.node;
            let x = self.x_hat[n] + d.velocity * s.free_translation[n] * beta;
            let q = (so3::exp(d.angular * s.free_rotation[n] * beta) * self.q_hat[n]).normalize();
            (x, q)
        }));
    }

    fn offset(&self, k: usize, n: usize, x: &[DVec3], q: &[DQuat]) -> ([f64; 6], DVec3) {
        let (x_ref, q_ref) = self.dashpot_ref[k];
        let (dx, turn) = (x[n] - x_ref, so3::log(q[n] * q_ref.conjugate()));
        ([dx.x, dx.y, dx.z, turn.x, turn.y, turn.z], turn)
    }

    /// The dashpots' potential at (`x`, `q`).
    pub(super) fn dashpot_energy(&self, s: &Structure, x: &[DVec3], q: &[DQuat]) -> f64 {
        if self.dashpot_ref.len() != s.dashpots.len() {
            return 0.0;
        }
        let mut sum = 0.0;
        for (k, d) in s.dashpots.iter().enumerate() {
            let (delta, _) = self.offset(k, d.node, x, q);
            let pushed = times(&d.matrix, delta);
            sum += 0.5 / self.beta
                * self.dashpot_scale
                * (0..6).map(|i| delta[i] * pushed[i]).sum::<f64>();
        }
        sum
    }

    /// Adds the dashpots' forces and torques at the structure's state to `direction` (as
    /// `gather` writes it: masked, times `scale`).
    pub(super) fn dashpot_forces(&mut self, s: &Structure, scale: f64) {
        if self.dashpot_ref.len() != s.dashpots.len() {
            return; // not referred: the initial accelerations' residual
        }
        let count = s.node_count();
        for (k, d) in s.dashpots.iter().enumerate() {
            let n = d.node;
            let (delta, turn) = self.offset(k, n, &s.position, &s.orientation);
            let f = times(&d.matrix, delta).map(|v| v * self.dashpot_scale);
            let force = -DVec3::new(f[0], f[1], f[2]) / self.beta;
            let torque = -(so3::inverse_left_jacobian(turn).transpose()
                * DVec3::new(f[3], f[4], f[5]))
                / self.beta;
            self.direction[n] += force * s.block_mask(n) * scale;
            self.direction[count + n] += torque * s.block_mask(count + n) * scale;
        }
    }
}

impl Structure {
    /// The dashpots' blocks for the solver's matrix, as stiffnesses (it takes them times β²;
    /// a dashpot's is β·D, times its `scale`): D's translation and rotation blocks over β.
    pub(super) fn dashpot_blocks(&self, beta: f64, scale: f64) -> Vec<(usize, DMat3)> {
        let n = self.node_count();
        let mut blocks = Vec::with_capacity(2 * self.dashpots.len());
        for d in &self.dashpots {
            for (at, node) in [(0, d.node), (3, n + d.node)] {
                let b = block(&d.matrix, at);
                if b != DMat3::ZERO {
                    blocks.push((node, b * (scale / beta)));
                }
            }
        }
        blocks
    }

    /// What node 0's dashpots (times `scale`) and added masses put in a rigid body's 6×6
    /// Newton matrix: β·D and M.
    pub(super) fn rigid_dashpot(&self, beta: f64, scale: f64) -> [[f64; 6]; 6] {
        let mut sum = [[0.0; 6]; 6];
        for a in self.added_mass.iter().filter(|a| a.node == 0) {
            for (c, column) in a.matrix.to_cols_array_2d().iter().enumerate() {
                for (r, m) in column.iter().enumerate() {
                    sum[r][c] += m;
                }
            }
        }
        for d in self.dashpots.iter().filter(|d| d.node == 0) {
            for (row, add) in sum.iter_mut().zip(&d.matrix) {
                for (a, b) in row.iter_mut().zip(add) {
                    *a += b * scale * beta;
                }
            }
        }
        sum
    }

    /// The work the dashpots did over a step from (`x0`, `q0`) to now (J), at the motion they
    /// ended on: −D·((v, ω) − (v₀, ω₀))·(Δx, Δθ). The energy guard credits it as it does the
    /// loads' (it is theirs, linearised).
    pub(super) fn dashpot_work(&self, x0: &[DVec3], q0: &[DQuat]) -> f64 {
        let mut work = 0.0;
        for d in &self.dashpots {
            let n = d.node;
            let (dv, dw) = (
                self.velocity[n] - d.velocity,
                self.angular_velocity[n] - d.angular,
            );
            let f = times(&d.matrix, [dv.x, dv.y, dv.z, dw.x, dw.y, dw.z]);
            let (dx, turn) = (
                self.position[n] - x0[n],
                so3::log(self.orientation[n] * q0[n].conjugate()),
            );
            work -= DVec3::new(f[0], f[1], f[2]).dot(dx) + DVec3::new(f[3], f[4], f[5]).dot(turn);
        }
        work
    }
}

impl super::ImplicitSolver {
    /// After a substep: the accelerations Newmark carries to the next are the forces', not
    /// the h/β-fold dashpots' its step took (the next step's mean would take the excess again
    /// and ring as before).
    pub(super) fn unscale_dashpots(&mut self, s: &Structure) {
        let w = &self.work;
        let excess = w.dashpot_scale - 1.0;
        self.dashpot_end.clear();
        // The water's impulse is no force on the body's own acceleration (`added.rs`).
        for a in &s.added_mass {
            let f = w.added_force(s, a) * s.free_translation[a.node];
            self.acceleration[a.node] -= f / s.mass[a.node];
        }
        for (k, d) in s.dashpots.iter().enumerate() {
            let n = d.node;
            let (delta, _) = w.offset(k, n, &s.position, &s.orientation);
            let f = times(&d.matrix, delta).map(|v| v / w.beta);
            let (force, torque) = (-DVec3::new(f[0], f[1], f[2]), -DVec3::new(f[3], f[4], f[5]));
            self.dashpot_end.push((n, force, torque));
            if excess == 0.0 {
                continue;
            }
            let f = f.map(|v| v * excess);
            self.acceleration[n] +=
                DVec3::new(f[0], f[1], f[2]) * s.free_translation[n] / s.mass[n];
            let inertia = w.inertia_star[n] + DMat3::from_diagonal(DVec3::ONE - s.free_rotation[n]);
            self.angular_acceleration[n] +=
                inertia.inverse() * (DVec3::new(f[3], f[4], f[5]) * s.free_rotation[n]);
        }
    }
}

impl super::ImplicitSolver {
    /// The scheme its last substep took (a fallback's, if the energy guard redid it).
    #[allow(clippy::float_cmp)] // Newmark's τ is β itself
    pub fn last_scheme(&self) -> super::Scheme {
        let w = &self.work;
        if w.tau == 0.0 {
            super::Scheme::BackwardEuler
        } else if w.tau == w.beta {
            super::Scheme::Newmark
        } else {
            super::Scheme::Bdf2
        }
    }
}
