//! Conversion from f64 simulation space to f32 camera-relative render space.
//!
//! GPUs work in f32, which has 24 bits of mantissa: at 500 km from the origin the spacing
//! between representable values is about 3 cm, so vertices visibly jitter. The fix is to
//! subtract the camera position *in f64* first. Everything the player can see up close is
//! then near zero, where f32 is very precise (about 6 µm at 100 m).

use crate::Frame64;
use glam::{DVec3, Mat4, Vec3};

/// Converts a simulation position (f64) into render space (f32, relative to the camera).
#[inline]
pub fn to_render_space(world: DVec3, camera: DVec3) -> Vec3 {
    (world - camera).as_vec3()
}

/// Model matrix for an object at `frame`, as seen from a camera at `camera` (f64 world).
pub fn model_matrix(frame: &Frame64, camera: DVec3) -> Mat4 {
    Mat4::from_rotation_translation(
        frame.rotation.as_quat(),
        to_render_space(frame.position, camera),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DQuat;

    /// Camera standing on the surface of a 500 km-radius planet.
    const CAMERA: DVec3 = DVec3::new(0.0, 500_000.0, 0.0);

    fn render_error(world: DVec3) -> f64 {
        let exact = world - CAMERA;
        (to_render_space(world, CAMERA).as_dvec3() - exact).length()
    }

    /// The plan's F0 requirement: render error below 1 mm at a 500 km offset.
    #[test]
    fn nearby_objects_are_sub_millimetre_precise_far_from_origin() {
        for offset in [
            DVec3::new(0.1, 0.0, 0.0),
            DVec3::new(1.234_567, -0.5, 2.25),
            DVec3::new(100.0, 3.0, -42.0),
            DVec3::new(1_000.0, 250.0, 999.0),
            DVec3::new(10_000.0, -10_000.0, 5_000.0),
        ] {
            let error = render_error(CAMERA + offset);
            assert!(error < 1e-3, "offset {offset}: error {error} m");
        }
    }

    /// Documents why camera-relative rendering exists: converting to f32 *before*
    /// subtracting the camera loses centimetres at planetary distances.
    #[test]
    fn naive_f32_conversion_would_jitter() {
        let world = CAMERA + DVec3::new(0.01, 0.01, 0.0);
        let naive = (world.as_vec3() - CAMERA.as_vec3()).as_dvec3();
        let naive_error = (naive - (world - CAMERA)).length();
        assert!(naive_error > 1e-3, "naive error was only {naive_error} m");
        assert!(render_error(world) < 1e-9);
    }

    #[test]
    fn model_matrix_places_object_relative_to_camera() {
        let frame = Frame64::new(
            CAMERA + DVec3::new(5.0, 0.0, 0.0),
            DQuat::from_rotation_y(0.5),
        );
        let m = model_matrix(&frame, CAMERA);
        let origin = m.transform_point3(Vec3::ZERO);
        assert!((origin - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-6);
    }
}
