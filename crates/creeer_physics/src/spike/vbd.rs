//! F2 candidate B (ADR-0004): Vertex Block Descent (Chen et al., SIGGRAPH 2024).
//!
//! Implicit Euler written as an energy minimisation, solved one node at a time:
//! for each node, take one Newton step on
//!
//! G(x) = m/(2h²)·|x − y|² + Σ elastic energy of its members,  y = x + h·v + h²·g,
//!
//! with a 3×3 Hessian per node, Gauss-Seidel style (nodes of one graph colour can run in
//! parallel). Each iteration is cheap; the question is how many are needed for steel.
//!
//! Augmented VBD (Giles et al. 2025) adds an augmented-Lagrangian term for *hard* constraints
//! (contacts, joints). For members with a finite material stiffness it reduces to VBD, so
//! plain VBD represents the family in this comparison.

use super::*;
use creeer_math::glam::DMat3;

/// A truss stepped with VBD.
#[derive(Clone, Debug)]
pub struct VbdTruss {
    /// The structure and its current state.
    pub truss: Truss,
    /// Gauss-Seidel sweeps over all nodes per step.
    pub iterations: u32,
    substeps: u32,
    h: f64,
    /// Members attached to each node.
    adjacency: Vec<Vec<usize>>,
    target: Vec<DVec3>,
    /// Acceleration of the previous step (for the warm start).
    acceleration: Vec<DVec3>,
}

impl VbdTruss {
    /// Prepares `truss` for ticks of `dt` seconds of `substeps` steps with `iterations`
    /// sweeps each.
    pub fn new(truss: Truss, dt: f64, substeps: u32, iterations: u32) -> Self {
        let mut adjacency = vec![Vec::new(); truss.node_count()];
        for (index, beam) in truss.beams.iter().enumerate() {
            adjacency[beam.a].push(index);
            adjacency[beam.b].push(index);
        }
        Self {
            target: vec![DVec3::ZERO; truss.node_count()],
            acceleration: vec![DVec3::ZERO; truss.node_count()],
            truss,
            iterations,
            substeps,
            h: dt / f64::from(substeps),
            adjacency,
        }
    }

    /// Advances one tick.
    pub fn step(&mut self) {
        for _ in 0..self.substeps {
            self.substep();
        }
    }

    fn substep(&mut self) {
        let h = self.h;
        let t = &mut self.truss;
        for i in 0..t.node_count() {
            let free = t.free_mask(i);
            t.previous[i] = t.position[i];
            self.target[i] = t.position[i] + (t.velocity[i] * h + GRAVITY * (h * h)) * free;
            // Warm start (Chen et al., "adaptive initialisation"): continue with the previous
            // acceleration along gravity, clamped to [0, g]. Starting from y instead drops
            // every node by h²·g each step, which few iterations cannot undo for the
            // long-wavelength bending of a large structure. (Using the full previous
            // acceleration was measured to be unstable.)
            let g = GRAVITY.length();
            let down = GRAVITY / g;
            let along = self.acceleration[i].dot(down).clamp(0.0, g);
            t.position[i] += (t.velocity[i] * h + down * (along * h * h)) * free;
        }
        for _ in 0..self.iterations {
            for i in 0..t.node_count() {
                let free = t.free_mask(i);
                if free == DVec3::ZERO {
                    continue;
                }
                let inertia = t.mass[i] / (h * h);
                let mut force = (self.target[i] - t.position[i]) * inertia;
                let mut hessian = DMat3::IDENTITY * inertia;
                for &index in &self.adjacency[i] {
                    let beam = &t.beams[index];
                    let other = if beam.a == i { beam.b } else { beam.a };
                    let delta = t.position[i] - t.position[other];
                    let length = delta.length();
                    let e = delta / length;
                    let k = 1.0 / beam.compliance;
                    let tension = k * (length - beam.rest_length);
                    force -= e * tension;
                    let outer = DMat3::from_cols(e * e.x, e * e.y, e * e.z);
                    hessian += outer * k;
                    // Geometric stiffness of a member in tension (the part that is always
                    // positive definite; compression would make the block indefinite).
                    if tension > 0.0 {
                        hessian += (DMat3::IDENTITY - outer) * (tension / length);
                    }
                }
                let p = DMat3::from_diagonal(free);
                let block = p * hessian * p + (DMat3::IDENTITY - p);
                t.position[i] += block.inverse() * (force * free);
            }
        }
        for i in 0..t.node_count() {
            let velocity = (t.position[i] - t.previous[i]) / h;
            self.acceleration[i] = (velocity - t.velocity[i]) / h;
            t.velocity[i] = velocity;
        }
    }
}
