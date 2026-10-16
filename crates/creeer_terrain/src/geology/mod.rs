//! Geology: what rock is where, and where the ore deposits are (data-driven).
//!
//! - **Provinces** divide the crust into regions with their own rock column (cratons,
//!   sedimentary basins, volcanic arcs, orogens, oceanic crust, ...). A point belongs to the
//!   province with the highest score: smooth noise plus a bias, minus a penalty for lying
//!   outside the province's elevation range.
//! - **Columns:** a province's layers from the surface down; their thickness varies
//!   smoothly across the planet, and folds bend them.
//! - **Deposits** (`deposits.rs`) override the layers inside their outline, except for the
//!   top layer (soil and mud stay on top: ore has to be found and dug up).
//!
//! Everything is a pure function of position and seed, so it is the same on every machine
//! and needs no storage; only player edits are stored (`edits.rs`).

mod data;
pub mod deposits;
pub mod rocks;
#[cfg(test)]
mod tests;

use crate::datum::Datum;
use crate::elevation::{Elevation, Octaves, fractal};
use crate::noise::Perlin;
use crate::planet::DataError;
use crate::tiles::TileKey;
use creeer_math::glam::DVec3;
use deposits::{Deposit, DepositHit, DepositKind};
use rocks::RockTable;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One layer of a province's column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    /// Rock (material id).
    pub rock: u16,
    /// Thickness range (m); infinite for the bottom layer.
    pub thickness: (f64, f64),
}

/// A crustal province.
#[derive(Clone, Debug, PartialEq)]
pub struct Province {
    /// Name.
    pub name: String,
    /// Surface elevation range it prefers (m).
    pub elevation: (f64, f64),
    /// Added to its score.
    pub bias: f64,
    /// Fold amplitude and wavelength (m).
    pub folds: Option<(f64, f64)>,
    /// Layers from the surface down.
    pub column: Vec<Layer>,
}

/// What is at a point underground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoSample {
    /// Rock (material id).
    pub rock: u16,
    /// Province index.
    pub province: usize,
    /// The deposit the point lies in, if any.
    pub deposit: Option<DepositHit>,
}

/// Score lost per metre the surface lies outside a province's elevation range.
const ELEVATION_PENALTY: f64 = 1.0 / 500.0;
/// Most layers per column.
const MAX_LAYERS: usize = 16;

/// The geology of a planet.
#[derive(Debug)]
pub struct Geology {
    /// Minerals and rocks.
    pub rocks: RockTable,
    /// Provinces.
    pub provinces: Vec<Province>,
    /// Deposit kinds.
    pub deposits: Vec<DepositKind>,
    elevation: Arc<Elevation>,
    datum: Datum,
    radius: f64,
    seed: u64,
    province_noise: Octaves,
    variation: f64,
    /// One noise per province score, per layer thickness slot, for folds, for outlines.
    province_noises: Vec<Perlin>,
    layer_noises: Vec<Perlin>,
    fold_noise: Perlin,
    outline_noise: Perlin,
    deposit_cache: Mutex<HashMap<(u16, TileKey), Option<Deposit>>>,
}

