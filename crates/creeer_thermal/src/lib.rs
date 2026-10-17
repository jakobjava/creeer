//! Thermal networks (F6.2): parts as lumped masses whose temperatures follow the heat they
//! gain, joined by conduction, convection and radiation, and held against fixed temperatures
//! (the air, the ground, the sky).
//!
//! - [`Enthalpy`]: a material's stored heat against temperature, melting included: the one
//!   quantity the step conserves.
//! - [`Network`]: nodes and links; [`Network::step`] advances it by backward Euler with
//!   Newton's method, island by island, in parallel, every joule booked in its [`Ledger`].
//! - [`coefficient`]: convection coefficients from the textbook correlations, with the
//!   [`Fluid`]'s (air's or water's) properties at the film temperature.
//!
//! [`Ledger`]: creeer_network::Ledger

pub mod contact;
mod convection;
mod enthalpy;
pub mod environment;
mod fluid;
#[cfg(test)]
mod fluid_tests;
mod island;
mod kernel;
mod network;
mod step;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation;

pub use convection::{Shape, coefficient};
pub use enthalpy::{Enthalpy, MELT_SPREAD, REFERENCE};
pub use fluid::{Fluid, Medium};
pub use island::{REUSE, TOLERANCE};
pub use network::{Link, Network, NodeId, NodeKind, STEFAN_BOLTZMANN};
pub use step::StepReport;
