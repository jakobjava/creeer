//! World composition: all simulation systems, simulation LOD, save/load.
//!
//! F3: a planet's terrain as the ground of the physics world, and its gravity.
//! F4: the universe: a generated solar system, vessels, and the physics bubble that moves
//! between frames (ADR-0007).

pub mod avatar;
pub mod planet;
#[cfg(test)]
mod planet_tests;
pub mod save;
pub mod universe;

pub use planet::{TerrainGround, planet_gravity};
pub use universe::Universe;
