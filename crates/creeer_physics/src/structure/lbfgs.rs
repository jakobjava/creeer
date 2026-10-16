//! L-BFGS on top of the factorised Newton matrix (Liu, Bouaziz & Kavan 2017, "Quasi-Newton
//! Methods for Real-Time Simulation of Hyperelastic Materials").
//!
//! The factorised matrix is exact for small deformations but misses how the stiffness turns
//! with the members once the structure has deformed; plain Newton with it then converges
//! only linearly (measured: about 13 iterations per step for a cantilever swinging ±0.1 m).
//! L-BFGS learns the missing curvature from the last few steps (pairs of step s and
//! gradient change y) and corrects the direction, at the cost of a few dot products.

use creeer_math::glam::DVec3;

/// How many (s, y) pairs are remembered.
const HISTORY: usize = 5;

/// Correction pairs collected within one time step.
#[derive(Clone, Debug, Default)]
pub(super) struct Lbfgs {
    steps: Vec<Vec<DVec3>>,
    changes: Vec<Vec<DVec3>>,
    /// 1 / (y·s) per pair.
    rho: Vec<f64>,
    alpha: Vec<f64>,
}

fn dot(a: &[DVec3], b: &[DVec3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a.dot(*b)).sum()
}

impl Lbfgs {
    /// Forgets all pairs (at the start of each time step).
    pub fn clear(&mut self) {
        self.steps.clear();
        self.changes.clear();
        self.rho.clear();
    }

    /// Remembers a step `s` and the gradient change `y` it caused. Pairs without positive
    /// curvature (y·s ≤ 0) would break the method and are skipped.
    pub fn push(&mut self, s: Vec<DVec3>, y: Vec<DVec3>) {
        let curvature = dot(&s, &y);
        if curvature <= 0.0 || !curvature.is_finite() {
            return;
        }
        if self.steps.len() == HISTORY {
            self.steps.remove(0);
            self.changes.remove(0);
            self.rho.remove(0);
        }
        self.steps.push(s);
        self.changes.push(y);
        self.rho.push(1.0 / curvature);
    }

    /// Turns the residual r = −gradient (in place) into the search direction, using
    /// `solve` (the factorised matrix) as the base inverse Hessian. Two-loop recursion.
    pub fn direction(&mut self, r: &mut [DVec3], mut solve: impl FnMut(&mut [DVec3])) {
        let pairs = self.steps.len();
        self.alpha.resize(pairs, 0.0);
        for k in (0..pairs).rev() {
            let a = self.rho[k] * dot(&self.steps[k], r);
            self.alpha[k] = a;
            for (v, y) in r.iter_mut().zip(&self.changes[k]) {
                *v -= *y * a;
            }
        }
        solve(r);
        for k in 0..pairs {
            let b = self.rho[k] * dot(&self.changes[k], r);
            let a = self.alpha[k];
            for (v, s) in r.iter_mut().zip(&self.steps[k]) {
                *v += *s * (a - b);
            }
        }
    }
}
