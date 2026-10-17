//! Electrochemistry: a reaction driven (electrolysis) or run (a fuel cell) by a current.
//!
//! - **Faraday's law:** z moles of electrons per mole of reaction, I·t/(z·F) moles of it.
//! - **Reversible voltage:** E = ΔG/(z·F), the least voltage that drives it, from the species'
//!   data at the cell's temperature, with the Nernst term for its gases' partial pressures:
//!   E = E° + RT/(zF)·ln(Π p_products^ν / Π p_reactants^ν) (bar; liquids and solids at 1).
//! - **Thermoneutral voltage:** ΔH/(z·F): above it the cell heats, below it (but above E) it
//!   draws the difference, T·ΔS, as heat from around it.
//!
//! The cell's electrical side (its overpotential, Butler–Volmer, and its resistance) is a
//! device of the circuit (`creeer_electric`): what it takes at E is the reaction's ΔG, the
//! rest its heat.

use crate::kinetics::Reaction;
use crate::system::System;
use creeer_materials::species::R;
use creeer_math::det;

/// The Faraday constant (C/mol): exact in the SI since 2019 (e·N_A).
pub const FARADAY: f64 = 96_485.332_12;

/// A reaction run by electrons: its stoichiometry and z, the electrons per mole of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Electrochemical {
    pub reaction: Reaction,
    pub electrons: f64,
}

impl Electrochemical {
    /// Its reversible voltage at `t` (V) with each gas at `pressure(species)` (bar).
    pub fn reversible_voltage(&self, sys: &System, t: f64, pressure: impl Fn(usize) -> f64) -> f64 {
        let (dg, _) = self.reaction.delta_rt(sys, t);
        let activity = |l: &[(usize, f64)]| -> f64 {
            l.iter()
                .filter(|(j, _)| *j < sys.gases)
                .map(|&(j, nu)| nu * det::ln(pressure(j)))
                .sum()
        };
        let nernst = activity(&self.reaction.products) - activity(&self.reaction.reactants);
        R * t * (dg + nernst) / (self.electrons * FARADAY)
    }

    /// Its thermoneutral voltage at `t` (V): ΔH/(z·F).
    pub fn thermoneutral_voltage(&self, sys: &System, t: f64) -> f64 {
        let (_, dh) = self.reaction.delta_rt(sys, t);
        R * t * dh / (self.electrons * FARADAY)
    }

    /// Moles of reaction `current` (A) drives in `dt` (s).
    pub fn extent(&self, current: f64, dt: f64) -> f64 {
        current * dt / (self.electrons * FARADAY)
    }
}
