//! What a body's surface is made of, for contacts: its elasticity, and how round the edges
//! and corners of its boxes are (no machined edge is perfectly sharp).
//!
//! Two surfaces pressed together deform both: the contact modulus is
//! E* = 1 / ((1 − ν₁²)/E₁ + (1 − ν₂²)/E₂) (Johnson, *Contact Mechanics*, 1985, §4.2).

use crate::structure::section::Material;

/// A body's surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    /// Young's modulus E (Pa).
    pub youngs_modulus: f64,
    /// Poisson's ratio ν.
    pub poisson_ratio: f64,
    /// Radius of the edges and corners of its boxes (m).
    pub rounding: f64,
}

/// Edge radius of a machined or welded box (m).
const ROUNDING: f64 = 0.002;

/// The largest Poisson's ratio of an isotropic solid (an incompressible one's).
const MOST_POISSON: f64 = 0.5;

impl Surface {
    /// Structural steel: E = 210 GPa, ν = 0.3.
    pub const STEEL: Self = Self::new(210.0e9, 0.3);
    /// Aluminium alloy: E = 69 GPa, ν = 0.33.
    pub const ALUMINIUM: Self = Self::new(69.0e9, 0.33);
    /// Concrete: E = 30 GPa, ν = 0.2.
    pub const CONCRETE: Self = Self::new(30.0e9, 0.2);
    /// Granite and similar hard rock: E = 50 GPa, ν = 0.25.
    pub const ROCK: Self = Self::new(50.0e9, 0.25);
    /// Hard rubber (shore A 70): E = 7 MPa, ν = 0.49 (nearly incompressible).
    pub const RUBBER: Self = Self::new(7.0e6, 0.49);

    /// A surface with the given elasticity and the usual edge radius.
    pub const fn new(youngs_modulus: f64, poisson_ratio: f64) -> Self {
        Self {
            youngs_modulus,
            poisson_ratio,
            rounding: ROUNDING,
        }
    }

    /// The surface of a member's material: ν = E/2G − 1, an isotropic solid's, at most ½.
    ///
    /// A beam's E and G need not be one isotropic solid's: wood's E is along the grain, its G
    /// across it (spruce C24: E/G = 16), and E/2G − 1 = 7 is no Poisson's ratio (an isotropic
    /// solid's lies within −1 < ν ≤ ½). With it 1 − ν² < 0, the contact modulus came out
    /// negative, and a face of a wooden member pressed into the ground pulled itself in: its
    /// energy fell without bound and the step ran off to 10¹⁵⁰ m (G2.3). Wood's own ratios lie
    /// near ½ (ν_LT 0.47, ν_RT 0.43 for spruce, *Wood Handbook* FPL-GTR-282, Table 5-2).
    pub fn of(material: &Material) -> Self {
        let derived = material.youngs_modulus / (2.0 * material.shear_modulus) - 1.0;
        Self::new(material.youngs_modulus, derived.min(MOST_POISSON))
    }

    /// Compliance (1 − ν²)/E (1/Pa).
    fn compliance(&self) -> f64 {
        (1.0 - self.poisson_ratio * self.poisson_ratio) / self.youngs_modulus
    }

    /// The contact modulus E* of this surface pressed against `other` (Pa).
    pub fn contact_modulus(&self, other: &Self) -> f64 {
        1.0 / (self.compliance() + other.compliance())
    }
}

/// The part of a shape that touches, which sets how curved the contact is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Feature {
    /// A flat face.
    Face,
    /// A box's edge (a cylinder of the surface's rounding radius).
    Edge,
    /// A box's corner (a ball of the surface's rounding radius).
    Corner,
    /// A round part of the given radius (a ball, a bump of the ground).
    Round(f64),
}

impl Feature {
    /// Principal curvatures (1/m) of the feature on a surface with edge radius `rounding`.
    fn curvatures(self, rounding: f64) -> [f64; 2] {
        match self {
            Self::Face => [0.0, 0.0],
            Self::Edge => [1.0 / rounding, 0.0],
            Self::Corner => [1.0 / rounding, 1.0 / rounding],
            Self::Round(r) => [1.0 / r, 1.0 / r],
        }
    }
}

/// The radius R of the sphere on a flat that makes the same Hertz contact as the two
/// features (each with its surface's edge radius): R = (R′·R″)^½ over the principal relative
/// radii (Johnson §4.2). Two edges are taken as crossing at right angles. An edge on a
/// face touches along a line; it is taken as a ball of the edge's radius.
pub fn effective_radius((a, rounding_a): (Feature, f64), (b, rounding_b): (Feature, f64)) -> f64 {
    let (ca, mut cb) = (a.curvatures(rounding_a), b.curvatures(rounding_b));
    if a == Feature::Edge && b == Feature::Edge {
        cb.swap(0, 1);
    }
    let (first, second) = (ca[0] + cb[0], ca[1] + cb[1]);
    match (first > 0.0, second > 0.0) {
        (true, true) => 1.0 / (first * second).sqrt(),
        (true, false) | (false, true) => 1.0 / first.max(second),
        // Two flat faces make a patch, never a point contact (`contact/law.rs`).
        (false, false) => {
            debug_assert!(false, "a point contact between two flat faces");
            1.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::section::STEEL_S235;

    #[test]
    fn contact_moduli_and_radii() {
        // Steel on steel: E* = E / (2(1 − ν²)).
        let steel = Surface::of(&STEEL_S235);
        assert!((steel.poisson_ratio - 0.3).abs() < 1e-3);
        let e = steel.contact_modulus(&steel);
        assert!((e / (210.0e9 / (2.0 * (1.0 - 0.09))) - 1.0).abs() < 1e-3);
        // A rigid body on rubber: rubber's own modulus over 1 − ν².
        let rubber = Surface::RUBBER;
        let rigid = Surface::new(1e30, 0.0);
        assert!((rubber.contact_modulus(&rigid) - 7.0e6 / (1.0 - 0.49 * 0.49)).abs() < 1.0);
        // Wood: E along the grain, G across it (E/2G − 1 = 7). Its surface takes ν = ½, and
        // presses with a positive modulus (G2.3: negative, a face pulled itself into the ground).
        let spruce = crate::structure::section::Material {
            youngs_modulus: 11.0e9,
            shear_modulus: 0.69e9,
            ..STEEL_S235
        };
        let wood = Surface::of(&spruce);
        assert!((wood.poisson_ratio - 0.5).abs() < 1e-12);
        let on_concrete = wood.contact_modulus(&Surface::CONCRETE);
        let expected = 1.0 / (0.75 / 11.0e9 + 0.96 / 30.0e9);
        assert!(
            (on_concrete / expected - 1.0).abs() < 1e-12,
            "{on_concrete}"
        );
        let r = |a, b| effective_radius((a, 0.002), (b, 0.002));
        // Ball on a face: its radius. Two balls: R₁R₂/(R₁ + R₂).
        assert!((r(Feature::Round(0.1), Feature::Face) - 0.1).abs() < 1e-12);
        assert!((r(Feature::Round(0.1), Feature::Round(0.3)) - 0.075).abs() < 1e-12);
        // Crossed cylinders of equal radius at right angles: a ball of that radius.
        assert!((r(Feature::Edge, Feature::Edge) - 0.002).abs() < 1e-12);
        // A ball on an edge: between the ball's radius and the edge's.
        let on_edge = r(Feature::Round(0.1), Feature::Edge);
        assert!(on_edge > 0.002 && on_edge < 0.1);
    }
}
