//! Exact segments: with the throttles fixed (or held, linear in the stocks: `held.rs`),
//! rates linear in the stocks make the stocks a linear system, ds/dt = A·s + c, solved
//! exactly by the matrix exponential. Only the stocks some rate depends on (the *states*)
//! need it; with their time integrals alongside (dI/dt = s), every flow's amount over the
//! segment is exact too: factor·(rate·h + Σ slope·(∫s dt − at·h)). A segment ends only at an
//! event or where a held stock is released, not by the rates' change: a tank draining
//! through its level's outflow over many time constants is one segment.
//!
//! Events of stocks whose flows are all steady are found in closed form (they move in
//! straight lines); the others by sampling the exact path (a path of m states turns at most
//! m − 1 times) and regula falsi on the first crossing.

use crate::expm::{Dense, Path};
use crate::held::Gate;
use crate::network::{Event, Network};
use crate::throttle::Side;
use crate::{ProcessId, StockId};

/// Iterations of regula falsi at most.
const FALSI: usize = 60;

/// What ends a segment, with what measures how far past it a state is (positive: past).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Ending {
    Empty(usize),
    Full(usize),
    /// Process, stock, and its box's edges.
    Stale(usize, usize, f64, f64),
    /// A held stock released (φ of gate `.0` reached 1): a new regime, no event.
    Release(usize),
}

/// Each flow's throttled rate as factor·(other flows' rates): (factor, process, flow).
pub(crate) struct Terms {
    flat: Vec<(f64, usize, usize)>,
    /// Where each flow's terms start in `flat` (and one past the last).
    starts: Vec<usize>,
}

impl Terms {
    /// Each flow's terms, in `layout`'s order.
    pub(crate) fn each(&self) -> impl Iterator<Item = &[(f64, usize, usize)]> {
        self.starts.windows(2).map(|w| &self.flat[w[0]..w[1]])
    }
}

pub(crate) struct Linear {
    pub(crate) path: Path,
    pub(crate) terms: Terms,
}

impl Network {
    /// The stocks some rate depends on, sorted (kept as `states`, made again whenever the
    /// processes change).
    pub(crate) fn find_states(&self) -> Vec<usize> {
        let mut states: Vec<usize> = (self.processes.iter())
            .flat_map(|p| p.flows.iter())
            .flat_map(|f| f.slope.iter().map(|&(s, _, _)| s.0))
            .collect();
        states.sort_unstable();
        states.dedup();
        states
    }

    fn terms(&self, gates: &[Gate]) -> Terms {
        let mut flat = Vec::with_capacity(self.layout.len());
        let mut starts = Vec::with_capacity(self.layout.len() + 1);
        for (p, process) in self.processes.iter().enumerate() {
            for (i, f) in process.flows.iter().enumerate() {
                starts.push(flat.len());
                match &gates[p] {
                    Gate::Fixed(u) => flat.push((*u, p, i)),
                    Gate::Held { demand, givers, .. } => {
                        flat.extend((givers.iter()).map(|&(g, j, u)| (-u * f.rate / demand, g, j)))
                    }
                }
            }
        }
        starts.push(flat.len());
        Terms { flat, starts }
    }

    fn linear(&self, gates: &[Gate]) -> Linear {
        let states = &self.states;
        let terms = self.terms(gates);
        let k = states.len();
        let index = |s: usize| states.binary_search(&s).ok();
        let mut m = Dense::zeros(2 * k + 1);
        for (&(_, stock), ts) in self.layout.iter().zip(terms.each()) {
            let Some(i) = index(stock) else { continue };
            for &(factor, p, j) in ts {
                let term = &self.processes[p].flows[j];
                let mut constant = term.rate;
                for &(s, d, at) in &term.slope {
                    *m.at(i, index(s.0).expect("a state")) += factor * d;
                    constant -= d * at;
                }
                *m.at(i, k) += factor * constant;
            }
        }
        for i in 0..k {
            *m.at(k + 1 + i, i) = 1.0;
        }
        Linear {
            path: Path::of(m, k),
            terms,
        }
    }

    /// Every stock's amount after `t` along the path ([s, 1, ∫s] then `z`) into `amounts`,
    /// each flow's amount into `moved`.
    pub(crate) fn along(
        &self,
        lin: &Linear,
        z: &[f64],
        t: f64,
        amounts: &mut [f64],
        moved: &mut Vec<f64>,
    ) {
        let k = self.states.len();
        let integral = |s: usize| z[k + 1 + self.states.binary_search(&s).expect("a state")];
        moved.clear();
        for (a, s) in amounts.iter_mut().zip(&self.stocks) {
            *a = s.amount;
        }
        for (&(_, stock), ts) in self.layout.iter().zip(lin.terms.each()) {
            let mut amount = 0.0;
            for &(factor, p, j) in ts {
                let term = &self.processes[p].flows[j];
                let mut x = term.rate * t;
                for &(s, d, at) in &term.slope {
                    x += d * (integral(s.0) - at * t);
                }
                amount += factor * x;
            }
            amounts[stock] += amount;
            moved.push(amount);
        }
    }

