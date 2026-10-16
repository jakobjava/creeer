//! Terrain chunk meshes for the terrain pass (`renderer/terrain_pass.rs`).
//!
//! Each chunk is uploaded once ([`crate::Renderer::insert_terrain`]) with its origin in f64
//! simulation space; vertices are relative to that origin in f32, and each frame the chunk is
//! drawn at its camera-relative origin, so terrain stays precise anywhere on a planet.

use bytemuck::{Pod, Zeroable};
use creeer_math::glam::Vec3;

/// A terrain vertex as stored on the GPU (20 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct TerrainVertex {
    /// Position relative to the chunk origin (m).
    pub position: [f32; 3],
    /// Unit normal (snorm) and, in `w`, how open the point is to the sky (0..127).
    pub normal: [i8; 4],
    /// Base colour (sRGB) and roughness in `w`.
    pub color: [u8; 4],
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
        }
    }
}

/// A chunk's mesh on the CPU.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainMeshData {
    /// Vertices relative to the chunk origin.
    pub vertices: Vec<TerrainVertex>,
    /// Triangle list, counter-clockwise seen from the air: the surface first, then the skirts.
    pub indices: Vec<u32>,
    /// Number of indices of the surface (the rest are skirts).
    pub surface_indices: u32,
    /// Index ranges (start, end) of the skirts of each chunk face (−X, +X, −Y, +Y, −Z, +Z),
    /// drawn only where [`crate::Frame::terrain`] asks for them.
    pub skirts: [(u32, u32); 6],
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
        (center, (hi - lo).length() * 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertices_pack_into_twenty_bytes() {
        assert_eq!(std::mem::size_of::<TerrainVertex>(), 20);
        let v = TerrainVertex::new(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y * 2.0,
            0.5,
            [10, 20, 30],
            1.0,
        );
        assert_eq!(v.normal, [0, 127, 0, 64]);
        assert_eq!(v.color, [10, 20, 30, 255]);
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
