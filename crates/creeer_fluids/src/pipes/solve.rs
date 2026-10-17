//! The heads' system A·H = b of each Newton iteration, solved to rounding by conjugate
//! gradients preconditioned with the last factorisation (an earlier iteration's or step's
//! A, whose laws' slopes have hardly moved): each iteration a product with A and a pair of
//! triangular solves, a tenth of a factorisation. When that converges slowly (the slopes
//! moved: a pump started, a valve opened) A is factorised afresh and solved directly.

use creeer_math::sparse::{Cholesky, Csc, NotPositiveDefinite};

/// Conjugate-gradient iterations tried before refactorising.
const TRIES: usize = 6;

/// Done when the preconditioned residual, M⁻¹·r (the error, M being near A), is this small
/// against the solution (a direct solve's rounding is some 10⁻¹⁵ of it).
const ROUNDING: f64 = 1e-13;

fn dot(x: &[f64], y: &[f64]) -> f64 {
    x.iter().zip(y).map(|(a, b)| a * b).sum()
}

fn largest(v: &[f64]) -> f64 {
    v.iter().fold(0.0f64, |m, x| m.max(x.abs()))
}

/// Solves `a`·x = `b`, x into `b`, with `factor` (refactorised from `a` when `fresh` is asked
/// for, or it is missing or no longer near). Returns whether it refactorised.
///
/// # Errors
/// If `a` is not positive definite.
pub(crate) fn solve(
    factor: &mut Option<Cholesky>,
    a: &Csc<f64>,
    b: &mut [f64],
    fresh: bool,
) -> Result<bool, NotPositiveDefinite> {
    if let Some(m) = factor.as_mut().filter(|_| !fresh)
        && refine(m, a, b)
    {
        return Ok(false);
    }
    match factor.as_mut() {
        Some(m) => m.refactor(a)?,
        None => *factor = Some(Cholesky::new(a)?),
    }
    factor.as_mut().expect("factorised").solve(b);
    Ok(true)
}

/// Preconditioned conjugate gradients from M⁻¹·b; true (x in `b`) once M⁻¹·r is rounding's.
/// (A preconditioner far from A — a junction that was cut off and is joined again — has a
/// near-singular direction: its M⁻¹·r stays large, and A is refactorised.)
fn refine(m: &mut Cholesky, a: &Csc<f64>, b: &mut [f64]) -> bool {
    let n = b.len();
    let mut x = b.to_vec();
    m.solve(&mut x);
    let ax = a.mul_vec(&x);
    let mut r: Vec<f64> = b.iter().zip(&ax).map(|(b, ax)| b - ax).collect();
    let mut z = r.clone();
    m.solve(&mut z);
    let mut p = z.clone();
    let mut rz = dot(&r, &z);
    for _ in 0..=TRIES {
        if largest(&z) <= ROUNDING * largest(&x) {
            b.copy_from_slice(&x);
            return true;
        }
        let ap = a.mul_vec(&p);
        let alpha = rz / dot(&p, &ap);
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        z.copy_from_slice(&r);
        m.solve(&mut z);
        let next = dot(&r, &z);
        let beta = next / rz;
        rz = next;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
    }
    false
}
