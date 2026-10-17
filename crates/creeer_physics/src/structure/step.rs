//! One time step: prediction, the minimisation, and the guards that keep it physical.
//!
//! - **Converge, don't truncate.** An unconverged step can inject energy (measured: a frame
//!   hit at 30 m/s explodes with a hard cap of 6 iterations, in both Newmark and BDF2).
//!   Every step iterates until its estimated remaining error is below the tolerance;
//!   `max_iterations` is only a safety net. Speed comes from needing few iterations
//!   (per-node co-rotation of the factorised matrix, L-BFGS), never from stopping early.
//! - **Energy guard.** Newmark (the trapezoidal rule) can gain energy in strongly nonlinear
//!   events even when converged (Simo & Tarnow 1992; measured ×4.5 in one tick of that
//!   impact). After each Newmark step the total energy is checked; if it rose, the step is
//!   redone with BDF2, which only dissipates.

use super::Structure;
use super::group::{NoCoupling, step_group};
use super::solver::{ImplicitSolver, Scheme};
use crate::so3;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use rayon::prelude::*;

/// Everything a rejected step must restore.
#[derive(Clone, Debug, Default)]
pub(super) struct Snapshot {
    vectors: [Vec<DVec3>; 8],
    quats: [Vec<creeer_math::glam::DQuat>; 2],
    /// The state at the end of the last step (to know whether its energy is still valid).
    pub(super) end_x: Vec<DVec3>,
    pub(super) end_q: Vec<creeer_math::glam::DQuat>,
}

impl ImplicitSolver {
    /// Advances `structure` by one step.
    pub fn step(&mut self, structure: &mut Structure) {
        step_group(&mut [(&mut *structure, &mut *self)], &mut NoCoupling);
        self.split_compressed(structure);
    }

    pub(super) fn save(&mut self, s: &Structure) {
        let [a, b, c, d, e, f, g, h] = &mut self.snapshot.vectors;
        for (to, from) in [
            (a, &s.position),
            (b, &s.velocity),
            (c, &s.angular_velocity),
            (d, &self.previous_x),
            (e, &self.previous_v),
            (f, &self.previous_w),
            (g, &self.acceleration),
            (h, &self.angular_acceleration),
        ] {
            to.clone_from(from);
        }
        let [p, q] = &mut self.snapshot.quats;
        p.clone_from(&s.orientation);
        q.clone_from(&self.previous_q);
    }

    pub(super) fn restore(&mut self, s: &mut Structure) {
        let [a, b, c, d, e, f, g, h] = &self.snapshot.vectors;
        for (to, from) in [
            (&mut s.position, a),
            (&mut s.velocity, b),
            (&mut s.angular_velocity, c),
            (&mut self.previous_x, d),
            (&mut self.previous_v, e),
            (&mut self.previous_w, f),
            (&mut self.acceleration, g),
            (&mut self.angular_acceleration, h),
        ] {
            to.clone_from(from);
        }
        let [p, q] = &self.snapshot.quats;
        s.orientation.clone_from(p);
        self.previous_q.clone_from(q);
    }

    /// Prepares a substep: prediction and Newton's starting point.
    pub(super) fn begin_substep(&mut self, s: &mut Structure, scheme: Scheme) {
        self.work.beta = self.beta_for(scheme);
        self.work.h = self.h;
        self.work.tau = match scheme {
            Scheme::BackwardEuler => 0.0,
            Scheme::Bdf2 => self.h / 3.0,
            Scheme::Newmark => self.work.beta,
        };
        self.predict(s, scheme);
        // Where Newton starts (the minimum it finds does not depend on it): a Taylor step
        // x + h·v + h²/2·a with the last acceleration, off by O(h³) for smooth motion. For
        // a truck crossing a bridge this saves 8 % of the iterations against x + h·v.
        // (Newmark's x* itself, x + h·v + h²/4·a, is a poor start: it is the minimum only
        // when the new acceleration is zero.)
        if scheme == Scheme::Newmark {
            let (h, this) = (self.h, &*self);
            let (free_t, free_r) = (&s.free_translation, &s.free_rotation);
            s.position
                .par_iter_mut()
                .zip(&mut s.orientation)
                .enumerate()
                .with_min_len(128)
                .for_each(|(i, (x, q))| {
                    let v = this.previous_v[i] + this.acceleration[i] * (0.5 * h);
                    let w = this.previous_w[i] + this.angular_acceleration[i] * (0.5 * h);
                    *x = this.previous_x[i] + v * free_t[i] * h;
                    *q = (so3::exp(w * free_r[i] * h) * this.previous_q[i]).normalize();
                });
        } else {
            s.position.clone_from(&self.work.x_star);
            s.orientation.clone_from(&self.work.q_star);
        }
    }

