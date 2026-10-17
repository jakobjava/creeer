//! Convection coefficients from the standard correlations (Incropera, *Fundamentals of Heat
//! and Mass Transfer*, chapters 7 and 9), evaluated with the fluid's properties at the film
//! temperature (the mean of surface and fluid), in air or in water.
//!
//! Natural convection (buoyancy, from the temperature difference and gravity) and forced
//! convection (from the flow speed) are combined as Churchill's cube sum,
//! Nu³ = Nu_natural³ + Nu_forced³, which tends to each where the other is negligible.

use crate::fluid::Medium;
use creeer_math::det;

/// A surface's shape for convection, with its characteristic length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A vertical plate of this height (m).
    VerticalPlate(f64),
    /// A horizontal plate facing up, of this length (area / perimeter, m).
    PlateUp(f64),
    /// A horizontal plate facing down.
    PlateDown(f64),
    /// A horizontal cylinder of this diameter (m).
    Cylinder(f64),
    /// A sphere of this diameter (m).
    Sphere(f64),
}

impl Shape {
    fn length(self) -> f64 {
        match self {
            Self::VerticalPlate(l)
            | Self::PlateUp(l)
            | Self::PlateDown(l)
            | Self::Cylinder(l)
            | Self::Sphere(l) => l,
        }
    }
}

/// Churchill–Chu's correction for the Prandtl number, [1 + (c/Pr)^(9/16)]^e.
fn churchill(pr: f64, c: f64, e: f64) -> f64 {
    det::pow(1.0 + det::pow(c / pr, 9.0 / 16.0), e)
}

/// The Nusselt number of natural convection at Rayleigh number `ra`.
fn natural(shape: Shape, ra: f64, pr: f64) -> f64 {
    match shape {
        // Churchill & Chu (1975): their laminar form below Ra 10⁹ (Incropera eq. 9.27, the
        // more accurate there: it is Ostrach's boundary-layer solution to 1 %), their form for
        // any Ra beyond 10¹⁰ (eq. 9.26, which reads 15–30 % high in the laminar range), blended
        // smoothly over the decade where the boundary layer turns turbulent.
        Shape::VerticalPlate(_) => {
            let laminar = 0.68 + 0.670 * det::pow(ra, 0.25) / churchill(pr, 0.492, 4.0 / 9.0);
            if ra <= 1e9 {
                return laminar;
            }
            let n = 0.825 + 0.387 * det::pow(ra, 1.0 / 6.0) / churchill(pr, 0.492, 8.0 / 27.0);
            let w = (det::log10(ra) - 9.0).min(1.0);
            let w = w * w * (3.0 - 2.0 * w);
            (1.0 - w) * laminar + w * n * n
        }
        // Hot surface up (Incropera eqs. 9.30–9.31, Lloyd & Moran).
        Shape::PlateUp(_) => {
            if ra < 1e7 {
                0.54 * det::pow(ra, 0.25)
            } else {
                0.15 * det::pow(ra, 1.0 / 3.0)
            }
        }
        // Hot surface down (Incropera eq. 9.32, Radziemska & Lewandowski).
        Shape::PlateDown(_) => 0.52 * det::pow(ra, 0.2),
        // Churchill & Chu (1975), horizontal cylinder (Incropera eq. 9.34).
        Shape::Cylinder(_) => {
            let n = 0.60 + 0.387 * det::pow(ra, 1.0 / 6.0) / churchill(pr, 0.559, 8.0 / 27.0);
            n * n
        }
        // Churchill (1983), sphere (Incropera eq. 9.35).
        Shape::Sphere(_) => 2.0 + 0.589 * det::pow(ra, 0.25) / churchill(pr, 0.469, 4.0 / 9.0),
    }
}

/// The Nusselt number of forced convection at Reynolds number `re`.
fn forced(shape: Shape, re: f64, pr: f64) -> f64 {
    let pr3 = det::pow(pr, 1.0 / 3.0);
    match shape {
        // Average over a plate, laminar to its end below Re 5e5, mixed beyond (Incropera
        // eqs. 7.30 and 7.38).
        Shape::VerticalPlate(_) | Shape::PlateUp(_) | Shape::PlateDown(_) => {
            if re < 5e5 {
                0.664 * re.sqrt() * pr3
            } else {
                (0.037 * det::pow(re, 0.8) - 871.0) * pr3
            }
        }
        // Churchill & Bernstein (1977), cylinder in crossflow (Incropera eq. 7.54).
        Shape::Cylinder(_) => {
            0.3 + 0.62 * re.sqrt() * pr3 / det::pow(1.0 + det::pow(0.4 / pr, 2.0 / 3.0), 0.25)
                * det::pow(1.0 + det::pow(re / 282_000.0, 5.0 / 8.0), 0.8)
        }
        // Whitaker (1972), sphere (Incropera eq. 7.56, the viscosity ratio taken as 1).
        Shape::Sphere(_) => {
            2.0 + (0.4 * re.sqrt() + 0.06 * det::pow(re, 2.0 / 3.0)) * det::pow(pr, 0.4)
        }
    }
}

/// The convection coefficient (W/(m²·K)) of a surface of `shape` at `surface` K in `medium`
/// at `fluid` K and `pressure` Pa, flowing past at `speed` m/s, under gravity `gravity` m/s².
///
/// A horizontal plate's face is the side buoyancy lifts the fluid from or onto: a plate
/// warmer than the fluid facing up and one colder facing down both have the plume rising off
/// them (Incropera §9.6); colder facing up, warmer facing down, the fluid lies against them.
/// Water below 4 °C, which rises as it cools, turns both over.
pub fn coefficient(
    medium: Medium,
    shape: Shape,
    surface: f64,
    fluid: f64,
    pressure: f64,
    speed: f64,
    gravity: f64,
) -> f64 {
    let f = medium.at(0.5 * (surface + fluid), pressure);
    let (nu, alpha, pr) = (f.kinematic_viscosity(), f.diffusivity(), f.prandtl());
    let l = shape.length();
    let buoyancy = gravity * f.expansion * (surface - fluid);
    let ra = buoyancy.abs() * l * l * l / (nu * alpha);
    let shape = match shape {
        Shape::PlateUp(l) if buoyancy < 0.0 => Shape::PlateDown(l),
        Shape::PlateDown(l) if buoyancy < 0.0 => Shape::PlateUp(l),
        other => other,
    };
    let re = speed * l / nu;
    // A sphere conducts into still fluid (Nu = 2) with no flow at all; both its correlations
    // hold that limit, so it is combined once, and their convective parts cube-summed.
    let conduction = if matches!(shape, Shape::Sphere(_)) {
        2.0
    } else {
        0.0
    };
    let n = if ra > 0.0 {
        natural(shape, ra, pr) - conduction
    } else {
        0.0
    };
    let m = if re > 0.0 {
        forced(shape, re, pr) - conduction
    } else {
        0.0
    };
    let nusselt = conduction + det::cbrt(n * n * n + m * m * m);
    nusselt * f.conductivity / l
}
