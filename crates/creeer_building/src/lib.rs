//! Building (G2, `docs/g2-plan.md`): the fasteners that join parts and the joint laws they make
//! (ADR-0033). Parts become members of one structure where they are fastened.

pub mod assembly;
#[cfg(test)]
mod assembly_tests;
pub mod fasteners;
pub mod law;
pub mod soil;
#[cfg(test)]
mod soil_tests;
#[cfg(test)]
mod tests;
