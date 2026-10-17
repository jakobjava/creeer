//! Vapour–liquid equilibrium from the species' Gibbs energies: a liquid's vapour pressure
//! is where its gas and its liquid are worth the same, their fugacities equal,
//! p·φ(p) = P°·exp((g°_l − g°_g)/RT)·exp(V_L·(p − P°)/RT): the vapour's fugacity coefficient
//! from its second virial coefficient (ln φ = B·p/RT) and the liquid's Poynting factor
//! from its molar volume (`real`), where they are known; ideal where not. A mixture of
//! liquids follows Raoult's law (ideal solution), y_i·P = x_i·P_sat,i, and a flash splits a
//! feed into its vapour and its liquid (Rachford–Rice).
//!
//! Not modelled: activity coefficients (an azeotrope such as ethanol–water's is beyond it),
//! the vapours' cross virial coefficients in a mixture, and the third virial term near the
//! critical point.

use crate::real::{Critical, critical};
use creeer_materials::species::{R, STANDARD_PRESSURE, Species};
use creeer_math::det;

/// A substance that can be gas and liquid.
#[derive(Clone, Copy, Debug)]
pub struct Volatile<'a> {
    pub gas: &'a Species,
    pub liquid: &'a Species,
    /// Its constants, where known (`real::SUBSTANCES`).
    pub fluid: Option<&'static Critical>,
}

impl<'a> Volatile<'a> {
    /// The substance of `gas` and `liquid`, with its constants if known.
    pub fn new(gas: &'a Species, liquid: &'a Species) -> Self {
        Self {
            gas,
            liquid,
            fluid: critical(&gas.name),
        }
    }

    /// Its vapour pressure at `t` (Pa).
    pub fn vapour_pressure(&self, t: f64) -> f64 {
        let ideal = STANDARD_PRESSURE * det::exp(self.liquid.g_rt(t) - self.gas.g_rt(t));
        let Some(fluid) = self.fluid else {
            return ideal;
        };
        let b = fluid.second_virial(t).unwrap_or(0.0);
        let v = fluid.liquid_volume(t, self.gas.molar_mass).unwrap_or(0.0);
        // p = ideal·exp((V_L·(p − P°) − B·p)/RT): the corrections are small, a fixed point.
        let mut p = ideal;
        for _ in 0..30 {
            p = ideal * det::exp((v * (p - STANDARD_PRESSURE) - b * p) / (R * t));
        }
        p
    }

    /// Where it boils under `p` (K): the temperature its vapour pressure is `p`, within its
    /// liquid's data. `None` if outside them (it boils below or above them).
    pub fn boiling_point(&self, p: f64) -> Option<f64> {
        let (mut lo, mut hi) = self.liquid.range();
        if self.vapour_pressure(lo) > p || self.vapour_pressure(hi) < p {
            return None;
        }
        // ln P_sat is nearly linear in 1/T (Clausius–Clapeyron): bisect on it.
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if self.vapour_pressure(mid) < p {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(0.5 * (lo + hi))
    }
}

/// A flash's result: the share of the feed that is vapour, and each substance's mole
/// fraction in the liquid and in the vapour.
#[derive(Clone, Debug, PartialEq)]
pub struct Flash {
    pub vapour: f64,
    pub liquid_fractions: Vec<f64>,
    pub vapour_fractions: Vec<f64>,
}

/// The flash of a feed with mole fractions `z` of `substances` at `t` and `p`: all liquid,
/// all vapour, or both, Raoult's K_i = P_sat,i/P and Σ z_i·(K_i − 1)/(1 + V·(K_i − 1)) = 0.
pub fn flash(substances: &[Volatile<'_>], z: &[f64], t: f64, p: f64) -> Flash {
    let k: Vec<f64> = substances
        .iter()
        .map(|s| s.vapour_pressure(t) / p)
        .collect();
    let f = |v: f64| -> f64 {
        z.iter()
            .zip(&k)
            .map(|(z, k)| z * (k - 1.0) / (1.0 + v * (k - 1.0)))
            .sum()
    };
    // Below the bubble point f(0) < 0: liquid; above the dew point f(1) > 0: vapour.
    let vapour = if f(0.0) <= 0.0 {
        0.0
    } else if f(1.0) >= 0.0 {
        1.0
    } else {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    };
    let x: Vec<f64> = z
        .iter()
        .zip(&k)
        .map(|(z, k)| z / (1.0 + vapour * (k - 1.0)))
        .collect();
    let y: Vec<f64> = x.iter().zip(&k).map(|(x, k)| x * k).collect();
    // Normalised (exact at the solution; at the bubble or dew point one side is a limit).
    let (sx, sy): (f64, f64) = (x.iter().sum(), y.iter().sum());
    Flash {
        vapour,
        liquid_fractions: x.iter().map(|x| x / sx).collect(),
        vapour_fractions: y.iter().map(|y| y / sy).collect(),
    }
}

/// The bubble point of a liquid with mole fractions `x` under `p` (K): where Σ x_i·P_sat,i
/// is `p`, between `low` and `high`.
pub fn bubble_point(substances: &[Volatile<'_>], x: &[f64], p: f64, low: f64, high: f64) -> f64 {
    let total = |t: f64| -> f64 {
        substances
            .iter()
            .zip(x)
            .map(|(s, x)| x * s.vapour_pressure(t))
            .sum()
    };
    let (mut lo, mut hi) = (low, high);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if total(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}