impl Geology {
    /// Loads the geology file `path`, with the minerals and rocks in `rock_dir`; depths are
    /// measured from the surface `height` metres above `datum`.
    pub fn load(
        path: &Path,
        rock_dir: &Path,
        elevation: Arc<Elevation>,
        datum: Datum,
        seed: u64,
    ) -> Result<Self, DataError> {
        let read = |p: &Path| {
            std::fs::read_to_string(p).map_err(|e| DataError(format!("{}: {e}", p.display())))
        };
        let rocks = RockTable::parse(
            &read(&rock_dir.join("minerals.ron"))?,
            &read(&rock_dir.join("rocks.ron"))?,
        )?;
        let def: data::GeologyDef = ron::from_str(&read(path)?)
            .map_err(|e| DataError(format!("{}: {e}", path.display())))?;
        let built = def
            .build(&rocks, elevation.radius)
            .map_err(|e| DataError(format!("{}: {}", path.display(), e.0)))?;
        if built.provinces.iter().any(|p| p.column.len() > MAX_LAYERS) {
            return Err(DataError(format!(
                "at most {MAX_LAYERS} layers per province"
            )));
        }
        let noise = |i: u64| Perlin::new(seed ^ 0x6765_6f6c_6f67_7900 ^ i.wrapping_mul(0x9e37));
        Ok(Self {
            province_noises: (0..built.provinces.len() as u64).map(noise).collect(),
            layer_noises: (100..100 + MAX_LAYERS as u64).map(noise).collect(),
            fold_noise: noise(200),
            outline_noise: noise(201),
            rocks,
            provinces: built.provinces,
            deposits: built.deposits,
            radius: elevation.radius,
            datum,
            elevation,
            seed,
            province_noise: built.province_noise,
            variation: built.variation,
            deposit_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Surface elevation (m) used to place provinces and deposits (detail below 50 m left
    /// out, so it is cheap and the same at every resolution).
    pub fn elevation_at(&self, up: DVec3) -> f64 {
        self.elevation.height(up, 50.0)
    }

    /// The province at unit direction `up` where the surface is at `height` (m).
    pub fn province_at(&self, up: DVec3, height: f64) -> usize {
        let p = up * self.radius;
        let mut best = (f64::MIN, 0);
        for (i, province) in self.provinces.iter().enumerate() {
            let (lo, hi) = province.elevation;
            let outside = (lo - height).max(height - hi).max(0.0);
            let score = fractal(&self.province_noises[i], p, self.province_noise, 0.0)
                + province.bias
                - outside * ELEVATION_PENALTY;
            if score > best.0 {
                best = (score, i);
            }
        }
        best.1
    }

    /// Noise in about [−1, 1] for deposit outlines.
    fn noise_at(&self, q: DVec3) -> f64 {
        // Perlin noise rarely exceeds ±0.6; stretch it to use the full range.
        1.6 * self.outline_noise.sample(q.x, q.y, q.z)
    }

    /// The rock of `province`'s column at `p`, `depth` metres below the surface.
    ///
    /// The top layer is the cover (soil, mud): it follows the surface, scaled by `cover`
    /// (1: full thickness; 0: bare bedrock, as on cliffs). The bedrock layers below it are
    /// bent by the province's folds, measured from the base of the cover.
    pub fn column_rock(&self, province: usize, p: DVec3, depth: f64, cover: f64) -> u16 {
        let province = &self.provinces[province];
        let depth = depth.max(0.0);
        let q = p.normalize() * (self.radius / self.variation);
        let thickness = |i: usize, layer: &Layer| {
            let (lo, hi) = layer.thickness;
            let t = 0.5 + 0.8 * self.layer_noises[i].sample(q.x, q.y, q.z);
            lo + (hi - lo) * t.clamp(0.0, 1.0)
        };
        let top_layer = &province.column[0];
        if top_layer.thickness.1.is_infinite() {
            return top_layer.rock;
        }
        let cover_base = thickness(0, top_layer) * cover.clamp(0.0, 1.0);
        if depth < cover_base {
            return top_layer.rock;
        }
        let mut below = depth - cover_base;
        if let Some((amplitude, wavelength)) = province.folds {
            let f = p / wavelength;
            below = (below + amplitude * self.fold_noise.sample(f.x, f.y, f.z)).max(0.0);
        }
        let mut top = 0.0;
        for (i, layer) in province.column.iter().enumerate().skip(1) {
            if layer.thickness.1.is_infinite() {
                return layer.rock;
            }
            top += thickness(i, layer);
            if below < top {
                return layer.rock;
            }
        }
        province.column.last().map_or(0, |l| l.rock)
    }

    /// What is at `p` (m, planet-centred) under a surface `height` metres above the datum;
    /// `deposits` are the candidates near `p` (from [`Self::deposits_near`]); `cover` scales
    /// the soil (see [`Self::column_rock`]).
    pub fn sample(&self, p: DVec3, height: f64, deposits: &[Deposit], cover: f64) -> GeoSample {
        let up = p.normalize();
        let depth = self.datum.radius(up) + height - p.length();
        let province = self.province_at(up, height);
        let rock = self.column_rock(province, p, depth, cover);
        let top = self.provinces[province].column[0].rock;
        let deposit = if rock == top {
            None
        } else {
            deposits.iter().find_map(|d| d.hit(p, self))
        };
        GeoSample {
            rock: deposit.map_or(rock, |hit| self.deposits[usize::from(hit.kind)].rock),
            province,
            deposit,
        }
    }

    /// [`Self::sample`] with its own deposit lookup and full soil (for single queries).
    pub fn sample_at(&self, p: DVec3, height: f64) -> GeoSample {
        let deposits = self.deposits_near(p, 0.0);
        self.sample(p, height, &deposits, 1.0)
    }

    /// How much of the soil cover stays on ground of `slope` radians: all of it up to 25°,
    /// none beyond 40° (soil slides off steep slopes, leaving rock outcrops).
    pub fn cover_on_slope(slope: f64) -> f64 {
        ((40f64.to_radians() - slope) / 15f64.to_radians()).clamp(0.0, 1.0)
    }
}
