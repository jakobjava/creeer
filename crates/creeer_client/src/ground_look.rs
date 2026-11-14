//! The ground's look on this client (G2.11, ADR-0037): the packs' ground layers
//! (`creeer_art::ground`) resolved against the rocks, the palette and the tree species, and
//! what each layer covers at a point of the ground.
//!
//! A layer grows where its ranges allow, as a tree species does (`flora/trees.ron`): the
//! ground's rock, its slope, its height above the datum and the latitude, each thinning it out
//! over a margin rather than at a line; and under trees, in the open, or both. "Under trees"
//! is where a tree species grows by its own ranges. Nothing grows on ground dug below the
//! natural surface: a hole shows the rock it was dug into.

use creeer_art::ground::{Forest, Ground};
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_render::GroundLayer;
use creeer_terrain::Terrain;
use creeer_terrain::tiles::HeightSampler;
use std::sync::{Arc, OnceLock};

/// How far from the camera trees are drawn (m: `tree_view.rs`'s `DRAWN`): beyond, the ground
/// shows the forest's crowns (`terrain.wgsl`). Kept equal to it by hand.
pub const TREES_DRAWN: f32 = 160.0;

/// Over how many degrees of latitude and slope, and metres of height, a range's edge fades.
const EDGE_DEGREES: f64 = 4.0;
const EDGE_METRES: f64 = 40.0;

/// Where something grows: |latitude| and slope (rad), elevation (m), its grounds.
#[derive(Clone, Debug, PartialEq)]
struct Range {
    latitude: (f64, f64),
    elevation: (f64, f64),
    slope: f64,
    grounds: Vec<u16>,
}

impl Range {
    /// How fully it grows (0–1) at |latitude| and slope (rad), elevation (m), on `rock`.
    fn at(&self, latitude: f64, elevation: f64, slope: f64, rock: u16) -> f64 {
        if !self.grounds.contains(&rock) {
            return 0.0;
        }
        let edge = EDGE_DEGREES.to_radians();
        let inside = |x: f64, (lo, hi): (f64, f64), e: f64| {
            ((x - lo) / e).clamp(0.0, 1.0) * ((hi - x) / e).clamp(0.0, 1.0)
        };
        inside(latitude, self.latitude, edge)
            * inside(elevation, self.elevation, EDGE_METRES)
            * ((self.slope - slope) / edge).clamp(0.0, 1.0)
    }
}

/// One layer, resolved.
#[derive(Clone, Debug, PartialEq)]
struct Layer {
    range: Range,
    forest: Forest,
    cover: f64,
    drawn: GroundLayer,
}

/// The ground's look, resolved.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct GroundLook {
    layers: Vec<Layer>,
    /// Where each tree species grows.
    forest: Vec<Range>,
    /// Each layer's name, in order.
    pub names: Vec<String>,
    /// The trees' crowns seen from afar (linear): the mean of the species' foliage colours.
    pub canopy: [f32; 3],
}

impl GroundLook {
    /// The look the client's packs give (`crate::content::data`), made once. A broken entry
    /// leaves the ground bare (the validator names it).
    pub fn current() -> Arc<Self> {
        static LOOK: OnceLock<Arc<GroundLook>> = OnceLock::new();
        Arc::clone(LOOK.get_or_init(|| {
            let data = crate::content::data();
            Arc::new(Self::from_data(&data).unwrap_or_else(|e| {
                eprintln!("the ground's look: {e}");
                Self::default()
            }))
        }))
    }

