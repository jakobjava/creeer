//! Reactors: a lumped mixture its reactions run in, stepped implicitly (BDF2, backward
//! Euler for its first two steps) by Newton's method on every species' moles and its
//! temperature together: kinetics as stiff as combustion's settle without oscillating, and
//! a step longer than a reaction's time reaches its equilibrium instead of passing it.
//!
//! - Batch at constant volume (energy balance on U = H − n_gas·R·T) or constant pressure
//!   (on H; its volume the ideal gas's);
//! - a CSTR: a feed in (mol/s at its temperature) and the mixture out at n/τ;
//! - heat through a conductance to surroundings and a power in.
//!
//! Every joule is booked in its ledger: stored its energy (H or U, from the elements at
//! 298.15 K), imported the heat and the feed's enthalpy, exported the outflow's.

use crate::dense;
use crate::kinetics::Reaction;
use crate::system::System;
use creeer_materials::species::R;
use creeer_network::{Ledger, Sum};

/// What a reactor holds fixed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Holding {
    /// Its volume (m³).
    Volume(f64),
    /// Its pressure (Pa).
    Pressure(f64),
}

/// A reactor.
#[derive(Clone, Debug)]
pub struct Reactor {
    pub moles: Vec<f64>,
    pub t: f64,
    pub holding: Holding,
    pub reactions: Vec<Reaction>,
    /// Held at its temperature (heat flows as needed), or adiabatic but for `ua`/`power`.
    pub isothermal: bool,
    /// Conductance to surroundings (W/K) at `surroundings` (K), and a power in (W).
    pub ua: f64,
    pub surroundings: f64,
    pub power: f64,
    /// A feed (mol/s of each species) at a temperature, and the residence time (s) the
    /// outflow n/τ follows: a CSTR.
    pub feed: Option<(Vec<f64>, f64)>,
    pub residence: Option<f64>,
    ledger: Ledger,
    /// The step before (BDF2's history) and steps since a restart.
    previous: Option<(Vec<f64>, f64)>,
    steps: u32,
}

impl Reactor {
    /// A reactor of `moles` at `t` (K), holding `holding`, with `reactions`, adiabatic.
    pub fn new(
        sys: &System,
        moles: Vec<f64>,
        t: f64,
        holding: Holding,
        reactions: Vec<Reaction>,
    ) -> Self {
        let mut reactor = Self {
            moles,
            t,
            holding,
            reactions,
            isothermal: false,
            ua: 0.0,
            surroundings: t,
            power: 0.0,
            feed: None,
            residence: None,
            ledger: Ledger::new(0.0),
            previous: None,
            steps: 0,
        };
        reactor.ledger = Ledger::new(reactor.energy(sys, &reactor.moles, reactor.t));
        reactor
    }

    /// Its energy (J): H at constant pressure, U = H − n_gas·R·T at constant volume.
    pub fn energy(&self, sys: &System, moles: &[f64], t: f64) -> f64 {
        let h = sys.enthalpy(moles, t);
        match self.holding {
            Holding::Pressure(_) => h,
            Holding::Volume(_) => h - moles[..sys.gases].iter().sum::<f64>() * R * t,
        }
    }

    /// Its volume (m³) with `moles` at `t`.
    pub fn volume(&self, sys: &System, moles: &[f64], t: f64) -> f64 {
        match self.holding {
            Holding::Volume(v) => v,
            Holding::Pressure(p) => (moles[..sys.gases].iter().sum::<f64>() * R * t / p).max(1e-30),
        }
    }

    /// Its pressure (Pa).
    pub fn pressure(&self, sys: &System) -> f64 {
        match self.holding {
            Holding::Pressure(p) => p,
            Holding::Volume(v) => self.moles[..sys.gases].iter().sum::<f64>() * R * self.t / v,
        }
    }

    /// Where its energy went.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// The step begins anew (a feed or a holding changed by a jump): backward Euler next.
    pub fn restart(&mut self) {
        self.steps = 0;
    }

    /// The rate of change of each species' moles (mol/s) at `moles`, `t`, and its
    /// derivatives with respect to the moles (row-major, into `jac`).
    fn change(&self, sys: &System, moles: &[f64], t: f64, rate: &mut [f64], jac: &mut [f64]) {
        let n = moles.len();
        let volume = self.volume(sys, moles, t);
        rate.fill(0.0);
        jac.fill(0.0);
        let mut d = vec![0.0; n];
        for reaction in &self.reactions {
            let r = reaction.rate(sys, moles, volume, t, &mut d) * volume;
            for (list, sign) in [(&reaction.reactants, -1.0), (&reaction.products, 1.0)] {
                for &(i, nu) in list {
                    rate[i] += sign * nu * r;
                    for j in 0..n {
                        jac[i * n + j] += sign * nu * d[j] * volume;
                    }
                }
            }
        }
        if let Some((feed, _)) = &self.feed {
            for (r, f) in rate.iter_mut().zip(feed) {
                *r += f;
            }
        }
        if let Some(tau) = self.residence {
            for i in 0..n {
                rate[i] -= moles[i] / tau;
                jac[i * n + i] -= 1.0 / tau;
            }
        }
    }

