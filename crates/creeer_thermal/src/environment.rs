//! What surroundings give and take by radiation: the sky's temperature, the Sun's light
//! through an atmosphere, and the view factors and exchange factors between surfaces.

use creeer_math::det;
use std::f64::consts::PI;

/// The solar constant: sunlight at 1 AU above any atmosphere (W/m²; Kopp & Lean 2011).
pub const SOLAR_CONSTANT: f64 = 1361.0;
/// The astronomical unit (m).
pub const AU: f64 = 1.495_978_707e11;
/// The cosmic microwave background: deep space's temperature (K).
pub const DEEP_SPACE: f64 = 2.725;
/// The fraction of direct sunlight one air mass of Earth's clear atmosphere lets through
/// (Meinel & Meinel 1976: I = I₀·0.7^(AM^0.678)).
pub const EARTH_CLEAR_SKY: f64 = 0.7;

/// Sunlight above the atmosphere at `distance` metres from the Sun (W/m²).
pub fn sunlight(distance: f64) -> f64 {
    SOLAR_CONSTANT * (AU / distance) * (AU / distance)
}

/// The air mass a ray from the Sun at `zenith` radians from straight up passes through
/// (relative to straight down; Kasten & Young 1989, good to the horizon), or none below it.
pub fn air_mass(zenith: f64) -> Option<f64> {
    let degrees = zenith.to_degrees();
    (degrees <= 90.0)
        .then(|| 1.0 / (det::cos(zenith) + 0.505_72 * det::pow(96.079_95 - degrees, -1.636_4)))
}

/// Direct sunlight reaching the ground (W/m², on a surface facing the Sun) from `above` W/m²
/// at the top of an atmosphere passing `clear` of it per air mass, with the Sun at `zenith`
/// radians (Meinel's fit; on an airless body all of it).
pub fn direct_sunlight(above: f64, zenith: f64, clear: Option<f64>) -> f64 {
    match (clear, air_mass(zenith)) {
        (None, _) => above,
        (Some(_), None) => 0.0,
        (Some(t), Some(m)) => above * det::pow(t, det::pow(m, 0.678)),
    }
}

/// The power (W) a flat surface of `area` m² with outward normal `normal` absorbs from
/// sunlight of `irradiance` W/m² coming from direction `to_sun` (unit vectors), with solar
/// absorptivity `absorptivity`.
pub fn absorbed(
    absorptivity: f64,
    area: f64,
    normal: [f64; 3],
    to_sun: [f64; 3],
    irradiance: f64,
) -> f64 {
    let facing = normal[0] * to_sun[0] + normal[1] * to_sun[1] + normal[2] * to_sun[2];
    absorptivity * area * irradiance * facing.max(0.0)
}

/// The clear night sky's effective temperature over air at `air` K (Swinbank 1963:
/// 0.0552·T^1.5), clouds covering `cloud` of it radiating back at the air's own
/// (the fourth powers blended: overcast, the sky is as warm as the air).
pub fn sky_temperature(air: f64, cloud: f64) -> f64 {
    let clear = 0.0552 * det::pow(air, 1.5);
    let c = cloud.clamp(0.0, 1.0);
    det::pow(
        (1.0 - c) * det::pow(clear, 4.0) + c * det::pow(air, 4.0),
        0.25,
    )
}

/// The view factor from one disc to a parallel coaxial one: radii `r1`, `r2`, `gap` apart
/// (Incropera Table 13.2).
pub fn coaxial_discs(r1: f64, r2: f64, gap: f64) -> f64 {
    let (a, b) = (r1 / gap, r2 / gap);
    let s = 1.0 + (1.0 + b * b) / (a * a);
    0.5 * (s - (s * s - 4.0 * (b / a) * (b / a)).sqrt())
}

/// The view factor between two equal, aligned, parallel rectangles `x` × `y`, `gap` apart
/// (Incropera Table 13.2).
pub fn aligned_rectangles(x: f64, y: f64, gap: f64) -> f64 {
    let (a, b) = (x / gap, y / gap);
    let (a2, b2) = (a * a, b * b);
    let log = 0.5 * det::ln((1.0 + a2) * (1.0 + b2) / (1.0 + a2 + b2));
    let ra = (1.0 + b2).sqrt();
    let rb = (1.0 + a2).sqrt();
    2.0 / (PI * a * b)
        * (log + a * ra * det::atan(a / ra) + b * rb * det::atan(b / rb)
            - a * det::atan(a)
            - b * det::atan(b))
}

/// The view factor from a rectangle `x` × `y` to one `x` × `z` at right angles to it,
/// sharing its edge of length `x` (Incropera Table 13.2).
pub fn perpendicular_rectangles(x: f64, y: f64, z: f64) -> f64 {
    let (h, w) = (z / x, y / x);
    let (h2, w2) = (h * h, w * w);
    let s = (h2 + w2).sqrt();
    let log = det::ln((1.0 + w2) * (1.0 + h2) / (1.0 + w2 + h2))
        + w2 * det::ln(w2 * (1.0 + w2 + h2) / ((1.0 + w2) * (w2 + h2)))
        + h2 * det::ln(h2 * (1.0 + h2 + w2) / ((1.0 + h2) * (h2 + w2)));
    (w * det::atan(1.0 / w) + h * det::atan(1.0 / h) - s * det::atan(1.0 / s) + 0.25 * log)
        / (PI * w)
}

/// The exchange factor (m², σ·F·(T₁⁴ − T₂⁴) is the net flow) between two grey, diffuse
/// surfaces of areas `a1`, `a2` and emissivities `e1`, `e2`, the first seeing the second with
/// view factor `f12`: the two surface resistances and the space resistance in series.
pub fn grey_exchange(a1: f64, e1: f64, a2: f64, e2: f64, f12: f64) -> f64 {
    1.0 / ((1.0 - e1) / (e1 * a1) + 1.0 / (a1 * f12) + (1.0 - e2) / (e2 * a2))
}

/// A fire's convection coefficient to what it engulfs (W/(m²·K)): EN 1991-1-2 3.2.1, for the
/// standard fire curve.
pub const FIRE_CONVECTION: f64 = 25.0;
/// A fire's emissivity (EN 1991-1-2 3.1(6)).
pub const FIRE_EMISSIVITY: f64 = 1.0;

/// The ISO 834 standard fire's gas temperature (K) `seconds` after it started:
/// 20 + 345·log₁₀(8·t + 1) °C, t in minutes (EN 1991-1-2 3.2.1).
pub fn iso_834(seconds: f64) -> f64 {
    293.15 + 345.0 * det::log10(8.0 * seconds / 60.0 + 1.0)
}
