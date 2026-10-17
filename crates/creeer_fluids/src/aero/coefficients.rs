//! Aerodynamic coefficients: a lifting surface's lift and drag at any angle of attack and
//! Mach number, skin friction, a sphere's drag.
//!
//! - **Attached flow:** C_L = a·(α − α₀), the finite wing's slope a from its section's a₀
//!   by Helmbold's formula (lifting-line theory, right for low aspect ratios too), induced
//!   drag C_L²/(π·e·A). Compressible: Prandtl–Glauert's 1/√(1 − M²) below Mach 0.7.
//! - **Stalled:** a flat plate's normal force C_N = C_N90·sin α (Hoerner), C_N90 by Viterna
//!   and Corrigan's 1.11 + 0.018·A; blended in over 2° past the stall.
//! - **Supersonic:** Ackeret's C_N = 4·sin α/√(M² − 1), capped at the stagnation pressure
//!   behind a normal shock (Rayleigh's pitot formula), and modified Newtonian
//!   C_N = C_p,max·sin² α where that is more (steep faces, hypersonic flight); the transonic
//!   gap between 0.7 and 1.3 is bridged linearly (no theory there is simple).
//! - **Skin friction:** Blasius's laminar 1.328/√Re to 5·10⁵, then Prandtl–Schlichting's
//!   turbulent law with its transition term; compressible by (1 + 0.144·M²)^−0.65.
//! - **Spheres:** Morrison's correlation of the standard drag curve (Stokes to the drag
//!   crisis, Re < 10⁶); at speed the measured rise to ≈ 1 through the sound barrier.

use creeer_math::det;
use std::f64::consts::PI;

/// The ratio of specific heats taken for air (diatomic; a CO₂ atmosphere's is 1.29, which
/// shifts its shock and heating terms a few per cent).
pub const GAMMA: f64 = 1.4;

/// A lifting section: its two-dimensional lift slope (per rad; 2π thin-airfoil theory),
/// zero-lift angle (rad; negative for a cambered section), stall angle from α₀ (rad), form
/// drag at zero lift (besides friction), and whether it is the same either way round (a
/// flat plate, a symmetric fin: flown backwards it lifts as forwards).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub lift_slope: f64,
    pub zero_lift: f64,
    pub stall: f64,
    pub form_drag: f64,
    pub symmetric: bool,
}

impl Section {
    /// A thin flat plate: 2π, stalling at 8° (measured 7–9° for sharp-edged plates at
    /// Re ≈ 10⁵–10⁶), no form drag at zero lift.
    pub const PLATE: Self = Self {
        lift_slope: 2.0 * PI,
        zero_lift: 0.0,
        stall: 8.0 * PI / 180.0,
        form_drag: 0.0,
        symmetric: true,
    };
    /// A cambered wing section like NACA 2412: slope 0.105 per degree, α₀ −2.1°, stalling
    /// 16° above it, form drag 0.002 (Abbott and von Doenhoff, Re 6·10⁶; its 0.006 minimum
    /// drag less the turbulent friction counted apart).
    pub const NACA_2412: Self = Self {
        lift_slope: 0.105 * 180.0 / PI,
        zero_lift: -2.1 * PI / 180.0,
        stall: 16.0 * PI / 180.0,
        form_drag: 0.002,
        symmetric: false,
    };
}

/// A finite wing's lift slope (per rad) from its section's `a0` at aspect ratio `aspect`
/// (Helmbold): a = a₀ / (√(1 + (a₀/πA)²) + a₀/πA).
pub fn wing_slope(a0: f64, aspect: f64) -> f64 {
    let k = a0 / (PI * aspect);
    a0 / ((1.0 + k * k).sqrt() + k)
}

