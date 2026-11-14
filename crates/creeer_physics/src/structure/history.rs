//! What a step changes in a body of one node (a rigid body), to put it back: its motion
//! and its solver's history. A copy of a few numbers, where a clone of the whole body
//! allocated dozens of vectors (redoing a step with substeps, `world/mod.rs`). And the
//! history rebuilt for a new time step.

use super::{ImplicitSolver, Structure};
use creeer_math::glam::{DQuat, DVec3};

impl ImplicitSolver {
    /// Changes the time step to `dt` (split into the solver's substeps) and re-linearises.
    /// The history is rebuilt for the new step from the current velocities and the last
    /// accelerations (x₋ = x − h·v + h²/2·a), so BDF2 continues a decelerating motion
    /// instead of restarting it as uniform (which would make or lose energy).
    pub fn set_time_step(&mut self, structure: &Structure, dt: f64) {
        let h = dt / f64::from(self.substeps);
        #[allow(clippy::float_cmp)] // the very same step: nothing to do
        if h == self.h {
            return;
        }
        self.h = h;
        self.work.beta = self.beta();
        self.refactor(structure);
        let s = structure;
        for i in 0..s.node_count() {
            let (a, alpha) = (self.acceleration[i], self.angular_acceleration[i]);
            self.previous_x[i] = s.position[i] - s.velocity[i] * h + a * (0.5 * h * h);
            self.previous_v[i] = s.velocity[i] - a * h;
            let back = -s.angular_velocity[i] * h + alpha * (0.5 * h * h);
            self.previous_q[i] = crate::so3::exp(back) * s.orientation[i];
            self.previous_w[i] = s.angular_velocity[i] - alpha * h;
        }
    }
}

/// The state of a one-node body at the start of a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeState {
    position: DVec3,
    orientation: DQuat,
    velocity: DVec3,
    angular_velocity: DVec3,
    previous: (DVec3, DVec3, DQuat, DVec3),
    acceleration: (DVec3, DVec3),
    loads: [DVec3; 4],
    /// Where the energy guard saw the last step end (`step.rs`'s snapshot), if it did: the
    /// guard holds the contacts found at a step's start to what the last step left in them
    /// only where the bodies are where it ended. Not put back, a step redone in substeps (a
    /// box turning fast, `world/turn.rs`) saw its body elsewhere than the redone attempt's end
    /// and held its contacts to nothing: the energy they were found with became motion. A
    /// pole's contacts found holding 2.5 GJ, against the 81 kJ the last step left in them,
    /// threw it at 1,254 m/s (G2.9, ADR-0005).
    end: Option<(DVec3, DQuat)>,
}

impl NodeState {
    /// The state of node 0 of `s` and its solver, if the structure is a single node.
    pub fn of(s: &Structure, solver: &ImplicitSolver) -> Option<Self> {
        let end = &solver.snapshot;
        (s.node_count() == 1 && s.element_count() == 0).then(|| Self {
            position: s.position[0],
            orientation: s.orientation[0],
            velocity: s.velocity[0],
            angular_velocity: s.angular_velocity[0],
            previous: (
                solver.previous_x[0],
                solver.previous_v[0],
                solver.previous_q[0],
                solver.previous_w[0],
            ),
            acceleration: (solver.acceleration[0], solver.angular_acceleration[0]),
            loads: [
                solver.previous_load[0],
                solver.previous_torque[0],
                solver.given[0][0],
                solver.given[1][0],
            ],
            end: (end.end_x.first().copied()).zip(end.end_q.first().copied()),
        })
    }

    /// The node's orientation.
    pub fn orientation(&self) -> DQuat {
        self.orientation
    }

    /// Puts the state back.
    pub fn restore(&self, s: &mut Structure, solver: &mut ImplicitSolver) {
        s.position[0] = self.position;
        s.orientation[0] = self.orientation;
        s.velocity[0] = self.velocity;
        s.angular_velocity[0] = self.angular_velocity;
        (
            solver.previous_x[0],
            solver.previous_v[0],
            solver.previous_q[0],
            solver.previous_w[0],
        ) = self.previous;
        (solver.acceleration[0], solver.angular_acceleration[0]) = self.acceleration;
        [
            solver.previous_load[0],
            solver.previous_torque[0],
            solver.given[0][0],
            solver.given[1][0],
        ] = self.loads;
        let end = &mut solver.snapshot;
        end.end_x.clear();
        end.end_q.clear();
        if let Some((x, q)) = self.end {
            end.end_x.push(x);
            end.end_q.push(q);
        }
    }
}
