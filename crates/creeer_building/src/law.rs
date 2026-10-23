//! A fastened joint's law (`creeer_physics::structure::JointLaw`) from its fasteners and the
//! members they join (G2.2, ADR-0033): Eurocode 5's rules for dowel-type fasteners (nails,
//! pegs), a friction clamp for lashings.
//!
//! EN 1995-1-1 is written in N and mm with characteristic values for design. A game wants the
//! joint as it is, not as it may be designed: the rules here take the woods' mean densities
//! and drop the code's safety-related factors (1.05 and 1.15 in eq. 8.6), so they give mean
//! capacities.

use creeer_math::det::pow;
use creeer_physics::structure::JointLaw;

/// One member at the joint, as the fasteners meet it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Side {
    /// Its wood's density as EN 1995-1-1 takes it, at 12 % moisture (kg/m³).
    pub density: f64,
    /// Its wood's compressive strength across the grain (Pa) and stiffness there (Pa).
    pub across: f64,
    pub across_stiffness: f64,
    /// How thick it is along the fasteners (m).
    pub thickness: f64,
    /// Its stiffness-proportional damping (s).
    pub damping: f64,
}

/// How the fasteners sit: how many, how far out from their middle (Σ r², m²), and the area
/// the two members bear on each other (m²).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub count: u32,
    pub spread: f64,
    pub contact: f64,
}

/// A dowel-type fastener: a nail (driven, smooth) or a peg (in a bored hole).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dowel {
    /// Diameter and length (m).
    pub diameter: f64,
    pub length: f64,
    /// Its own material's bending strength (Pa): a nail's wire tensile strength f_u, a peg's
    /// bending strength f_m.
    pub strength: f64,
    /// Driven without a bored hole (a nail) or in one (a peg).
    pub driven: bool,
    /// For a nail, its head's diameter (m: what pulls through).
    pub head: f64,
    /// Its slip at its peak and at failure (m).
    pub peak_slip: f64,
    pub failure_slip: f64,
}

/// A lashing: a strip or cord wound round both members.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lashing {
    /// Its cross-section (m²), tensile strength (Pa) and stiffness (Pa).
    pub area: f64,
    pub strength: f64,
    pub stiffness: f64,
    /// Turns round the members, and what share of its strength it is drawn to by hand.
    pub wraps: u32,
    pub pretension: f64,
    /// Friction between the members it clamps.
    pub friction: f64,
    /// Its slip at its peak and at failure (m).
    pub peak_slip: f64,
    pub failure_slip: f64,
}

/// The least pointside penetration that holds at all, in diameters: a nail's (EN 1995-1-1
/// §8.3.1.2(4): 8·d for full lateral capacity, below 6·d none), and a peg's, which runs
/// through the parts and needs only to reach past the shear plane by its own thickness.
const LEAST_PENETRATION: f64 = 6.0;
const LEAST_PEG_PENETRATION: f64 = 1.0;

/// Johansen's yield modes for one fastener in single shear, timber to timber (EN 1995-1-1
/// eq. 8.6 without its 1.05 and 1.15): embedment strengths `f1`, `f2` (N/mm²), penetrations
/// `t1`, `t2` (mm), diameter `d` (mm), yield moment `my` (N·mm). Returns the least, and
/// whether a mode with the fastener bent (c to f) governs, where the rope effect adds.
fn johansen(f1: f64, f2: f64, t1: f64, t2: f64, d: f64, my: f64) -> (f64, bool) {
    let b = f2 / f1;
    let r = t2 / t1;
    let a = f1 * t1 * d;
    let bb = f2 * t2 * d;
    let c = a / (1.0 + b)
        * ((b + 2.0 * b * b * (1.0 + r + r * r) + b * b * b * r * r).sqrt() - b * (1.0 + r));
    let dm = a / (2.0 + b)
        * ((2.0 * b * (1.0 + b) + 4.0 * b * (2.0 + b) * my / (f1 * d * t1 * t1)).sqrt() - b);
    let e = f1 * t2 * d / (1.0 + 2.0 * b)
        * ((2.0 * b * b * (1.0 + b) + 4.0 * b * (1.0 + 2.0 * b) * my / (f1 * d * t2 * t2)).sqrt()
            - b);
    let f = (2.0 * b / (1.0 + b)).sqrt() * (2.0 * my * f1 * d).sqrt();
    let rigid = a.min(bb);
    let bent = c.min(dm).min(e).min(f);
    (rigid.min(bent), bent < rigid)
}

/// What every joint shares: bearing, its axial stiffness and its damping, from the sides.
fn bearing(a: &Side, b: &Side, at: &Placement, between: f64) -> (f64, f64, f64) {
    let strength = a.across.min(b.across) * at.contact;
    // The two members pressed across their grain in series, over the distance between axes.
    let e = 1.0 / (1.0 / a.across_stiffness + 1.0 / b.across_stiffness);
    (strength, e * at.contact / between, a.damping.max(b.damping))
}

