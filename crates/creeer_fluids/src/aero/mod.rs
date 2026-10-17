//! Aerodynamics (F6.6): what the air does to a surface moving through it, from the
//! atmosphere's state where it is ([`crate::atmosphere::Air`]).
//!
//! - **Panels** (plates, fins, wings): lift and drag at any angle of attack and Mach number
//!   ([`coefficients`]), with skin friction on both faces.
//! - **Spheres:** the standard drag curve, Stokes to the drag crisis, and through the sound
//!   barrier. **Bluff bodies:** a fixed drag coefficient (a cube's 1.05, a canopy's).
//! - **Linearised:** the force with its slope by the velocity, its damping part (symmetric,
//!   positive semidefinite) for an implicit step's dashpot.
//! - **Heating:** the boundary layer's heat into the wall at its recovery temperature, by
//!   Reynolds's analogy on a panel and Whitaker's correlation on a sphere; at hypersonic
//!   speed Sutton and Graves's stagnation-point heating.

pub mod coefficients;
mod eigen;
#[cfg(test)]
mod tests;

use crate::atmosphere::Air;
pub use coefficients::Section;
use coefficients::{GAMMA, lift_drag, skin_friction, sphere_drag};
use creeer_math::det;
use creeer_math::glam::{DMat3, DVec3};
use std::f64::consts::PI;

/// Prandtl number taken for air.
const PRANDTL: f64 = 0.71;

/// Sutton and Graves's constant for air (kg^½/m): q = k·√(ρ/R)·v³ at a stagnation point of
/// nose radius R (NASA TR R-376, 1971).
const SUTTON_GRAVES: f64 = 1.7415e-4;

/// What meets the air.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A lifting surface: its area (m², one face), aspect ratio, span efficiency (Oswald's e),
    /// section, and its normal and chord (unit, at right angles, the chord pointing forward,
    /// in its body's frame).
    Panel {
        area: f64,
        aspect: f64,
        efficiency: f64,
        section: Section,
        normal: DVec3,
        chord: DVec3,
    },
    /// A sphere of diameter `diameter` (m).
    Sphere { diameter: f64 },
    /// A body the same from every side: its frontal area (m²) and drag coefficient.
    Bluff { area: f64, drag: f64 },
}

/// A surface on a body: its shape, and where on the body the air acts on it (m, body frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub shape: Shape,
    pub offset: DVec3,
}

impl Surface {
    /// The air's force on it (N, world frame) moving at `velocity` through the air (m/s,
    /// world), its body turned by `rotation` (body to world).
    pub fn force(&self, rotation: DMat3, velocity: DVec3, air: &Air) -> DVec3 {
        let u = -velocity;
        let speed = u.length();
        if speed <= 0.0 {
            return DVec3::ZERO;
        }
        let q = 0.5 * air.density * speed * speed;
        let mach = speed / air.sound;
        let along = u / speed;
        match self.shape {
            Shape::Panel {
                area,
                aspect,
                efficiency,
                section,
                normal,
                chord,
            } => {
                let (n, c) = (rotation * normal, rotation * chord);
                let alpha = det::atan2(u.dot(n), -u.dot(c));
                let (cl, cd) = lift_drag(&section, aspect, efficiency, alpha, mach);
                let lift = along.cross(c.cross(n)).normalize_or_zero();
                let length = (area / aspect).sqrt();
                let cf = skin_friction(air.density * speed * length / air.viscosity, mach);
                let tangential = u - n * u.dot(n);
                (lift * cl + along * cd) * (q * area)
                    + tangential * (air.density * speed * cf * area)
            }
            Shape::Sphere { diameter } => {
                let re = air.density * speed * diameter / air.viscosity;
                along * (q * 0.25 * PI * diameter * diameter * sphere_drag(re, mach))
            }
            Shape::Bluff { area, drag } => along * (q * area * drag),
        }
    }

    /// The force as [`Self::force`] and the damping part of its slope: D, symmetric positive
    /// semidefinite, with F(v + δv) ≈ F(v) − D·δv in every direction the force resists (the
    /// rest of the slope, lift turning with the flow or a stalled wing's negative one, is no
    /// damping: an implicit step takes it at the step's start).
    pub fn linearised(&self, rotation: DMat3, velocity: DVec3, air: &Air) -> (DVec3, DMat3) {
        let f = self.force(rotation, velocity, air);
        let d = 1e-6 * velocity.length().max(1e-3);
        let columns: [DVec3; 3] = std::array::from_fn(|k| {
            let mut v = velocity;
            v[k] += d;
            (self.force(rotation, v, air) - f) / d
        });
        let slope = DMat3::from_cols(columns[0], columns[1], columns[2]);
        let resist = (slope + slope.transpose()) * -0.5;
        (f, eigen::positive_part(resist))
    }

    /// The heat the boundary layer gives its wall (W; negative: takes) at `wall` (K), moving
    /// at `velocity` through `air`.
    pub fn heating(&self, velocity: DVec3, air: &Air, wall: f64) -> f64 {
        let speed = velocity.length();
        let mach = speed / air.sound;
        let cp = air.sound * air.sound / ((GAMMA - 1.0) * air.t);
        let rise = 0.5 * (GAMMA - 1.0) * mach * mach;
        match self.shape {
            Shape::Panel { area, aspect, .. } => {
                // Turbulent recovery r = Pr^⅓; St = C_f/2·Pr^−⅔ (Reynolds–Colburn).
                let recovery = air.t * (1.0 + det::cbrt(PRANDTL) * rise);
                let length = (area / aspect).sqrt();
                let cf = skin_friction(air.density * speed * length / air.viscosity, mach);
                let h = 0.5 * cf * det::pow(PRANDTL, -2.0 / 3.0) * air.density * speed * cp;
                h * 2.0 * area * (recovery - wall)
            }
            Shape::Sphere { diameter } => sphere_heating(diameter, speed, air, cp, rise, wall),
            Shape::Bluff { area, .. } => {
                sphere_heating((4.0 * area / PI).sqrt(), speed, air, cp, rise, wall)
            }
        }
    }
}

/// A sphere's heat: Whitaker's Nu = 2 + (0.4·Re^½ + 0.06·Re^⅔)·Pr^0.4 to the stagnation
/// temperature, or Sutton and Graves's stagnation heating over the front (its mean over the
/// hemisphere, cos θ falling as it does, a quarter of the stagnation point's on the whole
/// sphere's area) where that is more.
fn sphere_heating(diameter: f64, speed: f64, air: &Air, cp: f64, rise: f64, wall: f64) -> f64 {
    let stagnation = air.t * (1.0 + rise);
    let re = air.density * speed * diameter / air.viscosity;
    let nu = 2.0 + (0.4 * re.sqrt() + 0.06 * det::pow(re, 2.0 / 3.0)) * det::pow(PRANDTL, 0.4);
    let conductivity = air.viscosity * cp / PRANDTL;
    let area = PI * diameter * diameter;
    let convective = nu * conductivity / diameter * area * (stagnation - wall);
    let point = SUTTON_GRAVES * (air.density / (0.5 * diameter)).sqrt() * speed * speed * speed;
    let hypersonic = 0.25 * point * area * (1.0 - wall / stagnation);
    if hypersonic > convective {
        hypersonic
    } else {
        convective
    }
}
