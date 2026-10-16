//! The incremental potential G of one implicit step, its gradient, and the line search.
//! See `solver.rs` for the formula.
//!
//! Elements are evaluated in parallel; their contributions are then summed per node in a
//! fixed order (ascending element index), so the result has the same bits on any number of
//! threads.

use super::Structure;
use super::element::MEASURES;
use super::solver::Work;
use crate::so3;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use rayon::prelude::*;

/// Elements per parallel task (smaller tasks cost more in scheduling than they save).
const CHUNK: usize = 128;

/// Element forces (elastic plus damping, capped at the member's plastic capacity: see
/// `plastic.rs`), the element's share of G, and its stored elastic energy.
fn element_terms(
    work: &Work,
    s: &Structure,
    index: usize,
    measures: &[f64; MEASURES],
) -> ([f64; MEASURES], f64, f64) {
    let e = &s.elements[index];
    let gamma = e.material.damping / work.beta;
    let (energy, stored, force) =
        e.elastoplastic(measures, &work.s_hat[index], gamma, &work.caps[index]);
    (force, energy, stored)
}

impl Work {
    /// G at the structure's current state (`trial = false`) or at the trial state, and the
    /// elastic energy there. With `forces`, the same pass over the elements also computes
    /// their forces for [`Self::gather`] (a line search usually accepts its first trial, whose
    /// forces are then the next residual's: one pass over the elements instead of two).
    ///
    /// Inertia and gravity are taken relative to the prediction, so that large absolute
    /// energies do not swamp the small differences the line search compares.
    pub(super) fn evaluate(&mut self, s: &Structure, trial: bool, forces: bool) -> (f64, f64) {
        let (x, q) = if trial {
            (&self.trial_x, &self.trial_q)
        } else {
            (&s.position, &s.orientation)
        };
        let inv_b2 = 1.0 / (self.beta * self.beta);
        // Node terms and node rotations in one parallel pass (each parallel loop costs a
        // thread wake-up), the terms summed in node order.
        let mut rotations = std::mem::take(&mut self.rotations);
        let this = &*self;
        let nodes: Vec<f64> = rotations
            .par_iter_mut()
            .enumerate()
            .with_min_len(CHUNK)
            .map(|(i, rotation)| {
                *rotation = DMat3::from_quat(q[i]);
                let offset = x[i] - this.x_star[i];
                let turn = so3::log(q[i] * this.q_star[i].conjugate());
                let moved = x[i] - this.x_hat[i];
                // Air drag: a dashpot on the new velocity (x − x̂)/β.
                // A planet's field enters through its potential, relative to x* like the rest.
                let field = s.planet.as_ref().map_or(0.0, |g| {
                    s.mass[i] * g.potential_difference(x[i], this.x_star[i])
                });
                0.5 * this.drag_rate[i] / this.beta * moved.length_squared()
                    + 0.5
                        * inv_b2
                        * (s.mass[i] * offset.length_squared()
                            + turn.dot(this.inertia_star[i] * turn))
                    - (s.gravity * s.mass[i] + s.load[i] + this.coriolis[i]).dot(offset)
                    + field
                    - s.torque[i].dot(turn)
            })
            .collect();
        let value = nodes.iter().fold(0.0, |sum, v| sum + v);
        self.rotations = rotations;
        let mut element_force = std::mem::take(&mut self.element_force);
        let mut element_energy = std::mem::take(&mut self.element_energy);
        let this = &*self;
        element_energy
            .par_iter_mut()
            .zip(&mut element_force)
            .enumerate()
            .with_min_len(CHUNK)
            .for_each(|(index, (energy, force))| {
                let e = &s.elements[index];
                let (ra, rb) = (this.rotations[e.a], this.rotations[e.b]);
                if forces {
                    let (m, g) = e.kinematics(x[e.a], ra, x[e.b], rb);
                    let (sigma, total, stored) = element_terms(this, s, index, &m);
                    *force = std::array::from_fn(|group| {
                        (0..MEASURES).map(|k| g[k][group] * sigma[k]).sum()
                    });
                    *energy = [total, stored];
                } else {
                    let m = e.measures(x[e.a], ra, x[e.b], rb);
                    let (_, total, stored) = element_terms(this, s, index, &m);
                    *energy = [total, stored];
                }
            });
        self.element_force = element_force;
        self.element_energy = element_energy;
        let total = self.element_energy.iter().fold(value, |sum, e| sum + e[0]);
        let elastic = self.element_energy.iter().fold(0.0, |sum, e| sum + e[1]);
        (total, elastic)
    }

    /// −∇G at the structure's current state, scaled by β² to match the factorised matrix,
    /// with fixed axes zeroed. Written into `direction` (translation blocks, then rotation).
    pub(super) fn residual(&mut self, s: &Structure) {
        self.evaluate(s, false, true);
        self.gather(s);
    }

    /// The residual from the element forces of the last evaluation with `forces`, which must
    /// have been at the structure's current state. Sums per block in element order.
    pub(super) fn gather(&mut self, s: &Structure) {
        let n = s.node_count();
        let inv_b2 = 1.0 / (self.beta * self.beta);
        let scale = self.beta * self.beta;
        let mut direction = std::mem::take(&mut self.direction);
        let this = &*self;
        // Every block is summed on its own, in element order: parallel and deterministic.
        direction
            .par_iter_mut()
            .enumerate()
            .with_min_len(CHUNK)
            .for_each(|(block, out)| {
                let i = block % n;
                let mut r = if block < n {
                    let field = s
                        .planet
                        .as_ref()
                        .map_or(DVec3::ZERO, |g| g.acceleration(s.position[i]) * s.mass[i]);
                    s.gravity * s.mass[i] + s.load[i] + this.coriolis[i] + field
                        - (s.position[i] - this.x_star[i]) * (s.mass[i] * inv_b2)
                        - (s.position[i] - this.x_hat[i]) * (this.drag_rate[i] / this.beta)
                } else {
                    let turn = so3::log(s.orientation[i] * this.q_star[i].conjugate());
                    let inertial = so3::inverse_left_jacobian(turn).transpose()
                        * (this.inertia_star[i] * turn);
                    s.torque[i] - inertial * inv_b2
                };
                for &(element, group) in
                    &this.incidence[this.incidence_ptr[block]..this.incidence_ptr[block + 1]]
                {
                    r -= this.element_force[element as usize][usize::from(group)];
                }
                *out = r * s.block_mask(block) * scale;
            });
        self.direction = direction;
    }

    /// The Coriolis force of every node for the coming step, from its current velocity
    /// (zero without a planet; `gravity.rs`).
    pub(super) fn set_coriolis(&mut self, s: &Structure) {
        for (i, f) in self.coriolis.iter_mut().enumerate() {
            *f = s
                .planet
                .as_ref()
                .map_or(DVec3::ZERO, |g| g.coriolis(s.velocity[i]) * s.mass[i]);
        }
    }

    /// Sets the trial state x + α·d (rotations applied on the left, in the world frame).
    pub(super) fn set_trial(&mut self, s: &Structure, alpha: f64) {
        let n = s.node_count();
        for i in 0..n {
            self.trial_x[i] = s.position[i] + self.direction[i] * alpha;
            let turn: DQuat = so3::exp(self.direction[n + i] * alpha);
            self.trial_q[i] = (turn * s.orientation[i]).normalize();
        }
    }
}
