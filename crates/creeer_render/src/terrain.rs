//! Terrain chunk meshes for the terrain pass (`renderer/terrain_pass.rs`).
//!
//! Each chunk is uploaded once ([`crate::Renderer::insert_terrain`]) with its origin in f64
//! simulation space; vertices are relative to that origin in f32, and each frame the chunk is
//! drawn at its camera-relative origin, so terrain stays precise anywhere on a planet.

use bytemuck::{Pod, Zeroable};
use creeer_math::glam::Vec3;

/// A terrain vertex as stored on the GPU (32 bytes).
///
/// Besides its own surface it carries the next coarser level of detail: the normal there and
/// the distance along its normal to that surface. The shader blends towards them over the
/// distance band where a coarser neighbour may begin (geomorphing, `creeer_terrain::levels`),
/// so borders between levels and switches between them do not show. And how much of each of
/// the ground's layers (grass, moss, litter: [`GroundLayer`]) covers it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct TerrainVertex {
    /// Position relative to the chunk origin (m).
    pub position: [f32; 3],
    /// Unit normal (snorm) and, in `w`, how open the point is to the sky (0..127).
    pub normal: [i8; 4],
    /// Base colour (sRGB) and roughness in `w`.
    pub color: [u8; 4],
    /// Unit normal of the next coarser level (snorm), and in `w` how fully a forest grows here.
    pub coarse_normal: [i8; 4],
    /// Distance along `normal` to the next coarser level's surface (m).
    pub morph: f32,
    /// The share of the ground each of the four layers covers here (unorm, 0–255).
    pub cover: [u8; 4],
}

/// One of the ground's layers as the terrain shader lays it over the rock (`terrain.wgsl`;
/// the data: `creeer_art::ground`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundLayer {
    /// Its colour (linear RGB).
    pub colour: [f32; 3],
    /// The size of its patches (m).
    pub patch: f32,
    /// The size of its fine grain (m).
    pub grain: f32,
    /// How thickly it covers within its patches (0–1).
    pub density: f32,
}

/// The ground's layers as `globals.wgsl` carries them: each one's linear colour and patch size
/// (m), and each one's grain (m) and density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GroundUniform {
    pub layers: [[f32; 4]; 4],
    pub grain: [f32; 4],
    pub density: [f32; 4],
    /// The forest's crowns seen from afar: linear colour, and from how far away (m; 0 none).
    pub canopy: [f32; 4],
}

impl GroundUniform {
    /// The first four of `layers`; the rest unused.
    pub(crate) fn of(layers: &[GroundLayer]) -> Self {
        let mut out = Self {
            layers: [[0.0, 0.0, 0.0, 1.0]; 4],
            grain: [0.01; 4],
            density: [0.0; 4],
            canopy: [0.0; 4],
        };
        for (k, l) in layers.iter().take(4).enumerate() {
            out.layers[k] = [l.colour[0], l.colour[1], l.colour[2], l.patch.max(1e-3)];
            out.grain[k] = l.grain.max(1e-4);
            out.density[k] = l.density.clamp(0.0, 1.0);
        }
        out
    }
}

fn snorm(x: f32) -> i8 {
    #[allow(clippy::cast_possible_truncation)] // clamped to ±127
    let v = (x.clamp(-1.0, 1.0) * 127.0).round() as i8;
    v
}

fn unorm(x: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..255
    let v = (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    v
}

impl TerrainVertex {
    /// A vertex at `position` with unit `normal`, sky `openness` (0 = enclosed, 1 = open),
    /// sRGB colour and roughness (0–1).
    pub fn new(
        position: Vec3,
        normal: Vec3,
        openness: f32,
        color: [u8; 3],
        roughness: f32,
    ) -> Self {
        let n = normal.normalize_or(Vec3::Y);
        Self {
            position: position.to_array(),
            normal: [snorm(n.x), snorm(n.y), snorm(n.z), snorm(openness)],
            color: [color[0], color[1], color[2], unorm(roughness)],
            coarse_normal: [snorm(n.x), snorm(n.y), snorm(n.z), 0],
            morph: 0.0,
            cover: [0; 4],
        }
    }

    /// The same vertex where a forest grows `forest` fully (0–1): its crowns are drawn on the
    /// ground beyond where trees are (in `coarse_normal`'s spare byte).
    #[must_use]
    pub fn with_forest(mut self, forest: f32) -> Self {
        self.coarse_normal[3] = snorm(forest);
        self
    }

    /// The same vertex with the ground's layers covering `cover` of it each (0–1).
    #[must_use]
    pub fn with_cover(mut self, cover: [f32; 4]) -> Self {
        self.cover = cover.map(unorm);
        self
    }

    /// The same vertex with the next coarser level: its normal and the distance (m) along
    /// this vertex's normal to its surface.
    #[must_use]
    pub fn with_coarser(mut self, normal: Vec3, distance: f32) -> Self {
        let n = normal.normalize_or(Vec3::Y);
        self.coarse_normal = [snorm(n.x), snorm(n.y), snorm(n.z), self.coarse_normal[3]];
        self.morph = distance;
        self
    }
}

/// The ground's pattern repeats every this many metres along each body-fixed axis
/// (`terrain.wgsl`'s noise periods divide it). Each chunk is told its origin modulo this, so
/// the pattern is fixed to the ground and continuous across chunks of every size. (G2.9's 4 m
/// chunks broke the old rule that every chunk origin was a multiple of the pattern's 32 m: each
/// small chunk drew the same piece of it, and their borders showed as facets.)
pub(crate) const PATTERN_PERIOD: f64 = 1024.0;

/// Where `origin` (body-fixed, m) lies within the ground's pattern: each coordinate modulo
/// [`PATTERN_PERIOD`], in [0, period).
pub(crate) fn pattern_offset(origin: creeer_math::glam::DVec3) -> [f32; 3] {
    #[allow(clippy::cast_possible_truncation)] // within [0, 1024): exact enough in f32
    let wrap = |x: f64| x.rem_euclid(PATTERN_PERIOD) as f32;
    [wrap(origin.x), wrap(origin.y), wrap(origin.z)]
}

/// A chunk's mesh on the CPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainMeshData {
    /// Vertices relative to the chunk origin.
    pub vertices: Vec<TerrainVertex>,
    /// Triangle list, counter-clockwise seen from the air: the surface first, then the caps.
    pub indices: Vec<u32>,
    /// Number of indices of the surface (the rest are caps).
    pub surface_indices: u32,
    /// Index ranges (start, end) of the caps of each chunk face (−X, +X, −Y, +Y, −Z, +Z),
    /// drawn only where [`crate::Frame::terrain`] asks for them.
    pub caps: [(u32, u32); 6],
    /// Edge length of the chunk (m): it blends towards the coarser level between 4.25 and 5
    /// times this from the camera. 0: no blending.
    pub chunk_size: f32,
}

