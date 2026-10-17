//! A chemical system: the species a reactor, a flame or a furnace may hold, their elements,
//! and what a mixture of them is worth (enthalpy, entropy, Gibbs energy, heat capacity) as
//! an ideal gas with pure condensed phases.

use creeer_materials::species::{Phase, R, STANDARD_PRESSURE, Species, SpeciesTable};

/// The species of a system and the elements they are made of.
#[derive(Clone, Debug)]
pub struct System {
    /// The species (copied from the table), gases first, then condensed phases.
    pub species: Vec<Species>,
    /// How many of them are gases.
    pub gases: usize,
    /// The elements (atomic numbers), and atoms of each per formula unit of each species:
    /// `atoms[i][j]`, element i in species j.
    pub elements: Vec<u8>,
    pub atoms: Vec<Vec<f64>>,
}

impl System {
    /// The system of the species `names` from `table`.
    ///
    /// # Errors
    /// If a name is not in the table.
    pub fn new(table: &SpeciesTable, names: &[&str]) -> Result<Self, String> {
        let mut picked: Vec<Species> = names
            .iter()
            .map(|n| table.get(n).cloned().ok_or(format!("no species {n}")))
            .collect::<Result<_, _>>()?;
        // Gases first (stable within each kind).
        picked.sort_by_key(|s| s.phase != Phase::Gas);
        let gases = picked.iter().filter(|s| s.phase == Phase::Gas).count();
        let mut elements: Vec<u8> = picked
            .iter()
            .flat_map(|s| s.formula.atoms.keys().copied())
            .collect();
        elements.sort_unstable();
        elements.dedup();
        let atoms = elements
            .iter()
            .map(|z| {
                picked
                    .iter()
                    .map(|s| s.formula.atoms.get(z).copied().unwrap_or(0.0))
                    .collect()
            })
            .collect();
        Ok(Self {
            species: picked,
            gases,
            elements,
            atoms,
        })
    }

    /// Every species of `table` made only of the elements of `names` (as the CEA program
    /// takes its products): the system that can form from them, gases and condensed phases.
    ///
    /// # Errors
    /// If a name is not in the table.
    pub fn of_elements(table: &SpeciesTable, names: &[&str]) -> Result<Self, String> {
        let seed = Self::new(table, names)?;
        let all: Vec<&str> = table
            .all()
            .iter()
            .filter(|s| s.formula.atoms.keys().all(|z| seed.elements.contains(z)))
            .map(|s| s.name.as_str())
            .collect();
        Self::new(table, &all)
    }

    /// The index of species `name`.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.species.iter().position(|s| s.name == name)
    }

    /// Moles of each element in `moles` of the species.
    pub fn element_moles(&self, moles: &[f64]) -> Vec<f64> {
        self.atoms
            .iter()
            .map(|row| row.iter().zip(moles).map(|(a, n)| a * n).sum())
            .collect()
    }

    /// The chemical potential of species `j` over RT at `t` (K) and `p` (Pa) in a mixture
    /// with `moles` of each and `gas` moles of gas: g°/RT + ln(n_j/n) + ln(P/P°) for a gas,
    /// g°/RT for a pure condensed phase.
    pub fn mu_rt(&self, j: usize, t: f64, p: f64, moles: &[f64], gas: f64) -> f64 {
        let g = self.species[j].g_rt(t);
        if j < self.gases {
            let ln = creeer_math::det::ln;
            g + ln(moles[j].max(1e-300) / gas) + ln(p / STANDARD_PRESSURE)
        } else {
            g
        }
    }

    /// The mixture's enthalpy (J, from the elements at 298.15 K).
    pub fn enthalpy(&self, moles: &[f64], t: f64) -> f64 {
        self.species
            .iter()
            .zip(moles)
            .map(|(s, n)| n * s.h(t))
            .sum()
    }

    /// The mixture's heat capacity at constant pressure (J/K), frozen (no reactions).
    pub fn heat_capacity(&self, moles: &[f64], t: f64) -> f64 {
        self.species
            .iter()
            .zip(moles)
            .map(|(s, n)| n * s.cp(t))
            .sum()
    }

    /// The mixture's entropy (J/K) at `t` and `p`: the gases' with their mixing and pressure
    /// terms, the condensed phases' as pure.
    pub fn entropy(&self, moles: &[f64], t: f64, p: f64) -> f64 {
        let gas: f64 = moles[..self.gases].iter().sum();
        let ln = creeer_math::det::ln;
        (self.species.iter().zip(moles).enumerate())
            .filter(|(_, (_, n))| **n > 0.0)
            .map(|(j, (s, n))| {
                let mixing = if j < self.gases {
                    ln(n / gas) + ln(p / STANDARD_PRESSURE)
                } else {
                    0.0
                };
                n * (s.s(t) - R * mixing)
            })
            .sum()
    }

    /// The mixture's Gibbs energy (J) at `t` and `p`.
    pub fn gibbs(&self, moles: &[f64], t: f64, p: f64) -> f64 {
        let gas: f64 = moles[..self.gases].iter().sum();
        (0..self.species.len())
            .filter(|&j| moles[j] > 0.0)
            .map(|j| moles[j] * R * t * self.mu_rt(j, t, p, moles, gas))
            .sum()
    }

    /// The mixture's mass (kg).
    pub fn mass(&self, moles: &[f64]) -> f64 {
        self.species
            .iter()
            .zip(moles)
            .map(|(s, n)| n * s.molar_mass)
            .sum()
    }
}
