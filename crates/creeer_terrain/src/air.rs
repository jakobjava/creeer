//! A planet's atmosphere as its data file gives it: composition, the surface's pressure and
//! temperature at the datum, and its temperature profile in layers of constant lapse rate by
//! geopotential altitude (as the US Standard Atmosphere defines Earth's). What it makes of
//! them (pressure, density, sound) is `creeer_fluids::atmosphere`'s.

use crate::planet::DataError;
use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;

/// An atmosphere's data.
#[derive(Clone, Debug, PartialEq)]
pub struct AtmosphereData {
    /// Gases (species names of the chemistry's data) and mole fractions, summing to 1.
    pub composition: Vec<(String, f64)>,
    /// At the datum: pressure (Pa) and temperature (K).
    pub surface_pressure: f64,
    pub surface_temperature: f64,
    /// Layers bottom up: each's top (geopotential altitude, m) and lapse rate (K/m: the
    /// temperature's change with geopotential altitude).
    pub layers: Vec<(f64, f64)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AtmosphereDef {
    composition: Vec<(String, f64)>,
    surface_pressure: String,
    surface_temperature: String,
    layers: Vec<(String, String)>,
}

impl AtmosphereDef {
    pub(crate) fn build(&self) -> Result<AtmosphereData, DataError> {
        let total: f64 = self.composition.iter().map(|c| c.1).sum();
        if self.composition.iter().any(|c| c.1 < 0.0) || !(0.99..=1.01).contains(&total) {
            return Err(DataError(format!(
                "atmosphere.composition: mole fractions must sum to 1, not {total}"
            )));
        }
        let field = |name: &str, text: &str, dimension| {
            parse_as(text, dimension).map_err(|e| DataError(format!("atmosphere.{name}: {e}")))
        };
        let mut layers = Vec::with_capacity(self.layers.len());
        for (k, (top, lapse)) in self.layers.iter().enumerate() {
            let top = field(&format!("layers[{k}].top"), top, Dimension::LENGTH)?;
            let lapse = field(
                &format!("layers[{k}].lapse"),
                lapse,
                Dimension::TEMPERATURE.div(Dimension::LENGTH),
            )?;
            if layers.last().is_some_and(|&(last, _)| top <= last) || top <= 0.0 {
                return Err(DataError("atmosphere.layers: tops must rise".into()));
            }
            layers.push((top, lapse));
        }
        let data = AtmosphereData {
            composition: self
                .composition
                .iter()
                .map(|(n, x)| (n.clone(), x / total))
                .collect(),
            surface_pressure: field(
                "surface_pressure",
                &self.surface_pressure,
                Dimension::PRESSURE,
            )?,
            surface_temperature: field(
                "surface_temperature",
                &self.surface_temperature,
                Dimension::TEMPERATURE,
            )?,
            layers,
        };
        if data.surface_pressure <= 0.0 || data.surface_temperature <= 0.0 || data.layers.is_empty()
        {
            return Err(DataError(
                "atmosphere: surface pressure and temperature positive, a layer at least".into(),
            ));
        }
        Ok(data)
    }
}
