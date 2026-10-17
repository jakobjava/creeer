//! A panel's force's slope by the air's velocity over it, analytically (its Mach number and
//! friction coefficient held, as `linearised` holds them).
//!
//! With u the air's velocity relative to the panel, s = |u|, n its normal, c its chord,
//! ŝ = c × n its span, w = u × ŝ (the flow square to the span) and α = atan2(u·n, −u·c):
//!
//! F = ½ρA·[C_D(α)·s·u + C_L(α)·s²·ŵ] + ρ·A·C_f·s·P·u,  P = I − n·nᵀ,
//!
//! dα/du = (−(u·c)·n + (u·n)·c)/((u·n)² + (u·c)²), dŵ/du = −(I − ŵŵᵀ)·[ŝ]×/|w|.

use super::coefficients::{Section, lift_drag};
use creeer_math::glam::{DMat3, DVec3};

fn outer(a: DVec3, b: DVec3) -> DMat3 {
    DMat3::from_cols(a * b.x, a * b.y, a * b.z)
}

fn cross(r: DVec3) -> DMat3 {
    DMat3::from_cols(
        DVec3::new(0.0, r.z, -r.y),
        DVec3::new(-r.z, 0.0, r.x),
        DVec3::new(r.y, -r.x, 0.0),
    )
}

/// The panel's shape: area, aspect ratio, span efficiency, section; its normal and chord in
/// the world.
pub(super) struct Panel<'a> {
    pub area: f64,
    pub aspect: f64,
    pub efficiency: f64,
    pub section: &'a Section,
    pub n: DVec3,
    pub c: DVec3,
}

/// The force F and dF/du at `u` (≠ 0) in air of `density`, Mach number `mach` and friction
/// coefficient `cf`.
pub(super) fn slope(p: &Panel<'_>, u: DVec3, density: f64, mach: f64, cf: f64) -> (DVec3, DMat3) {
    let s = u.length();
    let (un, uc) = (u.dot(p.n), u.dot(p.c));
    let alpha = creeer_math::det::atan2(un, -uc);
    let (cl, cd) = lift_drag(p.section, p.aspect, p.efficiency, alpha, mach);
    // The coefficients' slopes by α (one-sided: 10⁻⁷ of a radian, its error as small).
    let delta = 1e-7;
    let above = lift_drag(p.section, p.aspect, p.efficiency, alpha + delta, mach);
    let (dcl, dcd) = ((above.0 - cl) / delta, (above.1 - cd) / delta);
    let plane = un * un + uc * uc;
    let grad = if plane > 0.0 {
        (p.n * -uc + p.c * un) / plane
    } else {
        DVec3::ZERO
    };
    let half = 0.5 * density * p.area;
    let along = u / s;
    // Drag.
    let mut j =
        (DMat3::IDENTITY * s + outer(u, along)) * (half * cd) + outer(u, grad) * (half * s * dcd);
    // Lift.
    let span = p.c.cross(p.n);
    let w = u.cross(span);
    let wl = w.length();
    let wh = if wl > 0.0 { w / wl } else { DVec3::ZERO };
    if wl > 0.0 {
        let turn = (DMat3::IDENTITY - outer(wh, wh)) * cross(span) * (-1.0 / wl);
        j += (outer(wh, grad) * (s * s * dcl) + outer(wh, u) * (2.0 * cl) + turn * (s * s * cl))
            * half;
    }
    // Friction on both faces.
    let plane_only = DMat3::IDENTITY - outer(p.n, p.n);
    let tangential = plane_only * u;
    j += (outer(tangential, along) + plane_only * s) * (density * p.area * cf);
    let f = (wh * cl * s * s + u * (cd * s)) * half + tangential * (density * s * cf * p.area);
    (f, j)
}
