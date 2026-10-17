//! A property that depends on temperature: straight lines between measured points (as the
//! standards tabulate them, e.g. EN 1993-1-2 for steel in fire), constant beyond the ends.

use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;

/// A property against temperature (K).
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    /// (temperature K, value), temperatures increasing.
    points: Vec<(f64, f64)>,
}

impl Curve {
    /// The same value at every temperature.
    pub fn constant(value: f64) -> Self {
        Self {
            points: vec![(0.0, value)],
        }
    }

    /// Through `points` (temperature K, value).
    ///
    /// # Errors
    /// If there are none, or the temperatures do not increase, or a number is not finite.
    pub fn new(points: Vec<(f64, f64)>) -> Result<Self, String> {
        if points.is_empty() {
            return Err("a curve needs a point".into());
        }
        if points.iter().any(|(t, v)| !t.is_finite() || !v.is_finite()) {
            return Err("a curve's numbers must be finite".into());
        }
        if points.windows(2).any(|w| w[1].0 <= w[0].0) {
            return Err("a curve's temperatures must increase".into());
        }
        Ok(Self { points })
    }

    /// The value at temperature `t` (K).
    pub fn at(&self, t: f64) -> f64 {
        let p = &self.points;
        let k = p.partition_point(|&(pt, _)| pt <= t);
        match k {
            0 => p[0].1,
            k if k == p.len() => p[k - 1].1,
            k => {
                let ((t0, v0), (t1, v1)) = (p[k - 1], p[k]);
                v0 + (v1 - v0) * (t - t0) / (t1 - t0)
            }
        }
    }

    /// ∫ value dT from `from` to `to` (K), exactly (a thermal elongation from its expansion
    /// coefficient, an enthalpy from a specific heat); negative if `to` < `from`.
    pub fn integral(&self, from: f64, to: f64) -> f64 {
        if to < from {
            return -self.integral(to, from);
        }
        // The curve is linear between consecutive breaks (its points, and the ends).
        let mut breaks: Vec<f64> = vec![from];
        breaks.extend(
            self.points
                .iter()
                .map(|p| p.0)
                .filter(|&t| from < t && t < to),
        );
        breaks.push(to);
        breaks
            .windows(2)
            .map(|w| 0.5 * (self.at(w[0]) + self.at(w[1])) * (w[1] - w[0]))
            .sum()
    }

    /// The smallest value anywhere.
    pub fn min(&self) -> f64 {
        self.points
            .iter()
            .map(|p| p.1)
            .fold(f64::INFINITY, f64::min)
    }

    /// The points.
    pub fn points(&self) -> &[(f64, f64)] {
        &self.points
    }
}

/// How a data file writes a property: one value with its unit, or a table against
/// temperature.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum CurveDef {
    /// `"210 GPa"`.
    Value(String),
    /// `[("20 °C", "210 GPa"), ("600 °C", "65 GPa")]`.
    Table(Vec<(String, String)>),
}

impl CurveDef {
    /// The curve, its values in `dimension`.
    ///
    /// # Errors
    /// If a unit does not parse or is of another dimension, or the table is not a curve.
    pub fn parse(&self, dimension: Dimension) -> Result<Curve, String> {
        let value = |s: &str| parse_as(s, dimension).map_err(|e| format!("{s:?}: {e}"));
        match self {
            Self::Value(s) => Ok(Curve::constant(value(s)?)),
            Self::Table(rows) => {
                let points = rows
                    .iter()
                    .map(|(t, v)| {
                        let t = parse_as(t, Dimension::TEMPERATURE)
                            .map_err(|e| format!("{t:?}: {e}"))?;
                        Ok((t, value(v)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Curve::new(points)
            }
        }
    }
}
