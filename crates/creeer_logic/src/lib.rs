//! Logic (F6.7, ADR-0015): signals on wires between gates, latches, timers, comparators and
//! PID blocks, sensors that read any domain and actuators that set it, and Lua 5.4
//! microcontrollers run within a budget, the same on every machine.

mod block;
mod network;
mod order;
mod persist;
mod run;
mod sandbox;
mod save;
mod script;
mod signal;

pub use block::{Compare, Kind, Pid};
pub use network::{BlockId, Network, Pin};
pub use script::{Limits, Script};
pub use signal::Signal;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod script_tests;