    /// How far past `ending` the stocks at `amounts` are (positive: past it).
    pub(crate) fn beyond(&self, ending: Ending, amounts: &[f64], gates: &[Gate]) -> f64 {
        match ending {
            Ending::Empty(k) => self.stocks[k].low - amounts[k],
            Ending::Full(k) => amounts[k] - self.stocks[k].high,
            Ending::Stale(_, s, lo, hi) => (lo - amounts[s]).max(amounts[s] - hi),
            Ending::Release(g) => self.factor(&gates[g], amounts) - 1.0,
        }
    }

    /// The endings to watch on the sampled path: the varying stocks' bounds (but the one
    /// each is held at from the start), boxes over them, held stocks' releases.
    fn watched(&self, start: &[f64], varying: &[bool], gates: &[Gate]) -> Vec<Ending> {
        let mut watch = Vec::new();
        for (k, s) in self.stocks.iter().enumerate().filter(|(k, _)| varying[*k]) {
            let at = crate::segment::bound_at(start[k], s);
            if s.low.is_finite() && at != Some(Side::Empty) {
                watch.push(Ending::Empty(k));
            }
            if s.high.is_finite() && at != Some(Side::Full) {
                watch.push(Ending::Full(k));
            }
        }
        for (p, process) in self.processes.iter().enumerate() {
            for &(s, lo, hi) in process.trust.iter().filter(|t| varying[t.0.0]) {
                watch.push(Ending::Stale(p, s.0, lo, hi));
            }
        }
        for (g, gate) in gates.iter().enumerate() {
            if matches!(gate, Gate::Held { .. }) {
                watch.push(Ending::Release(g));
            }
        }
        watch
    }

    /// The first ending of the steady stocks (straight lines from `start` at `net`).
    fn steady_ending(
        &self,
        start: &[f64],
        net: &[Option<f64>],
        most: f64,
    ) -> (f64, Option<Ending>) {
        let mut first = (most, None);
        let mut consider = |t: f64, e: Ending| {
            if t.is_finite() && t >= 0.0 && t < first.0 {
                first = (t, Some(e));
            }
        };
        for (k, s) in self.stocks.iter().enumerate() {
            let Some(v) = net[k] else { continue };
            let at = crate::segment::bound_at(start[k], s);
            if v < 0.0 && at != Some(Side::Empty) {
                consider((s.low - start[k]) / v, Ending::Empty(k));
            } else if v > 0.0 && at != Some(Side::Full) {
                consider((s.high - start[k]) / v, Ending::Full(k));
            }
        }
        for (p, process) in self.processes.iter().enumerate() {
            for &(s, lo, hi) in &process.trust {
                let Some(v) = net[s.0] else { continue };
                let t = if v < 0.0 {
                    (lo - start[s.0]) / v
                } else {
                    (hi - start[s.0]) / v
                };
                consider(t, Ending::Stale(p, s.0, lo, hi));
            }
        }
        first
    }

