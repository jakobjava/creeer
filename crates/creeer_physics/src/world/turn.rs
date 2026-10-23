//! Stepping the world in substeps while boxes (and hulls, flat-faced as they are) turn fast,
//! and checking the turn a step made.

use super::detect::Found;
use super::shape::Shape;
use super::{Body, World, sleep};
use crate::structure::NodeId;
use crate::structure::history::NodeState;
use creeer_math::glam::DQuat;

/// Largest turn of a box within one (sub)step (rad). An angle, not the sweep of its corners:
/// a many-faced hull's contact features change with its turn whatever its size (a limit of
/// 1.5 cm of sweep let two heap rocks of 0.25 m, turning 0.15 rad a step in two substeps,
/// sink 27 mm into each other, G1.8; `overlap_tests.rs`).
const MAX_TURN: f64 = 0.05;
/// Most substeps per step.
const MAX_SUBSTEPS: usize = 16;
/// A contact found this close (m) beyond what its two sides travel in the step counts as one
/// the step may close (the detection's own least margin).
const REACH: f64 = 0.005;

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
    /// Only boxes that touch something need them: one with no contact at the step's start
    /// reaches nothing within it (the detection's margin is twice its travel), and its
    /// features describe no contact to go wrong. Counting every box, a ragdoll's hand flung
    /// through the air at 27 rad/s took the whole world of a play scene to nine substeps a
    /// tick for a third of a second (G1.8).
    ///
    /// The spin at the start does not tell how far a box turns when it is struck during the
    /// step: a crate hit by a ball spun up within one, buried a corner 7.6 cm in its
    /// neighbour, and the overlap threw the pile apart (F5). So the step is checked against
    /// the turn it made, and redone from the start with the substeps that turn needs when a
    /// box turned more than twice [`MAX_TURN`] per substep.
    pub fn step(&mut self) {
        let _scope = creeer_core::profile::scope("physics");
        let found = self.detect_awake();
        let Plan { boxes, roots, need } = self.plan(&found);
        let n = need.iter().copied().max().unwrap_or(1);
        let saved = (!boxes.is_empty() && n < MAX_SUBSTEPS).then(|| {
            let bodies: Vec<Saved> = self.bodies.iter().map(Saved::of).collect();
            let multipliers: Vec<_> = self.joints.iter().map(|j| j.multipliers).collect();
            (bodies, self.anchors.clone(), self.left.clone(), multipliers)
        });
        let fast: Vec<bool> = roots.iter().map(|&r| need[r] > 1).collect();
        if n == 1 {
            self.step_found(&found);
        } else if (0..self.bodies.len()).all(|b| fast[b] || self.bodies[b].sleep.asleep) {
            self.step_in(n);
        } else {
            let once: Vec<Found> = found.into_iter().filter(|f| !fast[f.0]).collect();
            self.only = Some(fast.iter().map(|f| !f).collect());
            self.step_found(&once);
            self.only = Some(fast);
            self.step_in(n);
            self.only = None;
        }
        let Some((bodies, anchors, left, multipliers)) = saved else {
            return;
        };
        // Redone, group by group, where a box turned more than twice what its group's substeps
        // allow: those groups' bodies as they were, then stepped alone in the substeps the turn
        // needs (redone, the whole world took a play scene's tick to 79 ms, G1.8).
        let (mut turned, mut redo) = (0.0f64, vec![false; self.bodies.len()]);
        for &(i, node) in &boxes {
            let before = bodies[i].orientation(node);
            let after = self.bodies[i].structure.orientation[node];
            let turn = crate::so3::log(after * before.conjugate()).length();
            #[allow(clippy::cast_precision_loss)] // a few substeps
            let allowed = 2.0 * MAX_TURN * need[roots[i]] as f64;
            if turn > allowed {
                redo[roots[i]] = true;
                turned = turned.max(turn);
            }
        }
        let redo: Vec<bool> = roots.iter().map(|&r| redo[r]).collect();
        if !redo.contains(&true) {
            return;
        }
        for (b, (body, saved)) in self.bodies.iter_mut().zip(bodies).enumerate() {
            if redo[b] {
                saved.restore(body);
            }
        }
        self.anchors.retain(|a| !redo[a.a.0]);
        self.anchors
            .extend(anchors.into_iter().filter(|a| redo[a.a.0]));
        let inside = |side: Option<(usize, NodeId)>| side.is_some_and(|(b, _)| redo[b]);
        (self.left.energies).retain(|((a, b), _)| !inside(*a) && !inside(*b));
        (self.left.energies)
            .extend((left.energies.into_iter()).filter(|((a, b), _)| inside(*a) || inside(*b)));
        self.left.stepped.retain(|&b| !redo[b]);
        (self.left.stepped).extend(left.stepped.into_iter().filter(|&b| redo[b]));
        for (j, m) in self.joints.iter_mut().zip(multipliers) {
            if redo[j.a.0] {
                j.multipliers = m;
            }
        }
        crate::profile::add(crate::profile::Slot::TurnRedos, 1);
        self.only = Some(redo);
        self.step_in(substeps(turned));
        self.only = None;
    }

    /// Which boxes the step's turn limit watches, each body's group (its least body) and the
    /// substeps each group needs, from the contacts `found` at the step's start.
    pub(super) fn plan(&self, found: &[Found]) -> Plan {
        // How far a node's points can move in the step at their speed now.
        let travel = |(i, node): (usize, NodeId)| {
            let (s, shapes) = (&self.bodies[i].structure, &self.bodies[i].shapes);
            let radius = (shapes.iter())
                .filter(|(n, _)| *n == node)
                .map(|(_, shape)| shape.bounding_radius())
                .fold(0.0, f64::max);
            (s.velocity[node].length() + s.angular_velocity[node].length() * radius) * self.dt
        };
        // (The detection's margin is twice that and more: a hand flying past a crate 20 cm
        // away is no contact of the step's.) A body held in hands counts whatever it touches:
        // the hands' loads are explicit, a spring on a light body, and stay steady only in the
        // turn's substeps (a branch carried into a trunk spun up to 26 rad/s without, G1.8).
        let held = (self.bodies.iter().enumerate())
            .filter(|(_, b)| b.held)
            .flat_map(|(i, b)| b.shapes.iter().map(move |(node, _)| (i, *node)));
        let mut touching: Vec<(usize, NodeId)> = (found.iter())
            .filter(|(a, na, b, raw)| raw.gap() < REACH + travel((*a, *na)) + b.map_or(0.0, travel))
            .flat_map(|(a, na, b, _)| std::iter::once((*a, *na)).chain(*b))
            .chain(held)
            .collect();
        touching.sort_unstable();
        touching.dedup();
        let bodies = &self.bodies;
        let boxes: Vec<(usize, NodeId)> = touching
            .into_iter()
            .filter(|&(i, node)| {
                let body = &bodies[i];
                !body.sleep.asleep
                    && body.shapes.iter().any(|(n, shape)| {
                        *n == node && matches!(shape, Shape::Box { .. } | Shape::Hull(_))
                    })
            })
            .collect();
        // The substeps each group needs: its touching boxes' most. A group needing none steps
        // once while the others substep (one fast box made every group of a world substep,
        // G1.8): no contact joins two groups within the step, each found closed at its start.
        let roots = self.roots(found);
        let mut need = vec![1; self.bodies.len()];
        for &(i, node) in &boxes {
            let spin = self.bodies[i].structure.angular_velocity[node].length();
            let n = &mut need[roots[i]];
            *n = (*n).max(substeps(spin * self.dt));
        }
        Plan { boxes, roots, need }
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
        let only = self.only.clone();
        let stepped = |i: usize| only.as_ref().is_none_or(|only| only[i]);
        for (_, b) in self
            .bodies
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| stepped(*i))
        {
            b.solver.set_time_step(&b.structure, h);
        }
        self.dt = h;
        for _ in 0..n {
            self.step_once();
        }
        self.dt = dt;
        for (_, b) in self
            .bodies
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| stepped(*i))
        {
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

/// What a step's turn limit found ([`World::plan`]).
pub(super) struct Plan {
    /// The touching boxes and hulls awake: (body, node).
    pub(super) boxes: Vec<(usize, NodeId)>,
    /// Each body's group, as its least body.
    pub(super) roots: Vec<usize>,
    /// The substeps each group needs, by its least body.
    pub(super) need: Vec<usize>,
}
