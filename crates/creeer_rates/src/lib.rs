//! Abstract mode (F7.3, ADR-0020): machines far from every player run as rate equations
//! with buffers, so a factory keeps producing while nobody is near it (plan.md §1.6).
//!
//! - A [`Stock`] is an amount of something conserved (a species' mass, water's volume, a
//!   battery's charge, heat in a large mass) with the bounds it lies between.
//! - A [`Process`] is what a machine does per second at full rate: flows into and out of
//!   stocks, each depending linearly on the stocks it was measured against, trusted within
//!   a box of them. Its rates come from the machine's own full simulation (F7.4), never from
//!   a hand-written recipe.
//! - A [`Network`] advances its stocks segment by segment. A stock at a bound limits the
//!   processes that would push it past: its consumers share what flows in (or its producers
//!   what flows out) in proportion to what they ask, each also held to its other limits
//!   ([`throttle`]). Events (a stock reaching or leaving a bound, a process leaving its
//!   trusted box) end segments exactly where they happen.
//! - **Exact books:** each process's amount over a segment is computed once and applied to
//!   all its stocks; every stock keeps what came in and went out in compensated sums, so its
//!   amount is its start plus its inflow less its outflow, to rounding, over any time.
//!
//! Networks are independent: machines sharing a stock share a network; others are advanced
//! apart (in parallel: [`advance_all`]), and only when something needs them (lazily).

mod exact;
mod expm;
mod held;
pub mod network;
mod scalar;
mod segment;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_events;
pub mod throttle;

pub use network::{Event, Network};

use rayon::prelude::*;

/// A stock's index in its network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StockId(pub usize);

/// A process's index in its network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId(pub usize);

/// An amount of something conserved, between its bounds (`low` may be −∞ and `high` +∞:
/// the surroundings, a sink that never fills).
#[derive(Clone, Debug, PartialEq)]
pub struct Stock {
    pub name: String,
    pub amount: f64,
    pub low: f64,
    pub high: f64,
}

impl Stock {
    /// A stock of `amount` between `low` and `high`.
    pub fn new(name: &str, amount: f64, low: f64, high: f64) -> Self {
        Self {
            name: name.to_owned(),
            amount,
            low,
            high,
        }
    }

    /// The surroundings: a stock without bounds.
    pub fn unbounded(name: &str) -> Self {
        Self::new(name, 0.0, f64::NEG_INFINITY, f64::INFINITY)
    }
}

/// One flow of a process at full rate: into `stock` (negative: out of it), per second, as
/// measured with the stocks at `slope`'s points; it changes by `slope`'s gradients as the
/// stocks move from them.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub stock: StockId,
    pub rate: f64,
    /// (stock, d rate / d amount, the amount it was measured at).
    pub slope: Vec<(StockId, f64, f64)>,
}

impl Flow {
    /// A constant flow.
    pub fn constant(stock: StockId, rate: f64) -> Self {
        Self {
            stock,
            rate,
            slope: Vec::new(),
        }
    }

    /// Its full rate with the stocks at `amounts`.
    pub fn at(&self, amounts: &[f64]) -> f64 {
        self.slope
            .iter()
            .fold(self.rate, |r, &(s, d, at)| r + d * (amounts[s.0] - at))
    }
}

/// What a machine does per second at full rate, and where that is trusted.
#[derive(Clone, Debug, PartialEq)]
pub struct Process {
    pub name: String,
    pub flows: Vec<Flow>,
    /// (stock, lowest, highest): outside this box the rates are stale (the machine must be
    /// measured again: [`Event::Stale`]).
    pub trust: Vec<(StockId, f64, f64)>,
}

impl Process {
    /// A process of constant flows, trusted everywhere.
    pub fn constant(name: &str, flows: &[(StockId, f64)]) -> Self {
        Self {
            name: name.to_owned(),
            flows: flows.iter().map(|&(s, r)| Flow::constant(s, r)).collect(),
            trust: Vec::new(),
        }
    }
}

/// Advances every network to `time` side by side; returns each one's events.
pub fn advance_all(networks: &mut [Network], time: f64) -> Vec<Vec<Event>> {
    networks.par_iter_mut().map(|n| n.advance(time)).collect()
}