    /// An exact segment of at most `most` seconds under `gates`: applied; its length and
    /// the event it ends at.
    pub(crate) fn exact_segment(&mut self, most: f64, gates: &[Gate]) -> (f64, Option<Event>) {
        let lin = self.linear(gates);
        let k = self.states.len();
        let mut w = std::mem::take(&mut self.work);
        w.start.clear();
        w.start.extend(self.stocks.iter().map(|s| s.amount));
        let start = &w.start;
        // Steady stocks: every term into them of a flow with a constant rate.
        let net = &mut w.net;
        net.clear();
        net.resize(self.stocks.len(), Some(0.0));
        for (&(_, stock), ts) in self.layout.iter().zip(lin.terms.each()) {
            for &(factor, p, j) in ts {
                let term = &self.processes[p].flows[j];
                net[stock] = match net[stock] {
                    Some(v) if term.slope.is_empty() => Some(v + factor * term.rate),
                    _ => None,
                };
            }
        }
        w.varying.clear();
        w.varying.extend(net.iter().map(Option::is_none));
        let (mut h, mut ending) = self.steady_ending(start, net, most);
        let z0 = &mut w.z0;
        z0.clear();
        z0.resize(2 * k + 1, 0.0);
        for (i, &s) in self.states.iter().enumerate() {
            z0[i] = start[s];
        }
        z0[k] = 1.0;
        let (amounts, moved) = (&mut w.amounts, &mut w.moved);
        amounts.clear();
        amounts.extend_from_slice(start);
        let watch = self.watched(start, &w.varying, gates);
        let z = &mut w.z;
        z.clear();
        z.extend_from_slice(z0);
        // Whether the sampling reached the path's end (then `z` is it).
        let mut reached = false;
        if !watch.is_empty() && matches!(lin.path, Path::Scalar { .. }) {
            if let Some((e, t)) = self.scalar_ending((&lin, z0, start), h, &watch, gates) {
                (h, ending) = (t, Some(e));
            }
        } else if !watch.is_empty() {
            // Sample the path up to the steady ending for the first varying one.
            let samples = 4 * (k + 1);
            let dt = h / samples as f64;
            let step = match &lin.path {
                Path::Matrix(m) => Some(m.exp(dt)),
                Path::Scalar { .. } => None,
            };
            let mut found = None;
            'sampling: for i in 1..=samples {
                let t = dt * i as f64;
                match &step {
                    Some(step) => *z = step.apply(z),
                    None => lin.path.write(t, z0, z),
                }
                self.along(&lin, z, t, amounts, moved);
                for &e in &watch {
                    if self.beyond(e, amounts, gates) >= 0.0 {
                        found = Some((e, t - dt, t));
                        break 'sampling;
                    }
                }
            }
            if let Some((e, lo, hi)) = found {
                h = self.falsi(&lin, z0, e, gates, lo, hi);
                ending = Some(e);
            } else {
                reached = true;
            }
        }
        // (With no states the path is [1]: nothing to exponentiate.)
        if !reached {
            if k == 0 {
                z.clone_from(z0);
            } else {
                lin.path.write(h, z0, z);
            }
        }
        self.along(&lin, z, h, amounts, moved);
        for (&(_, s), &amount) in self.layout.iter().zip(moved.iter()) {
            self.stocks[s].amount += amount;
            if amount >= 0.0 {
                self.inflow[s].add(amount);
            } else {
                self.outflow[s].add(-amount);
            }
        }
        let throttle: Vec<f64> = (gates.iter())
            .map(|g| self.factor(g, amounts).min(1.0))
            .collect();
        self.throttle.clear();
        self.throttle.extend(throttle);
        self.work = w;
        let event = ending.and_then(|e| match e {
            Ending::Empty(k) => Some(Event::Empty {
                stock: StockId(k),
                time: 0.0,
            }),
            Ending::Full(k) => Some(Event::Full {
                stock: StockId(k),
                time: 0.0,
            }),
            Ending::Stale(p, ..) => Some(Event::Stale {
                process: ProcessId(p),
                time: 0.0,
            }),
            Ending::Release(_) => None,
        });
        (h, event)
    }

    /// The time in [lo, hi] at which `ending` is reached (the Illinois variant of regula
    /// falsi), just past it to rounding (a released stock is then no longer held).
    pub(crate) fn falsi(
        &self,
        lin: &Linear,
        z0: &[f64],
        e: Ending,
        gates: &[Gate],
        lo: f64,
        hi: f64,
    ) -> f64 {
        let (mut amounts, mut moved) = (vec![0.0; self.stocks.len()], Vec::new());
        let mut at = |t: f64| {
            let z = lin.path.at(t, z0);
            self.along(lin, &z, t, &mut amounts, &mut moved);
            self.beyond(e, &amounts, gates)
        };
        let (mut a, mut b) = (lo, hi);
        let (mut fa, mut fb) = (at(a), at(b));
        let mut side = 0;
        for _ in 0..FALSI {
            if b - a <= 1e-13 * b.max(1.0) {
                break;
            }
            let c = (b - fb * (b - a) / (fb - fa)).clamp(a, b);
            let c = if c <= a || c >= b { 0.5 * (a + b) } else { c };
            let fc = at(c);
            if fc >= 0.0 {
                (b, fb) = (c, fc);
                if side == 1 {
                    fa *= 0.5;
                }
                side = 1;
            } else {
                (a, fa) = (c, fc);
                if side == -1 {
                    fb *= 0.5;
                }
                side = -1;
            }
        }
        b
    }
}

/// Buffers an exact segment reuses.
#[derive(Clone, Debug, Default)]
pub(crate) struct Work {
    start: Vec<f64>,
    amounts: Vec<f64>,
    moved: Vec<f64>,
    z0: Vec<f64>,
    z: Vec<f64>,
    net: Vec<Option<f64>>,
    varying: Vec<bool>,
}
