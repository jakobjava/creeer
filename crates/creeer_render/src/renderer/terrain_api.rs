//! What the client hands the terrain pass: chunks, the ground's layers, its decoratives.

use super::*;
use crate::terrain::{GroundUniform, TerrainMeshData};
use creeer_math::glam::DVec3;

impl Renderer {
    /// Uploads (or replaces) terrain chunk `id`, whose vertices are relative to `origin`.
    pub fn insert_terrain(&mut self, id: u64, origin: DVec3, mesh: &TerrainMeshData) {
        self.terrain.insert(&self.device, id, 0, origin, mesh);
    }

    /// Uploads terrain chunk `id` of the body drawn in `Frame::terrain_frames[frame]`; its
    /// origin and vertices are in that body's fixed frame.
    pub fn insert_terrain_in(
        &mut self,
        id: u64,
        frame: u16,
        origin: DVec3,
        mesh: &TerrainMeshData,
    ) {
        self.terrain.insert(&self.device, id, frame, origin, mesh);
    }

    /// Removes terrain chunk `id` (no-op if absent).
    pub fn remove_terrain(&mut self, id: u64) {
        self.terrain.remove(id);
    }

    /// The ground's layers, in the order the vertices' `cover` names them (at most four; the
    /// rest are dropped).
    pub fn set_ground_layers(&mut self, layers: &[crate::GroundLayer]) {
        let canopy = self.terrain.layers.canopy;
        self.terrain.layers = GroundUniform {
            canopy,
            ..GroundUniform::of(layers)
        };
    }

    /// The forest's crowns as the ground shows them from `from` metres on, where trees are no
    /// longer drawn (`colour` linear; `from` 0: never), weighted by how fully a forest grows at
    /// each vertex (`TerrainVertex::with_forest`).
    pub fn set_canopy(&mut self, colour: [f32; 3], from: f32) {
        self.terrain.layers.canopy = [colour[0], colour[1], colour[2], from.max(0.0)];
    }

    /// The decoratives to draw this frame, camera-relative: grass tufts and grit (`cover.rs`).
    pub fn set_cover(
        &mut self,
        tufts: &[crate::cover::CoverInstance],
        pebbles: &[crate::cover::CoverInstance],
    ) {
        (self.terrain.cover).set(&self.device, &self.queue, [tufts, pebbles]);
    }

    /// Number of terrain chunks on the GPU.
    pub fn terrain_chunk_count(&self) -> usize {
        self.terrain.len()
    }
}
