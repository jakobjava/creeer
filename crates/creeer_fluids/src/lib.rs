//! Fluids (F6.6): atmospheres, pipe networks, aerodynamics and buoyancy.
//!
//! - [`atmosphere`]: a planet's air at any altitude (temperature, pressure, density,
//!   viscosity, sound) from its data, hydrostatic under its real gravity.

pub mod atmosphere;
#[cfg(test)]
mod atmosphere_tests;