    /// The energy flowing in at `moles`, `t` (W): heat, power, the feed's enthalpy less
    /// the outflow's. Split into what comes in and what goes out.
    fn energy_flows(&self, sys: &System, moles: &[f64], t: f64) -> (f64, f64) {
        let heat = self.ua * (self.surroundings - t) + self.power;
        let (mut gained, mut lost) = if heat >= 0.0 {
            (heat, 0.0)
        } else {
            (0.0, -heat)
        };
        if let Some((feed, t_in)) = &self.feed {
            gained += sys.enthalpy(feed, *t_in);
        }
        if let Some(tau) = self.residence {
            let out: Vec<f64> = moles.iter().map(|m| m / tau).collect();
            lost += sys.enthalpy(&out, t);
        }
        (gained, lost)
    }
}

/// How a step went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReactorReport {
    pub iterations: usize,
    pub converged: bool,
}

impl Reactor {
    /// Advances it by `h` seconds.
    pub fn step(&mut self, sys: &System, h: f64) -> ReactorReport {
        let n = self.moles.len();
        let bdf2 = self.steps >= 2 && self.previous.is_some();
        // y − (4/3)·y₀ + (1/3)·y₋₁ = (2/3)·h·f(y), or y − y₀ = h·f(y).
        let (beta, base_n, base_e): (f64, Vec<f64>, f64) = match (&self.previous, bdf2) {
            (Some((prev, prev_t)), true) => (
                2.0 * h / 3.0,
                self.moles
                    .iter()
                    .zip(prev)
                    .map(|(a, b)| (4.0 * a - b) / 3.0)
                    .collect(),
                (4.0 * self.energy(sys, &self.moles, self.t) - self.energy(sys, prev, *prev_t))
                    / 3.0,
            ),
            _ => (h, self.moles.clone(), self.energy(sys, &self.moles, self.t)),
        };
        let (mut y, mut t) = (self.moles.clone(), self.t);
        let (mut rate, mut jac) = (vec![0.0; n], vec![0.0; n * n]);
        let mut report = ReactorReport::default();
        let m = n + 1;
        for _ in 0..40 {
            report.iterations += 1;
            self.change(sys, &y, t, &mut rate, &mut jac);
            let mut a = vec![0.0; m * m];
            let mut b = vec![0.0; m];
            for i in 0..n {
                b[i] = -(y[i] - base_n[i] - beta * rate[i]);
                for j in 0..n {
                    a[i * m + j] = f64::from(u8::from(i == j)) - beta * jac[i * n + j];
                }
            }
            // ∂rate/∂T by a difference (Arrhenius and K_c both move with T).
            let dt = 1e-4 * t;
            let mut hot = vec![0.0; n];
            self.change(sys, &y, t + dt, &mut hot, &mut vec![0.0; n * n]);
            for i in 0..n {
                a[i * m + n] = -beta * (hot[i] - rate[i]) / dt;
            }
            if self.isothermal {
                a[n * m + n] = 1.0;
            } else {
                let (gin, gout) = self.energy_flows(sys, &y, t);
                let e = self.energy(sys, &y, t);
                b[n] = -(e - base_e - beta * (gin - gout));
                for j in 0..n {
                    let mut bumped = y.clone();
                    let step = 1e-8 * y.iter().sum::<f64>().max(1e-12);
                    bumped[j] += step;
                    let (bi, bo) = self.energy_flows(sys, &bumped, t);
                    a[n * m + j] =
                        (self.energy(sys, &bumped, t) - e - beta * ((bi - bo) - (gin - gout)))
                            / step;
                }
                let (hi, ho) = self.energy_flows(sys, &y, t + dt);
                a[n * m + n] =
                    (self.energy(sys, &y, t + dt) - e - beta * ((hi - ho) - (gin - gout))) / dt;
            }
            if !dense::solve(&mut a, &mut b, m) {
                break;
            }
            // Damped to keep every amount at least nought and the temperature positive.
            let mut lambda = 1.0f64;
            for i in 0..n {
                if y[i] + b[i] < 0.0 && b[i] < 0.0 {
                    lambda = lambda.min(0.99 * y[i] / -b[i]);
                }
            }
            if t + b[n] < 0.5 * t {
                lambda = lambda.min(0.5 * t / -b[n]);
            }
            let total: f64 = y.iter().sum::<f64>().max(1e-300);
            let mut moved = 0.0f64;
            for i in 0..n {
                y[i] = (y[i] + lambda * b[i]).max(0.0);
                moved = moved.max(b[i].abs() / total);
            }
            t += lambda * b[n];
            let full = lambda >= 1.0;
            if full && moved < 1e-12 && b[n].abs() < 1e-9 * t {
                report.converged = true;
                break;
            }
        }
        // The books. Reactions keep the energy (it counts heats of formation), so its change
        // is what flowed: the flows over the step, h·(in − out), and what the rule made of them
        // beyond that (BDF2's history; or, held isothermal, the heat that held it), on the side
        // it went.
        let before = self.energy(sys, &self.moles, self.t);
        let after = self.energy(sys, &y, t);
        let (gin, gout) = self.energy_flows(sys, &y, t);
        let (mut imported, mut exported) = (Sum::default(), Sum::default());
        imported.add(h * gin);
        exported.add(h * gout);
        let rest = after - before - h * (gin - gout);
        if rest >= 0.0 {
            imported.add(rest)
        } else {
            exported.add(-rest)
        }
        self.ledger.import(imported.value());
        self.ledger.export(exported.value());
        self.previous = Some((std::mem::replace(&mut self.moles, y), self.t));
        self.t = t;
        self.steps += 1;
        report
    }

    /// The energy held now (J), for the ledger's balance.
    pub fn stored(&self, sys: &System) -> f64 {
        self.energy(sys, &self.moles, self.t)
    }
}
