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
    fn lift(&self, u: f64) -> f64 {
        if u <= self.floor {
            0.0
        } else {
            self.scale * pow(u, self.exponent) - self.offset
        }
    }

    /// d(lift)/du.
    fn slope(&self, u: f64) -> f64 {
        if u <= self.floor {
            0.0
        } else {
            self.scale * self.exponent * pow(u, self.exponent - 1.0)
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
        // k·(|x| − u) = D′(u): its left side falls and D′ rises with u, so the root lies
        // between 0 and the increment without the rate. Newton, kept inside the bracket.
        let excess =
            |u: f64| k * (size - u) - self.capacity * (1.0 + rate.lift(u)) - self.hardening * u;
        let (mut lo, mut hi) = (0.0, plain);
        let mut u = plain;
        for _ in 0..100 {
            let f = excess(u);
            if f > 0.0 {
                lo = u;
            } else {
                hi = u;
            }
            let derivative = k + self.hardening + self.capacity * rate.slope(u);
            let mut next = u + f / derivative;
            if next.is_nan() || next <= lo || next >= hi {
                next = 0.5 * (lo + hi);
            }
            if (next - u).abs() <= 1e-15 * u.max(f64::MIN_POSITIVE) || hi - lo <= 1e-15 * hi {
                u = next;
                break;
            }
            u = next;
        }
        u.copysign(x)
    }
}
