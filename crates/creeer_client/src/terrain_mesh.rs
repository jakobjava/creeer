//! A terrain chunk's mesh for the GPU (`PlanetView`'s streamer builds them).

use crate::ground_look::GroundLook;
use creeer_render::{TerrainMeshData, TerrainVertex};
use creeer_terrain::Terrain;
use creeer_terrain::chunk::ChunkKey;

/// A chunk mesh for the GPU: rock colours, openness, roughness, and the ground's layers on it
/// (`GroundLook`).
pub fn render_mesh(terrain: &Terrain, key: ChunkKey) -> Option<TerrainMeshData> {
    let mesh = terrain.mesh_chunk(key)?;
    let rocks = &terrain.planet().geology.rocks.rocks;
    let look = GroundLook::current();
    let origin = key.origin();
    // A vertex lies up to half a cell off the natural surface it approximates: only deeper is
    // it dug.
    let tolerance = 0.5 * key.cell_size() + 0.05;
    let mut heights = terrain.heights().sampler(key.cell_size());
    let vertices = (0..mesh.positions.len())
        .map(|v| {
            let (p, n, m) = (mesh.positions[v], mesh.normals[v], mesh.materials[v]);
            let color = rocks.get(usize::from(m)).map_or([128; 3], |r| r.color);
            // Cap vertices stand for the surface point they are shaded as.
            let at = origin + mesh.anchor(v).as_dvec3();
            let surface = (at, n.as_dvec3());
            let (cover, forest) = look.cover_on(terrain, &mut heights, surface, m, tolerance);
            TerrainVertex::new(p, n, mesh.openness[v], color, 0.88)
                .with_coarser(mesh.coarse_normals[v], mesh.morph[v])
                .with_cover(cover)
                .with_forest(forest)
        })
        .collect();
    let index = |i: usize| u32::try_from(i).expect("chunk mesh too large");
    Some(TerrainMeshData {
        vertices,
        surface_indices: index(mesh.surface_indices),
        caps: mesh
            .cap_faces
            .clone()
            .map(|r| (index(r.start), index(r.end))),
        indices: mesh.indices,
        #[allow(clippy::cast_possible_truncation)] // chunk sizes fit f32
        chunk_size: key.size() as f32,
    })
}
