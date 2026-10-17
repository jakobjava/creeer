//! A grid's step: its voltages (constant-power loads iterated as current injections on the
//! kept factorisation), every branch's power, its motors' rotors moved by their torque
//! (backward Euler, the torque at the step's bus voltage), and the energy booked.
//!
//! Over a step the power is the phasors' average power (the step being whole cycles long);
//! by Tellegen's theorem the branches' complex powers sum to zero at the solution, so what
//! sources give is what lines, transformers and motors lose, loads take and shafts carry
//! away, to the iteration's tolerance.

use crate::grid::{Branch, Grid, NEUTRAL, ROOT3};
use creeer_math::sparse::Complex;
use creeer_network::Sum;

/// Constant-power loads iterate until no bus voltage moves by more than this (relative).
const TOLERANCE: f64 = 1e-12;
const MAX_ITERATIONS: usize = 100;

/// What a grid step did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridReport {
    /// Iterations of the constant-power loads, and whether they converged.
    pub iterations: usize,
    pub converged: bool,
}

impl Grid {
    /// Solves the voltages for the present impedances, loads and motor speeds, without
    /// stepping time (a frequency sweep, a look at a grid as it stands).
    pub fn solve(&mut self) -> GridReport {
        self.factorise();
        let n = self.buses - 1;
        let size = self.solver.as_ref().expect("factorised").size;
        let at = |b: usize| (b != NEUTRAL).then(|| b - 1);
        let mut x: Vec<Complex> = (0..size)
            .map(|k| {
                if k < n {
                    self.voltage[k + 1]
                } else {
                    Complex::default()
                }
            })
            .collect();
        let mut report = GridReport::default();
        for _ in 0..MAX_ITERATIONS {
            report.iterations += 1;
            let mut rhs = vec![Complex::default(); size];
            let mut extra = n;
            for b in &self.branches {
                match *b {
                    Branch::Source { a, emf, z } if z.abs() > 0.0 => {
                        if let Some(a) = at(a) {
                            rhs[a] += emf / Complex::from(ROOT3) / z;
                        }
                    }
                    Branch::Source { emf, .. } => {
                        rhs[extra] = emf / Complex::from(ROOT3);
                        extra += 1;
                    }
                    Branch::Load { a, power } => {
                        if let Some(a) = at(a) {
                            // Drawn: conj(S/V) per phase, at the iterate's voltage.
                            let v = x[a];
                            if v.abs() > 0.0 {
                                rhs[a] -= (power / Complex::from(3.0) / v).conj();
                            }
                        }
                    }
                    _ => {}
                }
            }
            let lu = self
                .solver
                .as_mut()
                .expect("factorised")
                .lu
                .as_mut()
                .expect("factorised");
            lu.solve(&mut rhs);
            let scale = rhs.iter().take(n).map(|v| v.abs()).fold(1.0, f64::max);
            let moved = rhs
                .iter()
                .take(n)
                .zip(&x)
                .map(|(a, b)| (*a - *b).abs())
                .fold(0.0, f64::max);
            x = rhs;
            let loads = self
                .branches
                .iter()
                .any(|b| matches!(b, Branch::Load { .. }));
            if !loads || moved <= TOLERANCE * scale {
                report.converged = true;
                break;
            }
        }
        self.voltage[1..=n].copy_from_slice(&x[..n]);
        self.powers(&x);
        report
    }

    /// Each branch's complex power at the solution `x` (three-phase).
    fn powers(&mut self, x: &[Complex]) {
        let n = self.buses - 1;
        let v = |b: usize| {
            if b == NEUTRAL {
                Complex::default()
            } else {
                x[b - 1]
            }
        };
        let three = Complex::from(3.0);
        let mut extra = n;
        for (k, b) in self.branches.iter().enumerate() {
            self.power[k] = match *b {
                Branch::Line { a, b, z } => {
                    let d = v(a) - v(b);
                    three * d * (d / z).conj()
                }
                Branch::Shunt { a, y } => three * v(a) * (y * v(a)).conj(),
                Branch::Source { a, emf, z } if z.abs() > 0.0 => {
                    // Into the bus from the EMF: (E − V)/z; the branch takes −V·conj(I).
                    let i = (emf / Complex::from(ROOT3) - v(a)) / z;
                    -(three * v(a) * i.conj())
                }
                Branch::Source { a, .. } => {
                    let i = x[extra];
                    extra += 1;
                    three * v(a) * i.conj()
                }
                Branch::Transformer { a, b, ratio, z, y } => {
                    let series = (v(a) - v(b) * Complex::from(ratio)) / z;
                    let ia = series + y * v(a);
                    let ib = series * Complex::from(-ratio);
                    three * (v(a) * ia.conj() + v(b) * ib.conj())
                }
                Branch::Load { power, .. } => power,
                Branch::Motor {
                    a, model, speed, ..
                } => {
                    let z = model.impedance(model.slip(speed, self.frequency));
                    three * v(a) * (v(a) / z).conj()
                }
            };
        }
    }

    /// Advances the grid by `h` seconds: solves it, moves its motors, books its energy.
    pub fn step(&mut self, h: f64) -> GridReport {
        let report = self.solve();
        let (mut supplied, mut dissipated, mut exported, mut imported) = (
            Sum::default(),
            Sum::default(),
            Sum::default(),
            Sum::default(),
        );
        let frequency = self.frequency;
        for (k, b) in self.branches.iter_mut().enumerate() {
            let p = self.power[k].re * h;
            match b {
                Branch::Source { a, emf, z } => {
                    // The EMF gives E·conj(I); its impedance loses |I|²·R.
                    let va = self.voltage[*a];
                    let given = if z.abs() > 0.0 {
                        let i = (*emf / Complex::from(ROOT3) - va) / *z;
                        let e = (*emf / Complex::from(ROOT3)) * i.conj();
                        dissipated.add(3.0 * i.norm_squared() * z.re * h);
                        3.0 * e.re * h
                    } else {
                        -p
                    };
                    supplied.add(given);
                }
                Branch::Load { .. } => exported.add(p),
                Branch::Motor {
                    a,
                    model,
                    speed,
                    load,
                } => {
                    let before = 0.5 * model.inertia * *speed * *speed;
                    let v = self.voltage[*a];
                    // J·(ω − ω₀)/h = T(ω) − load − b·ω, by Newton on ω (the bus voltage held).
                    let w0 = *speed;
                    let mut w = w0;
                    for _ in 0..30 {
                        let ws = model.synchronous(frequency);
                        let f = |w: f64| {
                            model.inertia * (w - w0) / h - model.torque(v, (ws - w) / ws, frequency)
                                + *load
                                + model.friction * w
                        };
                        let d = 1e-6 * ws;
                        let slope = (f(w + d) - f(w - d)) / (2.0 * d);
                        let step = f(w) / slope;
                        w -= step;
                        if step.abs() < 1e-12 * ws {
                            break;
                        }
                    }
                    *speed = w;
                    let work = *load * w * h;
                    if work >= 0.0 {
                        exported.add(work);
                    } else {
                        imported.add(-work);
                    }
                    let held = 0.5 * model.inertia * w * w - before;
                    dissipated.add(p - held - work);
                }
                _ => dissipated.add(p),
            }
        }
        // GMIN's leakage from every bus.
        for v in &self.voltage[1..] {
            dissipated.add(3.0 * crate::mna::GMIN * v.norm_squared() * h);
        }
        self.ledger.supply(supplied.value());
        self.ledger.dissipate(dissipated.value());
        self.ledger.export(exported.value());
        self.ledger.import(imported.value());

        report
    }
}
