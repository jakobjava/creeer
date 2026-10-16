//! Gravity in the frame a physics world runs in.
//!
//! Near a planet's surface the world runs in a frame that turns with the planet (plan.md §1,
//! principle 3): the ground stands still in it. In space it runs in a free-falling frame
//! that follows the ships (ADR-0007). Neither is inertial, so besides gravity fictitious
//! accelerations act:
//! - **centrifugal**, −ω×(ω×r): conservative, part of the effective potential
//!   Φ(r) = −μ/|r| − ½·|ω×r|²; it lowers effective gravity at the equator and makes the
//!   vertical of a plumb line deviate slightly from the direction to the centre;
//! - **Coriolis**, −2·ω×v: depends on velocity and does no work, so it has no potential. The
//!   implicit solver applies it as a force from the velocity at the start of each step (the
//!   error is of order (ω·h)², 10⁻¹¹ for a 6-hour day at 60 Hz);
//! - **tides**: the frame's origin falls towards every other body. What remains of their
//!   pull is the difference between their pull here and at the origin,
//!   μ·((d − r)/|d − r|³ − d/|d|³) for a body at d: the Sun stretches a planet by 5·10⁻⁷ m/s²
//!   at its surface, and in a free-falling frame this is all of gravity;
//! - **uniform**: an acceleration of the frame not due to gravity (its planet being pushed),
//!   seen as a uniform field opposite to it.
//!
//! The central body is a point mass (μ = G·M) at the origin; oblateness and mass
//! concentrations are not modelled. Over one step the tidal sources stand still: they move
//! by metres in a tick, at distances of thousands of kilometres.

use creeer_math::glam::DVec3;
use std::sync::Arc;

/// Another body pulling on the frame and everything in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tidal {
    /// Its gravitational parameter (m³/s²).
    pub mu: f64,
    /// Its position relative to the frame's origin, in frame axes (m).
    pub at: DVec3,
}

impl Tidal {
    /// Its pull here minus its pull at the origin (m/s²).
    fn acceleration(&self, x: DVec3) -> DVec3 {
        let d = self.at - x;
        let r = d.length();
        let r0 = self.at.length();
        d * (self.mu / (r * r * r)) - self.at * (self.mu / (r0 * r0 * r0))
    }

    /// Φ(x) − Φ(y) of its tidal potential −μ·(1/|d − x| − d·x/|d|³) (J/kg), with the
    /// difference of the reciprocal distances written without cancellation.
    fn potential_difference(&self, x: DVec3, y: DVec3) -> f64 {
        let (dx, dy) = (self.at - x, self.at - y);
        let (rx, ry) = (dx.length(), dy.length());
        let r0 = self.at.length();
        // 1/|d−x| − 1/|d−y| = (|d−y| − |d−x|)/(|d−x||d−y|), and
        // |d−y| − |d−x| = (x − y)·(dy + dx)/(|d−y| + |d−x|).
        let reciprocal = (x - y).dot(dx + dy) / ((rx + ry) * rx * ry);
        -self.mu * (reciprocal - self.at.dot(x - y) / (r0 * r0 * r0))
    }
}

/// Gravity seen from the frame of a physics world: a central body at the origin (μ = 0 in a
/// free-falling frame), the frame's rotation, a uniform field, and the tides of other bodies.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanetGravity {
    /// Gravitational parameter μ = G·M of the body at the origin (m³/s²); 0 for none.
    pub mu: f64,
    /// Angular velocity of the frame (rad/s): along the rotation axis.
    pub spin: DVec3,
    /// Uniform acceleration field (m/s²): minus the frame's non-gravitational acceleration.
    pub uniform: DVec3,
    /// Other bodies' tides.
    pub tidal: Arc<[Tidal]>,
}

impl PlanetGravity {
    /// A lone planet in its rotating frame.
    pub fn planet(mu: f64, spin: DVec3) -> Self {
        Self {
            mu,
            spin,
            uniform: DVec3::ZERO,
            tidal: Arc::new([]),
        }
    }

    /// Effective potential per unit mass (J/kg): −μ/|x| − ½·|ω×x|² − u·x + tides (zero at the
    /// origin).
    pub fn potential(&self, x: DVec3) -> f64 {
        let newton = if self.mu > 0.0 {
            -self.mu / x.length()
        } else {
            0.0
        };
        let tides: f64 = self
            .tidal
            .iter()
            .map(|t| t.potential_difference(x, DVec3::ZERO))
            .sum();
        newton - 0.5 * self.spin.cross(x).length_squared() - self.uniform.dot(x) + tides
    }

    /// Φ(x) − Φ(y) (J/kg), without the cancellation of subtracting two potentials of
    /// 5 MJ/kg that differ by millijoules (one step of motion on a 500 km planet).
    pub fn potential_difference(&self, x: DVec3, y: DVec3) -> f64 {
        let newton = if self.mu > 0.0 {
            let (rx, ry) = (x.length(), y.length());
            // |y| − |x| = (y − x)·(y + x) / (|x| + |y|)
            let dr = (y - x).dot(y + x) / (rx + ry);
            -self.mu * dr / (rx * ry)
        } else {
            0.0
        };
        // |ω×x|² − |ω×y|² = (ω×(x − y))·(ω×(x + y))
        let centrifugal = -0.5 * self.spin.cross(x - y).dot(self.spin.cross(x + y));
        let uniform = -self.uniform.dot(x - y);
        let tides: f64 = self
            .tidal
            .iter()
            .map(|t| t.potential_difference(x, y))
            .sum();
        newton + centrifugal + uniform + tides
    }

    /// Acceleration of gravity plus the centrifugal, uniform and tidal ones at `x` (m/s²):
    /// −∇Φ.
    pub fn acceleration(&self, x: DVec3) -> DVec3 {
        let newton = if self.mu > 0.0 {
            let r = x.length();
            -x * (self.mu / (r * r * r))
        } else {
            DVec3::ZERO
        };
        let tides: DVec3 = self.tidal.iter().map(|t| t.acceleration(x)).sum();
        newton - self.spin.cross(self.spin.cross(x)) + self.uniform + tides
    }

    /// Coriolis acceleration of a body moving at `v` in the rotating frame (m/s²).
    pub fn coriolis(&self, v: DVec3) -> DVec3 {
        -2.0 * self.spin.cross(v)
    }
}

#[cfg(test)]
#[path = "gravity_tests.rs"]
mod tests;
