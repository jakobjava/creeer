//! An electrolysis cell stack (run backwards, a fuel cell): `cells` cells in series, each at
//! its reversible voltage E (the reaction's ΔG/zF, set from the chemistry every step) plus
//! its overpotential, behind the stack's ohmic resistance. The overpotential follows
//! Butler–Volmer, symmetric: i = 2·i₀·A·sinh(η/b), b = RT/(α·z·F); driven above E it splits
//! (current in), below E it runs as a fuel cell (current out), at E nothing flows.
//!
//! What the stack takes at E is the reaction's work (exported to the chemistry, which makes
//! its products from it by Faraday's law); the rest, overpotential and resistance, is heat.

use crate::devices::K_OVER_Q;
use creeer_math::det;

/// An electrolysis cell stack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Electrolyzer {
    /// Cells in series, and each cell's electrode area (m²).
    pub cells: f64,
    pub area: f64,
    /// Exchange current density (A/m²), transfer coefficient α, electrons per reaction z.
    pub exchange: f64,
    pub alpha: f64,
    pub electrons: f64,
    /// The stack's ohmic resistance (Ω).
    pub ohmic: f64,
}

impl Electrolyzer {
    /// An alkaline water electrolyser's cell (300 cm², nickel electrodes in KOH): i₀, α and
    /// its resistance chosen to give a typical alkaline cell's 1.9 V at 0.3 A/cm² at 80 °C
    /// (0.2 Ω·cm²) above its 1.18 V reversible voltage.
    pub const ALKALINE_CELL: Self = Self {
        cells: 1.0,
        area: 0.03,
        exchange: 1.1e-6,
        alpha: 0.5,
        electrons: 2.0,
        ohmic: 0.2e-4 / 0.03,
    };

    /// The overpotential's scale b = RT/(α·z·F) at `t` (V).
    pub fn slope(&self, t: f64) -> f64 {
        K_OVER_Q * t / (self.alpha * self.electrons)
    }

    /// The current (A) and its slope (S) at voltage `v` across its cells' junctions (behind
    /// the resistance), each cell's reversible voltage `e`, at `t` (K).
    pub fn junction(&self, v: f64, e: f64, t: f64) -> (f64, f64) {
        let b = self.slope(t);
        let eta = v / self.cells - e;
        let scale = 2.0 * self.exchange * self.area;
        let x = eta / b;
        (
            scale * det::sinh(x),
            scale * det::cosh(x) / (b * self.cells),
        )
    }

    /// Newton's junction voltage limited: a step of the overpotential outward by more than 2b
    /// taken on the logarithm (SPICE's pnjlim, both ways: sinh grows either way), so the
    /// exponential cannot overflow.
    pub fn limit(&self, new: f64, old: f64, e: f64, t: f64) -> f64 {
        let b = self.slope(t);
        let (eta_new, eta_old) = (new / self.cells - e, old / self.cells - e);
        let step = eta_new - eta_old;
        if step.abs() > 2.0 * b && eta_new.abs() > eta_old.abs() {
            let held = eta_old + step.signum() * b * det::ln(1.0 + step.abs() / b);
            (held + e) * self.cells
        } else {
            new
        }
    }
}
