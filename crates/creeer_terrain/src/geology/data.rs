//! The geology file format (`data/planets/<planet>_geology.ron`) and its conversion.

use super::deposits::DepositKind;
use super::rocks::RockTable;
use super::{Layer, Province};
use crate::elevation::Octaves;
use crate::planet::DataError;
use crate::tiles::{TILE_CELLS, level_spacing};
use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename = "Geology")]
pub(super) struct GeologyDef {
    province_noise: OctavesDef,
    variation: String,
    provinces: Vec<ProvinceDef>,
    deposits: Vec<DepositDef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OctavesDef {
    wavelength: String,
    octaves: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvinceDef {
    name: String,
    elevation: (String, String),
    #[serde(default)]
    bias: f64,
    #[serde(default)]
    folds: Option<(String, String)>,
    column: Vec<LayerDef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerDef {
    rock: String,
    #[serde(default)]
    thickness: Option<(String, String)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DepositDef {
    name: String,
    rock: String,
    provinces: Vec<String>,
    #[serde(default)]
    elevation: Option<(String, String)>,
    #[serde(default)]
    latitude: Option<(String, String)>,
    cell: String,
    chance: f64,
    size: (String, String),
    thickness: (String, String),
    depth: (String, String),
    irregularity: f64,
}

fn q(field: &str, text: &str, dim: Dimension) -> Result<f64, DataError> {
    parse_as(text, dim).map_err(|e| DataError(format!("{field}: {e}")))
}

/// An ordered range of quantities.
fn range(field: &str, r: &(String, String), dim: Dimension) -> Result<(f64, f64), DataError> {
    let (lo, hi) = (q(field, &r.0, dim)?, q(field, &r.1, dim)?);
    if lo <= hi {
        Ok((lo, hi))
    } else {
        Err(DataError(format!("{field}: range {r:?} is reversed")))
    }
}

fn length_range(field: &str, r: &(String, String)) -> Result<(f64, f64), DataError> {
    let (lo, hi) = range(field, r, Dimension::LENGTH)?;
    if lo >= 0.0 {
        Ok((lo, hi))
    } else {
        Err(DataError(format!("{field}: lengths must not be negative")))
    }
}

/// What the geology file turns into.
pub(super) struct Built {
    pub province_noise: Octaves,
    pub variation: f64,
    pub provinces: Vec<Province>,
    pub deposits: Vec<DepositKind>,
}

impl GeologyDef {
    pub(super) fn build(self, rocks: &RockTable, radius: f64) -> Result<Built, DataError> {
        let rock = |field: &str, name: &str| {
            rocks
                .rock(name)
                .ok_or_else(|| DataError(format!("{field}: unknown rock {name:?}")))
        };
        let mut provinces = Vec::with_capacity(self.provinces.len());
        for p in &self.provinces {
            let field = format!("provinces[{}]", p.name);
            let mut column = Vec::with_capacity(p.column.len());
            for (i, layer) in p.column.iter().enumerate() {
                let last = i + 1 == p.column.len();
                let thickness = match (&layer.thickness, last) {
                    (Some(t), false) => length_range(&format!("{field}.column[{i}]"), t)?,
                    (None, true) => (f64::INFINITY, f64::INFINITY),
                    (Some(_), true) => {
                        return Err(DataError(format!(
                            "{field}: the last layer continues down and has no thickness"
                        )));
                    }
                    (None, false) => {
                        return Err(DataError(format!("{field}.column[{i}] needs a thickness")));
                    }
                };
                column.push(Layer {
                    rock: rock(&field, &layer.rock)?,
                    thickness,
                });
            }
            if column.is_empty() {
                return Err(DataError(format!("{field}: empty column")));
            }
            let folds = match &p.folds {
                Some((a, w)) => Some((
                    q(&field, a, Dimension::LENGTH)?,
                    q(&field, w, Dimension::LENGTH)?,
                )),
                None => None,
            };
            provinces.push(Province {
                name: p.name.clone(),
                elevation: range(
                    &format!("{field}.elevation"),
                    &p.elevation,
                    Dimension::LENGTH,
                )?,
                bias: p.bias,
                folds,
                column,
            });
        }
        let mut deposits = Vec::with_capacity(self.deposits.len());
        for d in &self.deposits {
            let field = format!("deposits[{}]", d.name);
            let province_ids = d
                .provinces
                .iter()
                .map(|name| {
                    provinces
                        .iter()
                        .position(|p| &p.name == name)
                        .ok_or_else(|| DataError(format!("{field}: unknown province {name:?}")))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let cell = q(&format!("{field}.cell"), &d.cell, Dimension::LENGTH)?;
            let size = length_range(&format!("{field}.size"), &d.size)?;
            // A deposit must fit in its cell and the cells around it (deposits.rs).
            if size.1 * (1.0 + d.irregularity) > 0.45 * cell {
                return Err(DataError(format!("{field}: size too large for the cell")));
            }
            if !(0.0..=1.0).contains(&d.chance) || !(0.0..0.5).contains(&d.irregularity) {
                return Err(DataError(format!(
                    "{field}: chance or irregularity out of range"
                )));
            }
            // The deepest cube-sphere level whose tiles are at least `cell` wide.
            let level = (0..20u8)
                .take_while(|&l| level_spacing(radius, l) * TILE_CELLS as f64 * 0.75 >= cell)
                .last()
                .unwrap_or(0);
            let infinite = (f64::NEG_INFINITY, f64::INFINITY);
            deposits.push(DepositKind {
                name: d.name.clone(),
                rock: rock(&field, &d.rock)?,
                provinces: province_ids,
                elevation: match &d.elevation {
                    Some(e) => range(&format!("{field}.elevation"), e, Dimension::LENGTH)?,
                    None => infinite,
                },
                latitude: match &d.latitude {
                    Some(l) => range(&format!("{field}.latitude"), l, Dimension::NONE)?,
                    None => (0.0, std::f64::consts::FRAC_PI_2),
                },
                level,
                chance: d.chance,
                size,
                thickness: length_range(&format!("{field}.thickness"), &d.thickness)?,
                depth: length_range(&format!("{field}.depth"), &d.depth)?,
                irregularity: d.irregularity,
            });
        }
        Ok(Built {
            province_noise: Octaves {
                wavelength: q(
                    "province_noise",
                    &self.province_noise.wavelength,
                    Dimension::LENGTH,
                )?,
                octaves: self.province_noise.octaves.clamp(1, 8),
                gain: 0.5,
            },
            variation: q("variation", &self.variation, Dimension::LENGTH)?,
            provinces,
            deposits,
        })
    }
}
