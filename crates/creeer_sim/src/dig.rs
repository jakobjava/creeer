//! Digging (G1.5, ADR-0032): one law for every tool and every ground. A blow breaks out the
//! volume its energy pays for at the ground's specific energy of excavation, V = η·E / e_s, and
//! wears the head that struck by abrasion against the ground's grains.
//!
//! - **e_s, solid rock:** its uniaxial compressive strength, the least specific energy of rock
//!   cutting (Teale 1965: the minimum specific energy of drilling approaches the compressive
//!   strength; Rabia 1985).
//! - **e_s, loose ground:** the passive resistance of soil ahead of a tool, Mohr–Coulomb's
//!   2c·√K_p + ½·γ·d·K_p with K_p = tan²(45° + φ/2) over a tool's bite d (Reece 1964's
//!   earthmoving equation; McKyes 1985: tillage tools' specific draft, 20–100 kPa in loam).
//! - **η:** at best 0.35, the mechanical efficiency of rock cutting at its best (Teale 1965:
//!   30–40 %); lessened by a blunt tip (half lost at a 2 mm radius: a point or an edge bites, a
//!   round face crushes) and, on solid rock, by a head softer than the rock's grains, which
//!   flattens rather than breaks them (as (H_tool/H_rock)², not measured: ADR-0032).
//! - **Wear:** Archard's abrasive wear V = K·W/H (Archard 1953; Rabinowicz 1995: K ≈ 10⁻³–10⁻²
//!   for three-body abrasion), each mineral abrading by its share and by its hardness against
//!   the head's: fully when harder than 1.2× the head's, a fiftieth when softer than 0.8×
//!   (Kruschov 1957; Richardson 1968: the soft-abrasive regime), and as the ground's strength
//!   (Plinninger 2002's rock abrasivity index, quartz content × strength, predicts tool wear).

use crate::strike::{self, Swing};
use creeer_math::det;
use creeer_terrain::geology::rocks::Strength;
use std::f64::consts::FRAC_PI_4;

/// The best mechanical efficiency of breaking rock.
pub const EFFICIENCY: f64 = 0.35;
/// The tip radius that halves it (m).
pub const BLUNT: f64 = 0.002;
/// How deep a hand tool bites loose ground (m).
pub const BITE: f64 = 0.1;
/// Archard's coefficient for a head's wear on the ground's grains (calibrated: a steel pick's
/// point loses about 0.2 mm³ a blow on granite, a few cm³ over a working day).
pub const WEAR: f64 = 3e-3;
/// The strength wear is scaled to (Pa).
const REFERENCE: f64 = 100e6;
/// The soft-abrasive regime: wear a fiftieth below 0.8 of the head's hardness, full above 1.2.
const SOFT: f64 = 0.02;
const SOFT_BELOW: f64 = 0.8;
const HARD_ABOVE: f64 = 1.2;

/// The fingertips' radius (m): bare hands dig as a blunt tip.
pub const FINGERS: f64 = 0.008;

/// Bare hands' swing into loose ground: the forearm and hand swung about the elbow, 2.2 % of an
/// 80 kg body, its centre 68.2 % and its radius of gyration 82.7 % of its 45 cm out (Winter
/// 2009's segment table, after Dempster 1955).
pub fn hands() -> Swing {
    let length = 0.45;
    strike::swing_of(0.022 * 80.0, 0.827 * length, 0.682 * length, length)
}

/// The specific energy of excavating ground of `strength` and bulk `density` (kg/m³) under
/// gravity `g` (m/s²): J/m³.
pub fn excavation_energy(strength: Strength, density: f64, g: f64) -> f64 {
    match strength {
        Strength::Solid { compressive } => compressive,
        Strength::Loose { cohesion, friction } => {
            let t = det::tan(FRAC_PI_4 + 0.5 * friction);
            let kp = t * t;
            2.0 * cohesion * kp.sqrt() + 0.5 * density * g * BITE * kp
        }
    }
}

/// The volume-weighted mean of `minerals`' hardnesses ((hardness, fraction) pairs).
pub fn mean_hardness(minerals: impl Iterator<Item = (f64, f64)>) -> f64 {
    minerals.map(|(h, f)| h * f).sum()
}

/// The share of a blow's energy that breaks ground: with a tip of radius `tip` (m), a head of
/// hardness `head` (Pa) against a ground of mean hardness `ground` (Pa), `solid` or loose (the
/// grains of loose ground are moved, not broken: the head's hardness does not matter).
pub fn efficiency(tip: f64, head: f64, ground: f64, solid: bool) -> f64 {
    let sharp = BLUNT / (BLUNT + tip.max(0.0));
    let hard = if solid {
        let h = head / ground;
        (h * h).min(1.0)
    } else {
        1.0
    };
    EFFICIENCY * sharp * hard
}

/// The volume a blow of `energy` (J) breaks out at `efficiency` of ground of specific energy
/// `excavation` (J/m³): m³.
pub fn dug(energy: f64, efficiency: f64, excavation: f64) -> f64 {
    if excavation <= 0.0 {
        return 0.0;
    }
    (energy * efficiency / excavation).max(0.0)
}

/// The peak force on a head (N) whose blow of `energy` (J) broke out `volume` (m³): the energy
/// spent over the depth it went in (the side of what it broke out, from a millimetre on hard
/// rock to a tool's bite in soil), never more than its whole impulse `impulse` (N·s) stopped in
/// `contact` seconds (on rock that does not give).
pub fn peak_force(energy: f64, volume: f64, (impulse, contact): (f64, f64)) -> f64 {
    let depth = det::cbrt(volume.max(0.0)).clamp(1e-3, BITE);
    (energy / depth).min(impulse / contact)
}

/// How much one mineral of hardness `mineral` abrades a head of hardness `head` (0.02–1).
pub fn abrasion(mineral: f64, head: f64) -> f64 {
    let r = mineral / head;
    if r <= SOFT_BELOW {
        SOFT
    } else if r >= HARD_ABOVE {
        1.0
    } else {
        // Log-linear between the regimes.
        let t = det::ln(r / SOFT_BELOW) / det::ln(HARD_ABOVE / SOFT_BELOW);
        det::pow(SOFT, 1.0 - t)
    }
}

/// The volume a head of hardness `head` (Pa) loses to a blow of `energy` (J) on ground of
/// `minerals` ((hardness, volume fraction) pairs) and specific energy `excavation` (J/m³): m³.
pub fn wear(
    energy: f64,
    head: f64,
    minerals: impl Iterator<Item = (f64, f64)>,
    excavation: f64,
) -> f64 {
    let abrasive: f64 = minerals.map(|(h, f)| f * abrasion(h, head)).sum();
    WEAR * abrasive * (excavation / REFERENCE) * energy.max(0.0) / head
}

/// The tip radius of an edge of included angle `angle` (rad) and `length` (m) that has lost
/// `lost` m³ along it: half the flat worn on it (a wedge cut back by w loses w²/(4·tan(θ/2)) a
/// metre).
pub fn worn_tip(angle: f64, length: f64, lost: f64) -> f64 {
    if length <= 0.0 {
        return 0.0;
    }
    let w = (4.0 * det::tan(0.5 * angle) * lost.max(0.0) / length).sqrt();
    0.5 * w
}

#[cfg(test)]
#[path = "dig_tests.rs"]
mod tests;
