//! Physical quantities written with their units, as data files use them: `"7850 kg/m^3"`,
//! `"500 km"`, `"9.81 m/s^2"`, `"6 h"` (plan.md §1, principle 7).
//!
//! [`parse`] returns the value in SI units together with its [`Dimension`]; loaders call
//! [`parse_as`] with the dimension they expect, so `"5 kg"` where a length belongs is an error
//! instead of a silently wrong number.
//!
//! Syntax: a number, then a unit expression. Factors are joined by `*`, `·` or a space and
//! may carry an integer power (`m^3`, `m**3`, `m³`, `s^-2`). Everything after a `/` up to
//! the next `/` is in the denominator: `W/m·K` is W/(m·K) and `J/kg/K` is J/(kg·K).
//! Parentheses group: `J/(kg·K)`. SI prefixes (`k`, `M`, `m`, `µ`/`u`, ...) apply to units
//! that take them. `°C` (or `degC`) is accepted only on its own and converts to kelvin.

mod expr;
#[cfg(test)]
mod tests;

use std::fmt;

/// Exponents of the SI base units m, kg, s, A, K, mol, cd.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dimension(pub [i8; 7]);

const fn base(index: usize) -> Dimension {
    let mut e = [0; 7];
    e[index] = 1;
    Dimension(e)
}

impl Dimension {
    /// A pure number.
    pub const NONE: Self = Self([0; 7]);
    /// Metre.
    pub const LENGTH: Self = base(0);
    /// Kilogram.
    pub const MASS: Self = base(1);
    /// Second.
    pub const TIME: Self = base(2);
    /// Ampere.
    pub const CURRENT: Self = base(3);
    /// Kelvin.
    pub const TEMPERATURE: Self = base(4);
    /// Mole.
    pub const AMOUNT: Self = base(5);
    /// Candela.
    pub const LUMINOSITY: Self = base(6);
    /// m².
    pub const AREA: Self = Self::LENGTH.pow(2);
    /// m³.
    pub const VOLUME: Self = Self::LENGTH.pow(3);
    /// m/s.
    pub const VELOCITY: Self = Self::LENGTH.div(Self::TIME);
    /// m/s².
    pub const ACCELERATION: Self = Self::VELOCITY.div(Self::TIME);
    /// kg/m³.
    pub const DENSITY: Self = Self::MASS.div(Self::VOLUME);
    /// N = kg·m/s².
    pub const FORCE: Self = Self::MASS.mul(Self::ACCELERATION);
    /// Pa = N/m².
    pub const PRESSURE: Self = Self::FORCE.div(Self::AREA);
    /// J = N·m.
    pub const ENERGY: Self = Self::FORCE.mul(Self::LENGTH);
    /// W = J/s.
    pub const POWER: Self = Self::ENERGY.div(Self::TIME);
    /// Hz = 1/s.
    pub const FREQUENCY: Self = Self::NONE.div(Self::TIME);

    /// Product of two dimensions.
    pub const fn mul(self, other: Self) -> Self {
        let mut e = self.0;
        let mut i = 0;
        while i < 7 {
            e[i] += other.0[i];
            i += 1;
        }
        Self(e)
    }

    /// Quotient of two dimensions.
    pub const fn div(self, other: Self) -> Self {
        self.mul(other.pow(-1))
    }

    /// Integer power.
    pub const fn pow(self, n: i8) -> Self {
        let mut e = self.0;
        let mut i = 0;
        while i < 7 {
            e[i] *= n;
            i += 1;
        }
        Self(e)
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];
        if *self == Self::NONE {
            return f.write_str("1 (dimensionless)");
        }
        let mut first = true;
        for (name, &e) in NAMES.iter().zip(&self.0).filter(|(_, e)| **e != 0) {
            if !first {
                f.write_str("·")?;
            }
            first = false;
            if e == 1 {
                f.write_str(name)?;
            } else {
                write!(f, "{name}^{e}")?;
            }
        }
        Ok(())
    }
}

/// A value in SI units with its dimension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantity {
    /// Value in SI base units.
    pub value: f64,
    /// Its dimension.
    pub dimension: Dimension,
}

/// Why a quantity could not be read.
#[derive(Clone, Debug, PartialEq)]
pub enum UnitError {
    /// No number at the start.
    BadNumber(String),
    /// A unit symbol that is not known.
    UnknownUnit(String),
    /// Malformed unit expression.
    Syntax(String),
    /// Correct syntax, but not the expected kind of quantity.
    WrongDimension {
        /// The text that was parsed.
        text: String,
        /// What the caller needed.
        expected: Dimension,
        /// What the text says.
        found: Dimension,
    },
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadNumber(t) => write!(f, "{t:?} does not start with a number"),
            Self::UnknownUnit(u) => write!(f, "unknown unit {u:?}"),
            Self::Syntax(t) => write!(f, "malformed unit expression {t:?}"),
            Self::WrongDimension {
                text,
                expected,
                found,
            } => write!(f, "{text:?} has dimension {found}, expected {expected}"),
        }
    }
}

impl std::error::Error for UnitError {}

/// Parses `"<number> <unit expression>"` into SI.
pub fn parse(text: &str) -> Result<Quantity, UnitError> {
    let text = text.trim();
    let (number, rest) = split_number(text).ok_or_else(|| UnitError::BadNumber(text.into()))?;
    let value: f64 = number
        .parse()
        .map_err(|_| UnitError::BadNumber(text.into()))?;
    let rest = rest.trim();
    if rest == "°C" || rest == "degC" {
        return Ok(Quantity {
            value: value + 273.15,
            dimension: Dimension::TEMPERATURE,
        });
    }
    if rest.is_empty() {
        return Ok(Quantity {
            value,
            dimension: Dimension::NONE,
        });
    }
    let (factor, dimension) = expr::unit_expression(rest)?;
    Ok(Quantity {
        value: value * factor,
        dimension,
    })
}

/// Parses `text` and checks that it has the `expected` dimension; returns the SI value.
pub fn parse_as(text: &str, expected: Dimension) -> Result<f64, UnitError> {
    let q = parse(text)?;
    if q.dimension == expected {
        Ok(q.value)
    } else {
        Err(UnitError::WrongDimension {
            text: text.into(),
            expected,
            found: q.dimension,
        })
    }
}

/// Splits off a leading decimal number (sign, digits, fraction, exponent).
fn split_number(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut i = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let digits_start = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    // An exponent only if digits follow, so "1 eV" keeps its unit.
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        let mut j = i + 1;
        if j < bytes.len() && matches!(bytes[j], b'+' | b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    Some(text.split_at(i))
}
