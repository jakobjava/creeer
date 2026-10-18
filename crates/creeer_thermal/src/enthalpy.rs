//! A material's specific enthalpy against temperature: the exact integral of its specific
//! heat (piecewise linear in the data, so piecewise quadratic here), plus its heat of fusion
//! spread over its melting range. The network keeps temperatures and balances these
//! enthalpies, so melting takes exactly the latent heat, and a step's energy is exact.
//!
//! A pure substance melts at one temperature (ice at 0 °C): its heat of fusion is spread over
//! [`MELT_SPREAD`] around it, so that the step's Newton iterations see a finite heat capacity.

use creeer_materials::{Curve, Melting};

/// The temperature range a pure substance's melting is spread over (K).
pub const MELT_SPREAD: f64 = 1.0;

/// Enthalpies are measured from this temperature (20 °C).
pub const REFERENCE: f64 = 293.15;

/// Specific enthalpy against temperature.
#[derive(Clone, Debug, PartialEq)]
pub struct Enthalpy {
    /// (temperature K, specific heat J/(kg·K), sensible enthalpy J/kg) at the specific heat's
    /// breakpoints.
    points: Vec<(f64, f64, f64)>,
    /// Melting: (start K, end K, heat of fusion J/kg).
    melt: Option<(f64, f64, f64)>,
    /// The enthalpy at [`REFERENCE`], subtracted from every value.
    zero: f64,
}

impl Enthalpy {
    /// From a material's specific heat and melting.
    pub fn new(specific_heat: &Curve, melting: Option<Melting>) -> Self {
        let mut points = Vec::with_capacity(specific_heat.points().len());
        let mut h = 0.0;
        for (k, &(t, c)) in specific_heat.points().iter().enumerate() {
            if k > 0 {
                let (t0, c0, _) = points[k - 1];
                h += 0.5 * (c0 + c) * (t - t0);
            }
            points.push((t, c, h));
        }
        let melt = melting.map(|m| {
            let (lo, hi) = if m.liquidus - m.solidus < MELT_SPREAD {
                let mid = 0.5 * (m.solidus + m.liquidus);
                (mid - 0.5 * MELT_SPREAD, mid + 0.5 * MELT_SPREAD)
            } else {
                (m.solidus, m.liquidus)
            };
            (lo, hi, m.latent_heat)
        });
        let mut this = Self {
            points,
            melt,
            zero: 0.0,
        };
        this.zero = this.at(REFERENCE);
        this
    }

    /// A constant specific heat, no melting.
    pub fn constant(specific_heat: f64) -> Self {
        Self::new(&Curve::constant(specific_heat), None)
    }

    fn sensible(&self, t: f64) -> f64 {
        let p = &self.points;
        let k = p.partition_point(|&(pt, _, _)| pt <= t);
        if k == 0 {
            let (t0, c0, h0) = p[0];
            return h0 + c0 * (t - t0);
        }
        let (t0, c0, h0) = p[k - 1];
        if k == p.len() {
            return h0 + c0 * (t - t0);
        }
        let (t1, c1, _) = p[k];
        let c = c0 + (c1 - c0) * (t - t0) / (t1 - t0);
        h0 + 0.5 * (c0 + c) * (t - t0)
    }

    fn specific_heat(&self, t: f64) -> f64 {
        let p = &self.points;
        let k = p.partition_point(|&(pt, _, _)| pt <= t);
        match k {
            0 => p[0].1,
            k if k == p.len() => p[k - 1].1,
            k => {
                let ((t0, c0, _), (t1, c1, _)) = (p[k - 1], p[k]);
                c0 + (c1 - c0) * (t - t0) / (t1 - t0)
            }
        }
    }

    /// Specific enthalpy at `t` (J/kg, zero at 20 °C).
    pub fn at(&self, t: f64) -> f64 {
        let latent = self.melt.map_or(0.0, |(lo, hi, l)| {
            l * ((t - lo) / (hi - lo)).clamp(0.0, 1.0)
        });
        self.sensible(t) + latent - self.zero
    }

    /// dH/dT at `t` (J/(kg·K)): the specific heat, plus the heat of fusion over the melting
    /// range while melting. At the range's ends, where H has a corner, the larger of its two
    /// slopes: Newton's step from a corner is then never too long (see the step's landing).
    pub fn capacity(&self, t: f64) -> f64 {
        let latent = match self.melt {
            Some((lo, hi, l)) if (lo..=hi).contains(&t) => l / (hi - lo),
            _ => 0.0,
        };
        self.specific_heat(t) + latent
    }

    /// [`Self::at`] and [`Self::capacity`] at once, from one search of the breakpoints.
    pub fn state(&self, t: f64) -> (f64, f64) {
        let p = &self.points;
        let k = p.partition_point(|&(pt, _, _)| pt <= t);
        let (h, c) = if k == 0 || k == p.len() {
            let (t0, c0, h0) = p[k.saturating_sub(1)];
            (h0 + c0 * (t - t0), c0)
        } else {
            let ((t0, c0, h0), (t1, c1, _)) = (p[k - 1], p[k]);
            let c = c0 + (c1 - c0) * (t - t0) / (t1 - t0);
            (h0 + 0.5 * (c0 + c) * (t - t0), c)
        };
        match self.melt {
            Some((lo, hi, l)) => {
                let latent = l * ((t - lo) / (hi - lo)).clamp(0.0, 1.0);
                let melting = if (lo..=hi).contains(&t) {
                    l / (hi - lo)
                } else {
                    0.0
                };
                (h + latent - self.zero, c + melting)
            }
            None => (h - self.zero, c),
        }
    }

    /// The temperature (K) at which its specific enthalpy is `h` (J/kg), from `guess`:
    /// Newton on H(T), which rises everywhere (its capacity is positive), to rounding.
    pub fn temperature(&self, h: f64, guess: f64) -> f64 {
        let mut t = guess;
        for _ in 0..60 {
            let (ht, c) = self.state(t);
            let step = (h - ht) / c;
            t += step;
            if step.abs() <= 1e-12 * t.abs() {
                break;
            }
        }
        t
    }

    /// Where its melting starts and ends (K), if it melts.
    pub fn melting(&self) -> Option<(f64, f64)> {
        self.melt.map(|(lo, hi, _)| (lo, hi))
    }
}
