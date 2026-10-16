//! Rotations as vectors (axis × angle) and back: the exponential and logarithm maps of
//! SO(3), written to stay accurate for tiny angles (where `acos`-based formulas lose half
//! their digits) and deterministic on every platform (`creeer_math::det`).

use creeer_math::det;
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// The rotation by angle |φ| about axis φ/|φ|.
pub fn exp(phi: DVec3) -> DQuat {
    let angle = phi.length();
    let (sin, cos) = det::sin_cos(0.5 * angle);
    // sin(θ/2)/θ, with its series near zero (the direct formula would divide 0 by 0).
    let scale = if angle < 1.0e-6 {
        0.5 - angle * angle / 48.0
    } else {
        sin / angle
    };
    let v = phi * scale;
    DQuat::from_xyzw(v.x, v.y, v.z, cos)
}

/// The rotation vector φ with `exp(φ) = q`, angle in [0, π].
pub fn log(q: DQuat) -> DVec3 {
    // q and −q are the same rotation; pick the one with the shorter angle.
    let q = if q.w < 0.0 { -q } else { q };
    let v = DVec3::new(q.x, q.y, q.z);
    let s = v.length();
    if s == 0.0 {
        return DVec3::ZERO;
    }
    // atan2 keeps full relative precision for small s, unlike acos(w).
    v * (2.0 * det::atan2(s, q.w) / s)
}

/// Skew-symmetric matrix [φ]× with [φ]×·v = φ × v.
pub fn skew(phi: DVec3) -> DMat3 {
    DMat3::from_cols(
        DVec3::new(0.0, phi.z, -phi.y),
        DVec3::new(-phi.z, 0.0, phi.x),
        DVec3::new(phi.y, -phi.x, 0.0),
    )
}

/// Inverse of the left Jacobian of SO(3): how log(exp(δ)·exp(φ)) changes with a small δ,
/// i.e. log(exp(δ)·exp(φ)) ≈ φ + J⁻¹(φ)·δ.
pub fn inverse_left_jacobian(phi: DVec3) -> DMat3 {
    let angle = phi.length();
    let k = skew(phi);
    // (1/θ² − (1 + cos θ)/(2θ·sin θ)), with its series 1/12 + θ²/720 near zero.
    let c = if angle < 1.0e-3 {
        1.0 / 12.0 + angle * angle / 720.0
    } else {
        let (sin, cos) = det::sin_cos(angle);
        1.0 / (angle * angle) - (1.0 + cos) / (2.0 * angle * sin)
    };
    DMat3::IDENTITY - k * 0.5 + k * k * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_and_log_are_inverse() {
        for phi in [
            DVec3::new(0.3, -1.2, 0.7),
            DVec3::new(1e-9, 2e-9, -3e-9),
            DVec3::new(0.0, 0.0, 3.0),
            DVec3::ZERO,
        ] {
            let back = log(exp(phi));
            assert!(
                (back - phi).length() <= 1e-15 * (1.0 + phi.length()),
                "{phi} -> {back}"
            );
        }
    }

    #[test]
    fn exp_matches_axis_angle() {
        let phi = DVec3::new(0.2, 0.5, -0.4);
        let expected = DQuat::from_axis_angle(phi.normalize(), phi.length());
        assert!((exp(phi) - expected).length() < 1e-15);
    }

    #[test]
    fn log_keeps_full_precision_for_tiny_angles() {
        let phi = DVec3::new(3e-12, -1e-12, 2e-12);
        let back = log(exp(phi));
        assert!((back - phi).length() < 1e-26, "{back}");
    }

    /// Checks the defining property against finite differences.
    #[test]
    fn inverse_left_jacobian_predicts_composition() {
        for phi in [DVec3::new(0.4, -0.9, 1.3), DVec3::new(1e-5, 0.0, 2e-5)] {
            let delta = DVec3::new(1e-7, -2e-7, 1.5e-7);
            let exact = log(exp(delta) * exp(phi)) - phi;
            let predicted = inverse_left_jacobian(phi) * delta;
            assert!(
                (exact - predicted).length() < 1e-13,
                "{exact} vs {predicted}"
            );
        }
    }
}
