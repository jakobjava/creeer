//! How one measure of a member (its axial force, torque or an end moment) flows plastically
//! within one step (`plastic.rs`).
//!
//! A plastic increment u = |p| of the measure costs the work D(u) = ∫ c(t) dt over the step,
//! c the capacity as the increment grows: the capacity at the step's start, hardened by the
//! increment itself (the flow stress rising with plastic strain, `Material::flow_stress`),
//! and lifted by the strain rate it implies (Cowper–Symonds, `RateSensitivity`). c only
//! grows with u, so D is convex and so is the member's incremental potential: the step
//! stays one convex minimisation.

use super::section::Material;
use creeer_math::det::pow;

/// The plastic flow of one measure in one step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flow {
    /// The capacity at the step's start (N or N·m).
    pub capacity: f64,
    /// How the capacity rises with the increment (N or N·m per unit of the measure).
    pub hardening: f64,
    /// The equivalent plastic strain per unit of the measure.
    per: f64,
    /// The step (s).
    h: f64,
    /// Cowper–Symonds: D (1/s; 0: rate-insensitive), 1/q, and the reference rate (1/s).
    sensitivity: (f64, f64, f64),
}

/// The rate's terms of a flow, worked out only for a measure that flows: two powers for
/// every measure of every member at every step, where almost none flows, were a cost
/// structures did not need to pay.
#[derive(Clone, Copy, Debug)]
struct Rate {
    /// (g/(h·D))^{1/q}: the lift per increment^{1/q}.
    scale: f64,
    /// (ε̇_ref/D)^{1/q}: the reference rate's own lift, which the static strength includes.
    offset: f64,
    /// The increment at the reference rate, ε̇_ref·h/g.
    floor: f64,
    /// 1/q.
    exponent: f64,
}

impl Rate {
    /// The lift of the capacity at increment `u` (relative).
    #[cfg(test)]
    fn lift(&self, u: f64) -> f64 {
        if u <= self.floor {
            0.0
        } else {
            self.scale * pow(u, self.exponent) - self.offset
        }
    }

    /// ∫ lift du from 0 to `u`.
    fn integral(&self, u: f64) -> f64 {
        if u <= self.floor {
            return 0.0;
        }
        let power = 1.0 + self.exponent;
        let rise = (pow(u, power) - pow(self.floor, power)) / power;
        self.scale * rise - self.offset * (u - self.floor)
    }
}

impl Flow {
    /// A measure that never flows (before a step has set its capacity).
    pub const NEVER: Self = Self {
        capacity: f64::INFINITY,
        hardening: 0.0,
        per: 0.0,
        h: 0.0,
        sensitivity: (0.0, 1.0, 0.0),
    };

    /// The flow of a measure with capacity `capacity` at plastic strain `strain` of
    /// `material`, `per` the equivalent plastic strain per unit of the measure, in steps of
    /// `h` seconds.
    pub fn new(capacity: f64, material: &Material, strain: f64, per: f64, h: f64) -> Self {
        let stress = material.flow_stress(strain);
        let hardening = if stress > 0.0 && stress.is_finite() {
            capacity * material.hardening_modulus(strain) / stress * per
        } else {
            0.0
        };
        let r = material.rate;
        let sensitivity = if !r.matters() || per.is_nan() || per <= 0.0 {
            (0.0, 1.0, 0.0)
        } else {
            (r.d, 1.0 / r.q, r.reference)
        };
        Self {
            capacity,
            hardening,
            per,
            h,
            sensitivity,
        }
    }

    /// The same flow with its work divided by `scale` (the series damper, `plastic.rs`).
    pub fn scaled(&self, scale: f64) -> Self {
        Self {
            capacity: self.capacity / scale,
            hardening: self.hardening / scale,
            ..*self
        }
    }

    /// The rate's terms, if the flow stress depends on the rate.
    fn rate(&self) -> Option<Rate> {
        let (d, exponent, reference) = self.sensitivity;
        (d > 0.0).then(|| Rate {
            scale: pow(self.per / (self.h * d), exponent),
            offset: pow(reference / d, exponent),
            floor: reference * self.h / self.per,
            exponent,
        })
    }

    /// The capacity at increment `u`: dD/du.
    #[cfg(test)]
    pub fn slope(&self, u: f64) -> f64 {
        let lift = self.rate().map_or(0.0, |r| r.lift(u));
        self.capacity * (1.0 + lift) + self.hardening * u
    }

    /// The work of increment `u`, D(u).
    pub fn work(&self, u: f64) -> f64 {
        let lifted = self.rate().map_or(0.0, |r| r.integral(u));
        self.capacity * (u + lifted) + 0.5 * self.hardening * u * u
    }

    /// The increment p that minimises ½·k·(x − p)² + D(|p|).
    pub fn increment(&self, x: f64, k: f64) -> f64 {
        let threshold = self.capacity / k;
        let size = x.abs();
        if size <= threshold {
            return 0.0;
        }
        // Without the rate: perfectly plastic (soft thresholding) or hardening (linear).
        let plain = if self.hardening == 0.0 {
            size - threshold
        } else {
            (k * size - self.capacity) / (k + self.hardening)
        };
        let Some(rate) = self.rate() else {
            return plain.copysign(x);
        };
        // At or below the reference rate's increment the rate lifts nothing.
        if plain <= rate.floor {
            return plain.copysign(x);
        }
        crate::profile::add(crate::profile::Slot::Increments, 1);
        // Above it the lift is scale·u^e − offset (e = 1/q), continuous at the floor. In
        // w = u^e the excess k·(|x| − u) − c·(1 + lift(u)) − H·u is
        //   f(w) = k·|x| − c·(1 − offset) − c·scale·w − (k + H)·w^q,
        // concave and falling, with its root between floor^e and plain^e: Newton from the
        // right falls onto it monotonically and quadratically, one power an iteration (in u,
        // where the lift rises as u^0.2, it took 10 to 15 iterations of two powers each,
        // most of a crash's step).
        let q = 1.0 / rate.exponent;
        let (stiff, constant) = (
            k + self.hardening,
            k * size - self.capacity * (1.0 - rate.offset),
        );
        let lift = self.capacity * rate.scale;
        // floor^e, where the lift is nought: scale·floor^e = offset.
        let low = rate.offset / rate.scale;
        // w^q: by multiplication for a whole q (steel's Cowper–Symonds q is 5: exact IEEE
        // products, the same on every platform), else the deterministic pow.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // whole, 1 to 8
        let whole = (q.fract() == 0.0 && (1.0..=8.0).contains(&q)).then_some(q as u32);
        let power = |w: f64| match whole {
            Some(n) => (1..n).fold(w, |p, _| p * w),
            None => pow(w, q),
        };
        // Two bounds above the root: the increment without the rate, and the root of f's
        // linear part (f there is −(k + H)·w^q < 0); from the nearer one Newton needs fewest
        // steps.
        let mut w = pow(plain, rate.exponent).min(constant / lift);
        let mut wq = power(w);
        for _ in 0..60 {
            crate::profile::add(crate::profile::Slot::FlowIterations, 1);
            let f = constant - lift * w - stiff * wq;
            let slope = -lift - stiff * q * wq / w;
            let next = (w - f / slope).max(low);
            // From the right it only falls; once it stops falling it is there.
            if next.partial_cmp(&w) != Some(std::cmp::Ordering::Less) {
                break;
            }
            let done = w - next <= 1e-15 * w;
            (w, wq) = (next, power(next));
            if done {
                break;
            }
        }
        wq.min(plain).copysign(x)
    }
}
