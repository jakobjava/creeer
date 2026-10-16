//! The terrain of one planet: its generator, the shared height tiles, the player's edits,
//! and queries of the density field.

use crate::chunk::{ChunkKey, Occupancy};
use crate::edits::{Edit, Edits};
use crate::geology::Geology;
use crate::occlusion;
use crate::planet::Planet;
use crate::surface_nets::{self, TerrainMesh};
use crate::tiles::HeightCache;
use creeer_math::glam::DVec3;
use std::sync::{Arc, RwLock, RwLockReadGuard};

/// Height tiles kept in memory (about 18 KB each).
const HEIGHT_TILES: usize = 4096;
/// Step of the finite differences for density gradients (m).
const GRADIENT_STEP: f64 = 0.01;

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
        let generated = p.length() - self.planet.datum.radius(p) - self.surface_height(p);
        self.edits().density(p, generated)
    }

    /// The density and its gradient at `p` (central differences; the gradient points into
    /// the air). The distance to the surface is about `density / |gradient|`.
    pub fn density_gradient(&self, p: DVec3) -> (f64, DVec3) {
        let edits = self.edits();
        let mut sampler = self.heights.sampler(1.0);
        let datum = self.planet.datum;
        let mut d = |q: DVec3| edits.density(q, q.length() - datum.radius(q) - sampler.height(q));
        let h = GRADIENT_STEP;
        let gradient = DVec3::new(
            d(p + DVec3::X * h) - d(p - DVec3::X * h),
            d(p + DVec3::Y * h) - d(p - DVec3::Y * h),
            d(p + DVec3::Z * h) - d(p - DVec3::Z * h),
        ) / (2.0 * h);
        (d(p), gradient)
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
        let mut mesh = surface_nets::mesh(&samples, |p, normal| {
            if let Some(rock) = Edits::material(&chunk_edits, p, tolerance) {
                return rock;
            }
            // A vertex lies up to a cell off the surface it approximates, and a coarse cell is
            // far thicker than the soil: the natural surface shows the rock *at* the surface,
            // the same at every level of detail. Only walls dug into the ground show the rock
            // at their depth.
            let up = p.normalize();
            let height = sampler.height(p);
            let dug = chunk_edits
                .iter()
                .any(|e| matches!(e, Edit::Dig(_)) && e.brush().distance(p).abs() <= tolerance);
            let at = if dug {
                p
            } else {
                up * (datum.radius(up) + height)
            };
            let slope = f64::from(normal.angle_between(up.as_vec3()));
            let cover = Geology::cover_on_slope(slope);
            geology.sample(at, height, &deposits, cover).rock
        });
        if mesh.is_empty() {
            return None;
        }
        mesh.openness = occlusion::openness(&samples, &mesh);
        // Generated terrain is a height field: open to the sky everywhere. Only edits make
        // caves, and only fine chunks resolve them.
        if !chunk_edits.is_empty() && key.level <= SKY_VIEW_LEVELS {
            self.add_sky_view(key, &mut mesh);
        }
        Some(mesh)
    }

    /// Darkens vertices that rock hides from the sky (tunnels, overhangs): three rays
    /// around the vertical, 32 m long, through the full terrain with its edits. Openness
    /// becomes local × (0.2 + 0.8 × open rays / 3).
    fn add_sky_view(&self, key: ChunkKey, mesh: &mut TerrainMesh) {
        let reach = SKY_VIEW_STEPS[SKY_VIEW_STEPS.len() - 1] + key.size();
        let (lo, hi) = (key.origin(), key.origin() + DVec3::splat(key.size()));
        let edits = self.edits().affecting(lo - reach, hi + reach, 0.0);
        let datum = self.planet.datum;
        let mut sampler = self.heights.sampler(1.0);
        let mut rock = |q: DVec3| {
            let generated = q.length() - datum.radius(q) - sampler.height(q);
            edits.iter().fold(generated, |d, e| e.apply(q, d)) < 0.0
        };
        for v in 0..mesh.surface_vertices {
            let p = key.origin() + mesh.positions[v].as_dvec3();
            let up = p.normalize();
            let (east, _) = crate::geology::deposits::tangent_frame(up);
            let rays = [
                up,
                (up + east * 0.5).normalize(),
                (up - east * 0.5).normalize(),
            ];
            let open = rays
                .iter()
                .filter(|&&r| !SKY_VIEW_STEPS.iter().any(|&s| rock(p + r * s)))
                .count();
            #[allow(clippy::cast_precision_loss)] // 0..=3
            let view = open as f32 / 3.0;
            mesh.openness[v] *= 0.2 + 0.8 * view;
        }
        for (i, &source) in mesh.skirt_sources.iter().enumerate() {
            mesh.openness[mesh.surface_vertices + i] = mesh.openness[source as usize];
        }
    }
}

/// Finest levels whose edited chunks get a sky view (cells up to 4 m).
const SKY_VIEW_LEVELS: u8 = 2;
/// Distances along each sky-view ray (m).
const SKY_VIEW_STEPS: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
