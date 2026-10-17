//! Chemical species and their thermodynamics (F6.5): NASA-9 polynomials for each species'
//! heat capacity, enthalpy and entropy in its standard state (1 bar), from NASA Glenn's
//! database (`data/chemistry/species.ron`: the CEA program's thermo.inp; McBride, Zehe &
//! Gordon, NASA/TP-2002-211556), 200 K to 6000 K (20,000 K for many gases).
//!
//! c_p/R = a₁T⁻² + a₂T⁻¹ + a₃ + a₄T + a₅T² + a₆T³ + a₇T⁴,
//! H/RT = −a₁T⁻² + a₂T⁻¹ ln T + a₃ + a₄T/2 + a₅T²/3 + a₆T³/4 + a₇T⁴/5 + b₁/T,
//! S/R = −a₁T⁻²/2 − a₂T⁻¹ + a₃ ln T + a₄T + a₅T²/2 + a₆T³/3 + a₇T⁴/4 + b₂.
//!
//! H at 298.15 K is the heat of formation (zero for elements in their reference state), so
//! enthalpies of different species add up across reactions. A condensed phase is a species
//! of its own (iron's α, γ and δ phases and its liquid each have theirs), valid over its
//! range only: which one exists at a temperature is for equilibrium to decide.

use crate::elements::atomic_number;
use crate::formula::Formula;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

/// The gas constant (J/(mol·K)), exact in the SI since 2019.
pub const R: f64 = 8.314_462_618;
/// The standard pressure of the data (Pa): 1 bar.
pub const STANDARD_PRESSURE: f64 = 1e5;

/// A species' phase.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
pub enum Phase {
    Gas,
    Liquid,
    Solid,
}

/// A chemical species.
#[derive(Clone, Debug, PartialEq)]
pub struct Species {
    /// Its name in the data (NASA's: `H2O`, `H2O(L)`, `Fe(a)`, `CaCO3(caL)`).
    pub name: String,
    pub formula: Formula,
    pub phase: Phase,
    /// Its molar mass (kg/mol).
    pub molar_mass: f64,
    /// Temperature range boundaries (K) and a polynomial for each range.
    ranges: Vec<f64>,
    polynomials: Vec<[f64; 9]>,
    /// Where its data come from.
    pub note: String,
}

#[derive(Deserialize)]
struct SpeciesDef {
    name: String,
    atoms: Vec<(String, f64)>,
    phase: Phase,
    ranges: Vec<f64>,
    polynomials: Vec<Vec<f64>>,
    note: String,
}

impl Species {
    fn build(def: SpeciesDef) -> Result<Self, String> {
        let mut atoms = BTreeMap::new();
        for (symbol, n) in &def.atoms {
            let z =
                atomic_number(symbol).ok_or(format!("{}: unknown element {symbol}", def.name))?;
            *atoms.entry(z).or_insert(0.0) += n;
        }
        let ordered = def.ranges.windows(2).all(|w| w[0] < w[1]);
        if def.polynomials.is_empty() || def.ranges.len() != def.polynomials.len() + 1 || !ordered {
            return Err(format!("{}: ranges do not fit its polynomials", def.name));
        }
        let polynomials = (def.polynomials.iter())
            .map(|p| {
                <[f64; 9]>::try_from(p.as_slice())
                    .map_err(|_| format!("{}: a polynomial of {} coefficients", def.name, p.len()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let formula = Formula { atoms };
        Ok(Self {
            molar_mass: formula.molar_mass(),
            name: def.name,
            formula,
            phase: def.phase,
            ranges: def.ranges,
            polynomials,
            note: def.note,
        })
    }

    /// The temperatures its data cover (K).
    pub fn range(&self) -> (f64, f64) {
        (self.ranges[0], self.ranges[self.ranges.len() - 1])
    }

    /// Whether its data cover `t` (K).
    pub fn covers(&self, t: f64) -> bool {
        let (low, high) = self.range();
        (low..=high).contains(&t)
    }

    /// The polynomial for `t` (K): its range's, or the nearest end's beyond them.
    fn polynomial(&self, t: f64) -> &[f64; 9] {
        let k = self.ranges[1..self.ranges.len() - 1]
            .iter()
            .take_while(|&&b| t > b)
            .count();
        &self.polynomials[k]
    }

    /// c_p/R at `t` (K).
    pub fn cp_r(&self, t: f64) -> f64 {
        let a = self.polynomial(t);
        (a[0] / t + a[1]) / t + a[2] + t * (a[3] + t * (a[4] + t * (a[5] + t * a[6])))
    }

    /// H/RT at `t` (K).
    pub fn h_rt(&self, t: f64) -> f64 {
        let a = self.polynomial(t);
        let ln = creeer_math::det::ln(t);
        -a[0] / (t * t)
            + a[1] * ln / t
            + a[2]
            + t * (a[3] / 2.0 + t * (a[4] / 3.0 + t * (a[5] / 4.0 + t * a[6] / 5.0)))
            + a[7] / t
    }

    /// S/R at `t` (K), in the standard state.
    pub fn s_r(&self, t: f64) -> f64 {
        let a = self.polynomial(t);
        let ln = creeer_math::det::ln(t);
        -a[0] / (2.0 * t * t) - a[1] / t
            + a[2] * ln
            + t * (a[3] + t * (a[4] / 2.0 + t * (a[5] / 3.0 + t * a[6] / 4.0)))
            + a[8]
    }

    /// G/RT at `t` (K), in the standard state: H/RT − S/R.
    pub fn g_rt(&self, t: f64) -> f64 {
        self.h_rt(t) - self.s_r(t)
    }

    /// Molar heat capacity (J/(mol·K)), enthalpy (J/mol), entropy (J/(mol·K)) and Gibbs
    /// energy (J/mol) at `t` (K), standard state.
    pub fn cp(&self, t: f64) -> f64 {
        R * self.cp_r(t)
    }
    pub fn h(&self, t: f64) -> f64 {
        R * t * self.h_rt(t)
    }
    pub fn s(&self, t: f64) -> f64 {
        R * self.s_r(t)
    }
    pub fn g(&self, t: f64) -> f64 {
        R * t * self.g_rt(t)
    }
}

/// Every species, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpeciesTable {
    species: Vec<Species>,
    index: HashMap<String, usize>,
}

/// The reference data shipped with the game.
const REFERENCE: &str = include_str!("../../../data/chemistry/species.ron");

impl SpeciesTable {
    /// Reads a species file.
    ///
    /// # Errors
    /// If it does not parse, a species is invalid, or a name repeats.
    pub fn parse(text: &str) -> Result<Self, String> {
        let defs: Vec<SpeciesDef> = ron::from_str(text).map_err(|e| format!("species: {e}"))?;
        let mut table = Self::default();
        for def in defs {
            let species = Species::build(def)?;
            if table
                .index
                .insert(species.name.clone(), table.species.len())
                .is_some()
            {
                return Err(format!("species {} defined twice", species.name));
            }
            table.species.push(species);
        }
        Ok(table)
    }

    /// The species the game ships with.
    ///
    /// # Panics
    /// If the shipped data is broken (a test keeps it whole).
    pub fn reference() -> Self {
        Self::parse(REFERENCE).expect("the shipped species parse")
    }

    /// The species `name` and its index.
    pub fn get(&self, name: &str) -> Option<&Species> {
        self.index.get(name).map(|&k| &self.species[k])
    }

    /// The index of species `name`.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    /// All species.
    pub fn all(&self) -> &[Species] {
        &self.species
    }
}
