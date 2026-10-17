//! One segment: its length (to the next event, and short enough that the rates change by
//! at most [`CHANGE`] over it), the throttles, and the amounts moved.
//!
//! The rates are integrated by the trapezoidal rule: the rates at the segment's start and
//! at its end as predicted from the start's (a predictor-corrector, second order). The
//! amounts are then each process's throttle times its mean rate times the length, applied to
//! every stock it flows into or out of: the books are exact whatever the integration's error.

use crate::network::{Event, Network};
use crate::throttle::{Side, throttles};
use crate::{ProcessId, StockId};

/// The most a flow's rate may change over a segment, relative to its size: the trapezoidal
/// rule's error is then below about 10⁻⁵ of the amount moved.
pub(crate) const CHANGE: f64 = 0.01;
/// A stock this close to a bound (relative to the bound's size, at least 1) is at it.
const AT: f64 = 1e-12;
/// Shortenings of a segment at most, to meet [`CHANGE`].
const HALVINGS: usize = 40;

/// Event `e` at `time`.
fn at_time(e: Event, time: f64) -> Event {
    match e {
        Event::Empty { stock, .. } => Event::Empty { stock, time },
        Event::Full { stock, .. } => Event::Full { stock, time },
        Event::Stale { process, .. } => Event::Stale { process, time },
    }
}

/// The bound stock `s` is at, if any.
pub(crate) fn bound_at(amount: f64, s: &crate::Stock) -> Option<Side> {
    at_bound(amount, s.low, s.high)
}

/// The bound an amount is at, if any.
fn at_bound(amount: f64, low: f64, high: f64) -> Option<Side> {
    let near = |b: f64| AT * b.abs().max(1.0);
    if low.is_finite() && amount - low <= near(low) {
        Some(Side::Empty)
    } else if high.is_finite() && high - amount <= near(high) {
        Some(Side::Full)
    } else {
        None
    }
}

impl Network {
    /// Each flow's full rate with the stocks at `amounts`, into `out` (flat, as `layout`).
    fn rates_into(&self, amounts: &[f64], out: &mut Vec<f64>) {
        out.clear();
        for process in &self.processes {
            out.extend(process.flows.iter().map(|f| f.at(amounts)));
        }
    }

    /// The throttles for flat `rates`, with the stocks at `amounts`.
    fn throttles_for(&self, rates: &[f64], amounts: &[f64]) -> Vec<f64> {
        let pinned: Vec<(usize, Side)> = (self.stocks.iter().enumerate())
            .filter_map(|(k, s)| at_bound(amounts[k], s.low, s.high).map(|side| (k, side)))
            .collect();
        if pinned.is_empty() {
            return vec![1.0; self.processes.len()];
        }
        throttles(self.processes.len(), &self.layout, rates, &pinned)
    }

    /// Each stock's net rate under `throttle`.
    fn net_into(&self, rates: &[f64], throttle: &[f64], net: &mut Vec<f64>) {
        net.clear();
        net.resize(self.stocks.len(), 0.0);
        for (&(p, s), &r) in self.layout.iter().zip(rates) {
            net[s] += throttle[p] * r;
        }
    }

    /// The first event within `h` (s) moving at `net` from `amounts`, and when.
    fn first_event(&self, amounts: &[f64], net: &[f64], h: f64) -> (f64, Option<Event>) {
        let mut first = (h, None);
        for (k, s) in self.stocks.iter().enumerate() {
            // Towards the bound it moves to, unless it is at that one already.
            let at = at_bound(amounts[k], s.low, s.high);
            let (bound, side) = if net[k] < 0.0 {
                (s.low, Side::Empty)
            } else {
                (s.high, Side::Full)
            };
            if at == Some(side) {
                continue;
            }
            let t = (bound - amounts[k]) / net[k];
            if t.is_finite() && t >= 0.0 && t < first.0 {
                let stock = StockId(k);
                let event = match side {
                    Side::Empty => Event::Empty { stock, time: 0.0 },
                    Side::Full => Event::Full { stock, time: 0.0 },
                };
                first = (t, Some(event));
            }
        }
        for (p, process) in self.processes.iter().enumerate() {
            for &(s, lo, hi) in &process.trust {
                let (x, v) = (amounts[s.0], net[s.0]);
                let t = if v < 0.0 { (lo - x) / v } else { (hi - x) / v };
                if t.is_finite() && t >= 0.0 && t < first.0 {
                    let process = ProcessId(p);
                    first = (t, Some(Event::Stale { process, time: 0.0 }));
                }
            }
        }
        first
    }

