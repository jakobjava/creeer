//! Chemistry (F6.5): mixtures of species (`creeer_materials::species`), their equilibrium,
//! their kinetics, and the reactors they run in.
//!
//! - [`System`]: a set of species, their elements, and a mixture's enthalpy, entropy and
//!   Gibbs energy (ideal gas, pure condensed phases).
//! - [`equilibrate`]: equilibrium by Gibbs energy minimisation under element balance (the
//!   NASA CEA method), at fixed temperature and pressure or adiabatic.

mod equilibrium;
#[cfg(test)]
mod equilibrium_tests;
mod phases;
#[cfg(test)]
mod reaction_tests;
mod system;

pub use equilibrium::{Equilibrium, Fixed, equilibrate};
pub use system::System;