    /// After the minimisation: velocities and accelerations of the new state.
    pub(super) fn finish_substep(&mut self, s: &mut Structure) {
        let beta = self.work.beta;
        let w = &self.work;
        // Every node on its own (in parallel: the same arithmetic per node, so the same bits).
        (
            s.velocity.par_iter_mut(),
            s.angular_velocity.par_iter_mut(),
            self.acceleration.par_iter_mut(),
            self.angular_acceleration.par_iter_mut(),
        )
            .into_par_iter()
            .enumerate()
            .with_min_len(128)
            .for_each(|(i, (v, omega, a, alpha))| {
                let (x, q) = (s.position[i], s.orientation[i]);
                *v = (x - w.x_hat[i]) / beta;
                *omega = so3::log(q * w.q_hat[i].conjugate()) / beta;
                // At the minimum M·(x − x*)/β² = f: this is the new acceleration.
                *a = (x - w.x_star[i]) / (beta * beta);
                *alpha = so3::log(q * w.q_star[i].conjugate()) / (beta * beta);
            });
    }

    /// For a rigid body (one node, no members): the dense 6×6 Newton matrix
    /// M + β²·H with the coupling's stiffness H, fixed axes replaced by identity rows.
    pub(super) fn rigid_matrix(
        &self,
        s: &Structure,
        coupling: [[f64; 6]; 6],
    ) -> Option<[[f64; 6]; 6]> {
        if !(s.elements.is_empty() && s.node_count() == 1) {
            return None;
        }
        let b2 = self.work.beta * self.work.beta;
        let inertia = self.work.inertia_star[0];
        let free: [f64; 6] = std::array::from_fn(|k| {
            if k < 3 {
                s.free_translation[0][k]
            } else {
                s.free_rotation[0][k - 3]
            }
        });
        Some(std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                if free[r] == 0.0 || free[c] == 0.0 {
                    return if r == c { 1.0 } else { 0.0 };
                }
                let mass = match (r < 3, c < 3) {
                    (true, true) if r == c => s.mass[0] + self.work.beta * self.work.drag_rate[0],
                    (false, false) => inertia.col(c - 3)[r - 3],
                    _ => 0.0,
                };
                mass + b2 * coupling[r][c]
            })
        }))
    }

    /// Applies the preconditioner (an approximate inverse of the Newton matrix) to `v`.
    pub(super) fn precondition(
        &mut self,
        s: &Structure,
        v: &mut [DVec3],
        rigid: Option<&[[f64; 6]; 6]>,
    ) {
        if let Some(matrix) = rigid {
            let rhs = [v[0].x, v[0].y, v[0].z, v[1].x, v[1].y, v[1].z];
            let x = dense_solve(matrix, rhs);
            v[0] = DVec3::new(x[0], x[1], x[2]);
            v[1] = DVec3::new(x[3], x[4], x[5]);
            return;
        }
        // The matrix belongs to the shape at factorisation time. Turning each node's
        // forces back by the rotation that node has made since, solving, and turning the
        // answer forward again makes it exact up to the *relative* rotation across a member,
        // which is far smaller than the rotations themselves.
        let n = s.node_count();
        for (b, d) in v.iter_mut().enumerate() {
            *d = self.turns[b % n].transpose() * *d;
        }
        self.chol.solve(v);
        for (b, d) in v.iter_mut().enumerate() {
            *d = self.turns[b % n] * *d;
        }
    }

    /// Each node's rotation since the matrix was built (identity for supported nodes, whose
    /// fixed axes are fixed in the world frame).
    pub(super) fn update_turns(&mut self, s: &Structure) {
        for (i, turn) in self.turns.iter_mut().enumerate() {
            let held = s.free_translation[i] != DVec3::ONE || s.free_rotation[i] != DVec3::ONE;
            *turn = if held {
                DMat3::IDENTITY
            } else {
                DMat3::from_quat(s.orientation[i] * self.factor_orientation[i].conjugate())
            };
        }
    }

    /// Computes x̂, q̂, the predictions x*, q*, and the damping reference ŝ.
    fn predict(&mut self, s: &Structure, scheme: Scheme) {
        let beta = self.work.beta;
        // Per node: x̂, q̂, x*, q*, inertia at q*, rotation of q̂ (in parallel; every node is
        // independent, so the result does not depend on the threads).
        // Written in place, each node by itself (the work arrays in parallel, by node).
        let this = (
            &self.previous_x,
            &self.previous_v,
            &self.previous_q,
            &self.previous_w,
            &self.acceleration,
            &self.angular_acceleration,
        );
        let w = &mut self.work;
        (
            w.x_hat.par_iter_mut(),
            w.q_hat.par_iter_mut(),
            w.x_star.par_iter_mut(),
            w.q_star.par_iter_mut(),
            w.inertia_star.par_iter_mut(),
            w.rotations.par_iter_mut(),
            w.drag_rate.par_iter_mut(),
        )
            .into_par_iter()
            .enumerate()
            .with_min_len(128)
            .for_each(|(i, out)| {
                let (previous_x, previous_v, previous_q, previous_w, acceleration, angular) = this;
                let (x, v) = (s.position[i], s.velocity[i]);
                let (q, omega) = (s.orientation[i], s.angular_velocity[i]);
                // Rotation carries angular momentum L = I·ω, not ω: when the inertia turns
                // with the node, ω must change (precession, gyroscopic effects).
                let world_inertia = |q: DQuat| {
                    let r = DMat3::from_quat(q);
                    r * s.inertia[i] * r.transpose()
                };
                let inertia_q = world_inertia(q);
                let momentum = inertia_q * omega;
                let transport = |q_hat: DQuat, w: DVec3| {
                    if q_hat == q {
                        w
                    } else {
                        world_inertia(q_hat).inverse() * (momentum + inertia_q * (w - omega))
                    }
                };
                // x* = x̂ + β·v̂, and the new velocity will be (x − x̂)/β.
                let (x_hat, v_hat, q_hat, w_hat) = match scheme {
                    Scheme::BackwardEuler => (x, v, q, omega),
                    // x_{n+1} = x + h·v + h²/4·(a + a_{n+1}), v_{n+1} = v + h/2·(a + a_{n+1}).
                    Scheme::Newmark => {
                        let q_hat = so3::exp(omega * beta) * q;
                        // Carry L to the predicted end orientation q* = exp(β·w)·q̂, which itself
                        // depends on w: a small fixed point (3 iterations reach ~1e-9 at 0.1 rad
                        // per step). Without it L drifts 1.8 % per step for a tumbling box.
                        let alpha = angular[i] * beta;
                        let mut w = transport(q_hat, omega) + alpha;
                        if q_hat != q {
                            for _ in 0..3 {
                                let q_end = so3::exp(w * beta) * q_hat;
                                w = transport(q_end, omega) + alpha;
                            }
                        }
                        (x + v * beta, v + acceleration[i] * beta, q_hat, w)
                    }
                    Scheme::Bdf2 => {
                        let q_hat = so3::exp(so3::log(q * previous_q[i].conjugate()) / 3.0) * q;
                        let w = transport(q_hat, (4.0 * omega - previous_w[i]) / 3.0);
                        (
                            x + (x - previous_x[i]) / 3.0,
                            (4.0 * v - previous_v[i]) / 3.0,
                            q_hat,
                            w,
                        )
                    }
                };
                let x_star = x_hat + v_hat * s.free_translation[i] * beta;
                let q_star = (so3::exp(w_hat * s.free_rotation[i] * beta) * q_hat).normalize();
                let r = DMat3::from_quat(q_star);
                *out.4 = r * s.inertia[i] * r.transpose();
                (*out.0, *out.1, *out.2, *out.3) = (x_hat, q_hat, x_star, q_star);
                *out.5 = DMat3::from_quat(q_hat);
                *out.6 = s.drag[i] * v.length();
            });
        self.previous_x.clone_from(&s.position);
        self.previous_v.clone_from(&s.velocity);
        self.previous_q.clone_from(&s.orientation);
        self.previous_w.clone_from(&s.angular_velocity);
        let w = &mut self.work;
        w.set_coriolis(
            s,
            Some((&self.acceleration, &self.angular_acceleration, self.h)),
        );
        let (x_hat, rotations, tau, h) = (&w.x_hat, &w.rotations, w.tau, w.h);
        let beta = w.beta;
        s.elements
            .par_iter()
            .zip(&mut w.s_hat)
            .zip(w.caps.par_iter_mut().zip(&mut w.scaled))
            .with_min_len(128)
            .for_each(|((e, s_hat), (caps, scaled))| {
                *s_hat = e.measures(x_hat[e.a], rotations[e.a], x_hat[e.b], rotations[e.b]);
                // The elastic part at x̂: x̂ continues the plastic flow too.
                for (s, rate) in s_hat.iter_mut().zip(&e.flow_rate) {
                    *s -= tau * rate;
                }
                // Plastic capacities for this step, from the axial force at its start.
                *caps = e.capacities(s_hat, h);
                let gamma = e.material.damping / beta;
                *scaled = caps.map(|f| f.scaled(1.0 + gamma));
            });
    }
}

/// Solves A·x = b for a symmetric positive-definite 6×6 matrix (Cholesky).
#[allow(clippy::needless_range_loop)] // triangular loops read best with indices
fn dense_solve(a: &[[f64; 6]; 6], b: [f64; 6]) -> [f64; 6] {
    let mut l = [[0.0; 6]; 6];
    for j in 0..6 {
        let mut d = a[j][j];
        for k in 0..j {
            d -= l[j][k] * l[j][k];
        }
        let d = d.max(1e-300).sqrt();
        l[j][j] = d;
        for i in j + 1..6 {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            l[i][j] = s / d;
        }
    }
    let mut y = [0.0; 6];
    for i in 0..6 {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i][k] * y[k];
        }
        y[i] = s / l[i][i];
    }
    let mut x = [0.0; 6];
    for i in (0..6).rev() {
        let mut s = y[i];
        for k in i + 1..6 {
            s -= l[k][i] * x[k];
        }
        x[i] = s / l[i][i];
    }
    x
}
