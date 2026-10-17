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

use super::{Network, NodeKind, PipeReport};
use creeer_network::Ledger;
use std::sync::mpsc;

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
        if let Some(every) = self.held.or(self.rate) {
            return every;
        }
        let (cost, share) = (self.estimated_cost(), self.budget_share());
        let every = RATES
            .into_iter()
            .find(|&k| cost / f64::from(k) <= share)
            .unwrap_or(RATES[RATES.len() - 1]);
        self.rate = Some(every);
        every
    }

    /// Holds the network at a step every `every` ticks (of [`RATES`]) whatever its cost, or
    /// (`None`) lets it pick again: for tests and benches of slow networks.
    pub fn hold_rate(&mut self, every: Option<u32>) {
        self.held = every;
    }

    /// One tick of `dt` seconds: the network steps on the ticks its rate falls on (the first
    /// one included), by its rate's whole interval. Returns the step's outcome and length, on
    /// the ticks a step ends.
    ///
    /// A network stepping every tick steps at once. A slower one (ADR-0016) begins its step on
    /// a worker from its state on the tick its rate falls on, and ends it on the next such tick
    /// (the step's heads and flows are those of its interval's end, taken at its end): the
    /// step runs beside the ticks between, not as one tick's spike. What it is begun from and
    /// ended on are fixed ticks: no thread's timing can change what it finds. Settings changed
    /// in between (a valve, a turbine's speed) count from the next step.
    pub fn tick(&mut self, dt: f64) -> Option<(PipeReport, f64)> {
        let every = self.ticks_per_step();
        let due = self.ticks.is_multiple_of(u64::from(every));
        self.ticks += 1;
        if !due {
            return None;
        }
        let h = dt * f64::from(every);
        if every == 1 {
            return Some((self.step(h), h));
        }
        let done = self.take_pending();
        self.begin(h);
        done
    }

    /// Begins a step of `h` on a worker, from the network as it is.
    fn begin(&mut self, h: f64) {
        let mut job = Network {
            nodes: self.nodes.clone(),
            links: self.links.clone(),
            liquid: self.liquid,
            gravity: self.gravity,
            ledger: Ledger::new(0.0),
            solver: self.solver.take(),
            ticks: self.ticks,
            rate: self.rate,
            held: self.held,
            pending: Pending::default(),
        };
        let (send, receive) = mpsc::channel();
        rayon::spawn(move || {
            let report = job.step(h);
            // (Taken on the tick the step ends; a network dropped before then never takes it.)
            let _ = send.send((job, report, h));
        });
        self.pending = Pending(Some(receive));
    }

    /// Ends the step under way, if any: what it found (the heads it solved for, the flows,
    /// which links are open), its solver back, its books added. Returns its outcome and length.
    pub(crate) fn take_pending(&mut self) -> Option<(PipeReport, f64)> {
        let receive = self.pending.0.take()?;
        let (job, report, h) = receive.recv().expect("a step under way ends");
        for (node, solved) in self.nodes.iter_mut().zip(&job.nodes) {
            if !matches!(node.kind, NodeKind::Reservoir) {
                node.head = solved.head;
            }
        }
        for (link, solved) in self.links.iter_mut().zip(&job.links) {
            (link.flow, link.open) = (solved.flow, solved.open);
        }
        self.solver = job.solver;
        let (supplied, dissipated, exported, imported) = job.ledger.totals();
        self.ledger.supply(supplied);
        self.ledger.dissipate(dissipated);
        self.ledger.export(exported);
        self.ledger.import(imported);
        Some((report, h))
    }
}

/// A slow network's step under way on a worker (`Network::tick`).
#[derive(Debug, Default)]
pub(crate) struct Pending(Option<mpsc::Receiver<(Network, PipeReport, f64)>>);

impl Clone for Pending {
    /// # Panics
    /// If a step is under way: a copy would lack its outcome.
    fn clone(&self) -> Self {
        assert!(
            self.0.is_none(),
            "a network cannot be copied while its step is under way"
        );
        Self(None)
    }
}
