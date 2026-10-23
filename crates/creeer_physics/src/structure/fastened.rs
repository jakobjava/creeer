//! Fastened joints (G2.1, ADR-0033): what nails, pegs, lashings (and later bolts and welds)
//! make of two members that touch.
//!
//! A fastener runs from one member's axis to the other's, across the faces where they touch,
//! and it is a short member itself: an [`Element`] between a node of each, of length L (the
//! distance between the two axes), whose section and material are made equivalent to the
//! fastener's law ([`JointLaw`]):
//!
//! - **slip across the joint** is the short member's shear: a lateral offset δ of its ends
//!   gives the force 12·EI/(L³·(1 + Φ))·δ, which for a member that shear dominates
//!   (Φ = 12·EI/(κ·G·A·L²) = [`SHEAR_DOMINATED`]) is κ·G·A/L·(1 + 1/Φ)⁻¹ ≈ the slip modulus.
//!   Its end moments are F·L/2, so the members' plastic hinges (`plastic.rs`) cap F at the
//!   fastener's yield, harden it to its peak and break it at its last slip: Johansen's
//!   ductile modes (EN 1995-1-1 §8.2) as the members' own plasticity;
//! - **along the fastener** the joint presses on the wood (bearing, stiff and strong) and
//!   pulls the fastener out (withdrawal): a capacity of its own, brittle, met with the slip
//!   force by EN 1995-1-1 §8.3.3's interaction (`Fastened::withdrawn`);
//! - **turning about the fastener** resists by its stiffness and yields at its torque: one
//!   nail is nearly a pin, a group of n holds Σ K·r².
//!
//! The joint has no mass (a nail's few grams are the members'), never buckles, and draws and
//! collides as nothing (`extent` 0).

use super::element::{Element, MEASURES};
use super::section::{Material, RateSensitivity, Section};
use creeer_math::glam::DQuat;

/// The equivalent member's Φ: shear dominates its lateral stiffness a hundredfold, so it
/// slips as a joint, not as a bent stub (the bending share, 1/Φ, is 1 %).
pub const SHEAR_DOMINATED: f64 = 100.0;

/// The equivalent material's yield stress (Pa): any scale works, the section takes the rest.
const SIGMA: f64 = 1.0e8;

/// What a fastened joint does, from its fasteners (all of them together).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointLaw {
    /// Along the fastener: stiffness (N/m), bearing capacity in compression (N) and
    /// withdrawal capacity in tension (N).
    pub axial: f64,
    pub bearing: f64,
    pub withdrawal: f64,
    /// Across it: slip modulus (N/m), yield, peak (N), slip at the peak and at failure (m).
    pub lateral: f64,
    pub lateral_yield: f64,
    pub lateral_peak: f64,
    pub slip_at_peak: f64,
    pub slip_at_failure: f64,
    /// About it: stiffness (N·m/rad) and the torque it yields at (N·m).
    pub twist: f64,
    pub twist_yield: f64,
    /// Withdrawal and slip interact as (F_ax/R_ax)^e + (F_la/R_la)^e ≤ 1 (EN 1995-1-1
    /// §8.3.3: e = 1 for smooth nails, 2 for others).
    pub interaction: f64,
    /// Stiffness-proportional damping c (s), as members' (`Material::damping`).
    pub damping: f64,
}

/// What a joint member keeps beyond a member's: its withdrawal and slip capacities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fastened {
    /// Withdrawal capacity (N).
    pub withdrawal: f64,
    /// Peak slip force (N).
    pub lateral: f64,
    /// The interaction exponent e.
    pub interaction: f64,
    /// b's rest frame relative to a's (its node turned apart from a's when the joint was
    /// made, `Structure::join`): the joint's axes at b are these, turned.
    pub to_b: DQuat,
}

