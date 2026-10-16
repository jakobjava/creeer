//! Species database, mixtures, material properties and microstructure.
//!
//! F3 brings the chemical elements and formulas (for the minerals of the planet's geology);
//! species, mixtures and reactions follow in F6.

pub mod elements;
pub mod formula;
#[cfg(test)]
mod tests;

pub use formula::{Formula, FormulaError};
