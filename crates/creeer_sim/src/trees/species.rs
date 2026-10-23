//! Species as the packs have them (`flora/trees.ron`), resolved against the materials, the rocks
//! and the palette, and checked.

use super::TREES;
use creeer_data::Data;
use creeer_data::merge::ron_list;
use creeer_materials::MaterialTable;
use creeer_terrain::geology::rocks::RockTable;
use serde::Deserialize;

/// A crown's outline: how a branch's reach falls off from the crown's widest to its top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Outline {
    /// Widest at its base, narrowing straight to the top (spruce, fir).
    Cone,
    /// Widest at its base, rounded to the top (pine, birch in the open).
    Paraboloid,
    /// Widest at its middle (broadleaves).
    Ellipsoid,
}

impl Outline {
    /// The reach at `u` (0 at the crown's top, 1 at its base) over the widest.
    pub fn reach(self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Self::Cone => u,
            Self::Paraboloid => u.sqrt(),
            Self::Ellipsoid => 2.0 * (u * (1.0 - u)).sqrt(),
        }
    }
}

/// Dry biomass (kg) from a tree's diameter d (cm) and height h (m), in Repola's form
/// (Silva Fennica 2008, 2009): ln y = b0 + b1·x/(x + k1) + b2·h/(h + k2), x the stump diameter
/// 2 + 1.25·d where `stump`, else d.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Biomass {
    pub b0: f64,
    pub b1: f64,
    pub k1: f64,
    #[serde(default)]
    pub b2: f64,
    #[serde(default = "one")]
    pub k2: f64,
    pub stump: bool,
}

fn one() -> f64 {
    1.0
}

impl Biomass {
    /// Its mass (kg) at diameter `d` (cm) and height `h` (m).
    pub fn of(self, d: f64, h: f64) -> f64 {
        let x = if self.stump { 2.0 + 1.25 * d } else { d };
        creeer_math::det::exp(self.b0 + self.b1 * x / (x + self.k1) + self.b2 * h / (h + self.k2))
    }
}

/// Where a species grows.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrowsDef {
    latitude: (f64, f64),
    elevation: (f64, f64),
    slope: f64,
    grounds: Vec<String>,
}

/// The crown.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crown {
    /// The live crown's length over the tree's height.
    pub ratio: f64,
    pub outline: Outline,
    /// Its widest radius (m) = `radius.0 + radius.1 · d` (d in m).
    pub radius: (f64, f64),
}

/// How it branches.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branching {
    /// Branch nodes along the crown, this far apart (m).
    pub spacing: f64,
    /// Branches at a node: a whorl, or 1 (alternate, a golden angle apart).
    pub per_node: (u8, u8),
    /// Their angle from the stem's up (°).
    pub angle: (f64, f64),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeciesDef {
    name: String,
    wood: String,
    grows: GrowsDef,
    per_m2: f64,
    dbh: (f64, f64),
    sizes: f64,
    height: (f64, f64, f64),
    taper: f64,
    crown: Crown,
    branches: Branching,
    branch_mass: Biomass,
    foliage_mass: Biomass,
    fresh: f64,
    bark: String,
    leaves: String,
    #[serde(default)]
    sources: String,
}

/// A species, resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct Species {
    pub name: String,
    /// The living wood (an engineering material's id) and what of it the trees need.
    pub wood: String,
    pub density: f64,
    /// Bending strength (the modulus of rupture), compression and tension along the grain, and
    /// compression across it (Pa).
    pub bending: f64,
    pub compression: f64,
    pub across: f64,
    pub youngs_modulus: f64,
    /// Where it grows: |latitude| (rad), elevation (m), the steepest slope (rad), and the
    /// grounds (terrain rock ids).
    pub latitude: (f64, f64),
    pub elevation: (f64, f64),
    pub slope: f64,
    pub grounds: Vec<u16>,
    pub per_m2: f64,
    /// Diameters at breast height (m), and the size law's exponent.
    pub dbh: (f64, f64),
    pub sizes: f64,
    /// Height (m) = 1.3 + (d / (a + b·d))^c, d in cm.
    pub height: (f64, f64, f64),
    pub taper: f64,
    pub crown: Crown,
    pub branching: Branching,
    /// Dry biomass of the living branches and the foliage, and fresh mass over dry.
    pub branch_mass: Biomass,
    pub foliage_mass: Biomass,
    pub fresh: f64,
    /// Colours (sRGB) of the bark and the foliage.
    pub bark: [u8; 3],
    pub leaves: [u8; 3],
}