    /// One segment of at most `most` seconds: applies it; returns its length and the event
    /// it ends at, if any.
    pub(crate) fn segment(&mut self, most: f64) -> (f64, Option<Event>) {
        let amounts: Vec<f64> = self.stocks.iter().map(|s| s.amount).collect();
        // A process outside its trusted box already: stale now.
        for (p, process) in self.processes.iter().enumerate() {
            let outside =
                (process.trust.iter()).any(|&(s, lo, hi)| !(lo..=hi).contains(&amounts[s.0]));
            if outside {
                let process = ProcessId(p);
                return (
                    0.0,
                    Some(Event::Stale {
                        process,
                        time: self.time,
                    }),
                );
            }
        }
        let mut scratch = std::mem::take(&mut self.scratch);
        self.rates_into(&amounts, &mut scratch.r0);
        let u0 = self.throttles_for(&scratch.r0, &amounts);
        self.net_into(&scratch.r0, &u0, &mut scratch.net);
        let mut held = std::mem::take(&mut scratch.held);
        self.held_into(
            (&amounts, &scratch.r0, &u0),
            &mut scratch.free,
            &mut scratch.s1,
            &mut held,
        );
        let mut gates = std::mem::take(&mut scratch.gates);
        let exact = self.gates_into(
            (&held, &u0),
            &mut scratch.steady,
            &mut scratch.holds,
            &mut gates,
        );
        if exact {
            let (h, event) = self.exact_segment(most, &gates);
            (scratch.held, scratch.gates) = (held, gates);
            self.scratch = scratch;
            return (h, event.map(|e| at_time(e, self.time + h)));
        }
        (scratch.held, scratch.gates) = (held.clone(), gates);
        let (mut h, mut event) = self.first_event(&amounts, &scratch.net, most);
        // Short enough that the rates change by at most CHANGE.
        // Each stock's throughput (the sum of its flows' sizes): what a flow's change is
        // measured against (a flow starting from nothing changes by all of itself at once).
        let mut through = vec![0.0f64; self.stocks.len()];
        for (&(p, s), &r) in self.layout.iter().zip(&scratch.r0) {
            through[s] += (u0[p] * r).abs();
        }
        let states = self.states.clone();
        let mut net1 = Vec::new();
        // The held stocks' nets unthrottled: held while those push past.
        let ones = vec![1.0; self.processes.len()];
        let (mut free0, mut free1) = (Vec::new(), Vec::new());
        self.net_into(&scratch.r0, &ones, &mut free0);
        for _ in 0..HALVINGS {
            scratch.s1.clear();
            scratch
                .s1
                .extend(amounts.iter().zip(&scratch.net).map(|(&a, &v)| a + v * h));
            self.rates_into(&scratch.s1, &mut scratch.r1);
            // A held stock is released where its unthrottled net turns inward (linear in time
            // along the segment): the segment ends there, the next in the new regime.
            self.net_into(&scratch.r1, &ones, &mut free1);
            let release = (held.iter())
                .filter_map(|&(k, side)| {
                    let (a, b) = (free0[k], free1[k]);
                    let crosses = match side {
                        Side::Empty => b >= 0.0,
                        Side::Full => b <= 0.0,
                    };
                    crosses.then(|| h * a / (a - b))
                })
                .filter(|t| t.is_finite() && *t > 0.0 && *t < h)
                .fold(f64::INFINITY, f64::min);
            if release < h {
                h = release;
                event = None;
                continue;
            }
            // In the segment's regime: the stocks held as at its start (one reaching its bound
            // at the end is the event that ends it).
            let u1 = self.throttles_for(&scratch.r1, &amounts);
            // The trapezoidal rule is exact for rates linear in time: they are, while the
            // stocks they depend on move steadily. So the measure is how much those stocks'
            // net rates change over the segment, relative to their throughput (a process held
            // at a full or empty stock whose flows scale together moves them steadily,
            // however its own rates change).
            self.net_into(&scratch.r1, &u1, &mut net1);
            let change = (states.iter())
                .map(|&s| {
                    let (a, b) = (scratch.net[s], net1[s]);
                    (a - b).abs() / through[s].max(a.abs()).max(b.abs()).max(f64::MIN_POSITIVE)
                })
                .fold(0.0, f64::max);
            if change <= CHANGE {
                break;
            }
            h *= (0.95 * CHANGE / change).max(0.1);
            event = None;
        }
        // The corrector: mean rates, their throttles, and the event again at the mean rates.
        let mut mean: Vec<f64> = Vec::new();
        let mut u = u0;
        for _ in 0..2 {
            mean.clear();
            mean.extend(
                scratch
                    .r0
                    .iter()
                    .zip(&scratch.r1)
                    .map(|(a, b)| 0.5 * (a + b)),
            );
            u = self.throttles_for(&mean, &amounts);
            self.net_into(&mean, &u, &mut scratch.net);
            let (t, e) = self.first_event(&amounts, &scratch.net, h);
            if e.is_some() {
                (h, event) = (t, e);
            }
            scratch.s1.clear();
            scratch
                .s1
                .extend(amounts.iter().zip(&scratch.net).map(|(&a, &v)| a + v * h));
            self.rates_into(&scratch.s1, &mut scratch.r1);
        }
        // Apply: each flow's amount once, to its stock and its books.
        for (&(p, s), &r) in self.layout.iter().zip(&mean) {
            let moved = u[p] * r * h;
            self.stocks[s].amount += moved;
            if moved >= 0.0 {
                self.inflow[s].add(moved);
            } else {
                self.outflow[s].add(-moved);
            }
        }
        self.throttle = u;
        self.scratch = scratch;
        (h, event.map(|e| at_time(e, self.time + h)))
    }
}
