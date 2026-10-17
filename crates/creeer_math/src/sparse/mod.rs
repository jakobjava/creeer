//! Sparse direct solvers for the simulation's networks (F6, ADR-0009): heat, electricity,
//! drivetrains and pipes all come down to solving a sparse linear system each step.
//!
//! - [`Assembly`]: a system's matrix pattern, made once from where its elements write, and
//!   the deterministic summation of their contributions each step.
//! - [`Lu`]: LU with partial pivoting for unsymmetric systems (circuits with sources, pipe
//!   networks), real or [`Complex`] (AC phasors), with a fast refactorisation that reuses the
//!   pivots.
//! - [`Cholesky`]: for symmetric positive-definite systems (conduction, resistor networks).
//!
//! Like the structure solver (ADR-0004), everything is plain scalar arithmetic in a fixed
//! order, without threads or SIMD dispatch: the same bits on every platform.

mod chol;
mod lu;
mod matrix;
mod order;
mod scalar;
#[cfg(test)]
mod tests;

pub use chol::{Cholesky, NotPositiveDefinite};
pub use lu::{Lu, PIVOT_TOLERANCE, Singular};
pub use matrix::{Assembly, Csc};
pub use order::minimum_degree;
pub use scalar::{Complex, Scalar};
