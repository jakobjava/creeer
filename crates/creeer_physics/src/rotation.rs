//! Overall rotation of a deforming body ("shape matching").
//!
//! Given the rest positions and current positions of a body's nodes, find the rotation R
//! that best maps the rest shape onto the current one, ignoring deformation. Solvers use it
//! to keep working in the body's own frame: a factorised stiffness matrix of the rest shape
//! stays valid for a rotated body when forces are rotated into the body frame and back.
//!
//! R is the rotational part of the best-fit linear map F = A_pq·A_qq⁻¹ (Müller et al.,
//! "Meshless deformations based on shape matching", 2005), with
//! A_pq = Σ m·(x − c)(x₀ − c₀)ᵀ and A_qq = Σ m·(x₀ − c₀)(x₀ − c₀)ᵀ. Normalising by A_qq
//! makes F ≈ R for any body shape, so the rotation extraction of Müller et al., "A Robust
//! Method to Extract the Rotational Part of Deformations" (2016), converges equally fast for
//! long, flat and compact bodies. It is warm-started from the previous rotation and never
//! flips into a reflection.

use creeer_math::glam::{DMat3, DQuat, DVec3};

/// Tracks the overall rotation of one body.
#[derive(Clone, Debug)]
pub struct ShapeMatcher {
    /// Rest positions relative to the rest centre of mass.
    rest: Vec<DVec3>,
    mass: Vec<f64>,
    total_mass: f64,
    /// A_qq⁻¹ (regularised, so flat and line-like bodies work too).
    inv_rest_covariance: DMat3,
    rotation: DQuat,
}

/// Mass-weighted centre of `points`.
fn centre(points: &[DVec3], mass: &[f64], total: f64) -> DVec3 {
    points
        .iter()
        .zip(mass)
        .fold(DVec3::ZERO, |acc, (p, m)| acc + *p * *m)
        / total
}

/// Outer product p·qᵀ (column j is p·q[j]).
fn outer(p: DVec3, q: DVec3) -> DMat3 {
    DMat3::from_cols(p * q.x, p * q.y, p * q.z)
}

impl ShapeMatcher {
    /// Prepares tracking for a body with the given rest shape and node masses.
    pub fn new(rest: &[DVec3], mass: &[f64]) -> Self {
        let total_mass: f64 = mass.iter().sum();
        let c0 = centre(rest, mass, total_mass);
        let rest: Vec<DVec3> = rest.iter().map(|p| *p - c0).collect();
        let aqq = rest
            .iter()
            .zip(mass)
            .fold(DMat3::ZERO, |acc, (q, m)| acc + outer(*q * *m, *q));
        // A body without extent along some axis (a flat panel, a single rod) has a singular
        // A_qq; a tiny isotropic term keeps it invertible without affecting other bodies.
        let regularise = DMat3::IDENTITY * (1.0e-9 * (aqq.x_axis.x + aqq.y_axis.y + aqq.z_axis.z));
        Self {
            rest,
            mass: mass.to_vec(),
            total_mass,
            inv_rest_covariance: (aqq + regularise).inverse(),
            rotation: DQuat::IDENTITY,
        }
    }

    /// The rotation found by the last [`Self::update`].
    pub fn rotation(&self) -> DQuat {
        self.rotation
    }

    /// Finds the rotation of the body at `current` positions (warm-started from the last
    /// call) and returns it.
    pub fn update(&mut self, current: &[DVec3]) -> DQuat {
        let c = centre(current, &self.mass, self.total_mass);
        let apq = current
            .iter()
            .zip(&self.rest)
            .zip(&self.mass)
            .fold(DMat3::ZERO, |acc, ((x, q), m)| {
                acc + outer((*x - c) * *m, *q)
            });
        self.rotation = extract_rotation(apq * self.inv_rest_covariance, self.rotation, 20);
        self.rotation
    }
}

