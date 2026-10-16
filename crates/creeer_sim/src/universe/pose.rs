//! Where a frame is in inertial space at one instant.

use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::world::frame::FrameChange;

/// A frame's origin, axes and their motion, in inertial (barycentric) coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Origin (m).
    pub position: DVec3,
    /// Velocity of the origin (m/s).
    pub velocity: DVec3,
    /// Frame axes → inertial axes.
    pub rotation: DQuat,
    /// Angular velocity of the axes, inertial axes (rad/s).
    pub spin: DVec3,
}

impl Pose {
    /// A non-rotating frame at `position` moving at `velocity`.
    pub fn inertial(position: DVec3, velocity: DVec3) -> Self {
        Self {
            position,
            velocity,
            rotation: DQuat::IDENTITY,
            spin: DVec3::ZERO,
        }
    }

    /// Inertial position and velocity of a point at `x` moving at `v` in this frame.
    pub fn to_inertial(&self, x: DVec3, v: DVec3) -> (DVec3, DVec3) {
        let r = self.rotation * x;
        (
            self.position + r,
            self.velocity + self.rotation * v + self.spin.cross(r),
        )
    }

    /// Frame coordinates of an inertial position and velocity.
    pub fn from_inertial(&self, x: DVec3, v: DVec3) -> (DVec3, DVec3) {
        let r = x - self.position;
        let back = self.rotation.conjugate();
        (back * r, back * (v - self.velocity - self.spin.cross(r)))
    }

    /// Frame coordinates of an inertial position only.
    pub fn point_from_inertial(&self, x: DVec3) -> DVec3 {
        self.rotation.conjugate() * (x - self.position)
    }

    /// Inertial orientation of something oriented `q` in this frame.
    pub fn orientation_to_inertial(&self, q: DQuat) -> DQuat {
        (self.rotation * q).normalize()
    }

    /// The change of coordinates from this frame to `new` (for `World::change_frame`).
    pub fn change_to(&self, new: &Self) -> FrameChange {
        let back = new.rotation.conjugate();
        let offset = self.position - new.position;
        FrameChange {
            rotation: (back * self.rotation).normalize(),
            translation: back * offset,
            velocity: back * (self.velocity - new.velocity - new.spin.cross(offset)),
            spin: back * (self.spin - new.spin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turning(position: DVec3, angle: f64) -> Pose {
        Pose {
            position,
            velocity: DVec3::new(3e4, -1e3, 20.0),
            rotation: DQuat::from_axis_angle(DVec3::new(0.3, 1.0, 0.1).normalize(), angle),
            spin: DVec3::new(0.0, 2.9e-4, 1e-5),
        }
    }

    #[test]
    fn a_change_agrees_with_going_through_inertial_coordinates() {
        let a = turning(DVec3::new(1.36e10, 2e7, -5e6), 0.7);
        let b = Pose::inertial(
            DVec3::new(1.36e10, 2.01e7, -5.2e6),
            DVec3::new(3.1e4, 0.0, 0.0),
        );
        let (x, v) = (DVec3::new(5.1e5, 2e3, -7e3), DVec3::new(10.0, -3.0, 2200.0));
        let (xi, vi) = a.to_inertial(x, v);
        let (xb, vb) = b.from_inertial(xi, vi);
        let change = a.change_to(&b);
        assert!((change.point(x) - xb).length() < 1e-5);
        assert!((change.velocity(x, v) - vb).length() < 1e-9);
        let (back_x, back_v) = a.from_inertial(xi, vi);
        assert!((back_x - x).length() < 1e-5);
        assert!((back_v - v).length() < 1e-9);
    }
}
