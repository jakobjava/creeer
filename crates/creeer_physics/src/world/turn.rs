//! Stepping the world in substeps while boxes (and hulls, flat-faced as they are) turn fast,
//! and checking the turn a step made.

use super::shape::Shape;
use super::{Body, World, sleep};
use crate::structure::NodeId;
use crate::structure::history::NodeState;
use creeer_math::glam::DQuat;

/// Largest turn of a box within one (sub)step (rad).
const MAX_TURN: f64 = 0.05;
/// Most substeps per step.
const MAX_SUBSTEPS: usize = 16;

/// The substeps a box turning `turn` (rad) in a step needs.
fn substeps(turn: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, clamped
    let n = (turn / MAX_TURN).ceil() as usize;
    n.clamp(1, MAX_SUBSTEPS)
}

impl World {
    /// Advances the world by one step, split into substeps while boxes turn fast: contacts
    /// are found at the start of a (sub)step with fixed normals and points, which describe a
    /// box that turns 0.3 rad in the step (a crate struck by a ball, 18 rad/s) badly enough
    /// to let corners pass into neighbours unnoticed. Substeps keep the turn below
    /// [`MAX_TURN`].
    ///
    /// The spin at the start does not tell how far a box turns when it is struck during the
    /// step: a crate hit by a ball spun up within one, buried a corner 7.6 cm in its
    /// neighbour, and the overlap threw the pile apart (F5). So the step is checked against
    /// the turn it made, and redone from the start with the substeps that turn needs when a
    /// box turned more than twice [`MAX_TURN`] per substep.
    pub fn step(&mut self) {
        let boxes = |bodies: &[Body]| -> Vec<(usize, NodeId)> {
            bodies
                .iter()
                .enumerate()
                .filter(|(_, b)| !b.sleep.asleep)
                .flat_map(|(i, b)| {
                    b.shapes
                        .iter()
                        .filter(|(_, shape)| matches!(shape, Shape::Box { .. } | Shape::Hull(_)))
                        .map(move |(node, _)| (i, *node))
                })
                .collect()
        };
        let spinning = boxes(&self.bodies);
        let turn = spinning
            .iter()
            .map(|&(i, node)| self.bodies[i].structure.angular_velocity[node].length())
            .fold(0.0, f64::max)
            * self.dt;
        let n = substeps(turn);
        let saved = (!spinning.is_empty() && n < MAX_SUBSTEPS).then(|| {
            let bodies: Vec<Saved> = self.bodies.iter().map(Saved::of).collect();
            (bodies, self.anchors.clone())
        });
        self.step_in(n);
        let Some((bodies, anchors)) = saved else {
            return;
        };
        // Every box awake at the start or woken in the step.
        let turned = boxes(&self.bodies)
            .iter()
            .map(|&(i, node)| {
                let before = bodies[i].orientation(node);
                let after = self.bodies[i].structure.orientation[node];
                crate::so3::log(after * before.conjugate()).length()
            })
            .fold(0.0, f64::max);
        #[allow(clippy::cast_precision_loss)] // a few substeps
        if turned > 2.0 * MAX_TURN * n as f64 {
            for (body, saved) in self.bodies.iter_mut().zip(bodies) {
                saved.restore(body);
            }
            self.anchors = anchors;
            self.step_in(substeps(turned));
        }
    }

    /// One step in `n` substeps.
    fn step_in(&mut self, n: usize) {
        if n == 1 {
            self.step_once();
            return;
        }
        let dt = self.dt;
        #[allow(clippy::cast_precision_loss)]
        let h = dt / n as f64;
        for b in &mut self.bodies {
            b.solver.set_time_step(&b.structure, h);
        }
        self.dt = h;
        for _ in 0..n {
            self.step_once();
        }
        self.dt = dt;
        for b in &mut self.bodies {
            b.solver.set_time_step(&b.structure, dt);
        }
    }
}

/// A body as a step found it, to step it again: a rigid body's few numbers, or the whole
/// of a structure (its members may yield or break in the step).
#[allow(clippy::large_enum_variant)] // the usual one, kept without an allocation per body
enum Saved {
    /// A body of one node.
    Rigid(NodeState, sleep::Sleep, bool),
    /// A structure.
    Whole(Box<Body>),
}

impl Saved {
    fn of(body: &Body) -> Self {
        match NodeState::of(&body.structure, &body.solver) {
            Some(state) => Self::Rigid(state, body.sleep.clone(), body.crushing),
            None => Self::Whole(Box::new(body.clone())),
        }
    }

    /// The orientation of node `node` as it was.
    fn orientation(&self, node: NodeId) -> DQuat {
        match self {
            Self::Rigid(state, ..) => state.orientation(),
            Self::Whole(body) => body.structure.orientation[node],
        }
    }

    fn restore(self, body: &mut Body) {
        match self {
            Self::Rigid(state, sleep, crushing) => {
                state.restore(&mut body.structure, &mut body.solver);
                body.sleep = sleep;
                body.crushing = crushing;
            }
            Self::Whole(saved) => *body = *saved,
        }
    }
}
