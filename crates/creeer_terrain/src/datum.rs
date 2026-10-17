//! Sea level: the surface of equal effective potential of a rotating planet.
//!
//! Terrain heights are measured from the datum, the "sea level" of the planet. On a
//! rotating planet a sphere is not level: the centrifugal potential −½·ω²·r²·cos²φ makes the
//! effective vertical lean towards the equator, and on a spherical datum flat ground would be
//! tilted by up to ω²·R/(2g) (0.12° on the reference planet): balls would roll and, from F6,
//! oceans would flow towards the equator. The datum is therefore the equipotential of the
//! same field the physics uses (`creeer_physics::gravity`): a point mass μ seen from the frame
//! turning at ω about +Y,
//!
//! μ/r + ½·ω²·r²·cos²φ = μ/R,
//!
//! through the poles at R. The equator bulges by about R·ω²R³/(2μ) (1.08 km on a 500 km
//! planet with a 6-hour day). A real planet's own flattening adds the attraction of its
//! bulge; with a point-mass field, this surface is the one that is level for the physics.

use creeer_math::glam::DVec3;

/// The datum of a planet rotating about +Y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Datum {
    /// Radius at the poles (m).
    pub polar_radius: f64,
    /// Gravitational parameter μ (m³/s²).
    pub mu: f64,
    /// Rotation rate ω (rad/s).
    pub omega: f64,
}

impl Datum {
    /// Radius of the datum (m) in direction `dir` (need not be unit).
    pub fn radius(&self, dir: DVec3) -> f64 {
        let length2 = dir.length_squared();
        // cos²φ = 1 − sin²φ, with sin φ = y/|d| (the axis is +Y).
        let cos2 = (1.0 - dir.y * dir.y / length2).max(0.0);
        let k = 0.5 * self.omega * self.omega * cos2;
        let target = self.mu / self.polar_radius;
        // Newton on f(r) = μ/r + k·r² − μ/R from r = R: converges to rounding in three steps
        // (the correction is 0.2 % of R).
        let mut r = self.polar_radius;
        for _ in 0..4 {
            let f = self.mu / r + k * r * r - target;
            let slope = -self.mu / (r * r) + 2.0 * k * r;
            r -= f / slope;
        }
        r
    }

    /// Radius of the datum (m) in the direction of point `p`, and its gradient with respect
    /// to `p` (per metre). The radius solves μ/r + k·r² = μ/R with k = ω²·cos²φ/2, so
    /// dr/dk = −r² / (−μ/r² + 2·k·r) (implicit function), and cos²φ = 1 − y²/|p|².
    pub fn radius_gradient(&self, p: DVec3) -> (f64, DVec3) {
        let r = self.radius(p);
        let length2 = p.length_squared();
        if 1.0 - p.y * p.y / length2 <= 0.0 {
            return (r, DVec3::ZERO); // on the axis cos²φ has its minimum
        }
        let cos2 = 1.0 - p.y * p.y / length2;
        let k = 0.5 * self.omega * self.omega * cos2;
        let dr_dk = -r * r / (-self.mu / (r * r) + 2.0 * k * r);
        let d_cos2 =
            DVec3::Y * (-2.0 * p.y / length2) + p * (2.0 * p.y * p.y / (length2 * length2));
        (r, d_cos2 * (dr_dk * 0.5 * self.omega * self.omega))
    }

    /// Radius at the equator (m), the largest.
    pub fn equatorial_radius(&self) -> f64 {
        self.radius(DVec3::X)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> Datum {
        Datum {
            polar_radius: 500_000.0,
            mu: 9.81 * 500_000.0 * 500_000.0,
            omega: std::f64::consts::TAU / 21_600.0,
        }
    }

    /// The effective potential, as the physics defines it (`creeer_physics::gravity`).
    fn potential(d: &Datum, x: DVec3) -> f64 {
        let spin = DVec3::Y * d.omega;
        -d.mu / x.length() - 0.5 * spin.cross(x).length_squared()
    }

    #[test]
    fn the_datum_is_an_equipotential() {
        let d = home();
        let pole = potential(&d, DVec3::Y * d.polar_radius);
        for dir in [
            DVec3::X,
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(0.3, -0.8, 0.5),
            DVec3::new(0.0, 0.2, -1.0),
        ] {
            let dir = dir.normalize();
            let here = potential(&d, dir * d.radius(dir));
            assert!(
                (here - pole).abs() < 1e-8 * pole.abs(),
                "{dir}: {here} vs {pole}"
            );
        }
        assert!((d.radius(DVec3::Y) - d.polar_radius).abs() < 1e-9);
    }

    #[test]
    fn the_equator_bulges_by_the_expected_amount() {
        let d = home();
        let r = d.polar_radius;
        let first_order = r * d.omega * d.omega * r * r * r / (2.0 * d.mu);
        let bulge = d.equatorial_radius() - d.polar_radius;
        assert!((bulge / first_order - 1.0).abs() < 0.01, "bulge {bulge} m");
        assert!((bulge - 1085.2).abs() < 0.1, "bulge {bulge} m");
    }

    #[test]
    fn level_ground_is_perpendicular_to_effective_gravity() {
        // At 45° latitude the datum's normal is the direction of effective gravity: the
        // numerical gradient of the potential is parallel to the gradient of r − datum(r).
        let d = home();
        let dir = DVec3::new(1.0, 1.0, 0.0).normalize();
        let x = dir * d.radius(dir);
        let h = 1.0;
        let grad = |f: &dyn Fn(DVec3) -> f64| {
            DVec3::new(
                f(x + DVec3::X * h) - f(x - DVec3::X * h),
                f(x + DVec3::Y * h) - f(x - DVec3::Y * h),
                f(x + DVec3::Z * h) - f(x - DVec3::Z * h),
            )
            .normalize()
        };
        let gravity = grad(&|p| potential(&d, p));
        let surface = grad(&|p| p.length() - d.radius(p));
        assert!(
            gravity.angle_between(surface) < 1e-6,
            "{}",
            gravity.angle_between(surface)
        );
        // The sphere's normal is 0.12° off.
        assert!(gravity.angle_between(dir) > 0.1f64.to_radians());
    }

    #[test]
    fn radius_gradient_matches_finite_differences() {
        let d = home();
        for p in [
            DVec3::new(3e5, 2e5, -1e5),
            DVec3::new(-1e5, -4.5e5, 2e5),
            DVec3::X * 5e5,
        ] {
            let (r, g) = d.radius_gradient(p);
            assert_eq!(r, d.radius(p));
            let h = 1.0;
            for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                let fd = (d.radius(p + axis * h) - d.radius(p - axis * h)) / (2.0 * h);
                assert!(
                    (fd - g.dot(axis)).abs() < 1e-9,
                    "{p}: {fd} vs {}",
                    g.dot(axis)
                );
            }
        }
    }
}
