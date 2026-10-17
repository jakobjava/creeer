//! The resolvable stiffness of a face (`patch.rs`): a layer over the face's domain that
//! moves only as the face moves as a whole (pushed in by σ₀, tilted by σ₁, σ₂ about the
//! domain's centroid), pressing per area with the cap's law (`law::Cap`): as stiff as the
//! limit up to the weight of the bodies it joins, stiffening with its pressure beyond.
//!
//! Its energy Ψ(σ) = Σ w·ψ(a·σ) sums the per-area energy ψ at the midpoints of the edges of
//! the domain's triangles (weights a third of each triangle's area): exact for a quadratic
//! ψ, so below the weight the layer is exactly ½·σᵀ·L·σ, L the limit pushing and the limit
//! times J/A tilting. Under more load it is a convex function of σ (ψ convex, its argument
//! affine) whose push and tilt stiffen together, as a heavier load's pressure does.

use super::cap::Cap;
use creeer_math::glam::{DMat3, DVec3};

/// The layer of a face.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// Each quadrature point's weight (m²) and its depth per motion, a = (1, s − s̄, t − t̄).
    points: Vec<(f64, DVec3)>,
    /// The per-area law: the cap's limit per area of the domain (Pa/m), and its reach.
    law: Cap,
    /// Σ w·a·aᵀ: the layer's Hessian per unit of the per-area limit, below the reach.
    quadratic: DMat3,
}

impl Layer {
    /// The layer of the domain's `triangles` (area seen along the normal, and the face
    /// coordinates (s, t) of their corners), centred on `mean`.
    pub fn new(triangles: &[(f64, [[f64; 2]; 3])], mean: [f64; 2]) -> Self {
        let mut points = Vec::with_capacity(3 * triangles.len());
        for &(area, corners) in triangles {
            for j in 0..3 {
                let (p, q) = (corners[j], corners[(j + 1) % 3]);
                let a = DVec3::new(
                    1.0,
                    0.5 * (p[0] + q[0]) - mean[0],
                    0.5 * (p[1] + q[1]) - mean[1],
                );
                points.push((area / 3.0, a));
            }
        }
        let quadratic = points.iter().fold(DMat3::ZERO, |m, &(w, a)| {
            m + DMat3::from_cols(a * a.x, a * a.y, a * a.z) * w
        });
        Self {
            points,
            law: Cap::NONE,
            quadratic,
        }
    }

    /// The layer of the cap `cap` over a domain of area `area` (m²).
    pub fn set(&mut self, cap: Cap, area: f64) {
        let area = area.max(1e-12);
        self.law = Cap {
            limit: cap.limit / area,
            reach: cap.reach,
            carried: cap.carried / area,
        };
    }

    /// Ψ(σ), its gradient and its Hessian.
    pub fn at(&self, sigma: DVec3) -> (f64, DVec3, DMat3) {
        // Below the reach everywhere: the quadratic ½·σᵀ·L·σ (one product, not one per point).
        if self
            .points
            .iter()
            .all(|&(_, a)| a.dot(sigma) <= self.law.reach)
        {
            let l = self.quadratic * self.law.limit;
            let gradient = l * sigma;
            return (0.5 * sigma.dot(gradient), gradient, l);
        }
        let mut energy = 0.0;
        let mut gradient = DVec3::ZERO;
        let mut hessian = DMat3::ZERO;
        for &(w, a) in &self.points {
            let (e, p, k) = self.law.at(a.dot(sigma));
            energy += w * e;
            gradient += a * (w * p);
            hessian += DMat3::from_cols(a * a.x, a * a.y, a * a.z) * (w * k);
        }
        (energy, gradient, hessian)
    }

    /// Whether the layer is quadratic between `from` and `to` (no point beyond its reach).
    pub fn linear(&self, from: DVec3, to: DVec3) -> bool {
        let d = self.law.reach;
        self.points
            .iter()
            .all(|&(_, a)| a.dot(from) <= d && a.dot(to) <= d)
    }
}
