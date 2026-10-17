//! Species database, mixtures, material properties and microstructure.
//!
//! - F3: the chemical elements and formulas (for the minerals of the planet's geology).
//! - F6: engineering materials with their properties against temperature
//!   ([`engineering`], `data/materials/`); species, mixtures and reactions follow (F6.5).

pub mod curve;
pub mod elements;
pub mod engineering;
pub mod formula;
#[cfg(test)]
mod material_tests;
pub mod table;
#[cfg(test)]
mod tests;

pub use curve::Curve;
pub use engineering::{Kind, Material, Melting, RateLaw, Structural};
pub use formula::{Formula, FormulaError};
pub use table::MaterialTable;
