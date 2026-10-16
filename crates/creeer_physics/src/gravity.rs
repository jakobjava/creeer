//! Gravity near a planet, in the planet's rotating frame.
//!
//! The simulation near a planet's surface runs in a frame that turns with the planet
//! (plan.md §1, principle 3): the ground stands still in it. That frame is not inertial, so
//! besides the planet's attraction two fictitious accelerations act:
//! - **centrifugal**, −ω×(ω×r): conservative, part of the effective potential
//!   Φ(r) = −μ/|r| − ½·|ω×r|²; it lowers effective gravity at the equator and makes the
//!   vertical of a plumb line deviate slightly from the direction to the centre;
//! - **Coriolis**, −2·ω×v: depends on velocity and does no work, so it has no potential. The
//!   implicit solver applies it as a force from the velocity at the start of each step (the
//!   error is of order (ω·h)², 10⁻¹¹ for a 6-hour day at 60 Hz).
//!
//! The planet is a point mass (μ = G·M) at the origin; its oblateness and mass
//! concentrations are not modelled.

use creeer_math::glam::DVec3;

/// Gravity of a planet seen from its rotating frame (planet centre at the origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetGravity {
    /// Gravitational parameter μ = G·M (m³/s²).
    pub mu: f64,
    /// Angular velocity of the frame (rad/s): along the rotation axis.
    pub spin: DVec3,
}

impl PlanetGravity {
    /// Effective potential per unit mass (J/kg): −μ/|x| − ½·|ω×x|².
    pub fn potential(&self, x: DVec3) -> f64 {
        -self.mu / x.length() - 0.5 * self.spin.cross(x).length_squared()
    }

    /// Φ(x) − Φ(y) (J/kg), without the cancellation of subtracting two potentials of
    /// 5 MJ/kg that differ by millijoules (one step of motion on a 500 km planet).
    pub fn potential_difference(&self, x: DVec3, y: DVec3) -> f64 {
        let (rx, ry) = (x.length(), y.length());
        // |y| − |x| = (y − x)·(y + x) / (|x| + |y|)
        let dr = (y - x).dot(y + x) / (rx + ry);
        let newton = -self.mu * dr / (rx * ry);
        // |ω×x|² − |ω×y|² = (ω×(x − y))·(ω×(x + y))
        let centrifugal = -0.5 * self.spin.cross(x - y).dot(self.spin.cross(x + y));
        newton + centrifugal
    }

    /// Acceleration of gravity plus the centrifugal one at `x` (m/s²): −∇Φ.
    pub fn acceleration(&self, x: DVec3) -> DVec3 {
        let r = x.length();
        -x * (self.mu / (r * r * r)) - self.spin.cross(self.spin.cross(x))
    }

    /// Coriolis acceleration of a body moving at `v` in the rotating frame (m/s²).
    pub fn coriolis(&self, v: DVec3) -> DVec3 {
        -2.0 * self.spin.cross(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PlanetGravity {
        // 500 km, 9.81 m/s² at the datum, a 6-hour day about +Y.
        PlanetGravity {
            mu: 9.81 * 500_000.0 * 500_000.0,
            spin: DVec3::Y * (std::f64::consts::TAU / 21_600.0),
        }
    }

    #[test]
    fn surface_gravity_and_the_centrifugal_reduction() {
        let g = home();
        let pole = g.acceleration(DVec3::Y * 500_000.0);
        assert!((pole.length() - 9.81).abs() < 1e-9);
        assert!(pole.normalize().dot(DVec3::NEG_Y) > 1.0 - 1e-15);
        // At the equator the centrifugal acceleration ω²R = 0.0423 m/s² points outward.
        let equator = g.acceleration(DVec3::X * 500_000.0);
        let omega = std::f64::consts::TAU / 21_600.0;
        assert!((equator.length() - (9.81 - omega * omega * 500_000.0)).abs() < 1e-9);
    }

    #[test]
    fn acceleration_is_minus_the_gradient_of_the_potential() {
        let g = home();
        let x = DVec3::new(210_000.0, 380_000.0, -240_000.0);
        let h = 0.5;
        let numeric = DVec3::new(
            g.potential_difference(x + DVec3::X * h, x - DVec3::X * h),
            g.potential_difference(x + DVec3::Y * h, x - DVec3::Y * h),
            g.potential_difference(x + DVec3::Z * h, x - DVec3::Z * h),
        ) / (2.0 * h);
        assert!((numeric + g.acceleration(x)).length() < 1e-9);
    }

    #[test]
    fn potential_differences_are_free_of_cancellation() {
        let g = home();
        let x = DVec3::new(0.0, 500_123.0, 0.0);
        let y = x + DVec3::new(1e-4, 2e-3, -5e-5);
        // For a tiny step the midpoint rule ΔΦ ≈ −a((x + y)/2)·(y − x) is exact to third
        // order (an error of ~10⁻²⁰ J/kg here).
        let expected = -g.acceleration((x + y) * 0.5).dot(y - x);
        let difference = g.potential_difference(y, x);
        assert!(
            (difference - expected).abs() < 1e-15,
            "{difference} vs {expected}"
        );
        // Subtracting the two 4.9 MJ/kg potentials directly loses about 10 digits.
        let naive = g.potential(y) - g.potential(x);
        assert!(
            (naive - expected).abs() > 1e-12,
            "the naive difference is not exact"
        );
        assert_eq!(g.potential_difference(x, x), 0.0);
    }

    #[test]
    fn coriolis_does_no_work_and_deflects_east_when_falling() {
        let g = home();
        let v = DVec3::new(3.0, -40.0, 2.0);
        assert!(g.coriolis(v).dot(v).abs() < 1e-12);
        // Falling at the equator (+X, spin +Y): deflected towards −Z, which is east when the
        // planet turns counter-clockwise about +Y (the surface moves towards −Z at +X).
        let falling = g.coriolis(DVec3::NEG_X * 10.0);
        let east = g.spin.cross(DVec3::X).normalize();
        assert!(falling.dot(east) > 0.0);
    }
}
