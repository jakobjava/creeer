//! Friction in pipes: the Darcy factor against the Reynolds number and the relative
//! roughness, laminar (64/Re) below Re 2000, Colebrook's (solved, not approximated) above
//! 4000, and between them a cubic that matches both its values and its slopes (Dunlop's
//! interpolation, as EPANET has it), so Newton's method never meets a jump. Each with its
//! exact slope by Re, for Newton's method (Colebrook's by its implicit derivative).

use creeer_math::det;

/// Laminar below, turbulent above.
pub(crate) const LAMINAR: f64 = 2000.0;
const TURBULENT: f64 = 4000.0;

/// Colebrook's friction factor at Reynolds number `re` and relative roughness `rr` (ε/D):
/// 1/√f = −2·log₁₀(rr/3.7 + 2.51/(Re·√f)), by Newton's method on x = 1/√f from Swamee and
/// Jain's explicit estimate (exact to rounding in four steps).
pub fn colebrook(re: f64, rr: f64) -> f64 {
    colebrook_slope(re, rr).0
}

/// Colebrook's factor and its slope df/dRe. With x = 1/√f, a = rr/3.7, b = 2.51/Re:
/// x = −2·log₁₀(a + b·x), so dx/dRe·(1 + 2b/(ln 10·(a + b·x))) = 2·x·b/(Re·ln 10·(a + b·x)),
/// and df/dRe = −2·x⁻³·dx/dRe.
pub fn colebrook_slope(re: f64, rr: f64) -> (f64, f64) {
    colebrook_from(re, rr, &mut Hint::default())
}

/// Where a pipe's last Colebrook solve ended: x = 1/√f at Reynolds number `re`, and dx/dRe
/// there. (Nought: none yet.)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hint {
    pub x: f64,
    pub re: f64,
    pub slope: f64,
}

/// As [`colebrook_slope`], Newton started from `hint`'s root carried to this Re along its
/// slope (first order: the start is off by the change squared, and one Newton step reaches
/// rounding, against four from Swamee and Jain's estimate, which costs three powers
/// besides); `hint` is left at this solve.
pub fn colebrook_from(re: f64, rr: f64, hint: &mut Hint) -> (f64, f64) {
    let ln10 = std::f64::consts::LN_10;
    let mut x = hint.x + hint.slope * (re - hint.re);
    if hint.x <= 0.0 || x <= 0.0 {
        let swamee = 0.25 / det::pow(det::log10(rr / 3.7 + 5.74 / det::pow(re, 0.9)), 2.0);
        x = 1.0 / swamee.sqrt();
    }
    let b = 2.51 / re;
    for _ in 0..8 {
        let inner = rr / 3.7 + b * x;
        let g = x + 2.0 * det::ln(inner) / ln10;
        let dg = 1.0 + 2.0 / ln10 * b / inner;
        let step = g / dg;
        x -= step;
        // Newton's next step is about this one squared over x (|g''/2g'| ≤ 1/x here): below
        // 10⁻⁷·x, the root is reached to 10⁻¹⁴, rounding.
        if step.abs() < 1e-7 * x {
            break;
        }
    }
    let inner = ln10 * (rr / 3.7 + b * x);
    let dx = 2.0 * x * b / (re * inner) / (1.0 + 2.0 * b / inner);
    *hint = Hint { x, re, slope: dx };
    (1.0 / (x * x), -2.0 / (x * x * x) * dx)
}

/// The Darcy friction factor at Reynolds number `re` (≥ 0) and relative roughness `rr`.
pub fn darcy(re: f64, rr: f64) -> f64 {
    darcy_slope(re, rr).0
}

/// The Darcy factor and its slope df/dRe.
pub fn darcy_slope(re: f64, rr: f64) -> (f64, f64) {
    darcy_from(re, rr, &mut Hint::default())
}

/// As [`darcy_slope`], Colebrook's solve started from `hint` ([`colebrook_from`]).
pub fn darcy_from(re: f64, rr: f64, hint: &mut Hint) -> (f64, f64) {
    if re <= LAMINAR {
        let re = re.max(1e-12);
        return (64.0 / re, -64.0 / (re * re));
    }
    if re >= TURBULENT {
        return colebrook_from(re, rr, hint);
    }
    // Hermite cubic on Re between the laminar law's end and Colebrook's start.
    let (f0, d0) = (64.0 / LAMINAR, -64.0 / (LAMINAR * LAMINAR));
    // (Its turbulent end from the pipe's hint too: the same for it every step, so one
    // logarithm instead of a cold solve; a household pipe flows here.)
    let (f1, d1) = colebrook_from(TURBULENT, rr, hint);
    let span = TURBULENT - LAMINAR;
    let s = (re - LAMINAR) / span;
    let (h00, h10, h01, h11) = (
        2.0 * s * s * s - 3.0 * s * s + 1.0,
        s * s * s - 2.0 * s * s + s,
        -2.0 * s * s * s + 3.0 * s * s,
        s * s * s - s * s,
    );
    let (g00, g10, g01, g11) = (
        6.0 * s * s - 6.0 * s,
        3.0 * s * s - 4.0 * s + 1.0,
        -6.0 * s * s + 6.0 * s,
        3.0 * s * s - 2.0 * s,
    );
    let f = h00 * f0 + h10 * span * d0 + h01 * f1 + h11 * span * d1;
    let slope = (g00 * f0 + g10 * span * d0 + g01 * f1 + g11 * span * d1) / span;
    (f, slope)
}
