//! Planet generation, geology, ore deposits, voxel octree, meshing and edits.
//!
//! F0 contains deterministic noise and the meshing spike; the production terrain arrives in F3.

mod air;
mod caps;
#[cfg(test)]
mod caps_tests;
pub mod chunk;
#[cfg(test)]
mod chunk_tests;
mod clip;
pub mod cube;
pub mod datum;
pub mod edits;
#[cfg(test)]
mod edits_tests;
pub mod elevation;
pub mod geology;
pub mod levels;
#[cfg(test)]
mod levels_tests;
pub mod monotone;
pub mod noise;
pub mod occlusion;
pub mod planet;
mod sea;
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

pub use air::AtmosphereData;
pub use planet::{DataError, Overrides, Planet};
pub use sea::OceanData;
pub use terrain::Terrain;
