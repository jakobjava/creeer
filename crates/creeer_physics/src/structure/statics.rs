//! Static equilibrium of structures under gravity and loads.
//!
//! - [`linear`]: small deformations, one sparse solve K·u = f (exact FEM, ADR-0002).
//! - [`nonlinear`]: large deformations (buckling, big deflections), by minimising the
//!   potential energy with the same Newton/line-search machinery as dynamics. A backward
//!   Euler step with an enormous time step *is* that minimisation: the inertia terms
//!   (∝ 1/β²) vanish and only elastic energy and external work remain.

use super::Structure;
use super::solver::{ImplicitSolver, Refactoring, Scheme};
use crate::sparse::{Cholesky, NotPositiveDefinite};
use creeer_math::glam::{DMat3, DVec3};

/// Small-deformation equilibrium: returns the displacement and the rotation vector of every
/// node, without changing the structure.
///
/// # Errors
/// If the structure is a mechanism (it can move without deforming, e.g. not supported).
pub fn linear(s: &Structure) -> Result<(Vec<DVec3>, Vec<DVec3>), NotPositiveDefinite> {
    let n = s.node_count();
    let mut diag = vec![DMat3::ZERO; 2 * n];
    let mut off = vec![DMat3::ZERO; s.off_block_count()];
    s.assemble(false, 0.0, |_| 1.0, &mut diag, &mut off);
    let mut chol = Cholesky::new(s.symbolic());
    chol.factor(&diag, &off)?;
    let mut f: Vec<DVec3> = (0..n)
        .map(|i| {
            let field = s
                .planet
                .as_ref()
                .map_or(DVec3::ZERO, |g| g.acceleration(s.position[i]) * s.mass[i]);
            (s.gravity * s.mass[i] + s.load[i] + field) * s.free_translation[i]
        })
        .chain((0..n).map(|i| s.torque[i] * s.free_rotation[i]))
        .collect();
    chol.solve(&mut f);
    let rotations = f.split_off(n);
    Ok((f, rotations))
}

/// Large-deformation equilibrium: moves the structure to the configuration of minimum
/// potential energy (starting from its current one) and zeroes its velocities. Returns the
/// Newton iterations used.
///
/// Members the load compresses enough are split (`buckling.rs`), and the structure with its
/// new joints is solved again until none is: in equilibrium only as it was before the split,
/// a 120 m tower rang at its new joints for seconds when its motion began (F5).
///
/// The structure must be supported: a free body under load has no equilibrium.
pub fn nonlinear(s: &mut Structure, max_iterations: u32) -> u32 {
    let mut iterations = 0;
    loop {
        let elements = s.element_count();
        iterations += equilibrium(s, max_iterations);
        if s.element_count() == elements {
            return iterations;
        }
    }
}

/// One solve of [`nonlinear`].
fn equilibrium(s: &mut Structure, max_iterations: u32) -> u32 {
    // β = 1000 s: inertia terms are ~10⁻⁶ of a typical stiffness and play no role in the
    // equilibrium; a larger β only worsens the matrix condition (10⁶× at 10⁶ s).
    let mut solver = ImplicitSolver::new(s, Scheme::BackwardEuler, 1.0e3, 1);
    solver.max_iterations = max_iterations;
    solver.refactoring = Refactoring::EveryIteration;
    solver.tolerance = 1.0e-10;
    solver.step(s);
    for (v, w) in s.velocity.iter_mut().zip(&mut s.angular_velocity) {
        *v = DVec3::ZERO;
        *w = DVec3::ZERO;
    }
    solver.last_iterations
}
