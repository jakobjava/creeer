//! Electrical networks (F6.3).
//!
//! - [`Circuit`]: DC and transient circuits by modified nodal analysis on the sparse LU
//!   (`creeer_math::sparse`): resistors and wires (their resistance following their
//!   temperature), capacitors, inductors, sources, switches, fuses, breakers, diodes and
//!   LEDs, batteries, solar modules, DC machines. Stepped by BDF2, nonlinear devices by
//!   Newton's method, island by island in parallel; every joule booked in its ledger.
//! - [`Grid`]: AC networks as phasors at their frequency, quasi-static each tick:
//!   transformers, induction motors, constant-power loads.
//!
//! What a 60 Hz tick cannot resolve is not pretended: PWM switching is an average, an AC
//! waveform a phasor, a transient faster than the tick settles where it settles.

pub mod cell;
#[cfg(test)]
mod cell_tests;
mod circuit;
mod component;
pub mod devices;
mod finish;
mod grid;
mod grid_step;
#[cfg(test)]
mod grid_tests;
pub mod induction;
mod mna;
mod nonlinear;
mod solve;
mod step;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation;

pub use cell::Electrolyzer;
pub use circuit::{Circuit, ComponentId};
pub use component::{CONTACT, GROUND, Kind, Node, Shaft};
pub use devices::{Battery, DcMachine, Diode, SolarModule};
pub use grid::{Branch, Bus, Grid, NEUTRAL};
pub use grid_step::GridReport;
pub use induction::InductionMotor;
pub use step::StepReport;
