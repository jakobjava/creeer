//! Grips (G2.10, ADR-0036): a hand holding a body firmly, a capped spring-damper from a point
//! of the body and its turn to where the hands mean them, moving, solved in the body's group's
//! minimisation as struts are (`joint/grip.rs`), not a load put on it before the step.
//!
//! The point is pulled by k·(target − p) + c·(target's velocity − p's), the pull at most
//! `most`: a vector Huber spring, convex, stiff within the cap and constant beyond it (a load
//! too heavy for the hands lags or sags, it is not dropped for it). The turn alike, by three
//! arms on the node's axes, [`ARM`] long, each pulled to the target turn's, their torque at
//! most `most_torque`. Its stiffness is set by a frequency, not N/m (k = m·ω²): the soft
//! arms before it (445 N/m) rang a 15 kg box at 0.87 Hz and the user saw it float.
//!
//! Its target moves (the hands carry it): its work is the hands', booked by the energy guard
//! as a moved joint point's is (`joint/project.rs`), and its spring stores none of the
//! group's energy. Grips are set for one step by whoever holds them and taken off after it;
//! they are not saved.

use super::World;
use super::joint::ARM;
use crate::structure::NodeId;
use creeer_math::glam::{DQuat, DVec3};

/// A grip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    /// The body and node held, and the point the hands hold it at (in the node's frame).
    pub a: (usize, NodeId),
    pub at_a: DVec3,
    /// Where the hands mean that point at the step's end (world), and how fast it moves.
    pub target: DVec3,
    pub target_velocity: DVec3,
    /// How they mean the node turned at the step's end, and its spin (rad/s).
    pub turn: DQuat,
    pub spin: DVec3,
    /// The point's stiffness (N/m), damping (N·s/m) and most pull (N).
    pub stiffness: f64,
    pub damping: f64,
    pub most: f64,
    /// Each arm's stiffness (N/m) and damping (N·s/m), and the most torque (N·m).
    pub turn_stiffness: f64,
    pub turn_damping: f64,
    pub most_torque: f64,
}

impl Grip {
    /// The node's arms in the world for its turn `q`.
    pub(crate) fn arms(q: DQuat) -> [DVec3; 3] {
        [DVec3::X, DVec3::Y, DVec3::Z].map(|e| q * (e * ARM))
    }

    /// The most pull on each arm: the most torque shared by the two arms that turn about an
    /// axis, each [`ARM`] out.
    pub(crate) fn most_arm(&self) -> f64 {
        self.most_torque / (2.0 * ARM)
    }

    /// What the hands pull the body with as it stands and moves now (after a step: at its
    /// end, the step's own force): the force at the grip's point and the torque about the
    /// node (N, N·m).
    pub fn pull(&self, world: &World) -> (DVec3, DVec3) {
        let s = &world.bodies[self.a.0].structure;
        let (x, q) = (s.position[self.a.1], s.orientation[self.a.1]);
        let (v, w) = (s.velocity[self.a.1], s.angular_velocity[self.a.1]);
        let arm = q * self.at_a;
        let (p, vp) = (x + arm, v + w.cross(arm));
        let force = (self.stiffness * (self.target - p)
            + self.damping * (self.target_velocity - vp))
            .clamp_length_max(self.most);
        let mut torque = arm.cross(force);
        for (own, want) in Self::arms(q).into_iter().zip(Self::arms(self.turn)) {
            let pull = (self.turn_stiffness * (want - own)
                + self.turn_damping * (self.spin.cross(want) - w.cross(own)))
            .clamp_length_max(self.most_arm());
            torque += own.cross(pull);
        }
        (force, torque)
    }
}

impl World {
    /// The grips for the next step: kept until their owner takes them off.
    pub fn grips(&self) -> &[Grip] {
        &self.grips
    }

    /// Sets a grip for the next step (its body woken); returns its index.
    pub fn add_grip(&mut self, grip: Grip) -> usize {
        self.bodies[grip.a.0].wake();
        self.grips.push(grip);
        self.grips.len() - 1
    }

    /// Takes every grip off.
    pub fn clear_grips(&mut self) {
        self.grips.clear();
    }

    /// The grips renumbered to the bodies' new indices (`map`), those on a body removed
    /// dropped.
    pub(super) fn renumber_grips(&mut self, map: &[Option<usize>]) {
        self.grips.retain_mut(|g| {
            let Some(Some(a)) = map.get(g.a.0).copied() else {
                return false;
            };
            g.a.0 = a;
            true
        });
    }
}
