//! What the simulation's networks share (F6, ADR-0009): heat, current, torque and flow along
//! networks are solved alike (`creeer_math::sparse`), and organised alike:
//!
//! - [`Islands`]: a network splits into the parts nothing joins, each solved on its own (in
//!   parallel), each able to sleep.
//! - [`Ledger`]: where a network's energy went: supplied, dissipated, passed to or from other
//!   domains, stored. A ledger that does not close is a bug (tests check every domain's).
//! - [`Exchange`]: energy passed between domains that step at different rates (heat at 20 Hz,
//!   current at 60 Hz): accumulated, never sampled, so nothing is lost or made between rates.
//! - [`Schedule`]: which ticks a domain steps on, spread so that the slow domains do not all
//!   step on the same tick.
//! - [`Sleep`]: an island that has stopped changing stops being solved, until something
//!   changes.

mod exchange;
mod islands;
mod ledger;
pub mod sleep;
#[cfg(test)]
mod tests;

pub use exchange::{Exchange, Schedule};
pub use islands::Islands;
pub use ledger::{Ledger, Sum};
pub use sleep::Sleep;
