//! Planet definitions, loaded from RON data files (`data/planets/*.ron`).
//!
//! Every physical quantity in the file is a string with its unit (`"500 km"`), checked for
//! the right dimension when loaded (`creeer_math::units`).

use crate::datum::Datum;
use crate::elevation::{Elevation, Layer, LayerKind, Mask, Octaves};
use crate::geology::Geology;
use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

/// A planet's bulk properties and terrain generator.
#[derive(Debug)]
pub struct Planet {
    /// Display name.
    pub name: String,
    /// World seed.
    pub seed: u64,
    /// Radius of the datum at the poles (m); the equator bulges by the rotation.
    pub radius: f64,
    /// Gravitational parameter μ = G·M (m³/s²), from the surface gravity at the datum.
    pub mu: f64,
    /// Sidereal rotation period (s).
    pub sidereal_day: f64,
    /// Tilt of the rotation axis against the orbit normal (rad).
    pub axial_tilt: f64,
    /// Sea level: the equipotential through the poles at `radius` (`datum.rs`).
    pub datum: Datum,
    /// Surface elevation model (heights above the datum).
    pub elevation: Arc<Elevation>,
    /// Rocks, provinces and ore deposits.
    pub geology: Geology,
    /// Its atmosphere, if it has one (a generated body from a template has none).
    pub atmosphere: Option<crate::air::AtmosphereData>,
}

impl Planet {
    /// Rotation rate (rad/s).
    pub fn angular_speed(&self) -> f64 {
        std::f64::consts::TAU / self.sidereal_day
    }

    /// Loads a planet from a RON file.
    pub fn load(path: &Path) -> Result<Self, DataError> {
        Self::load_with(path, None)
    }

    /// Loads a planet from a RON file used as a template for a generated body: its bulk
    /// numbers are replaced by `overrides`, and its relief and the wavelengths of its noise
    /// are scaled with the radius, so a 2 km asteroid has the shape of a 100 km one.
    pub fn load_with(path: &Path, overrides: Option<&Overrides>) -> Result<Self, DataError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| DataError(format!("{}: {e}", path.display())))?;
        let def: PlanetDef =
            ron::from_str(&text).map_err(|e| DataError(format!("{}: {e}", path.display())))?;
        let dir = path.parent().unwrap_or(Path::new("."));
        def.build(dir, overrides)
            .map_err(|e| DataError(format!("{}: {}", path.display(), e.0)))
    }
}

/// Bulk numbers of a generated body that replace a template's.
#[derive(Clone, Debug, PartialEq)]
pub struct Overrides {
    /// Name.
    pub name: String,
    /// Seed of its terrain.
    pub seed: u64,
    /// Radius (m).
    pub radius: f64,
    /// Gravitational parameter (m³/s²).
    pub mu: f64,
    /// Sidereal rotation period (s).
    pub sidereal_day: f64,
    /// Axial tilt (rad).
    pub axial_tilt: f64,
}

/// A problem in a data file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataError(pub String);

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DataError {}

/// Reads a quantity field, naming the field in the error.
fn quantity(field: &str, text: &str, dimension: Dimension) -> Result<f64, DataError> {
    parse_as(text, dimension).map_err(|e| DataError(format!("{field}: {e}")))
}

fn positive(field: &str, text: &str, dimension: Dimension) -> Result<f64, DataError> {
    let value = quantity(field, text, dimension)?;
    if value > 0.0 && value.is_finite() {
        Ok(value)
    } else {
        Err(DataError(format!("{field}: {text:?} must be positive")))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename = "Planet")]
