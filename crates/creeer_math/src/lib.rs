//! f64 math for the simulation: reference frames, camera-relative render transforms and
//! deterministic elementary functions, and quantities with units for data files.
//!
//! Conventions used across the whole project:
//! - SI units everywhere (m, kg, s, N, K, ...).
//! - Simulation math is f64; only render space is f32 (see [`render_space`]).
//! - Simulation code uses [`det`] instead of `f64::sin` & co. for cross-platform determinism.

pub mod codec;
pub mod det;
pub mod frame;
pub mod render_space;
pub mod sparse;
pub mod units;

pub use frame::Frame64;
/// Re-exported so every crate uses the same glam version and features.
pub use glam;
