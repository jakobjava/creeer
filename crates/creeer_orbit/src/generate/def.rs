//! The solar-system definition file (`data/systems/*.ron`): what the generator may make, with
//! every quantity written with its unit and converted to SI here.

use creeer_math::units::{Dimension, parse_as};
use serde::Deserialize;
use std::path::Path;

/// A problem in a definition file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefError(pub String);

impl std::fmt::Display for DefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DefError {}

fn q(field: &str, text: &str, dimension: Dimension) -> Result<f64, DefError> {
    parse_as(text, dimension).map_err(|e| DefError(format!("{field}: {e}")))
}

fn pair(field: &str, (a, b): &(String, String), d: Dimension) -> Result<(f64, f64), DefError> {
    let (a, b) = (q(field, a, d)?, q(field, b, d)?);
    if a > b {
        return Err(DefError(format!("{field}: range ({a}, {b}) is reversed")));
    }
    Ok((a, b))
}

type Range = (String, String);

#[derive(Deserialize)]
#[serde(rename = "System", deny_unknown_fields)]
struct SystemDef {
    name: String,
    seed: u64,
    star: StarDef,
    home: HomeDef,
    planets: PlanetsDef,
    rocky: ClassDef,
    giant: ClassDef,
    moon: ClassDef,
    moons: MoonsDef,
    belt: BeltDef,
    trojans: TrojansDef,
    stability: Stability,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StarDef {
    name: String,
    radius: String,
    surface_gravity: String,
    day: String,
    color: (u8, u8, u8),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeDef {
    planet: String,
    semi_major_axis: String,
    eccentricity: f64,
    moons: (u32, u32),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanetsDef {
    inner: String,
    outer: String,
    spacing: (f64, f64),
    min_hill_separation: f64,
    eccentricity: (f64, f64),
    inclination: Range,
    giant_beyond: String,
    giant_chance: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassDef {
    radius: Range,
    density: Range,
    #[serde(default)]
    day: Option<Range>,
    #[serde(default)]
    tilt: Option<Range>,
    #[serde(default)]
    moons: (u32, u32),
    #[serde(default)]
    surface: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoonsDef {
    roche_factor: f64,
    hill_fraction: f64,
    spacing: (f64, f64),
    min_hill_separation: f64,
    eccentricity: (f64, f64),
    inclination: Range,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BeltDef {
    inner: String,
    outer: String,
    minors: u32,
    majors: u32,
    minor_radius: Range,
    major_radius: Range,
    density: String,
    eccentricity: (f64, f64),
    inclination: Range,
    surface: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrojansDef {
    per_giant: (u32, u32),
    radius: Range,
    density: String,
}

/// How long generated orbits are checked (in orbits of the fastest body of each check).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Stability {
    /// Orbits of the innermost planet for the planets-only check.
    pub planet_orbits: f64,
    /// Orbits of the outermost moon for each planet's moon check.
    pub moon_orbits: f64,
    /// Systems tried before giving up.
    pub attempts: u32,
}

/// A body class (rocky planets, gas giants, moons) in SI units.
#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    /// Radius range (m).
    pub radius: (f64, f64),
    /// Mean density range (kg/m³).
    pub density: (f64, f64),
    /// Rotation period range (s); none for tidally locked moons.
    pub day: Option<(f64, f64)>,
    /// Axial tilt range (rad).
    pub tilt: (f64, f64),
    /// Number of moons.
    pub moons: (u32, u32),
    /// Terrain definition file for bodies of this class.
    pub surface: Option<String>,
}

/// The definition in SI units.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)] // fields mirror the data file
pub struct Params {
    pub name: String,
    pub seed: u64,
    pub star_name: String,
    pub star_radius: f64,
    pub star_gravity: f64,
    pub star_day: f64,
    pub star_color: [u8; 3],
    /// The home planet's data file (relative to `data/`) and its bulk numbers.
    pub home_file: String,
    pub home: HomePlanet,
    pub home_a: f64,
    pub home_e: f64,
    pub home_moons: (u32, u32),
    pub inner: f64,
    pub outer: f64,
    pub spacing: (f64, f64),
    pub planet_hill_separation: f64,
    pub planet_e: (f64, f64),
    pub planet_i: (f64, f64),
    pub giant_beyond: f64,
    pub giant_chance: f64,
    pub rocky: Class,
    pub giant: Class,
    pub moon: Class,
    pub roche_factor: f64,
    pub hill_fraction: f64,
    pub moon_spacing: (f64, f64),
    pub moon_hill_separation: f64,
    pub moon_e: (f64, f64),
    pub moon_i: (f64, f64),
    pub belt: (f64, f64),
    pub belt_minors: u32,
    pub belt_majors: u32,
    pub minor_radius: (f64, f64),
    pub major_radius: (f64, f64),
    pub belt_density: f64,
    pub belt_e: (f64, f64),
    pub belt_i: (f64, f64),
    pub asteroid_surface: Option<String>,
    pub trojans: (u32, u32),
    pub trojan_radius: (f64, f64),
    pub trojan_density: f64,
    pub stability: Stability,
}

/// The bulk numbers of the hand-made home planet, read from its terrain file.
#[derive(Clone, Debug, PartialEq)]
pub struct HomePlanet {
    /// Name.
    pub name: String,
    /// Seed.
    pub seed: u64,
    /// Radius (m).
    pub radius: f64,
    /// Surface gravity (m/s²).
    pub surface_gravity: f64,
    /// Sidereal day (s).
    pub day: f64,
    /// Axial tilt (rad).
    pub tilt: f64,
}

/// The fields of a terrain planet file the orbits need (the rest is ignored here).
#[derive(Deserialize)]
#[serde(rename = "Planet")]
struct PlanetBulk {
    name: String,
    seed: u64,
    radius: String,
    surface_gravity: String,
    sidereal_day: String,
    axial_tilt: String,
}

impl Params {
    /// Loads a definition; `data` is the data directory the file names are relative to.
    pub fn load(path: &Path, data: &Path) -> Result<Self, DefError> {
        let read = |p: &Path| {
            std::fs::read_to_string(p).map_err(|e| DefError(format!("{}: {e}", p.display())))
        };
        let def: SystemDef = ron::from_str(&read(path)?)
            .map_err(|e| DefError(format!("{}: {e}", path.display())))?;
        let home_path = data.join(&def.home.planet);
        let bulk: PlanetBulk = ron::Options::default()
            .without_recursion_limit()
            .from_str(&read(&home_path)?)
            .map_err(|e| DefError(format!("{}: {e}", home_path.display())))?;
        Self::from_def(def, bulk)
    }

    fn from_def(d: SystemDef, bulk: PlanetBulk) -> Result<Self, DefError> {
        use Dimension as D;
        let class = |name: &str, c: ClassDef| -> Result<Class, DefError> {
            Ok(Class {
                radius: pair(&format!("{name}.radius"), &c.radius, D::LENGTH)?,
                density: pair(&format!("{name}.density"), &c.density, D::DENSITY)?,
                day: c
                    .day
                    .as_ref()
                    .map(|r| pair(&format!("{name}.day"), r, D::TIME))
                    .transpose()?,
                tilt: c.tilt.as_ref().map_or(Ok((0.0, 0.0)), |r| {
                    pair(&format!("{name}.tilt"), r, D::NONE)
                })?,
                moons: c.moons,
                surface: c.surface,
            })
        };
        Ok(Self {
            name: d.name,
            seed: d.seed,
            star_name: d.star.name,
            star_radius: q("star.radius", &d.star.radius, D::LENGTH)?,
            star_gravity: q(
                "star.surface_gravity",
                &d.star.surface_gravity,
                D::ACCELERATION,
            )?,
            star_day: q("star.day", &d.star.day, D::TIME)?,
            star_color: [d.star.color.0, d.star.color.1, d.star.color.2],
            home: HomePlanet {
                name: bulk.name,
                seed: bulk.seed,
                radius: q("home radius", &bulk.radius, D::LENGTH)?,
                surface_gravity: q("home gravity", &bulk.surface_gravity, D::ACCELERATION)?,
                day: q("home day", &bulk.sidereal_day, D::TIME)?,
                tilt: q("home tilt", &bulk.axial_tilt, D::NONE)?,
            },
            home_file: d.home.planet,
            home_a: q("home.semi_major_axis", &d.home.semi_major_axis, D::LENGTH)?,
            home_e: d.home.eccentricity,
            home_moons: d.home.moons,
            inner: q("planets.inner", &d.planets.inner, D::LENGTH)?,
            outer: q("planets.outer", &d.planets.outer, D::LENGTH)?,
            spacing: d.planets.spacing,
            planet_hill_separation: d.planets.min_hill_separation,
            planet_e: d.planets.eccentricity,
            planet_i: pair("planets.inclination", &d.planets.inclination, D::NONE)?,
            giant_beyond: q("planets.giant_beyond", &d.planets.giant_beyond, D::LENGTH)?,
            giant_chance: d.planets.giant_chance,
            rocky: class("rocky", d.rocky)?,
            giant: class("giant", d.giant)?,
            moon: class("moon", d.moon)?,
            roche_factor: d.moons.roche_factor,
            hill_fraction: d.moons.hill_fraction,
            moon_spacing: d.moons.spacing,
            moon_hill_separation: d.moons.min_hill_separation,
            moon_e: d.moons.eccentricity,
            moon_i: pair("moons.inclination", &d.moons.inclination, D::NONE)?,
            belt: (
                q("belt.inner", &d.belt.inner, D::LENGTH)?,
                q("belt.outer", &d.belt.outer, D::LENGTH)?,
            ),
            belt_minors: d.belt.minors,
            belt_majors: d.belt.majors,
            minor_radius: pair("belt.minor_radius", &d.belt.minor_radius, D::LENGTH)?,
            major_radius: pair("belt.major_radius", &d.belt.major_radius, D::LENGTH)?,
            belt_density: q("belt.density", &d.belt.density, D::DENSITY)?,
            belt_e: d.belt.eccentricity,
            belt_i: pair("belt.inclination", &d.belt.inclination, D::NONE)?,
            asteroid_surface: d.belt.surface,
            trojans: d.trojans.per_giant,
            trojan_radius: pair("trojans.radius", &d.trojans.radius, D::LENGTH)?,
            trojan_density: q("trojans.density", &d.trojans.density, D::DENSITY)?,
            stability: d.stability,
        })
    }
}
