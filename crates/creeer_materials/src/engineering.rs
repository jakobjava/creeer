//! Engineering materials: what parts are made of, with the properties every domain needs
//! (`data/materials/*.ron`, every value with its source).
//!
//! - **Mechanical:** density; Young's and shear moduli, yield and ultimate strength against
//!   temperature (a steel frame in a fire loses half its strength by 600 °C, EN 1993-1-2);
//!   elongation at break and the hardening and strain-rate laws of the structure solver.
//! - **Thermal:** conductivity, specific heat and expansion against temperature; melting
//!   (solidus, liquidus, latent heat) or the temperature where it decomposes instead.
//! - **Electrical:** resistivity against temperature.
//! - **Surface:** thermal emissivity and solar absorptivity.

use crate::curve::{Curve, CurveDef};
use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;

/// What kind of material it is (how it fails, broadly).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Kind {
    /// Ductile metals.
    Metal,
    /// Timber.
    Wood,
    /// Concrete and mortar.
    Concrete,
    /// Glass.
    Glass,
    /// Natural stone.
    Rock,
    /// Water ice.
    Ice,
}

/// How a material melts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Melting {
    /// Where it starts to melt (K).
    pub solidus: f64,
    /// Where it is all liquid (K).
    pub liquidus: f64,
    /// Heat of fusion (J/kg).
    pub latent_heat: f64,
}

/// The strain-rate dependence of the flow stress (Cowper–Symonds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RateLaw {
    /// D (1/s).
    pub d: f64,
    /// q.
    pub q: f64,
    /// The strain rate the static strengths were measured at (1/s).
    pub reference: f64,
}

/// An engineering material.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// Its id in data (`"steel_s235"`).
    pub id: String,
    /// Its name.
    pub name: String,
    /// What kind it is.
    pub kind: Kind,
    /// Density (kg/m³).
    pub density: f64,
    /// Young's modulus (Pa).
    pub youngs_modulus: Curve,
    /// Shear modulus (Pa).
    pub shear_modulus: Curve,
    /// Yield strength (Pa); for brittle materials, the strength.
    pub yield_strength: Curve,
    /// Ultimate tensile strength (Pa).
    pub ultimate_strength: Curve,
    /// Compressive strength where it differs from the tensile (Pa).
    pub compressive_strength: Option<f64>,
    /// Wood's compressive strength across the grain (Pa): what an edge driven into it crushes
    /// (G1.7, chopping).
    pub across: Option<f64>,
    /// Plastic strain at break.
    pub elongation: f64,
    /// Plastic strain where strain hardening starts (after a yield plateau).
    pub hardening_start: f64,
    /// Plastic strain at the ultimate strength.
    pub uniform_elongation: f64,
    /// Strain-rate dependence, if it matters.
    pub rate: Option<RateLaw>,
    /// Material damping (loss factor of the structure solver's dashpots).
    pub damping: f64,
    /// Thermal conductivity (W/(m·K)).
    pub conductivity: Curve,
    /// Specific heat (J/(kg·K)).
    pub specific_heat: Curve,
    /// Coefficient of linear thermal expansion (1/K).
    pub expansion: Curve,
    /// Electrical resistivity (Ω·m).
    pub resistivity: Curve,
    /// Thermal emissivity of its usual surface.
    pub emissivity: f64,
    /// Solar absorptivity of its usual surface.
    pub absorptivity: f64,
    /// How it melts, if it does.
    pub melting: Option<Melting>,
    /// Where it decomposes, burns or softens instead (K), if it does.
    pub decomposes: Option<f64>,
    /// Where its values come from.
    pub sources: String,
}

/// A material's strength properties at one temperature, as the structure solver takes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Structural {
    /// Young's modulus (Pa).
    pub youngs_modulus: f64,
    /// Shear modulus (Pa).
    pub shear_modulus: f64,
    /// Density (kg/m³).
    pub density: f64,
    /// Yield strength (Pa).
    pub yield_strength: f64,
    /// Ultimate strength (Pa).
    pub ultimate_strength: f64,
    /// Damping.
    pub damping: f64,
    /// Plastic strain at break.
    pub elongation: f64,
    /// Plastic strain where hardening starts.
    pub hardening_start: f64,
    /// Plastic strain at the ultimate strength.
    pub uniform_elongation: f64,
    /// Strain-rate law.
    pub rate: Option<RateLaw>,
}

impl Material {
    /// Its strength properties at temperature `t` (K).
    pub fn structural(&self, t: f64) -> Structural {
        Structural {
            youngs_modulus: self.youngs_modulus.at(t),
            shear_modulus: self.shear_modulus.at(t),
            density: self.density,
            yield_strength: self.yield_strength.at(t),
            ultimate_strength: self.ultimate_strength.at(t),
            damping: self.damping,
            elongation: self.elongation,
            hardening_start: self.hardening_start,
            uniform_elongation: self.uniform_elongation,
            rate: self.rate,
        }
    }

    /// Volumetric heat capacity at `t` (J/(m³·K)).
    pub fn heat_capacity(&self, t: f64) -> f64 {
        self.density * self.specific_heat.at(t)
    }

