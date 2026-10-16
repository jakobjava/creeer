//! Moving a world into another frame: from a planet's rotating frame into a free-falling
//! one when ships leave the ground behind, or to a new floating origin next to them
//! (ADR-0007).
//!
//! Every body keeps its physical state; only the coordinates change. A point fixed in the
//! old frame at x moves in the new one with the old frame's origin and turns with its
//! rotation: x' = R·x + t, v' = R·v + Ω×(R·x) + u, orientation q' = R·q and angular velocity
//! ω' = R·ω + Ω, where Ω is the old frame's angular velocity seen from the new one.

use super::{Body, World};
use crate::gravity::PlanetGravity;
use creeer_math::glam::{DQuat, DVec3};

/// The old frame seen from the new one at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameChange {
    /// Rotation from old axes to new axes.
    pub rotation: DQuat,
    /// The old origin in new coordinates (m).
    pub translation: DVec3,
    /// The old origin's velocity in the new frame (m/s).
    pub velocity: DVec3,
    /// The old frame's angular velocity relative to the new one, in new axes (rad/s).
    pub spin: DVec3,
}

impl FrameChange {
    /// A pure shift of the origin by `offset` (the old origin lies at `offset` in the new
    /// coordinates) and of velocities by `velocity`.
    pub fn shift(offset: DVec3, velocity: DVec3) -> Self {
        Self {
            rotation: DQuat::IDENTITY,
            translation: offset,
            velocity,
            spin: DVec3::ZERO,
        }
    }

    /// A position in new coordinates.
    pub fn point(&self, x: DVec3) -> DVec3 {
        self.rotation * x + self.translation
    }

    /// The velocity in the new frame of something at `x` moving at `v` in the old one.
    pub fn velocity(&self, x: DVec3, v: DVec3) -> DVec3 {
        self.rotation * v + self.spin.cross(self.rotation * x) + self.velocity
    }

    /// The change back.
    pub fn inverse(&self) -> Self {
        let back = self.rotation.conjugate();
        let spin = -(back * self.spin);
        let translation = -(back * self.translation);
        // The new origin seen from the old frame: at −R⁻¹t, moving so that applying both
        // changes leaves velocities unchanged.
        let velocity = -(back * self.velocity) + spin.cross(translation);
        Self {
            rotation: back,
            translation,
            velocity,
            spin,
        }
    }
}

impl Body {
    /// Moves the body into another frame.
    pub fn change_frame(&mut self, change: &FrameChange) {
        let s = &mut self.structure;
        for i in 0..s.node_count() {
            let x = s.position[i];
            s.velocity[i] = change.velocity(x, s.velocity[i]);
            s.position[i] = change.point(x);
            s.rest[i] = change.point(s.rest[i]);
            s.orientation[i] = (change.rotation * s.orientation[i]).normalize();
            s.angular_velocity[i] = change.rotation * s.angular_velocity[i] + change.spin;
            s.load[i] = change.rotation * s.load[i];
            s.torque[i] = change.rotation * s.torque[i];
        }
        s.gravity = change.rotation * s.gravity;
        self.solver.reset_history(&self.structure);
        self.wake();
    }
}

impl World {
    /// Moves every body into another frame. Contacts and friction anchors are dropped (they
    /// are found again in the next step); fixed geometry and the ground stay where they
    /// are, so this is meant for worlds without them (ships in space).
    pub fn change_frame(&mut self, change: &FrameChange) {
        for body in &mut self.bodies {
            body.change_frame(change);
        }
        self.contacts.clear();
        self.groups.clear();
        self.anchors.clear();
    }

    /// Replaces the gravity of the world and of every body in it (tidal sources move from
    /// tick to tick).
    pub fn set_planet(&mut self, planet: Option<PlanetGravity>) {
        for body in &mut self.bodies {
            if planet.is_some() {
                body.structure.gravity = DVec3::ZERO;
            }
            body.structure.planet.clone_from(&planet);
        }
        self.planet = planet;
    }
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
