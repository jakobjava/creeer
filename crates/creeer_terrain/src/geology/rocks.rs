//! Minerals and rocks (`data/geology/minerals.ron`, `rocks.ron`).
//!
//! A rock is a mixture of minerals by volume plus pore space. Its bulk density follows:
//! ρ = (1 − φ)·Σ vᵢ·ρᵢ (dry pores), which the tests check against measured rock densities.

use crate::planet::DataError;
use creeer_materials::Formula;
use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;

/// A mineral.
#[derive(Clone, Debug)]
pub struct Mineral {
    /// Name.
    pub name: String,
    /// Chemical formula.
    pub formula: Formula,
    /// Density of the pure crystal (kg/m³).
    pub density: f64,
    /// Vickers hardness (Pa, G1.5).
    pub hardness: f64,
}

/// A rock: what terrain is made of. Its index in the [`RockTable`] is its material id.
#[derive(Clone, Debug)]
pub struct Rock {
    /// Name.
    pub name: String,
    /// Colour for rendering (sRGB, 0–255).
    pub color: [u8; 3],
    /// Pore volume fraction.
    pub porosity: f64,
    /// (mineral index, volume fraction of the solid), fractions summing to 1.
    pub minerals: Vec<(usize, f64)>,
    /// Bulk density (kg/m³).
    pub density: f64,
    /// Whether it is an ore (the art style lets its colour be more saturated).
    pub ore: bool,
    /// How it breaks (G1.6d), if it is a solid rock: none for loose ground.
    pub breaks: Option<Breaks>,
    /// How hard it is to dig (G1.5).
    pub strength: Strength,
    /// The minerals its deposit's grade scales (indices into its `minerals`; none for most).
    pub graded: Vec<usize>,
}

/// How hard a rock is to dig (G1.5, ADR-0032).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Strength {
    /// Solid rock: its uniaxial compressive strength (Pa).
    Solid { compressive: f64 },
    /// Loose ground: its cohesion (Pa) and its angle of internal friction (rad).
    Loose { cohesion: f64, friction: f64 },
}

/// How a rock breaks: what knapping and quarrying spend, and what edge it leaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breaks {
    /// Its grains' size (m): no edge is sharper than half a grain, and grains coarser than a
    /// flake is thin crumble rather than flake.
    pub grain: f64,
    /// Its fracture energy (J/m²): what a new crack's surface costs, G_c = K_Ic²/E.
    pub fracture_energy: f64,
}

