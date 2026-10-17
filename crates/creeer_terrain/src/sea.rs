//! A planet's ocean as its data file gives it: the water that fills every basin up to the
//! datum (sea level, the equipotential of its rotating field), and the water's density,
//! viscosity, speed of sound and temperature. What floats in it is `creeer_sim::sea`'s.

use crate::planet::DataError;
use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;

/// An ocean's data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanData {
    /// Density (kg/m³), dynamic viscosity (Pa·s), speed of sound (m/s), temperature (K).
    pub density: f64,
    pub viscosity: f64,
    pub sound: f64,
    pub temperature: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OceanDef {
    density: String,
    viscosity: String,
    sound: String,
    temperature: String,
}

impl OceanDef {
    pub(crate) fn build(&self) -> Result<OceanData, DataError> {
        let field = |name: &str, text: &str, dimension| match parse_as(text, dimension) {
            Ok(v) if v > 0.0 => Ok(v),
            Ok(_) => Err(DataError(format!("ocean.{name}: must be positive"))),
            Err(e) => Err(DataError(format!("ocean.{name}: {e}"))),
        };
        Ok(OceanData {
            density: field("density", &self.density, Dimension::DENSITY)?,
            viscosity: field(
                "viscosity",
                &self.viscosity,
                Dimension::PRESSURE.mul(Dimension::TIME),
            )?,
            sound: field("sound", &self.sound, Dimension::VELOCITY)?,
            temperature: field("temperature", &self.temperature, Dimension::TEMPERATURE)?,
        })
    }
}
