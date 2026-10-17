//! The endings of a one-state path, found exactly (no sampling): with one state s(t),
//! monotone (s∞ + (s₀ − s∞)·eᵃᵗ), every stock's amount is α + β·t + γ·∫s, so its rate of
//! change is linear in s and changes sign at most once: the amount is convex or concave in
//! time. It crosses a bound within a segment iff it is past it at the segment's end, or at
//! its one turning point (where its net rate, linear in s, is zero: s* by interpolation, t*
//! by the logarithm). A held stock's φ is linear in s: monotone, past iff past at the end.

use crate::exact::{Ending, Linear};
use crate::expm::Path;
use crate::held::Gate;
use crate::network::Network;

impl Network {
    /// Stock `m`'s net rate along `lin` with the stocks at `amounts`.
    fn net_at(&self, lin: &Linear, m: usize, amounts: &[f64]) -> f64 {
        let mut net = 0.0;
        for (&(_, stock), ts) in self.layout.iter().zip(lin.terms.each()) {
            if stock == m {
                for &(factor, p, j) in ts {
                    net += factor * self.processes[p].flows[j].at(amounts);
                }
            }
        }
        net
    }

    /// The first of `watch` reached within `h` along the one-state path `lin` from `z0`
    /// (the stocks at `start`), and when; `None` if none is.
    pub(crate) fn scalar_ending(
        &self,
        (lin, z0, start): (&Linear, &[f64], &[f64]),
        h: f64,
        watch: &[Ending],
        gates: &[Gate],
    ) -> Option<(Ending, f64)> {
        let Path::Scalar { a, c } = lin.path else {
            unreachable!("one state")
        };
        let (mut z, mut moved) = (Vec::with_capacity(3), Vec::new());
        // The stocks at `t`, and the state then.
        let mut at = |t: f64| -> (Vec<f64>, f64) {
            let mut amounts = vec![0.0; self.stocks.len()];
            lin.path.write(t, z0, &mut z);
            self.along(lin, &z, t, &mut amounts, &mut moved);
            (amounts, z[0])
        };
        let (end, sh) = at(h);
        let s0 = z0[0];
        let mut first: Option<(Ending, f64)> = None;
        for &e in watch {
            // The stock and the direction past (σ: +1 above, −1 below), if a stock's.
            let stock = match e {
                Ending::Empty(k) => Some((k, -1.0)),
                Ending::Full(k) => Some((k, 1.0)),
                Ending::Stale(_, s, lo, hi) => {
                    let x = end[s];
                    Some((s, if x - hi > lo - x { 1.0 } else { -1.0 }))
                }
                Ending::Release(_) => None,
            };
            let bracket = match stock {
                None => (self.beyond(e, &end, gates) >= 0.0).then_some((0.0, h)),
                Some((m, sigma)) => {
                    let (d0, dh) = (
                        sigma * self.net_at(lin, m, start),
                        sigma * self.net_at(lin, m, &end),
                    );
                    if d0 * dh >= 0.0 {
                        (self.beyond(e, &end, gates) >= 0.0).then_some((0.0, h))
                    } else {
                        // The turning point: the net rate is zero at s*.
                        let star = s0 + (sh - s0) * d0 / (d0 - dh);
                        let t = if a == 0.0 {
                            h * d0 / (d0 - dh)
                        } else {
                            let inf = -c / a;
                            creeer_math::det::ln((star - inf) / (s0 - inf)) / a
                        };
                        let t = if t.is_finite() {
                            t.clamp(0.0, h)
                        } else {
                            0.5 * h
                        };
                        if d0 > 0.0 {
                            // Rises to a peak at t, then falls.
                            (self.beyond(e, &at(t).0, gates) >= 0.0).then_some((0.0, t))
                        } else {
                            // Falls to a trough at t, then rises.
                            (self.beyond(e, &end, gates) >= 0.0).then_some((t, h))
                        }
                    }
                }
            };
            if let Some((lo, hi)) = bracket {
                let t = self.falsi(lin, z0, e, gates, lo, hi);
                if first.is_none_or(|(_, f)| t < f) {
                    first = Some((e, t));
                }
            }
        }
        first
    }
}
