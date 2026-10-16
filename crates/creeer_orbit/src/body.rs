//! Celestial bodies: their physical properties and their rotation.

use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};

/// Newtonian constant of gravitation (CODATA 2018), m³/(kg·s²).
pub const G: f64 = 6.674_30e-11;

/// What a body is (decides how it is drawn and whether it has a surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyKind {
    /// A star: shines, no surface to stand on.
    Star,
    /// A rocky planet with terrain.
    Planet,
    /// A gas giant: no solid surface.
    GasGiant,
    /// A moon with terrain.
    Moon,
    /// A small irregular body with terrain.
    Asteroid,
}

impl BodyKind {
    /// Whether the body has terrain one can stand on.
    pub fn has_surface(self) -> bool {
        matches!(self, Self::Planet | Self::Moon | Self::Asteroid)
    }
}

/// A body's rotation about a fixed axis (no torques act on it in F4: the axis keeps its
/// direction in inertial space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spin {
    /// Orientation at `epoch`: maps body-fixed coordinates (rotation axis = +Y) to inertial.
    pub orientation: DQuat,
    /// Rotation rate about the axis (rad/s; positive = counter-clockwise seen from +axis).
    pub rate: f64,
    /// Time of `orientation` (s).
    pub epoch: f64,
}

impl Spin {
    /// A body turning once per `period` seconds about an axis tilted by `tilt` radians from
    /// +Y towards +X (the reference plane's pole), oriented at `angle` at time 0.
    pub fn new(period: f64, tilt: f64, angle: f64) -> Self {
        let rate = if period.is_finite() && period != 0.0 {
            std::f64::consts::TAU / period
        } else {
            0.0
        };
        Self {
            orientation: DQuat::from_rotation_z(-tilt) * DQuat::from_rotation_y(angle),
            rate,
            epoch: 0.0,
        }
    }

    /// Rotation axis in inertial coordinates.
    pub fn axis(&self) -> DVec3 {
        self.orientation * DVec3::Y
    }

    /// Angular velocity vector (rad/s).
    pub fn angular_velocity(&self) -> DVec3 {
        self.axis() * self.rate
    }

    /// Body-fixed → inertial rotation at time `t`.
    pub fn orientation_at(&self, t: f64) -> DQuat {
        // Turning about the body's own +Y (the axis) applies on the right.
        let angle = (self.rate * (t - self.epoch)).rem_euclid(std::f64::consts::TAU);
        let (s, c) = det::sin_cos(0.5 * angle);
        (self.orientation * DQuat::from_xyzw(0.0, s, 0.0, c)).normalize()
    }
}

/// A body's physical properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Display name.
    pub name: String,
    /// What it is.
    pub kind: BodyKind,
    /// Gravitational parameter μ = G·M (m³/s²).
    pub mu: f64,
    /// Mean radius (m): collisions, drawing.
    pub radius: f64,
    /// Rotation.
    pub spin: Spin,
    /// Display colour (sRGB).
    pub color: [u8; 3],
    /// Seed of its terrain or appearance.
    pub seed: u64,
    /// Terrain definition (a planet data file under `data/`) for bodies with a surface.
    pub surface: Option<String>,
}

impl Body {
    /// Mass (kg).
    pub fn mass(&self) -> f64 {
        self.mu / G
    }

    /// Mean density (kg/m³).
    pub fn density(&self) -> f64 {
        self.mass() / (4.0 / 3.0 * std::f64::consts::PI * self.radius * self.radius * self.radius)
    }

    /// Gravity at the surface (m/s²), ignoring rotation.
    pub fn surface_gravity(&self) -> f64 {
        self.mu / (self.radius * self.radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spin_turns_about_the_tilted_axis() {
        let spin = Spin::new(21_600.0, 0.4, 0.0);
        let axis = spin.axis();
        assert!((axis.angle_between(DVec3::Y) - 0.4).abs() < 1e-15);
        assert!(axis.x > 0.0, "tilted towards +X");
        // A quarter day later, a point on the equator has turned 90° about the axis.
        let q0 = spin.orientation_at(0.0);
        let q1 = spin.orientation_at(5_400.0);
        let p = DVec3::X;
        let (a, b) = (q0 * p, q1 * p);
        assert!(a.dot(b).abs() < 1e-12);
        assert!(
            a.cross(b).normalize().dot(axis) > 1.0 - 1e-12,
            "counter-clockwise"
        );
        // The axis itself never moves.
        assert!((q1 * DVec3::Y - axis).length() < 1e-15);
        // ω × r is the velocity of a point fixed on the body.
        let h = 1e-3;
        let numeric =
            (spin.orientation_at(100.0 + h) * p - spin.orientation_at(100.0 - h) * p) / (2.0 * h);
        let analytic = spin
            .angular_velocity()
            .cross(spin.orientation_at(100.0) * p);
        assert!((numeric - analytic).length() < 1e-10);
    }

    #[test]
    fn home_like_planet_numbers() {
        let mu = 9.81 * 5e5 * 5e5;
        let body = Body {
            name: "Home".into(),
            kind: BodyKind::Planet,
            mu,
            radius: 5e5,
            spin: Spin::new(21_600.0, 0.0, 0.0),
            color: [80, 120, 200],
            seed: 1,
            surface: None,
        };
        assert!((body.mass() - 3.6746e22).abs() < 1e19);
        assert!((body.surface_gravity() - 9.81).abs() < 1e-12);
        // 13× Earth's density: KSP-scale planets are small and dense.
        assert!((body.density() / 5514.0 - 12.7).abs() < 0.1);
    }
}