impl JointLaw {
    /// The section and material of the equivalent member of length `length` (m), and what it
    /// keeps of the law.
    ///
    /// # Panics
    /// If the law is not physical (stiffnesses and capacities positive, the peak at or above
    /// the yield, failure at or past the peak's slip).
    pub fn equivalent(&self, length: f64) -> (Section, Material, Fastened) {
        assert!(
            length > 0.0
                && self.axial > 0.0
                && self.lateral > 0.0
                && self.twist > 0.0
                && self.bearing > 0.0
                && self.withdrawal > 0.0
                && self.lateral_yield > 0.0
                && self.lateral_peak >= self.lateral_yield
                && self.twist_yield > 0.0
                && self.slip_at_failure >= self.slip_at_peak,
            "a joint's law must be physical: {self:?}"
        );
        let area = self.bearing / SIGMA;
        let youngs = self.axial * length / area;
        let shear = self.lateral * length / area * (1.0 + 1.0 / SHEAR_DOMINATED);
        let i = SHEAR_DOMINATED * shear * area * length * length / (12.0 * youngs);
        // Plastic slip δ_p turns both ends' hinges by δ_p/L, each straining by half of that
        // (`Element::per_unit`): δ_p/L in all.
        let plastic = |slip: f64| ((slip - self.lateral_peak / self.lateral) / length).max(0.0);
        let uniform = plastic(self.slip_at_peak);
        let section = Section {
            area,
            i_y: i,
            i_z: i,
            torsion: self.twist * length / shear,
            shear_factor: [1.0; 2],
            extent: [0.0; 2],
            plastic_modulus: [self.lateral_yield * length / (2.0 * SIGMA); 2],
            plastic_torsion: self.twist_yield * 3f64.sqrt() / SIGMA,
            perimeter: 0.0,
        };
        let material = Material {
            youngs_modulus: youngs,
            shear_modulus: shear,
            density: 0.0,
            yield_strength: SIGMA,
            ultimate_strength: SIGMA * self.lateral_peak / self.lateral_yield,
            damping: self.damping,
            elongation: plastic(self.slip_at_failure).max(uniform),
            hardening_start: 0.0,
            uniform_elongation: uniform,
            rate: RateSensitivity::NONE,
        };
        let kept = Fastened {
            withdrawal: self.withdrawal,
            lateral: self.lateral_peak,
            interaction: self.interaction,
            to_b: DQuat::IDENTITY,
        };
        (section, material, kept)
    }
}

impl Fastened {
    /// Whether a joint member of length `length` with forces `force` (its measures' K·s) has
    /// its fasteners pulled out: tension along it against withdrawal, with the slip force.
    pub(crate) fn withdrawn(&self, force: &[f64; MEASURES], length: f64) -> bool {
        let pull = force[0];
        if pull <= 0.0 {
            return false;
        }
        // Each plane's shear is its end moments' sum over the length.
        let (vy, vz) = (
            (force[2] + force[3]) / length,
            (force[4] + force[5]) / length,
        );
        let slip = (vy * vy + vz * vz).sqrt();
        let e = self.interaction;
        // Beyond the interaction, not on it: a joint slipping at its peak (its ratio 1, to
        // rounding) holds until something pulls (a post's soil, which never pulls apart, broke
        // at its full pressure on it).
        let slipping = (slip / self.lateral).min(1.0);
        creeer_math::det::pow(pull / self.withdrawal, e) + creeer_math::det::pow(slipping, e) > 1.0
    }
}

impl Element {
    /// Whether it is a fastened joint, not a member.
    pub(crate) fn is_joint(&self) -> bool {
        self.fastened.is_some()
    }

    /// Its axes at end b for b's node turned by `rb`: a member's rest axes, a joint's turned
    /// into b's rest frame (`Fastened::to_b`).
    pub(crate) fn b_axes(
        &self,
        rb: creeer_math::glam::DMat3,
    ) -> (
        creeer_math::glam::DVec3,
        creeer_math::glam::DVec3,
        creeer_math::glam::DVec3,
    ) {
        match &self.fastened {
            None => (rb * self.axis, rb * self.y, rb * self.z),
            Some(f) => {
                let r = rb * creeer_math::glam::DMat3::from_quat(f.to_b);
                (r * self.axis, r * self.y, r * self.z)
            }
        }
    }
}
