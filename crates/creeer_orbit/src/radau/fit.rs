//! The fitted acceleration polynomials: step-size control, rescaling, and the conversion
//! between the Newton and power bases.

use super::coeffs::coefficients;
use super::{MAX_STRETCH, Radau, SAFETY};
use creeer_math::det;
use creeer_math::glam::DVec3;

impl Radau {
    /// The step size the fit asks for next (Pham, Rein & Spiegel 2024): the shortest
    /// dynamical time τ² = 2|a|²/(|ȧ|² + |a|·|ä|) at the end of the step, times (ε·7!)^{1/7}.
    ///
    /// Where a particle's acceleration passes through zero (the middle body of the
    /// figure-eight at t = 0) that ratio vanishes although the motion is nearly straight and
    /// easy to follow; asking for ever shorter steps there would never end. Each particle's
    /// time is therefore at least |ȧ|/|ä|, which is the same 1/ω on a circular orbit.
    pub(super) fn next_step(&self, dt: f64) -> f64 {
        let mut min_tau2 = f64::INFINITY;
        for (i, b) in self.b.iter().enumerate() {
            let (mut a, mut j, mut s) = (self.a0[i], DVec3::ZERO, DVec3::ZERO);
            for (m, bm) in b.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)] // m < 7
                let k = (m + 1) as f64;
                a += *bm;
                j += *bm * k;
                s += *bm * (k * (k - 1.0));
            }
            let (y2, y3, y4) = (a.length_squared(), j.length_squared(), s.length_squared());
            let denominator = y3 + (y2 * y4).sqrt();
            if denominator > 0.0 && y4 > 0.0 {
                min_tau2 = min_tau2.min((2.0 * y2 / denominator).max(y3 / y4));
            }
        }
        if min_tau2.is_finite() {
            det::pow(self.epsilon * 5040.0, 1.0 / 7.0) * min_tau2.sqrt() * dt
        } else {
            dt / SAFETY
        }
    }

    /// Rescales the fitted polynomials to a step `q` times as long.
    pub(super) fn rescale_fit(&mut self, q: f64) {
        if q > MAX_STRETCH {
            for poly in self.b.iter_mut().chain(&mut self.e) {
                *poly = [DVec3::ZERO; 7];
            }
            return;
        }
        for poly in self.b.iter_mut().chain(&mut self.e) {
            let mut qk = q;
            for coefficient in poly.iter_mut() {
                *coefficient *= qk;
                qk *= q;
            }
        }
    }

    /// Newton coefficients g from the polynomial coefficients b (the triangular inverse of
    /// bₘ = Σₖ c[k][m+1]·gₖ).
    pub(super) fn g_from_b(&mut self) {
        let co = coefficients();
        for (b, g) in self.b.iter().zip(&mut self.g) {
            for k in (1..8).rev() {
                let mut value = b[k - 1];
                for later in k + 1..8 {
                    value -= g[later - 1] * co.c[later][k];
                }
                g[k - 1] = value;
            }
        }
    }
}
