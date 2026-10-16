//! Chemical formulas: `"CuFeS2"`, `"Al(OH)3"`, `"CaSO4·2H2O"`, `"Fe4.5Ni4.5S8"`.
//!
//! A formula is parsed into atoms per formula unit, by atomic number. Groups in
//! parentheses take a count; `·` (or `*`) joins
//! parts such as water of crystallisation, each with an optional leading coefficient.
//! Decimal counts describe solid solutions (pentlandite, (Fe,Ni)₉S₈, as `Fe4.5Ni4.5S8`).

use crate::elements::{atomic_number, atomic_weight, symbol};
use std::collections::BTreeMap;
use std::fmt;

/// Atoms per formula unit, by atomic number.
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    /// Atomic number → number of atoms.
    pub atoms: BTreeMap<u8, f64>,
}

/// Why a formula could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaError(pub String);

impl fmt::Display for FormulaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormulaError {}

impl Formula {
    /// Parses a formula.
    pub fn parse(text: &str) -> Result<Self, FormulaError> {
        let err = || FormulaError(format!("malformed formula {text:?}"));
        let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        if chars.is_empty() {
            return Err(err());
        }
        let mut atoms = BTreeMap::new();
        for part in split_parts(&chars) {
            let mut at = 0;
            let coefficient = number(part, &mut at).unwrap_or(1.0);
            let group = sequence(part, &mut at, text)?;
            if at != part.len() || group.is_empty() {
                return Err(err());
            }
            for (z, n) in group {
                *atoms.entry(z).or_insert(0.0) += coefficient * n;
            }
        }
        Ok(Self { atoms })
    }

    /// Molar mass (kg/mol).
    pub fn molar_mass(&self) -> f64 {
        self.atoms
            .iter()
            .map(|(&z, &n)| atomic_weight(z) * n)
            .sum::<f64>()
            * 1e-3
    }

    /// Mass fraction of element `z` (0..1).
    pub fn mass_fraction(&self, z: u8) -> f64 {
        let n = self.atoms.get(&z).copied().unwrap_or(0.0);
        atomic_weight(z) * n * 1e-3 / self.molar_mass()
    }
}

impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (&z, &n) in &self.atoms {
            if (n - 1.0).abs() < 1e-12 {
                write!(f, "{}", symbol(z))?;
            } else {
                write!(f, "{}{n}", symbol(z))?;
            }
        }
        Ok(())
    }
}

/// Splits at `·` or `*`.
fn split_parts(chars: &[char]) -> Vec<&[char]> {
    chars.split(|&c| c == '·' || c == '*').collect()
}

/// A decimal number at `at`, if any.
fn number(chars: &[char], at: &mut usize) -> Option<f64> {
    let start = *at;
    while chars
        .get(*at)
        .is_some_and(|c| c.is_ascii_digit() || *c == '.')
    {
        *at += 1;
    }
    if *at == start {
        return None;
    }
    chars[start..*at].iter().collect::<String>().parse().ok()
}

/// Elements and parenthesised groups, each with an optional count, up to `)` or the end.
fn sequence(chars: &[char], at: &mut usize, text: &str) -> Result<Vec<(u8, f64)>, FormulaError> {
    let mut out = Vec::new();
    while let Some(&c) = chars.get(*at) {
        let group = if c == '(' {
            *at += 1;
            let inner = sequence(chars, at, text)?;
            if chars.get(*at) != Some(&')') || inner.is_empty() {
                return Err(FormulaError(format!("unbalanced parentheses in {text:?}")));
            }
            *at += 1;
            inner
        } else if c.is_ascii_uppercase() {
            let mut name = String::from(c);
            *at += 1;
            if let Some(&l) = chars.get(*at).filter(|l| l.is_ascii_lowercase()) {
                name.push(l);
                *at += 1;
            }
            let z = atomic_number(&name)
                .ok_or_else(|| FormulaError(format!("unknown element {name:?} in {text:?}")))?;
            vec![(z, 1.0)]
        } else {
            break;
        };
        let count = number(chars, at).unwrap_or(1.0);
        out.extend(group.into_iter().map(|(z, n)| (z, n * count)));
    }
    Ok(out)
}
