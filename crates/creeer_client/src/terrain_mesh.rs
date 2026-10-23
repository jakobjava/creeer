//! A terrain chunk's mesh for the GPU (`PlanetView`'s streamer builds them).

use creeer_render::{TerrainMeshData, TerrainVertex};
use creeer_terrain::Terrain;
use creeer_terrain::chunk::ChunkKey;

/// A chunk mesh for the GPU: rock colours, openness, roughness.
pub fn render_mesh(terrain: &Terrain, key: ChunkKey) -> Option<TerrainMeshData> {
    let mesh = terrain.mesh_chunk(key)?;
    let rocks = &terrain.planet().geology.rocks.rocks;
    let vertices = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .zip(&mesh.materials)
        .zip(&mesh.openness)
        .zip(mesh.coarse_normals.iter().zip(&mesh.morph))
        .map(|((((p, n), &m), &open), (coarse, &morph))| {
            let color = rocks.get(usize::from(m)).map_or([128; 3], |r| r.color);
            TerrainVertex::new(*p, *n, open, color, 0.88).with_coarser(*coarse, morph)
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
