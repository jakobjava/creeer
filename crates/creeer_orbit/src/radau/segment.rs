//! Dense output: a particle's motion anywhere inside a finished step, from the step's own
//! fitted acceleration polynomial (exact to the integrator's precision).

use super::{Poly, V_WEIGHT, X_WEIGHT};
use creeer_math::glam::DVec3;

/// x(h) − x₀ over a fraction `h` of a step, `hd` = h·dt seconds into it:
/// v₀·hd + hd²·(a₀/2 + Σ bₘ·h^{m+1}/((m+2)(m+3))) (Horner's rule).
#[inline]
pub(super) fn position_increment(b: &Poly, v0: DVec3, a0: DVec3, h: f64, hd: f64) -> DVec3 {
    let mut px = b[6] * (X_WEIGHT[6] * h);
    for m in (0..6).rev() {
        px = (px + b[m] * X_WEIGHT[m]) * h;
    }
    v0 * hd + (a0 * 0.5 + px) * (hd * hd)
}

/// v(h) − v₀: hd·(a₀ + Σ bₘ·h^{m+1}/(m+2)).
#[inline]
pub(super) fn velocity_increment(b: &Poly, a0: DVec3, h: f64, hd: f64) -> DVec3 {
    let mut pv = b[6] * (V_WEIGHT[6] * h);
    for m in (0..6).rev() {
        pv = (pv + b[m] * V_WEIGHT[m]) * h;
    }
    (a0 + pv) * hd
}

/// One particle over one finished step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Segment {
    /// Position at the start (m).
    pub x: DVec3,
    /// Velocity at the start (m/s).
    pub v: DVec3,
    /// Kahan compensations of x and v at the start (true value = stored − compensation).
    pub comp: [DVec3; 2],
    /// Acceleration at the start (m/s²).
    pub a: DVec3,
    /// The step's fitted acceleration polynomial: a(s) = a + Σ bₘ·s^{m+1}.
    pub b: Poly,
}

impl Segment {
    /// Position at fraction `s` ∈ [0, 1] of a step of `dt` seconds.
    #[inline]
    pub fn position(&self, dt: f64, s: f64) -> DVec3 {
        self.x + (position_increment(&self.b, self.v, self.a, s, s * dt) - self.comp[0])
    }

    /// Position and velocity at fraction `s` of a step of `dt` seconds.
    #[inline]
    pub fn state(&self, dt: f64, s: f64) -> (DVec3, DVec3) {
        let v = self.v + (velocity_increment(&self.b, self.a, s, s * dt) - self.comp[1]);
        (self.position(dt, s), v)
    }

    /// Acceleration at fraction `s`.
    #[inline]
    pub fn acceleration(&self, s: f64) -> DVec3 {
        let mut a = self.b[6] * s;
        for m in (0..6).rev() {
            a = (a + self.b[m]) * s;
        }
        self.a + a
    }
}
