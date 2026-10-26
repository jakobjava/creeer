//! What a step changes in a body of one node (a rigid body), to put it back: its motion
//! and its solver's history. A copy of a few numbers, where a clone of the whole body
//! allocated dozens of vectors (redoing a step with substeps, `world/mod.rs`).

use super::{ImplicitSolver, Structure};
use creeer_math::glam::{DQuat, DVec3};

/// The state of a one-node body at the start of a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeState {
    position: DVec3,
    orientation: DQuat,
    velocity: DVec3,
    angular_velocity: DVec3,
    previous: (DVec3, DVec3, DQuat, DVec3),
    acceleration: (DVec3, DVec3),
    loads: (DVec3, DVec3),
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
            loads: (solver.previous_load[0], solver.previous_torque[0]),
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
        (solver.previous_load[0], solver.previous_torque[0]) = self.loads;
        let end = &mut solver.snapshot;
        end.end_x.clear();
        end.end_q.clear();
        if let Some((x, q)) = self.end {
            end.end_x.push(x);
            end.end_q.push(q);
        }
    }
}
