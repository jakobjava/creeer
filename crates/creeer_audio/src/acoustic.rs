//! The physics a sound is worked out from (game thread): a material's elastic constants and loss
//! factor, how long two bodies touch when they meet (Hertz), and how loud a vibrating surface or
//! a body stopped short sounds at a metre (ADR-0037, decisions 2–4).

use std::f64::consts::PI;

/// Air at 20 °C: its density (kg/m³) and its speed of sound (m/s).
pub const AIR_DENSITY: f64 = 1.204;
pub const SOUND_SPEED: f64 = 343.2;
/// The pressure of 0 dB SPL (Pa).
pub const REFERENCE_PRESSURE: f64 = 20e-6;

/// What a material sounds by: its elastic constants (Pa), density (kg/m³) and loss factor η
/// (each mode decays at π·f·η per second: van den Doel and Pai 1998's internal friction).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Acoustic {
    pub youngs: f64,
    pub shear: f64,
    pub density: f64,
    pub loss: f64,
}

impl Acoustic {
    /// Poisson's ratio from E and G (kept within a solid's 0 to ½).
    pub fn poisson(&self) -> f64 {
        (self.youngs / (2.0 * self.shear) - 1.0).clamp(0.0, 0.49)
    }

    /// Its surface's compliance in a contact, (1 − ν²)/E (1/Pa).
    pub fn compliance(&self) -> f64 {
        let nu = self.poisson();
        (1.0 - nu * nu) / self.youngs
    }

    /// The speed of longitudinal waves along a bar, √(E/ρ) (m/s).
    pub fn bar_speed(&self) -> f64 {
        (self.youngs / self.density).sqrt()
    }

    /// The speed of shear waves, √(G/ρ) (m/s).
    pub fn shear_speed(&self) -> f64 {
        (self.shear / self.density).sqrt()
    }

    /// How fast a mode of `f` Hz decays (1/s): its amplitude falls as e^(−d·t), d = π·f·η.
    pub fn decay(&self, f: f64) -> f64 {
        PI * f * self.loss
    }
}

/// The contact modulus E* of two surfaces of these compliances (Pa).
pub fn contact_modulus(a: f64, b: f64) -> f64 {
    1.0 / (a + b)
}

/// How long two elastic bodies touch (s) when they meet at `speed` (m/s): their reduced mass
/// `mass` (kg), the contact's radius of curvature `radius` (m) and contact modulus `modulus`
/// (Pa). Hertz's result, t = 2.868·(m*²/(R*·E*²·v))^(1/5) (Johnson, Contact Mechanics, §11.4).
pub fn contact_time(mass: f64, radius: f64, modulus: f64, speed: f64) -> f64 {
    let v = speed.max(1e-3);
    2.868 * (mass * mass / (radius.max(1e-4) * modulus * modulus * v)).powf(0.2)
}

/// How much of a blow's impulse reaches a mode of `f` Hz when the force lasts `tau` seconds:
/// the envelope of a half-sine pulse's spectrum without its nulls, 1/(1 + (2fτ)²) (flat below
/// 1/(2τ), falling as 1/f² above). A soft contact rings only the low modes: a thud, not a click.
pub fn pulse_share(f: f64, tau: f64) -> f64 {
    let x = 2.0 * f * tau;
    1.0 / (1.0 + x * x)
}

/// How efficiently a face `width` metres wide radiates at `f` Hz (0 to 1): an unbaffled face
/// narrower than the sound's wavelength loses most of its sound round its edges (the acoustic
/// short circuit: Fahy and Gardonio, Sound and Structural Vibration, 2007), (kw)²/(1 + (kw)²).
pub fn radiation_efficiency(f: f64, width: f64) -> f64 {
    let kw = 2.0 * PI * f / SOUND_SPEED * width;
    kw * kw / (1.0 + kw * kw)
}

/// The pressure amplitude at a metre (Pa) of a surface of `area` m² and `width` m vibrating at
/// `f` Hz with velocity amplitude `velocity` (m/s): its power ρ₀c·S·⟨v²⟩·σ spread over a sphere,
/// p = ρ₀c·v·√(S·σ/(4π)).
pub fn radiated(f: f64, velocity: f64, area: f64, width: f64) -> f64 {
    let sigma = radiation_efficiency(f, width);
    AIR_DENSITY * SOUND_SPEED * velocity * (area * sigma / (4.0 * PI)).sqrt()
}

/// A body stopped short radiates a click (Koss and Alfredson 1973: "acceleration noise"): for a
/// body of radius `radius`, the pressure at a metre per m/s of its velocity's change (Pa·s/m),
/// ρ₀c·a/2, above the corner frequency c/(2πa) below which it radiates as a dipole (falling as
/// f²). Returns (gain, corner).
pub fn click(radius: f64) -> (f64, f64) {
    let a = radius.max(1e-3);
    (
        AIR_DENSITY * SOUND_SPEED * a / 2.0,
        SOUND_SPEED / (2.0 * PI * a),
    )
}

/// Sound pressure level (dB SPL) of a pressure amplitude `p` (Pa), taken as a sine's.
pub fn spl(p: f64) -> f64 {
    20.0 * (p.max(1e-12) / std::f64::consts::SQRT_2 / REFERENCE_PRESSURE).log10()
}

/// The pressure amplitude (Pa) of a level `db` dB SPL, taken as a sine's.
pub fn pressure(db: f64) -> f64 {
    REFERENCE_PRESSURE * std::f64::consts::SQRT_2 * 10f64.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hertz_contact_time_of_two_steel_balls_matches_the_textbook() {
        // Two 25 mm steel balls (E = 210 GPa, ν = 0.3) at 1 m/s touch for about 0.1 ms
        // (Johnson §11.4; Goldsmith's measurements of steel spheres: 50–150 µs).
        let steel = Acoustic {
            youngs: 210e9,
            shear: 80.8e9,
            density: 7850.0,
            loss: 1e-3,
        };
        let r = 0.0125;
        let m = 7850.0 * 4.0 / 3.0 * PI * r * r * r;
        let e = contact_modulus(steel.compliance(), steel.compliance());
        let t = contact_time(m / 2.0, r / 2.0, e, 1.0);
        assert!((50e-6..150e-6).contains(&t), "{t}");
        // Softer is longer: a rubber-like sole on soil lasts milliseconds.
        assert!(contact_time(10.0, 0.04, 5e6, 0.5) > 5e-3);
    }

    #[test]
    fn levels_and_pressures_invert() {
        for db in [0.0, 60.0, 94.0, 120.0] {
            assert!((spl(pressure(db)) - db).abs() < 1e-9);
        }
        // 94 dB SPL is 1 Pa RMS.
        assert!((pressure(94.0) / std::f64::consts::SQRT_2 - 1.0).abs() < 0.003);
    }

    #[test]
    fn a_narrow_face_radiates_little_below_its_wavelength() {
        assert!(radiation_efficiency(100.0, 0.003) < 1e-4);
        assert!(radiation_efficiency(10_000.0, 0.3) > 0.99);
        assert!(pulse_share(10.0, 1e-3) > 0.99 && pulse_share(5000.0, 1e-3) < 0.01);
    }
}