/// The law of `count` dowels through `a` (head side) into `b`, `between` (m) apart on their
/// axes, or why they cannot hold.
///
/// # Errors
/// If a dowel is too short to reach far enough into `b`.
pub fn dowels(
    dowel: &Dowel,
    a: &Side,
    b: &Side,
    at: &Placement,
    between: f64,
) -> Result<JointLaw, String> {
    let d = dowel.diameter * 1e3;
    let t1 = a.thickness * 1e3;
    let t2 = (dowel.length - a.thickness).min(b.thickness) * 1e3;
    let least = if dowel.driven {
        LEAST_PENETRATION
    } else {
        LEAST_PEG_PENETRATION
    } * d;
    if t2 < least {
        return Err(format!(
            "reaches {t2:.0} mm into the second part, {least:.0} mm needed"
        ));
    }
    // Embedment (EN 1995-1-1 eq. 8.15, 8.32), yield moment (8.14; a peg's plastic moment).
    let embed = |rho: f64| {
        if dowel.driven {
            0.082 * rho * pow(d, -0.3)
        } else {
            0.082 * (1.0 - 0.01 * d) * rho
        }
    };
    let (f1, f2) = (embed(a.density), embed(b.density));
    let fu = dowel.strength * 1e-6;
    let my = if dowel.driven {
        0.3 * fu * pow(d, 2.6)
    } else {
        fu * d * d * d / 6.0
    };
    let (yield_one, bent) = johansen(f1, f2, t1, t2, d, my);
    // Withdrawal (§8.3.2): smooth nails 20·10⁻⁶·ρ² over the pointside penetration, full from
    // 12·d, (t/4d − 2) of it from 8·d; head pull-through 70·10⁻⁶·ρ²·d_h². Pegs hold by friction
    // in their holes only: a tenth of their slip capacity (an estimate; EN 1995 gives none).
    let withdrawal_one = if dowel.driven {
        let reach = (t2 / (4.0 * d) - 2.0).clamp(0.0, 1.0);
        let pull = 20e-6 * b.density * b.density * d * t2 * reach;
        let head = 70e-6 * a.density * a.density * (dowel.head * 1e3) * (dowel.head * 1e3);
        pull.min(head).max(1.0)
    } else {
        0.1 * yield_one
    };
    // The rope effect (§8.2.2(2)): a quarter of the withdrawal, at most 15 % of Johansen's for
    // smooth nails, none for dowels.
    let rope = if dowel.driven && bent {
        (withdrawal_one / 4.0).min(0.15 * yield_one)
    } else {
        0.0
    };
    // Slip moduli (Table 7.1), per shear plane and fastener; between two woods, ρ_m = √(ρ₁ρ₂).
    let rho = (a.density * b.density).sqrt();
    let k_one = if dowel.driven {
        pow(rho, 1.5) * pow(d, 0.8) / 30.0
    } else {
        pow(rho, 1.5) * d / 23.0
    } * 1e3;
    let n = f64::from(at.count);
    let (strength, axial, damping) = bearing(a, b, at, between);
    let slip_k = n * k_one;
    // Turning: each fastener's slip at its radius, and its own diameter as the least arm.
    let arm2 = at.spread + n * dowel.diameter * dowel.diameter;
    let lateral_yield = n * yield_one;
    Ok(JointLaw {
        axial,
        bearing: strength,
        withdrawal: n * withdrawal_one,
        lateral: slip_k,
        lateral_yield,
        lateral_peak: lateral_yield + n * rope,
        slip_at_peak: dowel
            .peak_slip
            .max(1.5 * (lateral_yield + n * rope) / slip_k),
        slip_at_failure: dowel
            .failure_slip
            .max(2.0 * (lateral_yield + n * rope) / slip_k),
        twist: k_one * arm2,
        twist_yield: yield_one * arm2.sqrt() * n.sqrt(),
        interaction: if dowel.driven { 1.0 } else { 2.0 },
        damping,
    })
}

/// The law of a lashing round `a` and `b`, `between` (m) apart on their axes: a clamp. Its
/// turns press the members together with twice their tension each; they slip by friction on
/// that, from the hand-drawn tension at first, up to the cord's strength as slipping draws it
/// tight; pulled apart, the cord breaks.
pub fn lashing(cord: &Lashing, a: &Side, b: &Side, at: &Placement, between: f64) -> JointLaw {
    let strands = 2.0 * f64::from(cord.wraps);
    let breaks = cord.strength * cord.area;
    let (strength, axial, damping) = bearing(a, b, at, between);
    // A turn's length: round both members, about twice their depths and the gap.
    let around = 2.0 * (a.thickness + b.thickness) + 2.0 * between;
    let stretch = strands * cord.stiffness * cord.area / around;
    let lateral_yield = cord.friction * strands * cord.pretension * breaks;
    let lateral_peak = cord.friction * strands * breaks;
    let radius2 = at.contact / std::f64::consts::PI;
    JointLaw {
        axial,
        bearing: strength,
        withdrawal: strands * breaks,
        lateral: stretch,
        lateral_yield,
        lateral_peak,
        slip_at_peak: cord.peak_slip.max(1.5 * lateral_peak / stretch),
        slip_at_failure: cord.failure_slip.max(2.0 * lateral_peak / stretch),
        twist: stretch * radius2,
        twist_yield: lateral_yield * radius2.sqrt(),
        interaction: 2.0,
        damping,
    }
}
