//! A round's system solved through its constraints' Schur complement.
//!
//! The parts' block P (inertia over the step, shafts, viscous friction: symmetric positive
//! definite) is factorised, and the multipliers solved from S·λ = G·P⁻¹·b − b_c with
//! S = G·P⁻¹·C (G the constraints' rows, C their torques on the parts). S is factorised
//! with full pivoting: a redundant constraint (a stuck brake and a stuck bearing on one
//! shaft, the planets of a planetary set) is a dependent row, dropped with a zero
//! multiplier while the others carry its share: the constraints hold exactly, a held brake
//! never creeps (a compliance in their place made it creep by 1e-9 of a step's motion, and
//! a stopped part never quite stopped).

use crate::dense::{self, Full};

/// S is first scaled so that each constraint's own diagonal (its mobility: the speed one N·m
/// of it changes in a step) is ±1, so constraints on a 1000 kg·m² flywheel and on a
/// 1e-5 kg·m² pinion compare. Below this pivot a constraint is then taken as dependent on
/// the others: far above rounding's (1e-16 times P's condition, 1e5 for stiff shafts on
/// light parts), and a constraint this nearly dependent holds to within as much when dropped.
const DEPENDENT: f64 = 1e-9;

/// A system factorised for any right-hand side.
pub(crate) struct Factored {
    parts: usize,
    n: usize,
    /// P's LU and pivots; P⁻¹·C (parts × constraints, row-major); G (constraints × parts).
    p: Vec<f64>,
    pivots: Vec<usize>,
    y: Vec<f64>,
    g: Vec<f64>,
    s: Vec<f64>,
    full: Full,
    /// Each constraint's scale: 1/√|S_ii|.
    scale: Vec<f64>,
}

impl Factored {
    /// Factorises the `n`×`n` row-major system `a` whose first `parts` unknowns are speeds,
    /// the rest multipliers; `None` if its parts' block is singular.
    pub(crate) fn new(a: &[f64], n: usize, parts: usize) -> Option<Self> {
        let nc = n - parts;
        let mut p = vec![0.0; parts * parts];
        for r in 0..parts {
            p[r * parts..(r + 1) * parts].copy_from_slice(&a[r * n..r * n + parts]);
        }
        let pivots = dense::factor(&mut p, parts)?;
        let mut y = vec![0.0; parts * nc];
        for c in 0..nc {
            let column: Vec<f64> = (0..parts).map(|r| a[r * n + parts + c]).collect();
            for (r, v) in dense::solve(&p, &pivots, &column).into_iter().enumerate() {
                y[r * nc + c] = v;
            }
        }
        let mut g = vec![0.0; nc * parts];
        for c in 0..nc {
            g[c * parts..(c + 1) * parts]
                .copy_from_slice(&a[(parts + c) * n..(parts + c) * n + parts]);
        }
        let mut s = vec![0.0; nc * nc];
        for i in 0..nc {
            for j in 0..nc {
                s[i * nc + j] = (0..parts).map(|r| g[i * parts + r] * y[r * nc + j]).sum();
            }
        }
        let scale: Vec<f64> = (0..nc)
            .map(|i| {
                let d = s[i * nc + i].abs();
                if d > 0.0 { 1.0 / d.sqrt() } else { 1.0 }
            })
            .collect();
        for i in 0..nc {
            for j in 0..nc {
                s[i * nc + j] *= scale[i] * scale[j];
            }
        }
        let full = dense::factor_full(&mut s, nc, DEPENDENT);
        Some(Self {
            parts,
            n,
            p,
            pivots,
            y,
            g,
            s,
            full,
            scale,
        })
    }

    /// The solution for right-hand side `b`: the speeds, then the multipliers.
    pub(crate) fn solve(&self, b: &[f64]) -> Vec<f64> {
        let (parts, nc) = (self.parts, self.n - self.parts);
        let free = dense::solve(&self.p, &self.pivots, &b[..parts]);
        let rhs: Vec<f64> = (0..nc)
            .map(|i| {
                ((0..parts)
                    .map(|r| self.g[i * parts + r] * free[r])
                    .sum::<f64>()
                    - b[parts + i])
                    * self.scale[i]
            })
            .collect();
        let mut lambda = dense::solve_full(&self.s, &self.full, &rhs);
        for (l, q) in lambda.iter_mut().zip(&self.scale) {
            *l *= q;
        }
        let mut x = free;
        for (r, v) in x.iter_mut().enumerate() {
            *v -= (0..nc).map(|c| self.y[r * nc + c] * lambda[c]).sum::<f64>();
        }
        x.extend(lambda);
        x
    }
}
