//! A node's mass changing as it moves (G1.2: a jetpack's gas leaving the body that carries
//! it): its inertia scales with it (the same shape, the same density), and the solver's
//! inverse of it follows. Its velocity is the node's own: the mass leaving carries its share
//! of the momentum away.

use super::{ImplicitSolver, NodeId, Structure};
use creeer_math::glam::DMat3;

impl ImplicitSolver {
    /// Sets node `node` of `s` to `mass` (kg).
    ///
    /// # Panics
    /// If `mass` is not positive.
    pub fn set_mass(&mut self, s: &mut Structure, node: NodeId, mass: f64) {
        assert!(mass > 0.0, "a node's mass must be positive");
        let scale = mass / s.mass[node];
        s.mass[node] = mass;
        s.inertia[node] *= scale;
        let inertia = s.inertia[node];
        self.rest_inverse[node] = if inertia.determinant() > 0.0 {
            inertia.inverse()
        } else {
            DMat3::ZERO
        };
    }
}
