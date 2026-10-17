//! What each link does to the head: its loss h(Q) along it (H_a − H_b = h(Q)), and the
//! loss's slope dh/dQ, for Newton's method.

use super::friction::{Hint, LAMINAR, darcy_from};
use super::{Link, LinkKind, Liquid};

/// The least slope (m per m³/s) a link's law is given: where it is flat at no flow (a pump
/// at shut-off, a valve's minor loss alone) Newton's system would be singular; 10⁻⁶ m per
/// m³/s is a millimetre at a litre a second, beneath anything measured.
pub(crate) const FLATTEST: f64 = 1e-6;

fn area(diameter: f64) -> f64 {
    0.25 * std::f64::consts::PI * diameter * diameter
}

/// A pipe's or a valve's loss at `q`: friction (Darcy–Weisbach) and minor losses, v|v|/2g,
/// and its exact slope by q (the friction factor's own by Re included).
fn loss(kind: &LinkKind, liquid: &Liquid, g: f64, q: f64, hint: &mut Hint) -> (f64, f64) {
    match *kind {
        LinkKind::Pipe {
            length,
            diameter,
            roughness,
            minor,
            ..
        } => {
            let a = area(diameter);
            let v = q / a;
            let per_speed = liquid.density * diameter / liquid.viscosity;
            if per_speed * v.abs() <= LAMINAR {
                // Hagen–Poiseuille, linear (its slope finite at no flow), and the fittings'.
                let r =
                    32.0 * liquid.viscosity * length / (liquid.density * g * diameter * diameter);
                let h = r * v + minor * v * v.abs() / (2.0 * g);
                return (h, (r + minor * v.abs() / g) / a);
            }
            let (f, df) = darcy_from(per_speed * v.abs(), roughness / diameter, hint);
            let k = f * length / diameter + minor;
            let h = k * v * v.abs() / (2.0 * g);
            // dh/dv = (2k|v| + (L/D)·df/dRe·(ρD/μ)·v²)/2g.
            let slope =
                (2.0 * k * v.abs() + length / diameter * df * per_speed * v * v) / (2.0 * g);
            (h, slope / a)
        }
        LinkKind::Valve {
            diameter,
            open,
            opening,
        } => {
            let a = area(diameter);
            let (v, k) = (q / a, open / (opening * opening));
            (k * v * v.abs() / (2.0 * g), k * v.abs() / (g * a))
        }
        LinkKind::Pump {
            shutoff,
            resistance,
            speed,
            ..
        } => (
            -(speed * speed * shutoff - resistance * q * q.abs()),
            2.0 * resistance * q.abs(),
        ),
        LinkKind::Turbine {
            swirl,
            radius,
            loss,
            speed,
        } => (
            speed * (swirl * q - radius * radius * speed) / g + loss * q * q.abs(),
            speed * swirl / g + 2.0 * loss * q.abs(),
        ),
    }
}

/// Link `link`'s head loss at flow `q` and its slope.
pub(crate) fn law(link: &Link, liquid: &Liquid, g: f64, q: f64) -> (f64, f64) {
    law_from(link, liquid, g, q, &mut Hint::default())
}

/// As [`law`], its friction factor's solve started from `hint` (1/√f a step ago), left
/// with this one's.
pub(crate) fn law_from(
    link: &Link,
    liquid: &Liquid,
    g: f64,
    q: f64,
    hint: &mut Hint,
) -> (f64, f64) {
    let (h, slope) = loss(&link.kind, liquid, g, q, hint);
    (h, slope.max(FLATTEST))
}

/// Whether a link only lets its liquid forward (a check valve; a pump, whose impeller would
/// otherwise turn backwards).
pub(crate) fn one_way(link: &Link) -> bool {
    matches!(
        link.kind,
        LinkKind::Pipe { check: true, .. } | LinkKind::Pump { .. }
    )
}

/// Whether a link can carry anything (a valve shut cannot).
pub(crate) fn passable(link: &Link) -> bool {
    !matches!(link.kind, LinkKind::Valve { opening, .. } if opening <= 0.0)
}

/// A pump's efficiency at flow `q`: η_best at the best flow (scaled with its speed),
/// falling parabolically to nought at no flow and at twice the best.
pub fn efficiency(kind: &LinkKind, q: f64) -> f64 {
    let LinkKind::Pump {
        speed,
        best_flow,
        best_efficiency,
        ..
    } = *kind
    else {
        return 1.0;
    };
    let x = q / (best_flow * speed.max(1e-9));
    (best_efficiency * x * (2.0 - x)).max(0.02)
}
