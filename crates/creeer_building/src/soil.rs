//! A post in the ground (G2.4, ADR-0033): the soil round its buried length as springs at its
//! nodes, each a joint to the ground beside it (the Winkler model of laterally loaded piles,
//! with Broms' 1964 ultimate pressures; Reese & Van Impe, *Single Piles and Pile Groups under
//! Lateral Loading*, 2011).
//!
//! - **Across:** the soil's subgrade reaction, k_h = n_h·z/d for its friction (Terzaghi 1955)
//!   and 67·c/d for its cohesion (Davisson 1970), up to its ultimate pressure 3·K_p·σ'_v·d
//!   (Broms, cohesionless) plus 9·c·d below 1.5·d (Broms, cohesive; none above), each over
//!   the span Δz the node stands for. A short post's capacity is then Broms' by integration.
//! - **Along:** skin friction β·σ'_v·π·d·Δz with β = (1 − sin φ)·tan(⅔·φ) (Fellenius'
//!   β-method; timber on soil δ = ⅔·φ) plus the adhesion of its cohesion α·c, α = 0.5 (an
//!   estimate); pulled past it the post comes out. Pressed, the foot also bears (`tip`).
//! - **About:** the skin friction's torque round the post.
//!
//! Mixed soils (both c and φ) take both terms summed: Broms treats them apart (a
//! simplification recorded in `docs/future-improvements.md`).

use creeer_math::det;
use creeer_physics::structure::JointLaw;

/// A loose ground, as the geology gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Soil {
    /// Bulk density (kg/m³), friction angle (rad), cohesion (Pa).
    pub density: f64,
    pub friction: f64,
    pub cohesion: f64,
}

/// Terzaghi's n_h (N/m³) for a friction angle φ: loose sand (30°) 2.2 MN/m³ to dense (40°)
/// 7.0 MN/m³, dry (Terzaghi 1955, as Reese & Van Impe tabulate it), straight between.
fn n_h(friction: f64) -> f64 {
    let degrees = friction.to_degrees();
    let t = ((degrees - 30.0) / 10.0).clamp(0.0, 1.0);
    2.2e6 + t * (7.0e6 - 2.2e6)
}

/// How far a node's soil spring slips at its ultimate pressure (m): a tenth of the post's
/// diameter (p–y curves reach their ultimate at about 3–10 % of d; Reese & Van Impe), and
/// soil never breaks: it flows on at it (the joint's last slip set far beyond).
const ULTIMATE_SLIP: f64 = 0.1;

impl Soil {
    /// The spring of a post of diameter `d` (m) at depth `z` (m), standing for `span` (m) of
    /// its length, under gravity `g` (m/s²): a joint along the post (its axis the post's).
    pub fn spring(&self, d: f64, z: f64, span: f64, g: f64) -> JointLaw {
        let sigma = self.density * g * z;
        let kp = passive(self.friction);
        let cohesive = if z > 1.5 * d {
            9.0 * self.cohesion * d
        } else {
            0.0
        };
        let ultimate = ((3.0 * kp * sigma * d + cohesive) * span).max(1.0);
        let stiffness =
            ((n_h(self.friction) * z / d + 67.0 * self.cohesion / d) * d * span).max(1.0);
        let beta = (1.0 - det::sin(self.friction)) * det::tan(2.0 / 3.0 * self.friction);
        let skin =
            ((beta * sigma + 0.5 * self.cohesion) * std::f64::consts::PI * d * span).max(1.0);
        // Along: as stiff as across (t–z springs reach their skin friction at about 1 % of d).
        let slip = ULTIMATE_SLIP * d;
        JointLaw {
            axial: (skin / (0.01 * d)).max(stiffness),
            bearing: skin,
            // Soil does not snap: pulled, the post slides out at its skin friction (its axial
            // capacity, plastic), never let go by the brittle withdrawal of fasteners.
            withdrawal: f64::MAX,
            lateral: stiffness.max(ultimate / slip),
            lateral_yield: ultimate,
            lateral_peak: ultimate,
            slip_at_peak: slip,
            slip_at_failure: 100.0 * slip,
            twist: skin * 0.5 * d / (0.01 * d) * 0.5 * d,
            twist_yield: skin * 0.5 * d,
            interaction: 2.0,
            damping: 0.02,
        }
    }

    /// What a post's foot of diameter `d` bears at depth `z` (N): N_q·σ'_v + N_c·c over its
    /// area (Meyerhof's factors for a deep foundation's tip: N_q = e^{π·tan φ}·K_p,
    /// N_c = (N_q − 1)·cot φ, or 9 without friction).
    pub fn tip(&self, d: f64, z: f64, g: f64) -> f64 {
        let area = std::f64::consts::PI * 0.25 * d * d;
        let kp = passive(self.friction);
        let nq = det::exp(std::f64::consts::PI * det::tan(self.friction)) * kp;
        let nc = if self.friction > 1e-3 {
            (nq - 1.0) / det::tan(self.friction)
        } else {
            9.0
        };
        (nq * self.density * g * z + nc * self.cohesion) * area
    }
}

/// Rankine's passive coefficient K_p = tan²(45° + φ/2).
fn passive(friction: f64) -> f64 {
    let t = det::tan(std::f64::consts::FRAC_PI_4 + 0.5 * friction);
    t * t
}