impl Species {
    /// Its height (m) at diameter `dbh` (m).
    pub fn height_at(&self, dbh: f64) -> f64 {
        let d = dbh * 100.0;
        let (a, b, c) = self.height;
        1.3 + creeer_math::det::pow(d / (a + b * d), c)
    }
}

/// The species of the packs.
///
/// # Errors
/// If the file does not parse, a species names an unknown wood, rock or colour, or a number is
/// out of its range.
pub fn load(data: &Data) -> Result<Vec<Species>, String> {
    let defs = (data.table("trees", TREES))
        .merge(ron_list, |s: &SpeciesDef| &s.name)
        .map_err(|e| e.to_string())?;
    let materials = MaterialTable::from_data(data)?;
    let rocks = RockTable::from_data(data).map_err(|e| e.to_string())?;
    let palette = creeer_art::palette::Palette::from_data(data)?;
    let colour = |token: &str| {
        (palette.token(token))
            .and_then(|t| t.shade.fixed())
            .map(|s| s.0)
            .ok_or_else(|| format!("{TREES}: no fixed colour {token}"))
    };
    let positive = |x: f64| x.is_finite() && x > 0.0;
    let mut out = Vec::new();
    for s in defs {
        let bad = |what: &str| format!("{TREES}: {}: {what}", s.name);
        let wood = (materials.get(&s.wood)).ok_or_else(|| bad(&format!("no wood {}", s.wood)))?;
        let across = wood
            .across
            .ok_or_else(|| bad("its wood has no strength across"))?;
        let compression = wood
            .compressive_strength
            .unwrap_or(wood.ultimate_strength.at(20.0));
        let grounds = (s.grows.grounds.iter())
            .map(|g| rocks.rock(g).ok_or_else(|| bad(&format!("no ground {g}"))))
            .collect::<Result<Vec<_>, _>>()?;
        let b = &s.branches;
        let ok = positive(s.per_m2)
            && positive(s.dbh.0)
            && s.dbh.1 >= s.dbh.0
            && positive(s.sizes)
            && positive(s.height.0)
            && s.height.1 >= 0.0
            && positive(s.height.2)
            // A taper under ½ would give the crown's top the thickest branches (Leonardo's rule).
            && (0.5..=1.2).contains(&s.taper)
            && s.crown.ratio > 0.0
            && s.crown.ratio < 1.0
            && s.crown.radius.0 >= 0.0
            && positive(s.crown.radius.1)
            && positive(b.spacing)
            && b.per_node.0 >= 1
            && b.per_node.1 >= b.per_node.0
            && (0.0..=150.0).contains(&b.angle.0)
            && b.angle.1 >= b.angle.0
            && b.angle.1 <= 150.0
            && positive(s.branch_mass.k1)
            && positive(s.foliage_mass.k1)
            && positive(s.branch_mass.k2)
            && positive(s.foliage_mass.k2)
            && s.fresh >= 1.0
            && s.grows.latitude.0 <= s.grows.latitude.1
            && s.grows.elevation.0 <= s.grows.elevation.1
            && positive(s.grows.slope);
        if !ok {
            return Err(bad("a number out of its range"));
        }
        let species = Species {
            density: wood.density,
            bending: wood.ultimate_strength.at(20.0),
            compression,
            across,
            youngs_modulus: wood.youngs_modulus.at(20.0),
            wood: s.wood.clone(),
            latitude: (
                s.grows.latitude.0.to_radians(),
                s.grows.latitude.1.to_radians(),
            ),
            elevation: s.grows.elevation,
            slope: s.grows.slope.to_radians(),
            grounds,
            per_m2: s.per_m2,
            dbh: s.dbh,
            sizes: s.sizes,
            height: s.height,
            taper: s.taper,
            crown: s.crown,
            branching: s.branches,
            branch_mass: s.branch_mass,
            foliage_mass: s.foliage_mass,
            fresh: s.fresh,
            bark: colour(&s.bark)?,
            leaves: colour(&s.leaves)?,
            name: s.name,
        };
        if species.height_at(species.dbh.1) <= 1.3 || species.height_at(species.dbh.0) <= 1.3 {
            return Err(format!(
                "{TREES}: {}: heights not above 1.3 m",
                species.name
            ));
        }
        let _ = &s.sources;
        out.push(species);
    }
    if out.is_empty() {
        return Err(format!("{TREES}: no species"));
    }
    Ok(out)
}
