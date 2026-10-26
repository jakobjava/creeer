//! The terrain of one planet: its generator, the shared height tiles, the player's edits,
//! and queries of the density field.

use crate::chunk::{ChunkKey, ChunkSamples, Occupancy};
use crate::datum::Datum;
use crate::edits::{Edit, Edits};
use crate::geology::Geology;
use crate::levels::Levels;
use crate::occlusion;
use crate::planet::Planet;
use crate::surface_nets::{self, Site, TerrainMesh, WorldMesh};
use crate::tiles::{HeightCache, HeightSampler};
use creeer_math::glam::DVec3;
use std::sync::{Arc, RwLock, RwLockReadGuard};

/// Height tiles kept in memory (about 18 KB each).
const HEIGHT_TILES: usize = 4096;

/// A planet's terrain, shared by the simulation and the mesh workers.
#[derive(Debug)]
pub struct Terrain {
    planet: Arc<Planet>,
    heights: HeightCache,
    edits: RwLock<Edits>,
}

impl Terrain {
    /// The terrain of `planet`.
    pub fn new(planet: Planet) -> Self {
        let planet = Arc::new(planet);
        let heights = HeightCache::new(Arc::clone(&planet.elevation), HEIGHT_TILES);
        Self {
            planet,
            heights,
            edits: RwLock::new(Edits::default()),
        }
    }

    /// Another terrain of the same planet, with its own edits (another machine's: a game's
    /// clients each have their own).
    pub fn sibling(&self) -> Self {
        Self {
            planet: Arc::clone(&self.planet),
            heights: HeightCache::new(Arc::clone(&self.planet.elevation), HEIGHT_TILES),
            edits: RwLock::new(Edits::default()),
        }
    }

    /// The planet.
    pub fn planet(&self) -> &Planet {
        &self.planet
    }

    /// The height tiles.
    pub fn heights(&self) -> &HeightCache {
        &self.heights
    }

    /// The edits so far.
    pub fn edits(&self) -> RwLockReadGuard<'_, Edits> {
        self.edits
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Applies an edit.
    pub fn edit(&self, edit: Edit) {
        self.edits
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(edit);
    }

    /// Replaces edit `k` in its place ([`Edits::replace`]).
    pub fn replace_edit(&self, k: usize, edit: Edit) {
        self.edits
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(k, edit);
    }

    /// Replaces all edits (loading a save).
    pub fn set_edits(&self, edits: Edits) {
        *self
            .edits
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = edits;
    }

    /// Height (m) of the generated surface above the datum under `p`, at full resolution.
    pub fn surface_height(&self, p: DVec3) -> f64 {
        self.heights.sampler(1.0).height(p)
    }

    /// Density (m) at `p` at full resolution, edits included: the height above the surface
    /// (along the vertical; negative in rock).
    pub fn density(&self, p: DVec3) -> f64 {
        let generated = generated(&self.planet.datum, &mut self.heights.sampler(1.0), p);
        self.edits().density(p, generated)
    }

    /// Density (m) at `p` as generated, before any edit.
    pub fn generated_density(&self, p: DVec3) -> f64 {
        generated(&self.planet.datum, &mut self.heights.sampler(1.0), p)
    }

    /// The density and its gradient at `p` (the gradient points into the air). The distance to
    /// the surface is about `density / |gradient|`. The gradient is exact: the derivative of
    /// the interpolated heights, the datum and the distance from the centre (one height
    /// lookup), carried through the rounded max or min of each edit near `p`
    /// (`edit_gradient.rs`). The value is [`Self::density`]'s, bit for bit.
    pub fn density_gradient(&self, p: DVec3) -> (f64, DVec3) {
        if p == DVec3::ZERO {
            return (self.density(p), DVec3::ZERO); // the centre: no direction, no slope
        }
        let (height, height_gradient) = self.heights.sampler(1.0).height_gradient(p);
        let (radius, radius_gradient) = self.planet.datum.radius_gradient(p);
        let length = p.length();
        let generated = (
            length - radius - height,
            p / length - radius_gradient - height_gradient,
        );
        self.edits().density_gradient(p, generated)
    }

    /// The rock at `p` (material id), edits included; `None` in air.
    pub fn rock_at(&self, p: DVec3) -> Option<u16> {
        if self.density(p) >= 0.0 {
            return None;
        }
        let near: Vec<Edit> = self.edits().near(p).copied().collect();
        Some(Edits::material(&near, p, 0.0).unwrap_or_else(|| {
            // The same soil rule as the meshes: none on steep slopes.
            let (_, gradient) = self.density_gradient(p);
            let cover = Geology::cover_on_slope(gradient.angle_between(p.normalize()));
            let geology = &self.planet.geology;
            let deposits = geology.deposits_near(p, 0.0);
            geology
                .sample(p, self.surface_height(p), &deposits, cover)
                .rock
        }))
    }

    /// The mesh of a chunk, with each vertex's rock as its material; `None` if the chunk
    /// holds no surface.
    pub fn mesh_chunk(&self, key: ChunkKey) -> Option<TerrainMesh> {
        let (samples, world) = self.mesh_world(key)?;
        let mut mesh = world.local(key.origin());
        if mesh.is_empty() {
            return None;
        }
        mesh.openness = occlusion::openness(&samples, &mesh);
        // Generated terrain is a height field: open to the sky everywhere. Only edits make
        // caves, and only fine chunks resolve them.
        if key.level <= SKY_VIEW_LEVELS && !self.chunk_edits(key).is_empty() {
            self.add_sky_view(key, &mut mesh);
        }
        Some(mesh)
    }

