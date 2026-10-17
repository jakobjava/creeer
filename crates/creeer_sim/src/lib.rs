//! World composition: all simulation systems, simulation LOD, save/load.
//!
//! F3: a planet's terrain as the ground of the physics world, and its gravity.
//! F4: the universe: a generated solar system, vessels, and the physics bubble that moves
//! between frames (ADR-0007).
//! F6: heat in structures (`heat`): members lengthening and weakening as they warm.

pub mod avatar;
pub mod heat;
#[cfg(test)]
mod heat_tests;
pub mod planet;
#[cfg(test)]
mod planet_tests;
pub mod save;
pub mod universe;

pub use planet::{TerrainGround, planet_gravity};
pub use universe::Universe;
