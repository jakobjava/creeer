//! Building (G2, `docs/g2-plan.md`): the fasteners that join parts and the joint laws they make
//! (ADR-0033). Parts become members of one structure where they are fastened.

pub mod fasteners;
pub mod law;
#[cfg(test)]
mod tests;
