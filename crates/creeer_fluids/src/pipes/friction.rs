//! Friction in pipes: the Darcy factor against the Reynolds number and the relative
//! roughness, laminar (64/Re) below Re 2000, Colebrook's (solved, not approximated) above
//! 4000, and between them a cubic that matches both its values and its slopes (Dunlop's
//! interpolation, as EPANET has it), so Newton's method never meets a jump.

use creeer_math::det;

/// Laminar below, turbulent above.
const LAMINAR: f64 = 2000.0;
const TURBULENT: f64 = 4000.0;

/// Colebrook's friction factor at Reynolds number `re` and relative roughness `rr` (ε/D):
/// 1/√f = −2·log₁₀(rr/3.7 + 2.51/(Re·√f)), by Newton's method on x = 1/√f from Swamee and
/// Jain's explicit estimate (exact to rounding in four steps).
pub fn colebrook(re: f64, rr: f64) -> f64 {
    let ln10 = std::f64::consts::LN_10;
    let swamee = 0.25 / det::pow(det::log10(rr / 3.7 + 5.74 / det::pow(re, 0.9)), 2.0);
    let mut x = 1.0 / swamee.sqrt();
    for _ in 0..8 {
        let inner = rr / 3.7 + 2.51 * x / re;
        let g = x + 2.0 * det::ln(inner) / ln10;
        let dg = 1.0 + 2.0 / ln10 * (2.51 / re) / inner;
        let step = g / dg;
        x -= step;
        if step.abs() < 1e-14 * x {
            break;
        }
    }
    1.0 / (x * x)
}

/// The Darcy friction factor at Reynolds number `re` (≥ 0) and relative roughness `rr`.
pub fn darcy(re: f64, rr: f64) -> f64 {
    if re <= LAMINAR {
        return 64.0 / re.max(1e-12);
    }
    if re >= TURBULENT {
        return colebrook(re, rr);
    }
    // Hermite cubic on Re between the laminar law's end and Colebrook's start.
    let (f0, d0) = (64.0 / LAMINAR, -64.0 / (LAMINAR * LAMINAR));
    let f1 = colebrook(TURBULENT, rr);
    let d1 = (colebrook(TURBULENT * 1.001, rr) - f1) / (TURBULENT * 0.001);
    let span = TURBULENT - LAMINAR;
    let s = (re - LAMINAR) / span;
    let (h00, h10, h01, h11) = (
        2.0 * s * s * s - 3.0 * s * s + 1.0,
        s * s * s - 2.0 * s * s + s,
        -2.0 * s * s * s + 3.0 * s * s,
        s * s * s - s * s,
    );
    h00 * f0 + h10 * span * d0 + h01 * f1 + h11 * span * d1
}
