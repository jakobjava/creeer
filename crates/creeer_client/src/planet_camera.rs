//! Cameras placed on a planet: level with the horizon, looking a given way.

use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_render::Camera;

/// A camera at `position` looking along `forward`, level with the local horizon.
pub fn place(position: DVec3, forward: DVec3) -> Camera {
    place_up(position, forward, position.normalize())
}

/// A camera at `position` looking along `forward`, level with the horizon of `up` (for
/// frames not centred on a planet).
pub fn place_up(position: DVec3, forward: DVec3, up: DVec3) -> Camera {
    let mut camera = Camera::new(position);
    camera.level_to(up);
    // Yaw and pitch of `forward` in the horizon frame.
    let local = camera.frame.inverse() * forward.normalize();
    camera.yaw = det::atan2(-local.x, -local.z);
    camera.pitch = det::asin(local.y.clamp(-0.995, 0.995));
    camera
}
