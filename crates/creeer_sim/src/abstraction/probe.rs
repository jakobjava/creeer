//! A probe: the full machine run from the abstract stocks, its rates measured, the stretch
//! that ended corrected, the next one set (a predictor-corrector across probes).
//!
//! - **Measured:** after settling, two windows whose rates agree; their mean, at the
//!   windows' middle.
//! - **Predicted:** the next stretch's rates change linearly in time, at the slope from the
//!   last probe's rates to these (a long baseline: no noise of one window's to another's).
//! - **Corrected:** when the next probe measures the rates at the stretch's end, they missed
//!   the prediction by `m`; the stretch's amounts are corrected by `½·m·Δ` (the trapezoid of
//!   a miss growing from nothing at its start), scaled whole so no stock passes a bound.
//! - **Step control:** the miss is second order in the stretch's length, so the next length
//!   is `Δ·√(DRIFT/miss)`, growing at most twofold, between [`SHORTEST`] and [`LONGEST`].
//! - **After a bound** (a new regime) neither correction nor slope carries over.

use super::{
    Abstract, DRIFT, LONGEST, LONGEST_PROBE, Machine, SETTLE, SHORTEST, STEADY, Stretch, Trend,
    WINDOW,
};
use creeer_rates::{Event, Flow, Process, ProcessId, StockId};

impl<M: Machine> Abstract<M> {
    /// Runs `seconds` of full ticks (counted as a probe's), noting stocks that reach a
    /// bound; the amounts then.
    fn run(&mut self, seconds: f64) -> Vec<f64> {
        let dt = self.machine.dt();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ticks = (seconds / dt).round() as u64;
        let bounds = self.machine.stocks();
        let mut before = self.machine.amounts();
        for _ in 0..ticks {
            self.machine.tick();
            self.time += dt;
            let now = self.machine.amounts();
            for (k, b) in bounds.iter().enumerate() {
                let stock = StockId(k);
                let time = self.time;
                if before[k] > b.low && now[k] <= b.low {
                    self.events.push(Event::Empty { stock, time });
                } else if before[k] < b.high && now[k] >= b.high {
                    self.events.push(Event::Full { stock, time });
                }
            }
            before = now;
        }
        self.probe_ticks += ticks;
        before
    }

    /// Measures the machine from the abstract stocks, corrects the stretch that ended (if it
    /// ran to its own end: `drifted`), and starts the next stretch.
    pub(super) fn probe(&mut self, drifted: bool) {
        let begun = self.time;
        let events = self.events.len();
        let amounts = self.amounts();
        self.machine.set_amounts(&amounts);
        self.probes += 1;
        self.run(SETTLE);
        let mut a = [self.machine.amounts(), self.run(WINDOW), self.run(WINDOW)];
        let mut waited = SETTLE + 2.0 * WINDOW;
        while !steady(&a) && waited < LONGEST_PROBE {
            a.rotate_left(1);
            a[2] = self.run(WINDOW);
            waited += WINDOW;
        }
        let at = self.time - WINDOW;
        let rates: Vec<f64> = (a[2].iter().zip(&a[0]))
            .map(|(x, y)| (x - y) / (2.0 * WINDOW))
            .collect();
        let mut end = a[2].clone();
        self.scale.resize(rates.len(), 0.0);
        for (s, r) in self.scale.iter_mut().zip(&rates) {
            *s = s.max(r.abs());
        }
        // A stock that reached a bound while it ran is a new regime too.
        let drifted = drifted && self.events.len() == events;
        if !drifted {
            // A new regime (or the first probe): nothing to carry over.
            self.measured = None;
        }
        let middle = a[1].clone();
        let trends = self.trends(&rates, &middle, at);
        let mut length = SHORTEST;
        if let Some(s) = self.stretch.take().filter(|_| drifted) {
            let span = begun - s.began;
            // The prediction's miss where these rates were measured, and at the stretch's end.
            let predicted = |k: usize, t: f64, x: f64| s.rates[k] + s.trends[k].change(t - s.at, x);
            let miss = (0..rates.len())
                .map(|k| {
                    let scale = rates[k].abs().max(0.05 * self.scale[k]).max(1e-300);
                    (rates[k] - predicted(k, at, middle[k])).abs() / scale
                })
                .fold(0.0, f64::max);
            length = (span * (DRIFT / miss.max(1e-12)).sqrt())
                .min(2.0 * span)
                .clamp(SHORTEST, LONGEST);
            let fix: Vec<f64> = (0..rates.len())
                .map(|k| {
                    let back =
                        trends[k].change(begun - at, amounts[k]) - trends[k].change(0.0, middle[k]);
                    0.5 * (rates[k] + back - predicted(k, begun, amounts[k])) * span
                })
                .collect();
            self.correct(&mut end, &fix);
        }
        self.measured = Some((at, rates.clone(), middle));
        self.start(&end, &rates, &trends, at, length);
        self.stretch = Some(Stretch {
            rates,
            trends,
            at,
            began: self.time,
        });
    }

