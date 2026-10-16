//! Free-fly camera controls for the tech sandbox.
//!
//! WASD move, Space/Ctrl up/down, Shift ×10 speed, hold the right mouse button to look,
//! mouse wheel changes the base speed.

use creeer_math::det;

use creeer_render::Camera;
use std::f64::consts::FRAC_PI_2;
use winit::keyboard::KeyCode;

/// Radians of rotation per pixel of mouse movement.
const LOOK_SENSITIVITY: f64 = 0.0025;
/// Keeps the camera from flipping over the vertical.
const MAX_PITCH: f64 = FRAC_PI_2 - 0.01;

/// Accumulates input between frames and applies it to a [`Camera`].
#[derive(Debug)]
pub struct FlyCamera {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    fast: bool,
    /// Base movement speed (m/s).
    pub speed: f64,
    look_delta: (f64, f64),
}

impl Default for FlyCamera {
    fn default() -> Self {
        Self {
            forward: false,
            back: false,
            left: false,
            right: false,
            up: false,
            down: false,
            fast: false,
            speed: 10.0,
            look_delta: (0.0, 0.0),
        }
    }
}

impl FlyCamera {
    /// Records a key press or release. Returns whether the key is a camera control.
    pub fn key(&mut self, code: KeyCode, pressed: bool) -> bool {
        let flag = match code {
            KeyCode::KeyW => &mut self.forward,
            KeyCode::KeyS => &mut self.back,
            KeyCode::KeyA => &mut self.left,
            KeyCode::KeyD => &mut self.right,
            KeyCode::Space => &mut self.up,
            KeyCode::ControlLeft => &mut self.down,
            KeyCode::ShiftLeft => &mut self.fast,
            _ => return false,
        };
        *flag = pressed;
        true
    }

    /// Records relative mouse movement in pixels.
    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        self.look_delta.0 += dx;
        self.look_delta.1 += dy;
    }

    /// Mouse wheel: each notch changes the speed by 25 %.
    pub fn scroll(&mut self, notches: f64) {
        self.speed = (self.speed * det::pow(1.25, notches)).clamp(0.1, 1.0e6);
    }

    /// Releases all keys (e.g. when the window loses focus).
    pub fn release_all(&mut self) {
        *self = Self {
            speed: self.speed,
            ..Self::default()
        };
    }

    /// Applies accumulated look input and held keys for a frame lasting `dt` seconds.
    pub fn update(&mut self, camera: &mut Camera, dt: f64) {
        let (dx, dy) = std::mem::take(&mut self.look_delta);
        camera.yaw -= dx * LOOK_SENSITIVITY;
        camera.pitch = (camera.pitch - dy * LOOK_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);

        let axis = |positive: bool, negative: bool| {
            f64::from(u8::from(positive)) - f64::from(u8::from(negative))
        };
        let direction = camera.forward() * axis(self.forward, self.back)
            + camera.right() * axis(self.right, self.left)
            + camera.up() * axis(self.up, self.down);
        if let Some(direction) = direction.try_normalize() {
            let boost = if self.fast { 10.0 } else { 1.0 };
            camera.position += direction * self.speed * boost * dt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creeer_math::glam::DVec3;

    #[test]
    fn holding_w_moves_forward_at_speed() {
        let mut camera = Camera::new(DVec3::new(0.0, 500_000.0, 0.0));
        let mut fly = FlyCamera::default();
        fly.key(KeyCode::KeyW, true);
        fly.update(&mut camera, 0.5);
        let moved = camera.position - DVec3::new(0.0, 500_000.0, 0.0);
        assert!(
            (moved - DVec3::new(0.0, 0.0, -5.0)).length() < 1e-9,
            "{moved}"
        );
    }

    #[test]
    fn diagonal_movement_is_not_faster() {
        let mut camera = Camera::new(DVec3::ZERO);
        let mut fly = FlyCamera::default();
        fly.key(KeyCode::KeyW, true);
        fly.key(KeyCode::KeyD, true);
        fly.update(&mut camera, 1.0);
        assert!((camera.position.length() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn pitch_is_clamped_short_of_vertical() {
        let mut camera = Camera::new(DVec3::ZERO);
        let mut fly = FlyCamera::default();
        fly.mouse_motion(0.0, -1.0e6); // mouse far up
        fly.update(&mut camera, 0.0);
        assert!((camera.pitch - MAX_PITCH).abs() < 1e-12);
    }

    #[test]
    fn release_all_keeps_speed() {
        let mut fly = FlyCamera::default();
        fly.scroll(2.0);
        fly.key(KeyCode::KeyW, true);
        fly.release_all();
        assert!((fly.speed - 15.625).abs() < 1e-9);
        let mut camera = Camera::new(DVec3::ZERO);
        fly.update(&mut camera, 1.0);
        assert_eq!(camera.position, DVec3::ZERO);
    }
}
