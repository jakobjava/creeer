//! Mechanical drivetrains (F6.4): rotating parts joined by shafts, gears, belts, clutches,
//! brakes and bearings, in a housing.
//!
//! - [`Drivetrain`]: built from parts and elements; [`Drivetrain::step`] advances it by
//!   backward Euler, its rigid constraints as multipliers, its friction stuck or slipping
//!   by an active set; every joule booked in its ledger.
//! - Its housing takes the reactions ([`Drivetrain::reaction`]): Newton's third law for the
//!   structure it is mounted on, and the gyroscopic torque of its parts as the housing turns.
//! - An electric machine on a part sees, for each step, the speed it would end with and
//!   how that changes with its torque ([`Drivetrain::response`]): its circuit and its shaft
//!   are solved jointly.

mod book;
mod dense;
mod drivetrain;
mod elements;
#[cfg(test)]
mod gear_tests;
mod grip;
mod solve;
mod step;
#[cfg(test)]
mod tests;

pub use drivetrain::{Drivetrain, Element, Grip, HOUSING, Node, Part, belt_capacity};
pub use step::MechReport;
