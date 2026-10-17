//! Fluids (F6.6): atmospheres, pipe networks, aerodynamics and buoyancy.
//!
//! - [`atmosphere`]: a planet's air at any altitude (temperature, pressure, density,
//!   viscosity, sound) from its data, hydrostatic under its real gravity.
//! - [`pipes`]: liquid pipe networks (junctions, reservoirs, tanks, pipes, pumps, valves) by the
//!   global gradient method.

pub mod atmosphere;
#[cfg(test)]
mod atmosphere_tests;
pub mod pipes;
