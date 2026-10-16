//! Free camera with an f64 position and a reversed-Z infinite projection.

use creeer_math::glam::camera::rh::proj::directx;
use creeer_math::glam::{DQuat, DVec3, EulerRot, Mat4};

/// A camera in simulation space. Conventions: right-handed, looking down −Z of its horizon
/// frame when `yaw == pitch == 0`. The horizon frame (`frame`) says which way is up: the
/// identity for flat scenes (+Y up), the local vertical on a planet.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Position in simulation space (m, f64).
    pub position: DVec3,
    /// Rotation about +Y in radians; positive turns left.
    pub yaw: f64,
    /// Rotation about the camera's right axis in radians; positive looks up.
    pub pitch: f64,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Distance of the near clipping plane (m). There is no far plane.
    pub near: f32,
    /// Rotation from the world to the camera's horizon frame (its +Y is "up").
    pub frame: DQuat,
}

impl Camera {
    /// A camera at `position` looking down −Z with a 70° vertical field of view.
    pub fn new(position: DVec3) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 70f32.to_radians(),
            near: 0.05,
            frame: DQuat::IDENTITY,
        }
    }

    /// Orientation of the camera in simulation space.
    pub fn rotation(&self) -> DQuat {
        self.frame * DQuat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }

    /// Up in the camera's horizon frame.
    pub fn up(&self) -> DVec3 {
        self.frame * DVec3::Y
    }

    /// Turns the horizon frame so that `up` is up, by the smallest rotation: moving over a
    /// planet, the view keeps its heading and the horizon stays level.
    pub fn level_to(&mut self, up: DVec3) {
        let turn = DQuat::from_rotation_arc(self.up(), up.normalize());
        self.frame = (turn * self.frame).normalize();
    }

    /// Unit vector the camera looks along.
    pub fn forward(&self) -> DVec3 {
        self.rotation() * DVec3::NEG_Z
    }

    /// Unit vector to the camera's right.
    pub fn right(&self) -> DVec3 {
        self.rotation() * DVec3::X
    }

    /// Reversed-Z infinite perspective projection.
    ///
    /// Depth 1 is the near plane and depth approaches 0 at infinity. Floating-point values
    /// are densest near 0, which reversed-Z pairs with the far distance, so depth precision
    /// stays good from 5 cm to planetary distances (no z-fighting on far mountains).
    pub fn projection(&self, aspect: f32) -> Mat4 {
        // wgpu uses DirectX-style NDC: Y up, depth in [0, 1].
        directx::perspective_infinite_reverse(self.fov_y, aspect, self.near)
    }

    /// View-projection matrix for camera-relative render space, where the camera sits at
    /// the origin (see `creeer_math::render_space`). Only the rotation is needed.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * Mat4::from_quat(self.rotation().as_quat().conjugate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use creeer_math::glam::{Vec3, Vec4};
    use std::f64::consts::FRAC_PI_2;

    fn assert_vec_close(a: DVec3, b: DVec3) {
        assert!((a - b).length() < 1e-12, "{a} vs {b}");
    }

    #[test]
    fn default_orientation_looks_down_negative_z() {
        let camera = Camera::new(DVec3::ZERO);
        assert_vec_close(camera.forward(), DVec3::NEG_Z);
        assert_vec_close(camera.right(), DVec3::X);
    }

    #[test]
    fn yaw_turns_left_and_pitch_looks_up() {
        let mut camera = Camera::new(DVec3::ZERO);
        camera.yaw = FRAC_PI_2;
        assert_vec_close(camera.forward(), DVec3::NEG_X);
        camera.yaw = 0.0;
        camera.pitch = FRAC_PI_2;
        assert_vec_close(camera.forward(), DVec3::Y);
    }

    fn ndc_depth(camera: &Camera, render_space_point: Vec3) -> f32 {
        let clip = camera.view_projection(16.0 / 9.0) * render_space_point.extend(1.0);
        clip.z / clip.w
    }

    #[test]
    fn reversed_z_maps_near_plane_to_one_and_distance_towards_zero() {
        let camera = Camera::new(DVec3::ZERO);
        assert_relative_eq!(
            ndc_depth(&camera, Vec3::new(0.0, 0.0, -camera.near)),
            1.0,
            epsilon = 1e-6
        );
        // Depth must strictly decrease with distance, all the way to planetary scale.
        let mut previous = 1.0;
        for distance in [1.0, 10.0, 1e3, 1e5, 1e6, 1e7] {
            let depth = ndc_depth(&camera, Vec3::new(0.0, 0.0, -distance));
            assert!(
                depth > 0.0 && depth < previous,
                "{distance} m -> depth {depth}"
            );
            previous = depth;
        }
    }

    #[test]
    fn a_levelled_camera_keeps_its_heading() {
        let mut camera = Camera::new(DVec3::ZERO);
        camera.yaw = 0.3;
        let up = DVec3::new(0.0, 0.8, 0.6).normalize();
        camera.level_to(up);
        assert_vec_close(camera.up(), up);
        // Forward stays horizontal in the new frame, and level_to is idempotent.
        assert!(camera.forward().dot(up).abs() < 1e-12);
        let forward = camera.forward();
        camera.level_to(up);
        assert_vec_close(camera.forward(), forward);
    }

    #[test]
    fn points_behind_the_camera_are_clipped() {
        let camera = Camera::new(DVec3::ZERO);
        let clip = camera.view_projection(1.0) * Vec4::new(0.0, 0.0, 10.0, 1.0);
        assert!(
            clip.w < 0.0,
            "w = {} should be negative behind the camera",
            clip.w
        );
    }
}