struct PlanetDef {
    name: String,
    seed: u64,
    radius: String,
    surface_gravity: String,
    sidereal_day: String,
    axial_tilt: String,
    geology: String,
    elevation: ElevationDef,
    #[serde(default)]
    atmosphere: Option<crate::air::AtmosphereDef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ElevationDef {
    continents: OctavesDef,
    /// (continent noise value, height) points.
    curve: Vec<(f64, String)>,
    layers: Vec<LayerDef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OctavesDef {
    wavelength: String,
    octaves: u32,
    #[serde(default = "half")]
    gain: f64,
}

fn half() -> f64 {
    0.5
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaskDef {
    noise: OctavesDef,
    from: f64,
    to: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerDef {
    name: String,
    kind: LayerKind,
    noise: OctavesDef,
    amplitude: String,
    #[serde(default)]
    mask: Option<MaskDef>,
    #[serde(default)]
    land_only: bool,
}

impl OctavesDef {
    fn build(&self, field: &str, scale: f64) -> Result<Octaves, DataError> {
        if !(1..=24).contains(&self.octaves) {
            return Err(DataError(format!("{field}.octaves must be 1..=24")));
        }
        if !(self.gain > 0.0 && self.gain < 1.0) {
            return Err(DataError(format!("{field}.gain must be in (0, 1)")));
        }
        Ok(Octaves {
            wavelength: scale
                * positive(
                    &format!("{field}.wavelength"),
                    &self.wavelength,
                    Dimension::LENGTH,
                )?,
            octaves: self.octaves,
            gain: self.gain,
        })
    }
}

impl PlanetDef {
    /// Builds the planet; `dir` is the directory of its file (the geology file is relative
    /// to it, rocks and minerals are in `../geology`).
    fn build(self, dir: &Path, overrides: Option<&Overrides>) -> Result<Planet, DataError> {
        let template_radius = positive("radius", &self.radius, Dimension::LENGTH)?;
        let g = positive(
            "surface_gravity",
            &self.surface_gravity,
            Dimension::ACCELERATION,
        )?;
        let radius = overrides.map_or(template_radius, |o| o.radius);
        // Heights and wavelengths scale with the radius (1 for a planet's own file).
        let scale = radius / template_radius;
        let mu = overrides.map_or(g * radius * radius, |o| o.mu);
        let seed = overrides.map_or(self.seed, |o| o.seed);
        let e = self.elevation;
        let mut curve = Vec::with_capacity(e.curve.len());
        for (i, (x, h)) in e.curve.iter().enumerate() {
            let h = scale * quantity(&format!("elevation.curve[{i}]"), h, Dimension::LENGTH)?;
            if !(0.0..=1.0).contains(x) || curve.last().is_some_and(|&(last, _)| *x <= last) {
                return Err(DataError(
                    "elevation.curve: surface fractions must ascend within 0..=1".into(),
                ));
            }
            curve.push((*x, h));
        }
        if curve.len() < 2 {
            return Err(DataError("elevation.curve needs two points".into()));
        }
        let mut layers = Vec::with_capacity(e.layers.len());
        for l in &e.layers {
            let field = format!("elevation.layers[{}]", l.name);
            let mask = match &l.mask {
                Some(m) if 0.0 <= m.from && m.from < m.to && m.to <= 1.0 => Some(Mask {
                    noise: m.noise.build(&format!("{field}.mask"), scale)?,
                    from: m.from,
                    to: m.to,
                }),
                Some(_) => {
                    return Err(DataError(format!(
                        "{field}.mask: need 0 <= from < to <= 1 (surface fractions)"
                    )));
                }
                None => None,
            };
            layers.push(Layer {
                name: l.name.clone(),
                kind: l.kind,
                noise: l.noise.build(&field, scale)?,
                amplitude: scale
                    * positive(
                        &format!("{field}.amplitude"),
                        &l.amplitude,
                        Dimension::LENGTH,
                    )?,
                mask,
                land_only: l.land_only,
            });
        }
        let continents = e.continents.build("elevation.continents", scale)?;
        let elevation = Arc::new(Elevation::new(radius, seed, continents, curve, layers));
        let sidereal_day = match overrides {
            Some(o) => o.sidereal_day,
            None => positive("sidereal_day", &self.sidereal_day, Dimension::TIME)?,
        };
        let datum = Datum {
            polar_radius: radius,
            mu,
            omega: std::f64::consts::TAU / sidereal_day,
        };
        let atmosphere = match (&self.atmosphere, overrides) {
            (Some(a), None) => Some(a.build()?),
            _ => None,
        };
        Ok(Planet {
            atmosphere,
            datum,
            name: overrides.map_or(self.name, |o| o.name.clone()),
            seed,
            radius,
            mu,
            sidereal_day,
            axial_tilt: match overrides {
                Some(o) => o.axial_tilt,
                None => quantity("axial_tilt", &self.axial_tilt, Dimension::NONE)?,
            },
            geology: Geology::load(
                &dir.join(&self.geology),
                &dir.join("../geology"),
                Arc::clone(&elevation),
                datum,
                seed,
            )?,
            elevation,
        })
    }
}
