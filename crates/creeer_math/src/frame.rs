//! Rigid reference frames in double precision.

use glam::{DQuat, DVec3};
use std::ops::Mul;

/// A rigid reference frame: the pose of a local coordinate system inside its parent.
///
/// `transform_point` maps local coordinates into parent coordinates. Frames compose like
/// matrices: `(a * b).transform_point(p) == a.transform_point(b.transform_point(p))`, so
/// `planet_frame * vessel_frame` maps vessel-local points into the planet's parent frame.
///
/// All values are SI (metres) and f64: at 500 km from the origin, f64 still resolves
/// about 0.1 nm, while f32 would only resolve about 3 cm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame64 {
    /// Origin of the local frame, in parent coordinates (m).
    pub position: DVec3,
    /// Orientation of the local frame relative to the parent. Must be normalized.
    pub rotation: DQuat,
}

impl Frame64 {
    /// The frame that coincides with its parent.
    pub const IDENTITY: Self = Self {
        position: DVec3::ZERO,
        rotation: DQuat::IDENTITY,
    };

    /// Creates a frame from its origin and orientation (normalized).
    pub fn new(position: DVec3, rotation: DQuat) -> Self {
        Self { position, rotation }
    }

    /// A frame that is only translated relative to its parent.
    pub fn from_position(position: DVec3) -> Self {
        Self::new(position, DQuat::IDENTITY)
    }

    /// Maps a point from local to parent coordinates.
    #[inline]
    pub fn transform_point(&self, local: DVec3) -> DVec3 {
        self.rotation * local + self.position
    }

    /// Maps a direction (velocity, force, ...) from local to parent coordinates.
    /// Directions are rotated but not translated.
    #[inline]
    pub fn transform_vector(&self, local: DVec3) -> DVec3 {
        self.rotation * local
    }

    /// Maps a point from parent to local coordinates.
    #[inline]
    pub fn inverse_transform_point(&self, parent: DVec3) -> DVec3 {
        self.rotation.conjugate() * (parent - self.position)
    }

    /// Maps a direction from parent to local coordinates.
    #[inline]
    pub fn inverse_transform_vector(&self, parent: DVec3) -> DVec3 {
        self.rotation.conjugate() * parent
    }

    /// The frame that undoes this one: `f.inverse() * f == IDENTITY`.
    pub fn inverse(&self) -> Self {
        let rotation = self.rotation.conjugate();
        Self {
            position: rotation * -self.position,
            rotation,
        }
    }
}

impl Default for Frame64 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Frame64 {
    type Output = Self;

    /// Composes `self` (outer) with `child` (inner).
    fn mul(self, child: Self) -> Self {
        Self {
            position: self.transform_point(child.position),
            // Renormalising keeps rounding error from accumulating over long chains.
            rotation: (self.rotation * child.rotation).normalize(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use proptest::prelude::*;
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn rotation_then_translation() {
        // Local frame 10 m up the x axis, rotated 90° about z.
        let frame = Frame64::new(
            DVec3::new(10.0, 0.0, 0.0),
            DQuat::from_rotation_z(FRAC_PI_2),
        );
        let p = frame.transform_point(DVec3::new(1.0, 0.0, 0.0));
        assert_relative_eq!(p.x, 10.0, epsilon = 1e-12);
        assert_relative_eq!(p.y, 1.0, epsilon = 1e-12);
        assert_relative_eq!(p.z, 0.0, epsilon = 1e-12);
        // Directions ignore the translation.
        let v = frame.transform_vector(DVec3::X);
        assert_relative_eq!(v.y, 1.0, epsilon = 1e-12);
        assert_relative_eq!(v.x, 0.0, epsilon = 1e-12);
    }

    fn arb_vec(scale: f64) -> impl Strategy<Value = DVec3> {
        (-scale..scale, -scale..scale, -scale..scale).prop_map(|(x, y, z)| DVec3::new(x, y, z))
    }

    fn arb_rotation() -> impl Strategy<Value = DQuat> {
        (arb_vec(1.0), -10.0..10.0f64).prop_filter_map("degenerate axis", |(axis, angle)| {
            (axis.length() > 1e-3).then(|| DQuat::from_axis_angle(axis.normalize(), angle))
        })
    }

    /// Up to ±1e9 m covers a solar-system-sized frame hierarchy.
    fn arb_frame() -> impl Strategy<Value = Frame64> {
        (arb_vec(1e9), arb_rotation()).prop_map(|(p, r)| Frame64::new(p, r))
    }

    fn assert_close(a: DVec3, b: DVec3, scale: f64) {
        // f64 has ~16 significant digits; allow a few rounding steps at the given scale.
        let tolerance = 1e-14 * scale.max(1.0) * 16.0;
        assert!(
            (a - b).length() <= tolerance,
            "{a} vs {b}: error {} > {tolerance}",
            (a - b).length()
        );
    }

    proptest! {
        #[test]
        fn inverse_transform_undoes_transform(frame in arb_frame(), p in arb_vec(1e9)) {
            let back = frame.inverse_transform_point(frame.transform_point(p));
            assert_close(back, p, frame.position.length() + p.length());
        }

        #[test]
        fn inverse_frame_composes_to_identity(frame in arb_frame(), p in arb_vec(1e9)) {
            let round_trip = (frame.inverse() * frame).transform_point(p);
            assert_close(round_trip, p, frame.position.length() + p.length());
        }

        #[test]
        fn composition_matches_nested_application(
            outer in arb_frame(), inner in arb_frame(), p in arb_vec(1e9)
        ) {
            let composed = (outer * inner).transform_point(p);
            let nested = outer.transform_point(inner.transform_point(p));
            let scale = outer.position.length() + inner.position.length() + p.length();
            assert_close(composed, nested, scale);
        }

        #[test]
        fn rotation_preserves_distances(frame in arb_frame(), a in arb_vec(1e3), b in arb_vec(1e3)) {
            let local = (a - b).length();
            let parent = (frame.transform_point(a) - frame.transform_point(b)).length();
            // Absolute error grows with the frame's offset, since both points pass through it.
            let tolerance = 1e-13 * frame.position.length().max(1.0) * 16.0;
            prop_assert!((local - parent).abs() <= tolerance);
        }
    }
}
