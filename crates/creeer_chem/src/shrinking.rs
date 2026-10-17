//! Gas–solid reactions by the shrinking-core model (O. Levenspiel, Chemical Reaction
//! Engineering, 3rd ed., ch. 25): a gas A reaching a particle of solid B through a gas film,
//! then through the layer of product already made, reacts at the shrinking core's surface,
//! A(g) + b B(s) → products. Each step alone would convert the particle fully in its own
//! time, and in series their times add:
//!
//! - film: t = τ_film·X, τ_film = ρ_B·R/(3·b·k_g·C);
//! - product layer: t = τ_layer·(1 − 3(1 − X)^(2/3) + 2(1 − X)), τ_layer = ρ_B·R²/(6·b·D_e·C);
//! - reaction: t = τ_reaction·(1 − (1 − X)^(1/3)), τ_reaction = ρ_B·R/(b·k_s·C).
//!
//! ρ_B is the solid's molar density (mol/m³), R the particle's radius, C the gas's driving
//! concentration (mol/m³): for a reversible reduction its excess over equilibrium,
//! C_A − C_A,eq, so a particle stops converting where the gas is in equilibrium with it.

use creeer_math::det;

/// A particle's properties.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// Radius (m), the solid reactant's molar density (mol/m³), moles of it per mole of gas.
    pub radius: f64,
    pub density: f64,
    pub b: f64,
    /// Film mass transfer (m/s), effective diffusivity in the product layer (m²/s), surface
    /// rate constant (m/s). Infinite: no resistance there.
    pub film: f64,
    pub diffusivity: f64,
    pub surface: f64,
}

impl Particle {
    /// The time each step alone takes to convert it fully at driving concentration `c`.
    fn times(&self, c: f64) -> (f64, f64, f64) {
        let base = self.density * self.radius / (self.b * c);
        (
            base / (3.0 * self.film),
            base * self.radius / (6.0 * self.diffusivity),
            base / self.surface,
        )
    }

    /// The time to reach conversion `x` at driving concentration `c` (s).
    pub fn time_to(&self, x: f64, c: f64) -> f64 {
        let (film, layer, reaction) = self.times(c);
        let left = (1.0 - x).max(0.0);
        film * x
            + layer * (1.0 - 3.0 * det::pow(left, 2.0 / 3.0) + 2.0 * left)
            + reaction * (1.0 - det::cbrt(left))
    }

    /// Its conversion after `dt` more seconds at driving concentration `c`, from `x`: the
    /// time it would have taken to reach `x` at this concentration, plus `dt`, inverted.
    /// No driving force (the gas at or below equilibrium): no change.
    pub fn convert(&self, x: f64, c: f64, dt: f64) -> f64 {
        if c <= 0.0 || x >= 1.0 {
            return x;
        }
        let target = self.time_to(x, c) + dt;
        if target >= self.time_to(1.0, c) {
            return 1.0;
        }
        let (mut lo, mut hi) = (x, 1.0);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if self.time_to(mid, c) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }
}
