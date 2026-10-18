//! World composition: all simulation systems, simulation LOD, save/load.
//!
//! F3: a planet's terrain as the ground of the physics world, and its gravity.
//! F4: the universe: a generated solar system, vessels, and the physics bubble that moves
//! between frames (ADR-0007).
//! F6: heat in structures (`heat`): members lengthening and weakening as they warm.
//! F6: electric machines on drivetrains (`drive`): circuit and shaft solved as one.
//! F6: turbines on drivetrains (`hydro`): the water's torque on a runner every tick.

pub mod abstraction;
#[cfg(test)]
mod abstraction_tests;
pub mod aero;
#[cfg(test)]
mod aero_tests;
pub mod avatar;
pub mod character;
pub mod content;
#[cfg(test)]
mod content_tests;
pub mod controls;
#[cfg(test)]
mod controls_tests;
pub mod drive;
#[cfg(test)]
mod drive_tests;
pub mod electrochem;
#[cfg(test)]
mod electrochem_tests;
pub mod heat;
#[cfg(test)]
mod heat_tests;
pub mod hydro;
#[cfg(test)]
mod hydro_tests;
#[cfg(test)]
mod joule_tests;
pub mod lod;
#[cfg(test)]
mod lod_tests;
pub mod mount;
pub mod planet;
#[cfg(test)]
mod planet_tests;
pub mod powerhouse;
pub mod save;
pub mod sea;
#[cfg(test)]
mod station_tests;
pub mod surroundings;
#[cfg(test)]
mod surroundings_tests;
pub mod universe;

pub use planet::{TerrainGround, planet_gravity};
pub use universe::Universe;