/// Refines `q` towards the rotational part of `a` (Müller et al. 2016), stopping early once
/// the correction is below 1e-12 rad.
pub fn extract_rotation(a: DMat3, q: DQuat, max_iterations: u32) -> DQuat {
    let mut q = q;
    for _ in 0..max_iterations {
        let r = DMat3::from_quat(q);
        let torque = r.x_axis.cross(a.x_axis) + r.y_axis.cross(a.y_axis) + r.z_axis.cross(a.z_axis);
        let alignment = (r.x_axis.dot(a.x_axis) + r.y_axis.dot(a.y_axis) + r.z_axis.dot(a.z_axis))
            .abs()
            + 1.0e-12;
        let omega = torque / alignment;
        let angle = omega.length();
        if angle < 1.0e-12 {
            break;
        }
        q = (DQuat::from_axis_angle(omega / angle, angle) * q).normalize();
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Angle between two rotations (rad). Unlike `DQuat::angle_between`, which goes through
    /// acos, this stays accurate for tiny angles.
    fn angle(a: DQuat, b: DQuat) -> f64 {
        2.0 * (a * b.conjugate()).xyz().length()
    }

    /// A box twice as long as it is wide, with uneven masses.
    fn body() -> (Vec<DVec3>, Vec<f64>) {
        let points = (0..8u8)
            .map(|i| {
                DVec3::new(
                    f64::from(i & 1),
                    f64::from((i >> 1) & 1),
                    f64::from(i >> 2) * 2.0,
                )
            })
            .collect();
        let mass = (0..8u8).map(|i| 1.0 + f64::from(i) * 0.3).collect();
        (points, mass)
    }

    #[test]
    fn recovers_a_large_rotation_of_a_rigid_body() {
        let (rest, mass) = body();
        let truth = DQuat::from_axis_angle(DVec3::new(1.0, 2.0, -0.5).normalize(), 2.5);
        let moved: Vec<DVec3> = rest
            .iter()
            .map(|p| truth * *p + DVec3::splat(7.0))
            .collect();
        let mut matcher = ShapeMatcher::new(&rest, &mass);
        // From a cold start a large rotation needs a few updates.
        for _ in 0..5 {
            matcher.update(&moved);
        }
        let error = angle(matcher.rotation(), truth);
        assert!(error < 1e-9, "error {error:e}");
    }

    #[test]
    fn ignores_deformation_without_rotation() {
        let (rest, mass) = body();
        let moved: Vec<DVec3> = rest
            .iter()
            .map(|p| *p * DVec3::new(1.2, 0.9, 1.05))
            .collect();
        let mut matcher = ShapeMatcher::new(&rest, &mass);
        let error = angle(matcher.update(&moved), DQuat::IDENTITY);
        // Not exactly zero: the tiny regularisation of A_qq tilts F by about 1e-11 rad.
        assert!(error < 1e-9, "error {error:e}");
    }

    #[test]
    fn tracks_a_spinning_body_one_tick_at_a_time() {
        let (rest, mass) = body();
        let axis = DVec3::new(0.3, 1.0, 0.2).normalize();
        let mut matcher = ShapeMatcher::new(&rest, &mass);
        // 3 rad/s at 60 Hz, i.e. 0.05 rad per update.
        for tick in 1..=300u32 {
            let truth = DQuat::from_axis_angle(axis, 3.0 * f64::from(tick) / 60.0);
            let moved: Vec<DVec3> = rest.iter().map(|p| truth * *p).collect();
            let error = angle(matcher.update(&moved), truth);
            assert!(error < 1e-9, "tick {tick}: {error:e}");
        }
    }

    #[test]
    fn works_for_a_flat_body() {
        let rest: Vec<DVec3> = (0..6u8)
            .map(|i| DVec3::new(f64::from(i % 3), f64::from(i / 3), 0.0))
            .collect();
        let mass = vec![2.0; rest.len()];
        let truth = DQuat::from_axis_angle(DVec3::new(0.2, -0.4, 1.0).normalize(), 0.3);
        let moved: Vec<DVec3> = rest.iter().map(|p| truth * *p).collect();
        let mut matcher = ShapeMatcher::new(&rest, &mass);
        let error = angle(matcher.update(&moved), truth);
        assert!(error < 1e-6, "error {error:e}");
    }
}
