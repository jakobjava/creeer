//! Reaction kinetics: Arrhenius rates, reversible through the equilibrium constant (detailed
//! balance), so a reaction slows to a stop exactly where equilibrium is and never beyond.
//!
//! Net rate r = k_f(T)·(Π c_r^ν_r − Π c_p^ν_p / K_c(T)) (mol/(m³·s)), concentrations of gases
//! c = n/V (mol/m³), pure condensed phases at activity 1; k_f = A·T^β·exp(−E_a/RT);
//! K_c = K_p·(P°/RT)^Δν_gas with K_p = exp(−ΔG°/RT) from the species' data, the reaction's
//! own orders its stoichiometry (an elementary step).

use crate::system::System;
use creeer_materials::species::{R, STANDARD_PRESSURE};
use creeer_math::det;

/// A reaction: what it consumes and makes (species of a system, moles per mole of
/// reaction), its rate constant k = A·T^β·exp(−E_a/RT) (SI: m³, mol, s, J), and whether it
/// runs backwards too.
#[derive(Clone, Debug, PartialEq)]
pub struct Reaction {
    pub reactants: Vec<(usize, f64)>,
    pub products: Vec<(usize, f64)>,
    pub a: f64,
    pub beta: f64,
    pub activation: f64,
    pub reversible: bool,
}

impl Reaction {
    /// The reaction `reactants` → `products` (names, moles) in `sys`.
    ///
    /// # Errors
    /// If a species is not in the system or the elements do not balance.
    pub fn new(
        sys: &System,
        reactants: &[(&str, f64)],
        products: &[(&str, f64)],
        a: f64,
        beta: f64,
        activation: f64,
    ) -> Result<Self, String> {
        let find = |list: &[(&str, f64)]| -> Result<Vec<(usize, f64)>, String> {
            list.iter()
                .map(|&(n, nu)| {
                    sys.index(n)
                        .map(|j| (j, nu))
                        .ok_or(format!("no species {n}"))
                })
                .collect()
        };
        let reaction = Self {
            reactants: find(reactants)?,
            products: find(products)?,
            a,
            beta,
            activation,
            reversible: true,
        };
        for (i, row) in sys.atoms.iter().enumerate() {
            let side = |l: &[(usize, f64)]| l.iter().map(|&(j, nu)| row[j] * nu).sum::<f64>();
            let (r, p) = (side(&reaction.reactants), side(&reaction.products));
            if (r - p).abs() > 1e-9 * r.abs().max(1.0) {
                return Err(format!(
                    "element {} does not balance: {r} → {p}",
                    sys.elements[i]
                ));
            }
        }
        Ok(reaction)
    }

    /// Its forward rate constant at `t` (K).
    pub fn rate_constant(&self, t: f64) -> f64 {
        self.a * det::pow(t, self.beta) * det::exp(-self.activation / (R * t))
    }

    /// ΔG°/RT and ΔH°/RT of a mole of it at `t` (standard states: gases at 1 bar).
    pub fn delta_rt(&self, sys: &System, t: f64) -> (f64, f64) {
        let side = |l: &[(usize, f64)], f: &dyn Fn(usize) -> f64| {
            l.iter().map(|&(j, nu)| nu * f(j)).sum::<f64>()
        };
        let g = |j: usize| sys.species[j].g_rt(t);
        let h = |j: usize| sys.species[j].h_rt(t);
        (
            side(&self.products, &g) - side(&self.reactants, &g),
            side(&self.products, &h) - side(&self.reactants, &h),
        )
    }

    /// ln K_c at `t`: −ΔG°/RT + Δν_gas·ln(P°/RT), concentrations in mol/m³.
    pub fn ln_kc(&self, sys: &System, t: f64) -> f64 {
        let side = |l: &[(usize, f64)]| {
            l.iter()
                .map(|&(j, nu)| nu * sys.species[j].g_rt(t))
                .sum::<f64>()
        };
        let gas = |l: &[(usize, f64)]| {
            l.iter()
                .filter(|(j, _)| *j < sys.gases)
                .map(|(_, nu)| nu)
                .sum::<f64>()
        };
        let dg = side(&self.products) - side(&self.reactants);
        -dg + (gas(&self.products) - gas(&self.reactants)) * det::ln(STANDARD_PRESSURE / (R * t))
    }

    /// Its net rate (mol/(m³·s)) and the rate's derivatives with respect to each species'
    /// moles (into `d`, per mol, for Newton), with `moles` of each in `volume` (m³) at `t`.
    /// A condensed reactant that has run out stops the forward reaction (and likewise back).
    pub fn rate(&self, sys: &System, moles: &[f64], volume: f64, t: f64, d: &mut [f64]) -> f64 {
        let k = self.rate_constant(t);
        let kc_inv = if self.reversible {
            det::exp(-self.ln_kc(sys, t))
        } else {
            0.0
        };
        d.fill(0.0);
        let side = |list: &[(usize, f64)], scale: f64, d: &mut [f64]| -> f64 {
            let mut product = 1.0;
            for &(j, nu) in list {
                if j < sys.gases {
                    product *= det::pow((moles[j] / volume).max(0.0), nu);
                } else if moles[j] <= 0.0 {
                    return 0.0;
                }
            }
            for &(j, nu) in list {
                if j < sys.gases && moles[j] > 0.0 {
                    d[j] += scale * product * nu / moles[j];
                }
            }
            scale * product
        };
        let forward = side(&self.reactants, k, d);
        let backward = side(&self.products, -k * kc_inv, d);
        forward + backward
    }
}
