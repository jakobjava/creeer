//! Planet generation, geology, ore deposits, voxel octree, meshing and edits.
//!
//! F0 contains deterministic noise and the meshing spike; the production terrain arrives in F3.

pub mod chunk;
#[cfg(test)]
mod chunk_tests;
pub mod cube;
pub mod datum;
pub mod edits;
#[cfg(test)]
mod edits_tests;
pub mod elevation;
pub mod geology;
pub mod noise;
pub mod occlusion;
pub mod planet;
pub mod spike;
pub mod stream;
#[cfg(test)]
mod stream_tests;
pub mod surface_nets;
pub mod terrain;
#[cfg(test)]
mod tests;
pub mod tiles;
#[cfg(test)]
mod tiles_tests;

pub use planet::{DataError, Planet};
pub use terrain::Terrain;

/// Path of a file in the repository's `data/` directory (for tools and tests).
pub fn data_path(relative: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(relative)
}
