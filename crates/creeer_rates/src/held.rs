//! Stocks held at a bound, and how the processes held there run over an exact segment.
//!
//! A stock is *held* when it is at a bound and its unthrottled net would push it past (its
//! consumers ask more than flows in, or its producers give more than flows out): the
//! processes pushing past run at the factor φ that keeps it there. When those processes'
//! own rates are constant and nothing else holds them, φ is what flows in over what they
//! ask, `supply(s) / demand`: linear in the stocks, like every other rate. Their flows are
//! then linear in the stocks too, and the exact segments (`exact.rs`) take them as such,
//! until φ reaches 1 (the stock is released).

use crate::network::Network;
use crate::throttle::Side;

/// How a process's throttle behaves over an exact segment.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Gate {
    /// A constant factor.
    Fixed(f64),
    /// Held at `stock`: φ = −Σ (giving flows' rates · their factors) / `demand`, `demand` the
    /// held flows' rates at that stock (signed: what flows in balances what they take).
    Held {
        stock: usize,
        demand: f64,
        /// (process, flow index) of each flow giving to the stock, and its process's factor.
        givers: Vec<(usize, usize, f64)>,
    },
}

impl Network {
    /// The stocks held at a bound, with the flat rates `rates` and throttles `u` at
    /// `amounts`.
    /// Into `out`; `free` and `net` are buffers.
    pub(crate) fn held_into(
        &self,
        (amounts, rates, u): (&[f64], &[f64], &[f64]),
        free: &mut Vec<f64>,
        net: &mut Vec<f64>,
        out: &mut Vec<(usize, Side)>,
    ) {
        out.clear();
        let at = |(k, s): (usize, &crate::Stock)| crate::segment::bound_at(amounts[k], s).is_some();
        if !self.stocks.iter().enumerate().any(at) {
            return;
        }
        free.clear();
        free.resize(self.stocks.len(), 0.0);
        net.clear();
        net.resize(self.stocks.len(), 0.0);
        let mut size = vec![0.0; self.stocks.len()];
        for (&(p, s), &r) in self.layout.iter().zip(rates) {
            free[s] += r;
            net[s] += u[p] * r;
            size[s] += r.abs();
        }
        // Pushed past by more than rounding of what flows through it (a push of 10⁻¹⁷ where
        // two flows just balance holds nothing: its release would come at once, for ever).
        let past = |k: usize| 1e-12 * size[k];
        out.extend((self.stocks.iter().enumerate()).filter_map(|(k, s)| {
            match crate::segment::bound_at(amounts[k], s) {
                Some(Side::Empty) if free[k] < -past(k) && net[k] <= 0.0 => Some((k, Side::Empty)),
                Some(Side::Full) if free[k] > past(k) && net[k] >= 0.0 => Some((k, Side::Full)),
                _ => None,
            }
        }));
    }

    /// The gates of an exact segment from throttles `u`, if one applies.
    ///
    /// A held stock whose flows all move steadily (constant rates, of processes whose own
    /// throttles are steady) keeps its processes' throttles as they start, exactly. A held
    /// stock fed by rates that vary needs its held processes constant and held there alone,
    /// and its givers not held anywhere: then φ is linear in the stocks.
    /// Into `gates` (whether one applies); `steady` and `holds` are buffers.
    pub(crate) fn gates_into(
        &self,
        (held, u): (&[(usize, Side)], &[f64]),
        steady: &mut Vec<bool>,
        holds: &mut Vec<usize>,
        gates: &mut Vec<Gate>,
    ) -> bool {
        gates.clear();
        gates.extend(u.iter().map(|&x| Gate::Fixed(x)));
        if held.is_empty() {
            return true;
        }
        let pushes_past = |side: Side, amount: f64| match side {
            Side::Empty => amount < 0.0,
            Side::Full => amount > 0.0,
        };
        // Steady processes: constant rates, held only at steady stocks (a fixed point).
        let at = |k: usize| {
            (self.layout.iter())
                .filter(move |&&(_, s)| s == k)
                .map(|&(p, _)| p)
        };
        steady.clear();
        steady.extend((self.processes.iter()).map(|p| p.flows.iter().all(|f| f.slope.is_empty())));
        holds.clear();
        holds.resize(self.processes.len(), 0);
        for &(k, side) in held {
            for (p, process) in self.processes.iter().enumerate() {
                let past =
                    (process.flows.iter()).any(|f| f.stock.0 == k && pushes_past(side, f.rate));
                holds[p] += usize::from(past);
            }
        }
        loop {
            let mut changed = false;
            for &(k, _) in held {
                if at(k).all(|p| steady[p]) {
                    continue;
                }
                for p in at(k) {
                    if steady[p] {
                        steady[p] = false;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for &(k, side) in held {
            if at(k).all(|p| steady[p]) {
                continue; // its throttles stay as they start
            }
            let mut demand = 0.0;
            let mut givers = Vec::new();
            let mut holders = Vec::new();
            for (p, process) in self.processes.iter().enumerate() {
                for (i, f) in process.flows.iter().enumerate() {
                    if f.stock.0 != k || f.rate == 0.0 && f.slope.is_empty() {
                        continue;
                    }
                    if pushes_past(side, f.rate) && f.slope.is_empty() {
                        demand += f.rate;
                        holders.push(p);
                    } else if pushes_past(side, f.rate) {
                        return false; // a held process whose rate varies
                    } else {
                        givers.push((p, i, u[p]));
                    }
                }
            }
            for &p in &holders {
                let constant = self.processes[p].flows.iter().all(|f| f.slope.is_empty());
                if !constant || holds[p] > 1 {
                    return false; // varying, or held at another stock too
                }
                gates[p] = Gate::Held {
                    stock: k,
                    demand,
                    givers: givers.clone(),
                };
            }
        }
        // Givers must run at a fixed factor (none held itself).
        let held = |p: usize| matches!(gates[p], Gate::Held { .. });
        gates.iter().all(|g| match g {
            Gate::Held { givers, .. } => givers.iter().all(|&(p, _, _)| !held(p)),
            Gate::Fixed(_) => true,
        })
    }

    /// Gate `g`'s factor with the stocks at `amounts`.
    pub(crate) fn factor(&self, g: &Gate, amounts: &[f64]) -> f64 {
        match g {
            Gate::Fixed(u) => *u,
            Gate::Held { demand, givers, .. } => {
                let supply: f64 = givers
                    .iter()
                    .map(|&(p, i, u)| u * self.processes[p].flows[i].at(amounts))
                    .sum();
                -supply / demand
            }
        }
    }
}
