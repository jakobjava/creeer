//! A round's system solved through its constraints' Schur complement, in buffers each
//! drivetrain keeps from step to step (a thousand drivetrains a tick: allocating them anew
//! each round was most of the time).
//!
//! The parts' block P (inertia over the step, shafts, viscous friction: symmetric positive
//! definite) is factorised by Cholesky, and the multipliers solved from S·λ = G·P⁻¹·b − b_c
//! with S = G·P⁻¹·C (G the constraints' rows, C their torques on the parts). S is factorised
//! with full pivoting: a redundant constraint (a stuck brake and a stuck bearing on one
//! shaft, the planets of a planetary set) is a dependent row, dropped with a zero
//! multiplier while the others carry its share: the constraints hold exactly, a held brake
//! never creeps (a compliance in their place made it creep by 1e-9 of a step's motion, and
//! a stopped part never quite stopped).

use crate::dense;

/// S is first scaled so that each constraint's own diagonal (its mobility: the speed one N·m
/// of it changes in a step) is ±1, so constraints on a 1000 kg·m² flywheel and on a
/// 1e-5 kg·m² pinion compare. Below this pivot a constraint is then taken as dependent on
/// the others: far above rounding's (1e-16 times P's condition, 1e5 for stiff shafts on
/// light parts), and a constraint this nearly dependent holds to within as much when dropped.
const DEPENDENT: f64 = 1e-9;

/// A round's system and its factors.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scratch {
    /// The system (`step.rs` assembles it): `n` unknowns, the first `parts` speeds, the rest
    /// multipliers; row-major matrix, right-hand side, each element's multiplier column.
    pub n: usize,
    pub parts: usize,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub column: Vec<Option<usize>>,
    /// P's Cholesky factor; P⁻¹·C (parts × constraints); G (constraints × parts); S's LU
    /// with its pivoting, rank and each constraint's scale 1/√|S_ii|.
    p: Vec<f64>,
    y: Vec<f64>,
    g: Vec<f64>,
    s: Vec<f64>,
    rows: Vec<usize>,
    cols: Vec<usize>,
    rank: usize,
    scale: Vec<f64>,
    rhs: Vec<f64>,
    lambda: Vec<f64>,
    z: Vec<f64>,
}

impl Scratch {
    /// Starts a system of `n` unknowns, `parts` of them speeds, for `elements` elements: all
    /// zero (its matrix only if `matrix`).
    pub(crate) fn reset(&mut self, n: usize, parts: usize, elements: usize, matrix: bool) {
        (self.n, self.parts) = (n, parts);
        if matrix {
            self.a.clear();
            self.a.resize(n * n, 0.0);
        }
        self.b.clear();
        self.b.resize(n, 0.0);
        self.column.clear();
        self.column.resize(elements, None);
    }

    /// Factorises the system; false if its parts' block is not positive definite (a part
    /// without inertia).
    pub(crate) fn factor(&mut self) -> bool {
        let (n, parts) = (self.n, self.parts);
        let nc = n - parts;
        self.p.clear();
        for r in 0..parts {
            self.p.extend_from_slice(&self.a[r * n..r * n + parts]);
        }
        if !dense::cholesky(&mut self.p, parts) {
            return false;
        }
        self.y.clear();
        self.y.resize(parts * nc, 0.0);
        for c in 0..nc {
            self.z.clear();
            self.z.extend((0..parts).map(|r| self.a[r * n + parts + c]));
            dense::cholesky_solve(&self.p, parts, &mut self.z);
            for (r, v) in self.z.iter().enumerate() {
                self.y[r * nc + c] = *v;
            }
        }
        self.g.clear();
        for c in 0..nc {
            let row = (parts + c) * n;
            self.g.extend_from_slice(&self.a[row..row + parts]);
        }
        self.s.clear();
        for i in 0..nc {
            for j in 0..nc {
                let v = (0..parts)
                    .map(|r| self.g[i * parts + r] * self.y[r * nc + j])
                    .sum();
                self.s.push(v);
            }
        }
        self.scale.clear();
        self.scale.extend((0..nc).map(|i| {
            let d = self.s[i * nc + i].abs();
            if d > 0.0 { 1.0 / d.sqrt() } else { 1.0 }
        }));
        for i in 0..nc {
            for j in 0..nc {
                self.s[i * nc + j] *= self.scale[i] * self.scale[j];
            }
        }
        self.rank = dense::factor_full(&mut self.s, nc, DEPENDENT, &mut self.rows, &mut self.cols);
        true
    }

    /// The solution for right-hand side `b` into `x`: the speeds, then the multipliers.
    pub(crate) fn solve(&mut self, b: &[f64], x: &mut Vec<f64>) {
        let (parts, nc) = (self.parts, self.n - self.parts);
        x.clear();
        x.extend_from_slice(&b[..parts]);
        dense::cholesky_solve(&self.p, parts, x);
        self.rhs.clear();
        for i in 0..nc {
            let free: f64 = (0..parts).map(|r| self.g[i * parts + r] * x[r]).sum();
            self.rhs.push((free - b[parts + i]) * self.scale[i]);
        }
        self.lambda.clear();
        self.lambda.resize(nc, 0.0);
        let pivoting = (self.rows.as_slice(), self.cols.as_slice(), self.rank);
        dense::solve_full(&self.s, pivoting, &self.rhs, &mut self.lambda, &mut self.z);
        for (l, q) in self.lambda.iter_mut().zip(&self.scale) {
            *l *= q;
        }
        for (r, v) in x.iter_mut().enumerate() {
            *v -= (0..nc)
                .map(|c| self.y[r * nc + c] * self.lambda[c])
                .sum::<f64>();
        }
        x.extend_from_slice(&self.lambda);
    }
}
