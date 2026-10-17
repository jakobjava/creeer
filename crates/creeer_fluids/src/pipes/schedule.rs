//! Each liquid network's own rate (ADR-0014): every tick (60 Hz) when its step fits its share
//! of the pipes' budget, else every third (20 Hz), at most every sixth (10 Hz). Large meshed
//! networks are the ones that slow: their factorisation's work grows faster than their pipes.
//!
//! The rate comes from an estimate of a step's cost on the minimum-spec CPU made from the
//! network's structure alone: a step's fixed cost; its links (two Newton iterations' laws, in
//! parallel from 256 links, and its content carried); its factor's size (one factorisation and
//! about five solves with it: the first iteration's, the second's preconditioned conjugate
//! gradients). Never from a measured time: the same network picks the same rate on every
//! machine and every run. It is worked out again only when the network's structure changes
//! (a node or link added).
//!
//! The constants are this machine's measurements, doubled for the minimum-spec CPU's half
//! speed (as the benches project): a 21-link house, 2.4 µs a step with its content; the fluids
//! bench's 4,900-pipe town grid, 1.5 ms (16 threads).
//!
//! **Shares.** A network's share of the 1 ms budget for 5,000 segments is its links' part of
//! them, but at least 50 segments' (10 µs): a step's fixed cost (about 2 µs on the minimum-spec
//! CPU) is not paid per link, and small networks, which a world has many of, run every tick.

use super::{Network, PipeReport};

/// The pipes' budget a tick on the minimum-spec CPU (ns), and the segments it is for.
pub const BUDGET_NS: f64 = 1.0e6;
pub const BUDGET_SEGMENTS: f64 = 5_000.0;
/// The least share a network has, in segments.
pub const LEAST_SHARE: f64 = 50.0;

/// Ticks a step may take: 60, 20 or 10 Hz.
pub const RATES: [u32; 3] = [1, 3, 6];

/// Minimum-spec nanoseconds: a step's fixed cost; a link's law (sequential, and in parallel
/// from `PARALLEL` links) and its content carried; a factorisation's multiply-add; an entry of
/// L in a solve (forward and back).
const FIXED_NS: f64 = 2_500.0;
const LAW_NS: f64 = 60.0;
const LAW_PARALLEL_NS: f64 = 32.0;
const PARALLEL: f64 = 256.0;
const CARRY_NS: f64 = 50.0;
const FLOP_NS: f64 = 2.0;
const SOLVE_NS: f64 = 7.0;

impl Network {
    /// A step's estimated cost on the minimum-spec CPU (ns), from the network's structure.
    pub fn estimated_cost(&mut self) -> f64 {
        let mut solver = self
            .solver
            .take()
            .unwrap_or_else(|| super::liquid::Solver::new(self));
        let (nnz, flops) = solver.pattern_size(self);
        self.solver = Some(solver);
        #[allow(clippy::cast_precision_loss)] // counts far below 2^52
        let (links, nnz, flops) = (self.links.len() as f64, nnz as f64, flops as f64);
        let law = if links >= PARALLEL {
            LAW_PARALLEL_NS
        } else {
            LAW_NS
        };
        FIXED_NS + links * (2.0 * law + CARRY_NS) + flops * FLOP_NS + 5.0 * nnz * SOLVE_NS
    }

    /// Its share of the pipes' budget a tick (ns): its links' part of the budget's segments,
    /// at least [`LEAST_SHARE`]'s.
    pub fn budget_share(&self) -> f64 {
        #[allow(clippy::cast_precision_loss)] // counts far below 2^52
        let links = self.links.len() as f64;
        BUDGET_NS * links.max(LEAST_SHARE) / BUDGET_SEGMENTS
    }

    /// Ticks a step takes: the fewest of [`RATES`] whose cost a tick fits its share (the most,
    /// 10 Hz, if none does).
    pub fn ticks_per_step(&mut self) -> u32 {
        if let Some(every) = self.solver.as_ref().and_then(|s| s.every) {
            return every;
        }
        let (cost, share) = (self.estimated_cost(), self.budget_share());
        let every = RATES
            .into_iter()
            .find(|&k| cost / f64::from(k) <= share)
            .unwrap_or(RATES[RATES.len() - 1]);
        self.solver.as_mut().expect("estimated").every = Some(every);
        every
    }

    /// One tick of `dt` seconds: the network steps on the ticks its rate falls on (the first
    /// one included), by its rate's whole interval. Returns the step's outcome and length, on
    /// the ticks it stepped.
    pub fn tick(&mut self, dt: f64) -> Option<(PipeReport, f64)> {
        let every = self.ticks_per_step();
        let due = self.ticks.is_multiple_of(u64::from(every));
        self.ticks += 1;
        due.then(|| {
            let h = dt * f64::from(every);
            (self.step(h), h)
        })
    }
}