    /// A chunk's samples and its mesh in world coordinates, before shading.
    pub(crate) fn mesh_world(&self, key: ChunkKey) -> Option<(ChunkSamples, WorldMesh)> {
        if self.occupancy(key) != Occupancy::Mixed {
            return None;
        }
        let samples = self.sample_chunk(key);
        if !samples.has_surface() {
            return None;
        }
        let geology = &self.planet.geology;
        let reach = key.size() * 0.87; // half the chunk's diagonal
        let deposits = geology.deposits_near(key.center(), reach);
        let chunk_edits = self.chunk_edits(key);
        let tolerance = 0.5 * key.cell_size();
        let mut sampler = self.heights.sampler(key.cell_size());
        let datum = self.planet.datum;
        let (lo, hi) = (key.origin(), key.origin() + DVec3::splat(key.size()));
        let mut levels = Levels::new(self, lo, hi, key.cell_size());
        let world = surface_nets::mesh_world(&samples, &mut |point, normal, site| {
            let refined = match site {
                Site::Cell { low, edge } => levels.refine(point, normal, (low, edge)),
                Site::Cap { depth } => levels.fixed(point, normal, depth),
            };
            let (p, normal) = (refined.anchor, refined.normal);
            if let Some(rock) = Edits::material(&chunk_edits, p, tolerance) {
                return (refined, rock);
            }
            // A vertex lies up to a cell off the surface it approximates, and a coarse cell is
            // far thicker than the soil: the natural surface shows the rock *at* the surface,
            // the same at every level of detail. Only what lies below the natural surface (a
            // wall dug into the ground) shows the rock at its depth, so the layers appear on it
            // as level bands. (Whether a vertex lay within half a cell of a dig brush decided
            // it before: on a dug rim, neighbours flipped between soil and rock, a sawtooth of
            // colour; G1.3c.)
            let up = p.normalize();
            let height = sampler.height(p);
            let surface = up * (datum.radius(up) + height);
            let at = if surface.length() - p.length() > tolerance {
                p
            } else {
                surface
            };
            let slope = f64::from(normal.angle_between(up.as_vec3()));
            let cover = Geology::cover_on_slope(slope);
            (refined, geology.sample(at, height, &deposits, cover).rock)
        });
        Some((samples, world?))
    }

    /// Darkens vertices that rock hides from the sky (tunnels, overhangs): three rays
    /// around the vertical, 32 m long, through the full terrain with its edits. Openness
    /// becomes local × (0.2 + 0.8 × open rays / 3).
    ///
    /// A ray marches in steps of half the density (the height above the ground; for an edit,
    /// at most its distance to the brush), at least a quarter metre: fixed samples at 1, 2,
    /// 4, … 32 m stepped over thin roofs (found in F5).
    fn add_sky_view(&self, key: ChunkKey, mesh: &mut TerrainMesh) {
        let reach = SKY_VIEW_REACH + key.size();
        let (lo, hi) = (key.origin(), key.origin() + DVec3::splat(key.size()));
        let edits = self.edits().affecting(lo - reach, hi + reach, 0.0);
        let datum = self.planet.datum;
        let mut sampler = self.heights.sampler(1.0);
        let round = crate::edits::round(crate::edits::FINEST_CELL);
        let mut density = |q: DVec3| {
            let generated = generated(&datum, &mut sampler, q);
            edits.iter().fold(generated, |d, e| e.apply(q, d, round))
        };
        let mut blocked = |p: DVec3, ray: DVec3| {
            let mut s = SKY_VIEW_START;
            while s <= SKY_VIEW_REACH {
                let d = density(p + ray * s);
                if d < 0.0 {
                    return true;
                }
                s += (0.5 * d).max(SKY_VIEW_MIN_STEP);
            }
            false
        };
        for v in 0..mesh.positions.len() {
            let p = key.origin() + mesh.anchor(v).as_dvec3();
            let up = p.normalize();
            let (east, _) = crate::geology::deposits::tangent_frame(up);
            let rays = [
                up,
                (up + east * 0.5).normalize(),
                (up - east * 0.5).normalize(),
            ];
            let open = rays.iter().filter(|&&r| !blocked(p, r)).count();
            #[allow(clippy::cast_precision_loss)] // 0..=3
            let view = open as f32 / 3.0;
            mesh.openness[v] *= 0.2 + 0.8 * view;
        }
    }
}

/// Finest levels whose edited chunks get a sky view (cells up to 4 m).
const SKY_VIEW_LEVELS: u8 = crate::chunk::METRE_LEVEL + 2;
/// Where sky-view rays start, past the vertex's own surface (m).
const SKY_VIEW_START: f64 = 1.0;
/// Length of the sky-view rays (m).
const SKY_VIEW_REACH: f64 = 32.0;
/// Smallest step of a sky-view ray (m): rock thinner than this may be missed.
const SKY_VIEW_MIN_STEP: f64 = 0.25;

/// The generated density at `p` (m): its height above the surface, from `sampler`'s heights.
/// The planet's centre has no direction to look up a height in and is deep rock (it gave NaN,
/// read as air: a hidden bubble in the coarsest chunks, found in F5).
pub(crate) fn generated(datum: &Datum, sampler: &mut HeightSampler<'_>, p: DVec3) -> f64 {
    if p == DVec3::ZERO {
        return -datum.polar_radius;
    }
    p.length() - datum.radius(p) - sampler.height(p)
}