impl TerrainMeshData {
    /// A ball around the vertices: centre (relative to the chunk origin) and radius, for
    /// culling.
    pub fn bounds(&self) -> (Vec3, f32) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            let p = Vec3::from(v.position);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        if self.vertices.is_empty() {
            return (Vec3::ZERO, 0.0);
        }
        let center = (lo + hi) * 0.5;
        // Blending moves vertices along their normals by up to their morph distance.
        let reach = self
            .vertices
            .iter()
            .map(|v| v.morph.abs())
            .fold(0.0, f32::max);
        (center, (hi - lo).length() * 0.5 + reach)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertices_pack_into_thirty_two_bytes() {
        assert_eq!(std::mem::size_of::<TerrainVertex>(), 32);
        let v = TerrainVertex::new(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y * 2.0,
            0.5,
            [10, 20, 30],
            1.0,
        );
        assert_eq!(v.normal, [0, 127, 0, 64]);
        assert_eq!(v.color, [10, 20, 30, 255]);
        // Without a coarser level, it blends to itself.
        assert_eq!(v.coarse_normal, [0, 127, 0, 0]);
        let w = v.with_coarser(Vec3::X, -1.5);
        assert_eq!(
            (w.coarse_normal, w.morph, w.normal),
            ([127, 0, 0, 0], -1.5, v.normal)
        );
        assert_eq!(v.cover, [0; 4]);
        assert_eq!(v.with_cover([0.0, 1.0, 0.5, 2.0]).cover, [0, 255, 128, 255]);
    }

    /// Two neighbouring chunks of any level see their common face at the same place in the
    /// pattern (up to whole periods), however far from the planet's centre (the facets of
    /// G2.11: 4 m chunks each drew the same 4 m of a 32 m pattern).
    #[test]
    fn neighbouring_chunks_meet_in_the_pattern() {
        use creeer_math::glam::DVec3;
        for size in [4.0, 8.0, 16.0, 32.0, 4096.0] {
            for k in [-130_001.0, -3.0, 0.0, 255.0, 256.0, 124_999.0] {
                let a = DVec3::new(k * size, -7.0 * size, 501_000.0 - (501_000.0 % size));
                let b = a + DVec3::X * size;
                let (pa, pb) = (pattern_offset(a), pattern_offset(b));
                // A's far face at x = size, B's near face at x = 0.
                #[allow(clippy::cast_possible_truncation)]
                let gap = f64::from(pa[0] + size as f32 - pb[0]);
                let periods = gap / PATTERN_PERIOD;
                assert!(
                    (periods - periods.round()).abs() < 1e-6,
                    "size {size}, chunk {k}: {pa:?} + {size} vs {pb:?}"
                );
                assert!(pa.iter().chain(&pb).all(|&x| (0.0..1024.0).contains(&x)));
                assert_eq!((pa[1], pa[2]), (pb[1], pb[2]));
            }
        }
    }

    #[test]
    fn bounds_enclose_every_vertex() {
        let mesh = TerrainMeshData {
            vertices: [
                Vec3::ZERO,
                Vec3::new(4.0, 0.0, 0.0),
                Vec3::new(0.0, 3.0, 2.0),
            ]
            .map(|p| TerrainVertex::new(p, Vec3::Y, 1.0, [0; 3], 0.5))
            .to_vec(),
            indices: vec![0, 1, 2],
            ..TerrainMeshData::default()
        };
        let (c, r) = mesh.bounds();
        for v in &mesh.vertices {
            assert!((Vec3::from(v.position) - c).length() <= r + 1e-6);
        }
    }
}

/// Where a body's terrain is drawn: its body-fixed frame in simulation space. Chunks are
/// uploaded in body-fixed coordinates; each frame says where the body is and how it is
/// turned, so a moon's terrain can be drawn while it moves and spins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainFrame {
    /// The body's centre (m).
    pub position: creeer_math::glam::DVec3,
    /// Body-fixed → simulation-space rotation.
    pub rotation: creeer_math::glam::DQuat,
}

impl TerrainFrame {
    /// Body-fixed coordinates are simulation space.
    pub const IDENTITY: Self = Self {
        position: creeer_math::glam::DVec3::ZERO,
        rotation: creeer_math::glam::DQuat::IDENTITY,
    };
}