fn smooth_step(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The stagnation pressure coefficient behind a normal shock at Mach `m` > 1 (Rayleigh):
/// 1.839 hypersonic for γ = 1.4.
pub fn stagnation_cp(m: f64) -> f64 {
    let (g, m2) = (GAMMA, m * m);
    let ratio = det::pow(
        (g + 1.0) * (g + 1.0) * m2 / (4.0 * g * m2 - 2.0 * (g - 1.0)),
        g / (g - 1.0),
    ) * (1.0 - g + 2.0 * g * m2)
        / (g + 1.0);
    2.0 / (g * m2) * (ratio - 1.0)
}

/// Lift and drag coefficients (without friction) of a surface of `section` and `aspect` at
/// angle of attack `alpha` ∈ (−π, π] and Mach `mach`; `efficiency` is its span efficiency
/// (Oswald's e).
pub fn lift_drag(
    section: &Section,
    aspect: f64,
    efficiency: f64,
    alpha: f64,
    mach: f64,
) -> (f64, f64) {
    // A symmetric section flown backwards is the same section turned round: its angle is
    // π − α, its lift reversed (the lift's direction turns with the chord's).
    if section.symmetric && alpha.abs() > 0.5 * PI {
        let (cl, cd) = lift_drag(
            section,
            aspect,
            efficiency,
            alpha.signum() * PI - alpha,
            mach,
        );
        return (-cl, cd);
    }
    let subsonic = |m: f64| subsonic(section, aspect, efficiency, alpha, m);
    if mach <= 0.7 {
        return subsonic(mach);
    }
    let supersonic = |m: f64| supersonic(alpha, m, section.form_drag);
    if mach >= 1.3 {
        return supersonic(mach);
    }
    let w = (mach - 0.7) / 0.6;
    let (a, b) = (subsonic(0.7), supersonic(1.3));
    ((1.0 - w) * a.0 + w * b.0, (1.0 - w) * a.1 + w * b.1)
}

fn subsonic(section: &Section, aspect: f64, efficiency: f64, alpha: f64, mach: f64) -> (f64, f64) {
    let beta = (1.0 - mach * mach).sqrt();
    let a = wing_slope(section.lift_slope / beta, aspect);
    let attached_cl = a * (alpha - section.zero_lift);
    let attached_cd = section.form_drag + attached_cl * attached_cl / (PI * efficiency * aspect);
    let (sin, cos) = det::sin_cos(alpha);
    let normal = (1.11 + 0.018 * aspect.min(50.0)) * sin;
    let (plate_cl, plate_cd) = (normal * cos, normal * sin + section.form_drag);
    let w = smooth_step(((alpha - section.zero_lift).abs() - section.stall) / (2.0 * PI / 180.0));
    (
        (1.0 - w) * attached_cl + w * plate_cl,
        (1.0 - w) * attached_cd + w * plate_cd,
    )
}

fn supersonic(alpha: f64, mach: f64, form_drag: f64) -> (f64, f64) {
    let (sin, cos) = det::sin_cos(alpha);
    let cp = stagnation_cp(mach);
    let ackeret = (4.0 * sin.abs() / (mach * mach - 1.0).sqrt()).min(cp);
    let normal = ackeret.max(cp * sin * sin) * sin.signum();
    (normal * cos, normal * sin + form_drag)
}

/// A flat plate's mean skin-friction coefficient at Reynolds number `re` (on its length) and
/// Mach `mach`.
pub fn skin_friction(re: f64, mach: f64) -> f64 {
    let re = re.max(1.0);
    let incompressible = if re < 5e5 {
        1.328 / re.sqrt()
    } else {
        let lg = det::log10(re);
        (0.455 / det::pow(lg, 2.58) - 1700.0 / re).max(1.328 / re.sqrt())
    };
    incompressible * det::pow(1.0 + 0.144 * mach * mach, -0.65)
}

/// A sphere's drag coefficient at Reynolds number `re` (on its diameter) and Mach `mach`.
pub fn sphere_drag(re: f64, mach: f64) -> f64 {
    let re = re.max(1e-9);
    let a = re / 5.0;
    let b = re / 263_000.0;
    let c = re / 1e6;
    let viscous = 24.0 / re
        + 2.6 * a / (1.0 + det::pow(a, 1.52))
        + 0.411 * det::pow(b, -7.94) / (1.0 + det::pow(b, -8.0))
        + 0.25 * c / (1.0 + c);
    // Measured at high Reynolds numbers (Bailey and Hiatt, AIAA J. 10, 1972), Mach to drag.
    const BY_MACH: [(f64, f64); 12] = [
        (0.5, 0.0),
        (0.6, 0.52),
        (0.8, 0.60),
        (0.9, 0.68),
        (1.0, 0.82),
        (1.1, 0.92),
        (1.2, 0.98),
        (1.5, 1.03),
        (2.0, 1.02),
        (3.0, 0.97),
        (5.0, 0.93),
        (10.0, 0.92),
    ];
    let by_mach = if mach <= 0.5 {
        0.0
    } else if mach >= 10.0 {
        0.92
    } else {
        let k = BY_MACH
            .windows(2)
            .position(|w| mach <= w[1].0)
            .unwrap_or(BY_MACH.len() - 2);
        let ((m0, c0), (m1, c1)) = (BY_MACH[k], BY_MACH[k + 1]);
        c0 + (c1 - c0) * (mach - m0) / (m1 - m0)
    };
    viscous.max(by_mach)
}
