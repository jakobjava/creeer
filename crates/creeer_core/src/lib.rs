//! Core runtime: fixed-tick clock, simulation thread, timing statistics.

pub mod clock;
pub mod sim_thread;
pub mod stats;

pub use clock::FixedClock;
pub use sim_thread::{Published, SimThread, Simulation};
pub use stats::{RateMeter, TimingStats};
