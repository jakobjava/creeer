//! Orbital mechanics (ADR-0007).
//!
//! - [`body`]: celestial bodies and their rotation.
//! - [`radau`]: the adaptive 15th-order integrator every trajectory goes through.
//! - [`majors`] and [`ephemeris`]: the major bodies under their mutual gravity and pushes,
//!   stored as trajectories everything else can query.
//! - [`rails`]: ships and small bodies propagated against the ephemeris.
//! - [`analysis`]: hierarchy, Lagrange points, Hill and Roche limits, transfers.
//! - [`generate`]: procedural solar systems from data, checked for stability.
//! - [`kepler`]: exact two-body motion and orbital elements (analysis, start states, tests).

pub mod analysis;
pub mod body;
pub mod ephemeris;
pub mod generate;
pub mod kepler;
pub mod majors;
pub mod radau;
pub mod rails;
#[cfg(test)]
mod rails_tests;
#[cfg(test)]
mod testkit;

pub use body::{Body, BodyKind, G, Spin};
pub use ephemeris::Ephemeris;
pub use majors::Push;
