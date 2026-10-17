//! Unified physics: nodes, beams, rigid bodies, contacts, plasticity, fracture.
//!
//! Structures and contacts follow ADR-0004 and ADR-0005; planet gravity and terrain
//! contacts were added in F3 (`gravity`, `world::ground`).

pub mod gravity;
pub mod profile;
pub mod rotation;
pub mod so3;
pub mod sparse;
pub mod spike;
pub mod structure;
pub mod world;