    /// Adds `fix` to `end`, scaled whole (keeping what it conserves) so that no stock passes
    /// a bound, and writes it into the machine.
    fn correct(&mut self, end: &mut [f64], fix: &[f64]) {
        let bounds = self.machine.stocks();
        // A stock at a bound stays there: its correction past it is its rate's rounding (a
        // dry tank's 10⁻¹³ m³/s), not water, and must not stop every other's.
        let at = |x: f64, b: &super::StockDef| {
            let near = |v: f64| (x - v).abs() <= 1e-9 * v.abs().max(1.0);
            near(b.low) || near(b.high)
        };
        let fix: Vec<f64> = (fix.iter().zip(end.iter()).zip(&bounds))
            .map(|((&c, &x), b)| {
                if at(x, b) && (x + c < b.low || x + c > b.high) {
                    0.0
                } else {
                    c
                }
            })
            .collect();
        let lambda = (fix.iter().zip(end.iter()).zip(&bounds))
            .map(|((&c, &x), b)| {
                if x + c < b.low && c < 0.0 {
                    ((b.low - x) / c).max(0.0)
                } else if x + c > b.high && c > 0.0 {
                    ((b.high - x) / c).max(0.0)
                } else {
                    1.0
                }
            })
            .fold(1.0, f64::min);
        for (x, c) in end.iter_mut().zip(&fix) {
            *x += lambda * c;
        }
        self.machine.set_amounts(end);
    }

    /// The next stretch: from the stocks `end`, the rates `rates` at `at` changing by
    /// `slope`, trusted for `length` seconds.
    fn start(&mut self, end: &[f64], rates: &[f64], trends: &[Trend], at: f64, length: f64) {
        let mut flows: Vec<Flow> = (0..rates.len())
            .filter(|&k| rates[k] != 0.0 || trends[k] != Trend::Time(0.0))
            .map(|k| Flow {
                stock: self.ids[k],
                rate: rates[k],
                slope: match trends[k] {
                    Trend::Time(0.0) => Vec::new(),
                    Trend::Time(s) => vec![(self.clock, s, at)],
                    Trend::Own { beta, at: x0 } => vec![(self.ids[k], beta, x0)],
                },
            })
            .collect();
        flows.push(Flow::constant(self.clock, 1.0));
        for (k, &s) in self.ids.iter().enumerate() {
            self.network.set_amount(s, end[k]);
        }
        self.network.set_amount(self.clock, self.time);
        self.network.set_time(self.time);
        let process = Process {
            name: "machine".into(),
            flows,
            trust: vec![(self.clock, self.time, self.time + length)],
        };
        if self.network.processes().is_empty() {
            self.network.add_process(process);
        } else {
            self.network.set_process(ProcessId(0), process);
        }
    }
}

/// Whether the rates over two windows (amounts `a` at their ends) agree: within [`STEADY`]
/// of each other, or moving less than 10⁻⁶ of the stock over a window.
fn steady(a: &[Vec<f64>; 3]) -> bool {
    (0..a[0].len()).all(|k| {
        let (d1, d2) = (a[1][k] - a[0][k], a[2][k] - a[1][k]);
        let bend = (d1 - d2).abs();
        bend <= STEADY * d1.abs().max(d2.abs()) || bend <= 1e-6 * a[2][k].abs().max(1.0)
    })
}

impl Trend {
    /// How much a rate following it has changed `dt` seconds on, its stock at `x`.
    pub(super) fn change(self, dt: f64, x: f64) -> f64 {
        match self {
            Self::Time(s) => s * dt,
            Self::Own { beta, at } => beta * (x - at),
        }
    }
}

impl<M: Machine> Abstract<M> {
    /// Each stock's trend from the last probe's rates to `rates` (measured at `at`, the
    /// stocks then `middle`): a stock that relaxes follows its own amount when its rate falls
    /// as it rises (Newton cooling: exponential, exact), any other follows time.
    fn trends(&self, rates: &[f64], middle: &[f64], at: f64) -> Vec<Trend> {
        let relaxes: Vec<bool> = self.machine.stocks().iter().map(|d| d.relaxes).collect();
        let Some((t0, r0, x0)) = self.measured.as_ref().filter(|m| at > m.0) else {
            return vec![Trend::Time(0.0); rates.len()];
        };
        (0..rates.len())
            .map(|k| {
                let dx = middle[k] - x0[k];
                let beta = (rates[k] - r0[k]) / dx;
                if relaxes[k] && dx.abs() > 1e-12 * middle[k].abs().max(1.0) && beta < 0.0 {
                    Trend::Own {
                        beta,
                        at: middle[k],
                    }
                } else {
                    Trend::Time((rates[k] - r0[k]) / (at - t0))
                }
            })
            .collect()
    }
}
