//! Two-body (Kepler) motion: exact propagation of a state, and orbital elements.
//!
//! In the n-body world of ADR-0007 nothing moves on a fixed conic. Kepler's solution is still
//! the backbone of every analysis: osculating elements for the map view, the start states of
//! a generated system, Hohmann transfers, and the oracle the n-body integrator is tested
//! against (two bodies must follow a conic to integrator precision).
//!
//! Propagation uses Lagrange's f and g with the universal anomaly χ (Battin; Vallado,
//! *Fundamentals of Astrodynamics*, algorithm 8), so ellipses, the parabola and hyperbolas
//! share one code path. Kepler's equation in χ is solved by Newton's method inside a bracket
//! (the residual increases monotonically, its slope is the radius r > 0), so it always
//! converges.
//!
//! **Axes.** The world is Y-up: planets turn about +Y and prograde orbits have their angular
//! momentum along +Y. Elements use the textbook frame with the pole as z, mapped as
//! (x, y, z)_textbook = (X, −Z, Y)_world, which is right-handed.

mod elements;
#[cfg(test)]
mod tests;

pub use elements::Elements;

use creeer_math::det;
use creeer_math::glam::DVec3;

/// World vector → textbook axes (pole = z).
#[inline]
pub(crate) fn to_textbook(v: DVec3) -> DVec3 {
    DVec3::new(v.x, -v.z, v.y)
}

/// Textbook axes (pole = z) → world vector.
#[inline]
pub(crate) fn from_textbook(v: DVec3) -> DVec3 {
    DVec3::new(v.x, v.z, -v.y)
}

/// Stumpff functions C(z) = (1 − cos √z)/z and S(z) = (√z − sin √z)/z^{3/2}, continued to
/// z ≤ 0 with cosh and sinh. Both are smooth through 0, where a series replaces the
/// formulas (they cancel badly there).
pub fn stumpff(z: f64) -> (f64, f64) {
    if z.abs() < 1.0 {
        stumpff_series(z)
    } else if z > 0.0 {
        let q = z.sqrt();
        let half = det::sin(0.5 * q);
        // 1 − cos q = 2 sin²(q/2): no cancellation.
        (2.0 * half * half / z, (q - det::sin(q)) / (z * q))
    } else {
        let q = (-z).sqrt();
        let half = det::sinh(0.5 * q);
        (2.0 * half * half / -z, (det::sinh(q) - q) / (-z * q))
    }
}

/// C = Σ (−z)^k / (2k + 2)!, S = Σ (−z)^k / (2k + 3)!; for |z| ≤ 2, 12 terms reach
/// 2¹²/27! < 1e-24.
fn stumpff_series(z: f64) -> (f64, f64) {
    let (mut c, mut s) = (0.0, 0.0);
    let mut term_c = 0.5; // (−z)^k / (2k+2)!
    let mut term_s = 1.0 / 6.0; // (−z)^k / (2k+3)!
    for k in 0..12u32 {
        c += term_c;
        s += term_s;
        let k = f64::from(k);
        term_c *= -z / ((2.0 * k + 3.0) * (2.0 * k + 4.0));
        term_s *= -z / ((2.0 * k + 4.0) * (2.0 * k + 5.0));
    }
    (c, s)
}

/// The state after `dt` seconds of two-body motion about a point mass with parameter `mu`
/// (m³/s²), from position `r0` (m) and velocity `v0` (m/s) relative to it. `dt` may be
/// negative.
pub fn propagate(mu: f64, r0: DVec3, v0: DVec3, dt: f64) -> (DVec3, DVec3) {
    if dt == 0.0 {
        return (r0, v0);
    }
    let r0n = r0.length();
    let sqrt_mu = mu.sqrt();
    let alpha = 2.0 / r0n - v0.length_squared() / mu; // 1/a
    let sigma = r0.dot(v0) / sqrt_mu;
    // Whole revolutions change nothing; dropping them keeps χ small and exact.
    let dt = if alpha > 0.0 {
        let period = std::f64::consts::TAU / (sqrt_mu * alpha * alpha.sqrt());
        dt % period
    } else {
        dt
    };
    let chi = solve_universal(sqrt_mu, r0n, sigma, alpha, dt);
    let chi2 = chi * chi;
    let z = alpha * chi2;
    let (c, s) = stumpff(z);
    let f = 1.0 - chi2 / r0n * c;
    let g = dt - chi2 * chi / sqrt_mu * s;
    let r = r0 * f + v0 * g;
    let rn = r.length();
    let f_dot = sqrt_mu / (rn * r0n) * chi * (z * s - 1.0);
    let g_dot = 1.0 - chi2 / rn * c;
    (r, r0 * f_dot + v0 * g_dot)
}

