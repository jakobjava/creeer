//! Added mass: the water a submerged body must set moving as it moves (a sphere's is half the
//! water it displaces). It is inertia without weight, and it changes as the body goes in or
//! out of the water; so it enters the step as momentum exchanged with that water: over the
//! step the water takes the impulse M·v − P, M its added mass now, P the momentum the water
//! had with the body at the step's start (whoever sets it says how much: none for water the
//! body just entered, M₀·v₀ for water that moved with it).
//!
//! In the step's objective that is the potential ½/β²·yᵀ·M·y − P·y/β, y = x − x̂ (the new
//! velocity is y/β): its force, −(M·v − P)/β, taken over the step by any scheme, is that
//! impulse. It is no force on the body's own acceleration, which Newmark carries to the next
//! step: with M fixed, the body moves as one of mass m + M under the other forces, by the
//! same trapezoidal rule (energy kept), and going in, it shares its momentum with the water
//! (energy lost to the splash).

use super::solver::Work;
use super::{NodeId, Structure};
use creeer_math::glam::{DMat3, DVec3};

/// Water moving with node `node` this step: its added mass (kg; symmetric positive
/// semidefinite, world frame) and the momentum (kg·m/s, world) it had with the node at the
/// step's start. Set before each step by whatever it is immersed in; not saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AddedMass {
    pub node: NodeId,
    pub matrix: DMat3,
    pub momentum: DVec3,
}

impl Work {
    /// The added masses' potential at positions `x`.
    pub(super) fn added_energy(&self, s: &Structure, x: &[DVec3]) -> f64 {
        if !self.referred {
            return 0.0;
        }
        let beta = self.beta;
        s.added_mass
            .iter()
            .map(|a| {
                let y = x[a.node] - self.x_hat[a.node];
                0.5 / (beta * beta) * y.dot(a.matrix * y) - a.momentum.dot(y) / beta
            })
            .sum()
    }

    /// Their force on node `a.node` at the structure's state: −(M·v − P)/β.
    pub(super) fn added_force(&self, s: &Structure, a: &AddedMass) -> DVec3 {
        let v = (s.position[a.node] - self.x_hat[a.node]) / self.beta;
        -(a.matrix * v - a.momentum) / self.beta
    }

    /// Adds their forces to `direction` (masked, times `scale`, as `gather` writes it).
    pub(super) fn added_forces(&mut self, s: &Structure, scale: f64) {
        for a in &s.added_mass {
            let f = self.added_force(s, a);
            self.direction[a.node] += f * s.block_mask(a.node) * scale;
        }
    }
}

impl Structure {
    /// Their blocks for the solver's matrix, as stiffnesses (taken times β²): M/β².
    pub(super) fn added_blocks(&self, beta: f64) -> Vec<(usize, DMat3)> {
        (self.added_mass.iter())
            .filter(|a| a.matrix != DMat3::ZERO)
            .map(|a| (a.node, a.matrix / (beta * beta)))
            .collect()
    }

    /// The work the water's impulse did on the nodes over a step from `x0` of `h` (J):
    /// −(M·v − P)·Δx/h, the impulse at the step's mean velocity (Newmark's; backward Euler's
    /// end velocity makes it a little more, its own dissipation).
    pub(super) fn added_work(&self, x0: &[DVec3], h: f64) -> f64 {
        (self.added_mass.iter())
            .map(|a| {
                let n = a.node;
                -(a.matrix * self.velocity[n] - a.momentum).dot(self.position[n] - x0[n]) / h
            })
            .sum()
    }
}
