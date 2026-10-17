//! A drivetrain mounted on a body's node (F6.4, ADR-0012): its housing turns with the node,
//! and its reaction is a torque on the node, so a motor spinning up a flywheel turns the
//! body the other way.
//!
//! **Angular momentum, exactly.** The drivetrain's reaction is its parts' change of angular
//! momentum, the housing's turning included as spin × L with the spin it starts the step
//! with; the housing really turns by what the body's step makes of it. The difference is
//! O(h) per step (0.2 % of a 10 kg·m²/s flywheel's momentum over ten seconds in a tumbling
//! station). So each step the mount compares the impulse it gave the body with what the
//! parts' momentum, seen from outside, actually changed by, and gives the difference with
//! the next step's torque: the body's and the parts' momentum together then stay what they
//! were, less only the one step's correction still to come ([`Mount::owed`]).

use creeer_math::glam::{DMat3, DVec3};
use creeer_mech::Drivetrain;
use creeer_physics::structure::NodeId;
use creeer_physics::world::World;

/// A drivetrain's housing fixed to a node of a body.
#[derive(Clone, Debug)]
pub struct Mount {
    /// The body (its index in the world) and its node the housing is fixed to.
    pub body: usize,
    pub node: NodeId,
    /// The correction still owed to the body (N·m, world frame, for the next step).
    carry: DVec3,
    /// This step's: the parts' momentum seen from outside at its start, the torque given.
    start: DVec3,
    given: DVec3,
}

impl Mount {
    /// A housing fixed to node `node` of body `body`.
    pub fn new(body: usize, node: NodeId) -> Self {
        Self {
            body,
            node,
            carry: DVec3::ZERO,
            start: DVec3::ZERO,
            given: DVec3::ZERO,
        }
    }

    fn turn(&self, world: &World) -> DMat3 {
        DMat3::from_quat(world.bodies[self.body].structure.orientation[self.node])
    }

    /// Before the drivetrain's step: the housing's spin in its own frame, to step it with.
    pub fn spin(&mut self, world: &World, d: &Drivetrain) -> DVec3 {
        let turn = self.turn(world);
        self.start = turn * d.momentum();
        turn.transpose() * world.bodies[self.body].structure.angular_velocity[self.node]
    }

    /// After the drivetrain's step, before the world's: its reaction (and the correction
    /// owed) put on the node, on top of what else acts there.
    pub fn push(&mut self, world: &mut World, d: &Drivetrain) {
        let turn = self.turn(world);
        self.given = turn * d.reaction() + self.carry;
        world.bodies[self.body].structure.torque[self.node] += self.given;
    }

    /// After the world's step of `h`: the torque taken off the node again, and what it
    /// gave set against what the parts' momentum changed by, the difference owed.
    pub fn settle(&mut self, world: &mut World, d: &Drivetrain, h: f64) {
        world.bodies[self.body].structure.torque[self.node] -= self.given;
        let now = self.turn(world) * d.momentum();
        // The body gained h·given (the correction owed included); it should have gained
        // −(now − start). What is still owed is what was, less what this step settled.
        self.carry -= (self.given * h + (now - self.start)) / h;
    }

    /// The angular impulse still owed to the body (kg·m²/s, world frame): the body's and the
    /// parts' momentum plus this is what they started with (and, for a body stepped by
    /// Newmark, plus the half of the last torque's impulse its step carries into the next:
    /// h/2·[`Mount::given`]).
    pub fn owed(&self, h: f64) -> DVec3 {
        self.carry * h
    }

    /// The torque it gave the node in the last step (N·m, world frame).
    pub fn given(&self) -> DVec3 {
        self.given
    }
}