/// Kepler's equation in the universal anomaly: F(χ) = σ·χ²·C + (1 − α·r0)·χ³·S + r0·χ − √μ·t,
/// with σ = r0·v0/√μ. F increases monotonically (F' = r > 0); returns its root.
fn solve_universal(sqrt_mu: f64, r0: f64, sigma: f64, alpha: f64, dt: f64) -> f64 {
    let residual = |chi: f64| {
        let chi2 = chi * chi;
        let z = alpha * chi2;
        let (c, s) = stumpff(z);
        let value =
            sigma * chi2 * c + (1.0 - alpha * r0) * chi2 * chi * s + r0 * chi - sqrt_mu * dt;
        let slope = sigma * chi * (1.0 - z * s) + (1.0 - alpha * r0) * chi2 * c + r0;
        (value, slope)
    };
    // F(0) = −√μ·t: the root lies on the side of t. Bracket it by doubling.
    let direction = dt.signum();
    let mut guess = if alpha > 0.0 {
        sqrt_mu * alpha * dt
    } else {
        // Vallado's start for hyperbolas: χ ≈ sign(t)·√(−a)·ln(−2μαt / (r0·v0 + sign(t)·
        // √(−μa)·(1 − r0α))).
        let a = 1.0 / alpha;
        let denominator =
            sigma * sqrt_mu + direction * (-sqrt_mu * sqrt_mu * a).sqrt() * (1.0 - r0 * alpha);
        let argument = -2.0 * sqrt_mu * sqrt_mu * alpha * dt / denominator;
        if argument > 1.0 && argument.is_finite() {
            direction * (-a).sqrt() * det::ln(argument)
        } else {
            sqrt_mu * dt / r0
        }
    };
    if guess == 0.0 || !guess.is_finite() {
        guess = direction * 1e-3;
    }
    let (mut lo, mut hi) = if direction > 0.0 {
        (0.0, guess.abs())
    } else {
        (-guess.abs(), 0.0)
    };
    let outer = if direction > 0.0 { hi } else { lo };
    let mut bound = outer;
    for _ in 0..200 {
        let (value, _) = residual(bound);
        if value * direction > 0.0 {
            break;
        }
        bound *= 2.0;
    }
    if direction > 0.0 {
        hi = bound;
    } else {
        lo = bound;
    }
    // Newton's method safeguarded by bisection ("rtsafe", Numerical Recipes §9.4): bisect
    // whenever Newton would leave the bracket or does not halve the step. Plain Newton crawls
    // on near-radial hyperbolas, where F grows exponentially.
    let mut chi = guess.clamp(lo, hi);
    let mut step_old = hi - lo;
    let mut step = step_old;
    let (mut value, mut slope) = residual(chi);
    for _ in 0..200 {
        if value == 0.0 {
            return chi;
        }
        let leaves = ((chi - hi) * slope - value) * ((chi - lo) * slope - value) > 0.0;
        let slow = (2.0 * value).abs() > (step_old * slope).abs();
        step_old = step;
        if leaves || slow {
            step = 0.5 * (hi - lo);
            chi = lo + step;
        } else {
            step = value / slope;
            chi -= step;
        }
        if step.abs() <= 1e-15 * chi.abs().max(1e-300) {
            return chi;
        }
        (value, slope) = residual(chi);
        if value < 0.0 {
            lo = chi;
        } else {
            hi = chi;
        }
    }
    chi
}

/// Solves Kepler's equation M = E − e·sin E for the eccentric anomaly E (elliptic, e < 1).
pub fn eccentric_anomaly(mean: f64, e: f64) -> f64 {
    let m = mean.rem_euclid(std::f64::consts::TAU);
    // A start that converges for every e < 1 (Danby): E₀ = M + 0.85·e·sign(sin M).
    let mut big_e = m + 0.85 * e * det::sin(m).signum();
    for _ in 0..50 {
        let (s, c) = det::sin_cos(big_e);
        let f = big_e - e * s - m;
        let step = f / (1.0 - e * c);
        big_e -= step;
        if step.abs() < 1e-15 * (1.0 + big_e.abs()) {
            break;
        }
    }
    big_e
}

/// Solves the hyperbolic Kepler equation M = e·sinh H − H for H (e > 1).
pub fn hyperbolic_anomaly(mean: f64, e: f64) -> f64 {
    let mut h = asinh(mean / e);
    for _ in 0..100 {
        let f = e * det::sinh(h) - h - mean;
        let step = f / (e * det::cosh(h) - 1.0);
        h -= step;
        if step.abs() < 1e-15 * (1.0 + h.abs()) {
            break;
        }
    }
    h
}

/// Inverse hyperbolic sine, deterministic (through `det::ln`).
pub fn asinh(x: f64) -> f64 {
    let a = x.abs();
    let value = if a > 1e8 {
        // √(a² + 1) = a to rounding (and a² would overflow near 1e154).
        det::ln(a) + std::f64::consts::LN_2
    } else {
        det::ln_1p(a + a * a / (1.0 + (1.0 + a * a).sqrt()))
    };
    value.copysign(x)
}