    /// The look `data` gives.
    ///
    /// # Errors
    /// If the ground, the rocks, the palette or the trees do not load.
    pub fn from_data(data: &creeer_data::Data) -> Result<Self, String> {
        let ground = Ground::from_data(data)?;
        let rocks = creeer_terrain::geology::rocks::RockTable::from_data(data)
            .map_err(|e| e.to_string())?;
        let palette = creeer_art::palette::Palette::from_data(data)?;
        let species = creeer_sim::trees::species::load(data)?;
        let mut layers = Vec::new();
        for l in ground.layers.iter().take(creeer_art::ground::MOST_LAYERS) {
            let colour = (palette.token(&l.token))
                .and_then(|t| t.shade.fixed())
                .ok_or_else(|| format!("{}: no colour {}", l.name, l.token))?;
            #[allow(clippy::cast_possible_truncation)] // colours and sizes to the GPU's f32
            let drawn = GroundLayer {
                colour: colour.linear().map(|c| c as f32),
                patch: l.patch as f32,
                grain: l.grain as f32,
                density: l.density as f32,
            };
            layers.push(Layer {
                range: Range {
                    latitude: (l.latitude.0.to_radians(), l.latitude.1.to_radians()),
                    elevation: l.elevation,
                    slope: l.slope.to_radians(),
                    grounds: l.on.iter().filter_map(|g| rocks.rock(g)).collect(),
                },
                forest: l.forest,
                cover: l.cover,
                drawn,
            });
        }
        let forest = (species.iter())
            .map(|s| Range {
                latitude: s.latitude,
                elevation: s.elevation,
                slope: s.slope,
                grounds: s.grounds.clone(),
            })
            .collect();
        let mut canopy = [0.0f32; 3];
        for s in &species {
            let linear = creeer_art::Srgb(s.leaves).linear();
            for (c, l) in canopy.iter_mut().zip(linear) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                let share = (l / species.len() as f64) as f32;
                *c += share;
            }
        }
        Ok(Self {
            names: ground.layers.iter().map(|l| l.name.clone()).collect(),
            layers,
            forest,
            canopy,
        })
    }

    /// The layers as the renderer lays them.
    pub fn drawn(&self) -> Vec<GroundLayer> {
        self.layers.iter().map(|l| l.drawn).collect()
    }

    /// How fully a forest grows (0–1) at |latitude| and slope (rad), elevation (m), on `rock`:
    /// the best any species' ranges allow.
    fn forest(&self, latitude: f64, elevation: f64, slope: f64, rock: u16) -> f64 {
        (self.forest.iter())
            .map(|f| f.at(latitude, elevation, slope, rock))
            .fold(0.0, f64::max)
    }

    /// How fully a forest grows at surface point `p` (planet-centred, m) of `normal`, `rock` and
    /// `elevation` (m): what the ground shows of the trees' crowns from beyond where trees are
    /// drawn (`terrain.wgsl`).
    pub fn forest_at(&self, p: DVec3, normal: DVec3, rock: u16, elevation: f64) -> f64 {
        let up = p.normalize_or(DVec3::Y);
        let latitude = det::asin(up.y.clamp(-1.0, 1.0)).abs();
        let slope = det::acos(normal.normalize_or(up).dot(up).clamp(-1.0, 1.0));
        self.forest(latitude, elevation, slope, rock)
    }

    /// The share of the ground each layer covers at `p` (planet-centred, m) on `rock`, where
    /// the ground's normal is `normal` and its height above the datum `elevation` (m).
    pub fn cover(&self, p: DVec3, normal: DVec3, rock: u16, elevation: f64) -> [f32; 4] {
        let up = p.normalize_or(DVec3::Y);
        let latitude = det::asin(up.y.clamp(-1.0, 1.0)).abs();
        let slope = det::acos(normal.normalize_or(up).dot(up).clamp(-1.0, 1.0));
        let forest = self.forest(latitude, elevation, slope, rock);
        let mut out = [0.0; 4];
        for (k, l) in self.layers.iter().enumerate() {
            let shade = match l.forest {
                Forest::Any => 1.0,
                Forest::Only => forest,
                Forest::Not => 1.0 - forest,
            };
            #[allow(clippy::cast_possible_truncation)] // a share, 0–1
            let share = (l.cover * shade * l.range.at(latitude, elevation, slope, rock)) as f32;
            out[k] = share;
        }
        out
    }

    /// The share of the ground each layer covers at the surface point `p` of `terrain` (its
    /// natural height read by `heights`, a sampler at the chunk's own spacing: a finer one
    /// would make tiles of a metre for every vertex of a chunk kilometres wide), and how fully
    /// a forest grows there; nothing if `p` lies more than `tolerance` (m) below the natural
    /// surface: dug.
    pub fn cover_on(
        &self,
        terrain: &Terrain,
        heights: &mut HeightSampler<'_>,
        (p, normal): (DVec3, DVec3),
        rock: u16,
        tolerance: f64,
    ) -> ([f32; 4], f32) {
        let up = p.normalize_or(DVec3::Y);
        let datum = terrain.planet().datum.radius(up);
        let natural = heights.height(up);
        let elevation = p.length() - datum;
        if elevation < natural - tolerance {
            return ([0.0; 4], 0.0);
        }
        #[allow(clippy::cast_possible_truncation)] // a share, 0–1
        let forest = self.forest_at(p, normal, rock, natural) as f32;
        (self.cover(p, normal, rock, natural), forest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base packs' look, and the soil's and granite's ids.
    fn base() -> (GroundLook, u16, u16) {
        let data = creeer_data::Data::base();
        let rocks = creeer_terrain::geology::rocks::RockTable::from_data(&data).expect("rocks");
        let look = GroundLook::from_data(&data).expect("the base look");
        let id = |name| rocks.rock(name).expect(name);
        (look, id("soil"), id("granite"))
    }

    /// A point at `latitude` (°) on a planet of 500 km, and its ground's normal tilted by
    /// `slope` (°).
    fn at(latitude: f64, slope: f64) -> (DVec3, DVec3) {
        let (s, c) = det::sin_cos(latitude.to_radians());
        let up = DVec3::new(c, s, 0.0);
        let (ts, tc) = det::sin_cos(slope.to_radians());
        (up * 500_000.0, up * tc + DVec3::Z * ts)
    }

    fn share(look: &GroundLook, name: &str, cover: [f32; 4]) -> f32 {
        let k = look.names.iter().position(|n| n == name).expect(name);
        cover[k]
    }

    #[test]
    fn a_temperate_forest_floor_is_litter_and_moss_without_grass() {
        let (look, soil, _) = base();
        let (p, n) = at(45.0, 5.0);
        let cover = look.cover(p, n, soil, 300.0);
        assert!(share(&look, "litter", cover) > 0.5, "{cover:?}");
        assert!(share(&look, "moss", cover) > 0.3, "{cover:?}");
        assert!(share(&look, "grass", cover) < 0.01, "{cover:?}");
    }

    /// Nearer the equator than any tree of the packs grows: grass, and no forest floor.
    #[test]
    fn grass_grows_in_the_open() {
        let (look, soil, _) = base();
        let (p, n) = at(10.0, 5.0);
        let cover = look.cover(p, n, soil, 300.0);
        assert!(share(&look, "grass", cover) > 0.5, "{cover:?}");
        assert!(share(&look, "litter", cover) < 1e-6, "{cover:?}");
    }

    #[test]
    fn bare_rock_steep_ground_and_the_far_north_stay_bare() {
        let (look, soil, granite) = base();
        let (p, n) = at(45.0, 5.0);
        assert_eq!(look.cover(p, n, granite, 300.0), [0.0; 4]);
        let (p, n) = at(45.0, 60.0);
        assert_eq!(look.cover(p, n, soil, 300.0), [0.0; 4]);
        let (p, n) = at(80.0, 5.0);
        assert_eq!(look.cover(p, n, soil, 300.0), [0.0; 4]);
    }
}