    /// Thermal diffusivity at `t` (m²/s): how fast heat spreads through it.
    pub fn diffusivity(&self, t: f64) -> f64 {
        self.conductivity.at(t) / self.heat_capacity(t)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialDef {
    id: String,
    name: String,
    kind: Kind,
    density: String,
    youngs_modulus: CurveDef,
    shear_modulus: CurveDef,
    yield_strength: CurveDef,
    ultimate_strength: CurveDef,
    #[serde(default)]
    compressive_strength: Option<String>,
    #[serde(default)]
    across: Option<String>,
    elongation: f64,
    hardening_start: f64,
    uniform_elongation: f64,
    #[serde(default)]
    rate: Option<(String, f64, String)>,
    damping: f64,
    conductivity: CurveDef,
    specific_heat: CurveDef,
    expansion: CurveDef,
    resistivity: CurveDef,
    emissivity: f64,
    absorptivity: f64,
    #[serde(default)]
    melting: Option<(String, String, String)>,
    #[serde(default)]
    decomposes: Option<String>,
    sources: String,
}

const CONDUCTIVITY: Dimension = Dimension::POWER.div(Dimension::LENGTH.mul(Dimension::TEMPERATURE));
const SPECIFIC_HEAT: Dimension = Dimension::ENERGY.div(Dimension::MASS.mul(Dimension::TEMPERATURE));
const PER_KELVIN: Dimension = Dimension::NONE.div(Dimension::TEMPERATURE);
const RESISTIVITY: Dimension = Dimension::POWER
    .div(Dimension::CURRENT.pow(2))
    .mul(Dimension::LENGTH);
const PER_SECOND: Dimension = Dimension::FREQUENCY;
const SPECIFIC_ENERGY: Dimension = Dimension::ENERGY.div(Dimension::MASS);

impl MaterialDef {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// The material, its values checked.
    pub(crate) fn build(self) -> Result<Material, String> {
        let quantity = |s: &str, d: Dimension| parse_as(s, d).map_err(|e| format!("{s:?}: {e}"));
        let fraction = |v: f64, what: &str| {
            if (0.0..=1.0).contains(&v) {
                Ok(v)
            } else {
                Err(format!("{what} must be in [0, 1]"))
            }
        };
        let m = Material {
            density: quantity(&self.density, Dimension::DENSITY)?,
            youngs_modulus: self.youngs_modulus.parse(Dimension::PRESSURE)?,
            shear_modulus: self.shear_modulus.parse(Dimension::PRESSURE)?,
            yield_strength: self.yield_strength.parse(Dimension::PRESSURE)?,
            ultimate_strength: self.ultimate_strength.parse(Dimension::PRESSURE)?,
            compressive_strength: self
                .compressive_strength
                .map(|s| quantity(&s, Dimension::PRESSURE))
                .transpose()?,
            across: (self.across)
                .map(|s| quantity(&s, Dimension::PRESSURE))
                .transpose()?,
            elongation: self.elongation,
            hardening_start: self.hardening_start,
            uniform_elongation: self.uniform_elongation,
            rate: self
                .rate
                .map(|(d, q, reference)| {
                    Ok::<_, String>(RateLaw {
                        d: quantity(&d, PER_SECOND)?,
                        q,
                        reference: quantity(&reference, PER_SECOND)?,
                    })
                })
                .transpose()?,
            damping: self.damping,
            conductivity: self.conductivity.parse(CONDUCTIVITY)?,
            specific_heat: self.specific_heat.parse(SPECIFIC_HEAT)?,
            expansion: self.expansion.parse(PER_KELVIN)?,
            resistivity: self.resistivity.parse(RESISTIVITY)?,
            emissivity: fraction(self.emissivity, "emissivity")?,
            absorptivity: fraction(self.absorptivity, "absorptivity")?,
            melting: self
                .melting
                .map(|(solidus, liquidus, latent)| {
                    Ok::<_, String>(Melting {
                        solidus: quantity(&solidus, Dimension::TEMPERATURE)?,
                        liquidus: quantity(&liquidus, Dimension::TEMPERATURE)?,
                        latent_heat: quantity(&latent, SPECIFIC_ENERGY)?,
                    })
                })
                .transpose()?,
            decomposes: self
                .decomposes
                .map(|s| quantity(&s, Dimension::TEMPERATURE))
                .transpose()?,
            id: self.id,
            name: self.name,
            kind: self.kind,
            sources: self.sources,
        };
        check(&m)?;
        Ok(m)
    }
}

/// What every material must satisfy to be usable by the solvers.
fn check(m: &Material) -> Result<(), String> {
    let positive = [
        ("density", m.density),
        ("Young's modulus", m.youngs_modulus.min()),
        ("shear modulus", m.shear_modulus.min()),
        ("yield strength", m.yield_strength.min()),
        ("conductivity", m.conductivity.min()),
        ("specific heat", m.specific_heat.min()),
        ("resistivity", m.resistivity.min()),
    ];
    if let Some((what, _)) = positive.iter().find(|(_, v)| *v <= 0.0) {
        return Err(format!("{what} must be positive everywhere"));
    }
    let strengths = m
        .ultimate_strength
        .points()
        .iter()
        .all(|&(t, u)| u >= m.yield_strength.at(t));
    if !strengths {
        return Err("ultimate strength below yield".into());
    }
    let strains = 0.0 <= m.hardening_start
        && m.hardening_start <= m.uniform_elongation
        && m.uniform_elongation <= m.elongation;
    if !strains {
        return Err("needs 0 <= hardening start <= uniform elongation <= elongation".into());
    }
    if let Some(melt) = m.melting
        && !(melt.solidus <= melt.liquidus && melt.latent_heat >= 0.0)
    {
        return Err("melting: solidus above liquidus, or negative latent heat".into());
    }
    Ok(())
}
