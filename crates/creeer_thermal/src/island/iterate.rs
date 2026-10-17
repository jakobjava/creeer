//! A Newton iteration's two halves: the residual and Jacobian, and the factorisation.

use super::{Context, IslandSolver};
use crate::kernel::{FIXED, TO_FIXED, each, each_most, relative};
use creeer_math::sparse::Cholesky;

/// Newton stops when no node's residual, over its heat capacity, exceeds this (K): the
/// temperature error it bounds (the capacities' inverse times the Jacobian has eigenvalues of
/// at least one, so the residual so scaled is never smaller than the error it leaves).
pub const TOLERANCE: f64 = 1e-7;
/// A factorisation is kept while every entry of the matrix is within this fraction of it.
pub const REUSE: f64 = 0.05;
/// ... and while each iteration with it reduces the error at least this much.
pub(super) const SLOW: f64 = 0.1;
/// An island whose refactorisation costs at most this many solves refactorises whenever its
/// matrix changes at all: an exact Newton step then saves at least the solve it costs.
const CHEAP: f64 = 1.0;

impl IslandSolver {
    /// The residual and the Jacobian at the nodes' temperatures; returns the error the
    /// residual bounds (K) and how far the Jacobian has drifted from its factorisation.
    pub(super) fn evaluate(&mut self, c: &Context<'_>, dt: f64, parallel: bool) -> (f64, f64) {
        let (nodes, ends, rad) = (&self.node_state, &self.ends, &self.radiation);
        each(&mut self.hot, parallel, |j, s| {
            let e = ends[j];
            let (tb, fixed) = match e.b() {
                Ok(b) => (nodes[b].t, false),
                Err(b) => (c.temperature[b], true),
            };
            s.linearise(nodes[e.a()].t, tb, rad[j], fixed);
        });
        let (links, ptr, incident) = (&self.hot, &self.incident_ptr, &self.incident);
        let (mass, enthalpy) = (&self.mass, &self.enthalpy);
        each_most(&mut self.node_state, parallel, |i, s| {
            let (mut gain, mut lost) = (s.heat, 0.0);
            let (h, capacity) = enthalpy[i].state(s.t);
            s.capacity = mass[i] * capacity;
            let mut drift = relative(s.capacity, s.factored);
            for &j in &incident[ptr[i]..ptr[i + 1]] {
                let link = &links[(j & !(FIXED | TO_FIXED)) as usize];
                let q = dt * link.flow;
                gain = if j & FIXED != 0 { gain - q } else { gain + q };
                if j & TO_FIXED != 0 {
                    lost += q;
                }
                drift = drift.max(link.drift);
            }
            (s.gain, s.lost) = (gain, lost);
            s.residual = mass[i] * (h - s.h0) - gain;
            ((s.residual / s.capacity).abs(), drift)
        })
    }

    /// Factorises the Jacobian, or keeps the last factorisation (see the module's notes):
    /// `drift`, how far the Jacobian is from it; `slow`, it was just seen to converge slowly.
    /// Returns (factorised, the factorisation in use is stale).
    pub(super) fn factorise(&mut self, dt: f64, drift: f64, slow: bool) -> (bool, bool) {
        if let Some(chol) = &self.cholesky {
            let change = relative(dt, self.factored_dt).max(drift);
            if change == 0.0 {
                return (false, false);
            }
            if !slow && change <= REUSE && chol.factor_to_solve() > CHEAP {
                return (false, true);
            }
        }
        let contributions = &mut self.contributions;
        contributions.clear();
        for s in &mut self.node_state {
            contributions.push(s.capacity);
            s.factored = s.capacity;
        }
        for (hot, e) in self.hot.iter_mut().zip(&self.ends) {
            (hot.factored, hot.drift) = ((hot.da, hot.coupling), 0.0);
            if e.b().is_ok() {
                contributions.extend([
                    dt * hot.da,
                    dt * hot.coupling,
                    -dt * hot.coupling,
                    -dt * hot.coupling,
                ]);
            } else {
                contributions.push(dt * hot.da);
            }
        }
        self.factored_dt = dt;
        self.assembly.fill(contributions, &mut self.matrix.values);
        let positive = "a thermal network's matrix is positive definite";
        match &mut self.cholesky {
            Some(chol) => chol.refactor(&self.matrix).expect(positive),
            None => self.cholesky = Some(Cholesky::new(&self.matrix).expect(positive)),
        }
        (true, false)
    }
}
