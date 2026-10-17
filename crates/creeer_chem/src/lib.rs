//! Chemistry (F6.5): mixtures of species (`creeer_materials::species`), their equilibrium,
//! their kinetics, and the reactors they run in.
//!
//! - [`System`]: a set of species, their elements, and a mixture's enthalpy, entropy and
//!   Gibbs energy (ideal gas, pure condensed phases).
//! - [`equilibrate`]: equilibrium by Gibbs energy minimisation under element balance (the
//!   NASA CEA method), at fixed temperature and pressure or adiabatic.
//! - [`vle`]: vapour pressures, boiling points, Raoult's law and flashes.

mod dense;
pub mod electrolysis;
#[cfg(test)]
mod electrolysis_tests;
mod equilibrium;
#[cfg(test)]
mod equilibrium_tests;
pub mod kinetics;
mod phases;
#[cfg(test)]
mod reaction_tests;
pub mod reactor;
#[cfg(test)]
mod reactor_tests;
pub mod real;
pub mod shrinking;
#[cfg(test)]
mod shrinking_tests;
mod system;
pub mod transport;
#[cfg(test)]
mod transport_tests;
pub mod vle;
#[cfg(test)]
mod vle_tests;

pub use equilibrium::{Equilibrium, Fixed, equilibrate};
pub use system::System;
