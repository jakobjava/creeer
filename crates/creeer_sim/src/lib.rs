//! World composition: all simulation systems, simulation LOD, save/load.
//!
//! F3: a planet's terrain as the ground of the physics world, and its gravity.

pub mod planet;
#[cfg(test)]
mod planet_tests;

pub use planet::{TerrainGround, planet_gravity};