/// All minerals and rocks.
#[derive(Clone, Debug, Default)]
pub struct RockTable {
    /// Minerals.
    pub minerals: Vec<Mineral>,
    /// Rocks; the index is the terrain material id.
    pub rocks: Vec<Rock>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MineralDef {
    name: String,
    formula: String,
    density: String,
    hardness: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RockDef {
    name: String,
    color: String,
    porosity: f64,
    #[serde(default)]
    ore: bool,
    minerals: Vec<(String, f64)>,
    #[serde(default)]
    grain: Option<String>,
    #[serde(default)]
    fracture_energy: Option<String>,
    #[serde(default)]
    strength: Option<String>,
    #[serde(default)]
    cohesion: Option<String>,
    #[serde(default)]
    friction: Option<f64>,
    #[serde(default)]
    graded: Vec<String>,
}

impl RockTable {
    /// Parses the mineral and rock files.
    pub fn parse(minerals: &str, rocks: &str) -> Result<Self, DataError> {
        fn err(file: &'static str) -> impl Fn(ron::error::SpannedError) -> DataError {
            move |e| DataError(format!("{file}: {e}"))
        }
        let mineral_defs: Vec<MineralDef> = ron::from_str(minerals).map_err(err("minerals"))?;
        let rock_defs: Vec<RockDef> = ron::from_str(rocks).map_err(err("rocks"))?;
        Self::from_defs(plain(mineral_defs), plain(rock_defs))
    }

    /// The minerals and rocks of every pack of `data` (tables `minerals` and `rocks`).
    pub fn from_data(data: &creeer_data::Data) -> Result<Self, DataError> {
        use creeer_data::merge::ron_list;
        let minerals = (data.table("minerals", "geology/minerals.ron"))
            .merge_traced(ron_list, |m: &MineralDef| &m.name)
            .map_err(|e| DataError(e.to_string()))?;
        let rocks = (data.table("rocks", "geology/rocks.ron"))
            .merge_traced(ron_list, |r: &RockDef| &r.name)
            .map_err(|e| DataError(e.to_string()))?;
        Self::from_defs(minerals, rocks)
    }

    /// The table from its entries, each with where it came from (`"pack: path: "` in errors).
    fn from_defs(
        mineral_defs: Vec<(MineralDef, String)>,
        rock_defs: Vec<(RockDef, String)>,
    ) -> Result<Self, DataError> {
        let at = |origin: &str| {
            if origin.is_empty() {
                String::new()
            } else {
                format!("{origin}: ")
            }
        };
        let mut table = Self::default();
        for (m, origin) in mineral_defs {
            let context = |e: String| DataError(format!("{}mineral {}: {e}", at(&origin), m.name));
            let formula = Formula::parse(&m.formula).map_err(|e| context(e.to_string()))?;
            let density =
                parse_as(&m.density, Dimension::DENSITY).map_err(|e| context(e.to_string()))?;
            let hardness =
                parse_as(&m.hardness, Dimension::PRESSURE).map_err(|e| context(e.to_string()))?;
            if hardness.is_nan() || hardness <= 0.0 {
                return Err(context("hardness must be positive".into()));
            }
            if table.minerals.iter().any(|x| x.name == m.name) {
                return Err(DataError(format!("mineral {} defined twice", m.name)));
            }
            table.minerals.push(Mineral {
                name: m.name,
                formula,
                density,
                hardness,
            });
        }
        for (r, origin) in rock_defs {
            let context =
                |what: String| DataError(format!("{}rock {}: {what}", at(&origin), r.name));
            if table.rock(&r.name).is_some() {
                return Err(context("defined twice".into()));
            }
            let mut minerals = Vec::with_capacity(r.minerals.len());
            for (name, fraction) in &r.minerals {
                let index = table
                    .minerals
                    .iter()
                    .position(|m| &m.name == name)
                    .ok_or_else(|| context(format!("unknown mineral {name:?}")))?;
                if !(*fraction > 0.0 && *fraction <= 1.0) {
                    return Err(context(format!("fraction of {name} must be in (0, 1]")));
                }
                minerals.push((index, *fraction));
            }
            let total: f64 = minerals.iter().map(|(_, f)| f).sum();
            if (total - 1.0).abs() > 1e-6 {
                return Err(context(format!("mineral fractions sum to {total}, not 1")));
            }
            if !(0.0..1.0).contains(&r.porosity) {
                return Err(context("porosity must be in [0, 1)".into()));
            }
            let solid: f64 = minerals
                .iter()
                .map(|&(m, f)| f * table.minerals[m].density)
                .sum();
            let breaks = match (&r.grain, &r.fracture_energy) {
                (Some(g), Some(e)) => Some(Breaks {
                    grain: parse_as(g, Dimension::LENGTH).map_err(|e| context(e.to_string()))?,
                    fracture_energy: parse_as(e, Dimension::ENERGY.div(Dimension::AREA))
                        .map_err(|e| context(e.to_string()))?,
                }),
                (None, None) => None,
                _ => return Err(context("a grain and a fracture energy, or neither".into())),
            };
            let pressure =
                |s: &str| parse_as(s, Dimension::PRESSURE).map_err(|e| context(e.to_string()));
            let strength = match (&r.strength, &r.cohesion, r.friction) {
                (Some(s), None, None) => Strength::Solid {
                    compressive: pressure(s)?,
                },
                (None, Some(c), Some(f)) if (0.0..90.0).contains(&f) => Strength::Loose {
                    cohesion: pressure(c)?,
                    friction: f.to_radians(),
                },
                _ => {
                    return Err(context(
                        "a strength, or a cohesion and a friction angle (0–90°)".into(),
                    ));
                }
            };
            if let Strength::Solid { compressive: v } | Strength::Loose { cohesion: v, .. } =
                strength
                && !(v >= 0.0 && v.is_finite())
            {
                return Err(context(format!("strength {v} Pa")));
            }
            let mut graded = Vec::with_capacity(r.graded.len());
            for name in &r.graded {
                let k = (minerals.iter())
                    .position(|&(m, _)| &table.minerals[m].name == name)
                    .ok_or_else(|| context(format!("graded {name:?} is not among its minerals")))?;
                graded.push(k);
            }
            table.rocks.push(Rock {
                strength,
                graded,
                breaks,
                color: parse_color(&r.color).ok_or_else(|| context("bad colour".into()))?,
                porosity: r.porosity,
                density: (1.0 - r.porosity) * solid,
                ore: r.ore,
                minerals,
                name: r.name,
            });
        }
        if table.rocks.len() > usize::from(u16::MAX) {
            return Err(DataError("too many rocks".into()));
        }
        Ok(table)
    }

    /// Material id of a rock by name.
    pub fn rock(&self, name: &str) -> Option<u16> {
        self.rocks
            .iter()
            .position(|r| r.name == name)
            .map(|i| u16::try_from(i).expect("checked when loading"))
    }

    /// A rock's minerals by name and mass fraction of its solid (dry), its graded minerals'
    /// volume fractions scaled by `grade` (a deposit's, relative to the rock's average; 1
    /// elsewhere) and the rest by what keeps the sum 1; the grade at most what fills it.
    pub fn composition(&self, rock: u16, grade: f64) -> Vec<(String, f64)> {
        let rock = &self.rocks[usize::from(rock)];
        let share: f64 = rock.graded.iter().map(|&k| rock.minerals[k].1).sum();
        let grade = if share > 0.0 {
            grade.clamp(0.0, 1.0 / share)
        } else {
            1.0
        };
        let rest = if share < 1.0 {
            (1.0 - grade * share) / (1.0 - share)
        } else {
            0.0
        };
        let volumes: Vec<f64> = (rock.minerals.iter().enumerate())
            .map(|(k, &(_, v))| {
                v * if rock.graded.contains(&k) {
                    grade
                } else {
                    rest
                }
            })
            .collect();
        let solid: f64 = (rock.minerals.iter().zip(&volumes))
            .map(|(&(m, _), v)| v * self.minerals[m].density)
            .sum();
        (rock.minerals.iter().zip(&volumes))
            .map(|(&(m, _), v)| {
                let mineral = &self.minerals[m];
                (mineral.name.clone(), v * mineral.density / solid)
            })
            .collect()
    }

    /// A rock's minerals' hardnesses (Pa) and volume fractions.
    pub fn hardnesses(&self, rock: u16) -> impl Iterator<Item = (f64, f64)> + '_ {
        (self.rocks[usize::from(rock)].minerals.iter())
            .map(|&(m, v)| (self.minerals[m].hardness, v))
    }

    /// Mass fraction of element `z` in a rock (0..1, dry).
    pub fn element_fraction(&self, rock: u16, z: u8) -> f64 {
        let rock = &self.rocks[usize::from(rock)];
        let solid: f64 = rock
            .minerals
            .iter()
            .map(|&(m, v)| v * self.minerals[m].density)
            .sum();
        rock.minerals
            .iter()
            .map(|&(m, v)| {
                let mineral = &self.minerals[m];
                v * mineral.density / solid * mineral.formula.mass_fraction(z)
            })
            .sum()
    }
}

/// `#rrggbb` to bytes.
fn parse_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#').filter(|h| h.len() == 6)?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Entries with no origin (a file read on its own).
fn plain<T>(defs: Vec<T>) -> Vec<(T, String)> {
    defs.into_iter().map(|d| (d, String::new())).collect()
}
